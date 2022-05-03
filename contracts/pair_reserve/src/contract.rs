use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Api, Binary, CosmosMsg, Decimal, Deps,
    DepsMut, Env, Fraction, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20CoinVerified, Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use protobuf::Message;

use astroport::asset::{addr_validate_to_lower, format_lp_token_name, Asset, AssetInfo, PairInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::factory::PairType;
use astroport::pair_reserve::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, FlowParams, InitParams, InstantiateMsg,
    MigrateMsg, PoolParams, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
    UpdateFlowParams, UpdateParams,
};
use astroport::querier::{query_fee_info, query_supply};
use astroport::DecimalCheckedOps;

use crate::error::ContractError;
use crate::general::{
    check_asset_info, get_oracle_price, get_share_in_assets, pool_info, validate_addresses,
    AssetsValidator, ParametersValidator, RateDirection,
};
use crate::math::{assert_max_spread, compute_reverse_swap, compute_swap, replenish_pools};
use crate::response::MsgInstantiateContractResponse;
use crate::state::{Config, CONFIG, OWNERSHIP_PROPOSAL};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-reserve-pair";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if
/// the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.asset_infos.validate(deps.api)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let init_params: InitParams = from_binary(
        &msg.init_params
            .ok_or_else(|| StdError::generic_err("Initial parameters were not found"))?,
    )?;

    let oracles = init_params
        .oracles
        .iter()
        .map(|oracle| addr_validate_to_lower(deps.api, oracle))
        .collect::<StdResult<Vec<_>>>()?;

    let mut pool_params = PoolParams {
        entry: Default::default(),
        exit: Default::default(),
        last_repl_block: env.block.height,
        oracles,
    };

    init_params
        .pool_params
        .entry
        .as_ref()
        .ok_or(StdError::generic_err("Entry flow params are not set"))?;
    init_params
        .pool_params
        .exit
        .as_ref()
        .ok_or(StdError::generic_err("Exit flow params are not set"))?;
    update_flow_params(&mut pool_params.entry, init_params.pool_params.entry);
    update_flow_params(&mut pool_params.exit, init_params.pool_params.exit);
    pool_params.validate(None)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Reserve {},
        },
        factory_addr: addr_validate_to_lower(deps.api, &msg.factory_addr)?,
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        providers_whitelist: vec![],
        pool_params,
    };

    CONFIG.save(deps.storage, &config)?;

    let token_name = format_lp_token_name(msg.asset_infos, &deps.querier)?;

    // Create the LP token contract
    let sub_msg = SubMsg::reply_on_success(
        WasmMsg::Instantiate {
            code_id: msg.token_code_id,
            msg: to_binary(&astroport::token::InstantiateMsg {
                name: token_name,
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
            })?,
            funds: vec![],
            admin: None,
            label: String::from("Astroport LP token"),
        },
        0,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

/// ## Description
/// The entry point to the contract for processing replies from submessages.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`Reply`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.pair_info.liquidity_token != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    config.pair_info.liquidity_token =
        addr_validate_to_lower(deps.api, res.get_contract_address())?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Not supported.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ProvideLiquidity {
///             assets,
///             slippage_tolerance,
///             auto_stake,
///             receiver,
///         }** Provides liquidity in the pair with the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs a swap operation with the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets, receiver, ..
        } => provide_liquidity(deps, env, info, assets, receiver),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => {
            check_asset_info(deps.api, &offer_asset.info)?;

            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }
            offer_asset.assert_sent_native_token_balance(&info)?;

            let to_addr = to
                .map(|addr| addr_validate_to_lower(deps.api, &addr))
                .transpose()?
                .unwrap_or(info.sender.clone());

            swap(
                deps,
                env,
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
        ExecuteMsg::UpdateProvidersWhitelist {
            append_addrs,
            remove_addrs,
        } => {
            let mut config = CONFIG.load(deps.storage)?;
            // Authorization check
            if info.sender != config.owner {
                Err(ContractError::Unauthorized {})
            } else {
                let result = update_addr_list(
                    deps.api,
                    &mut config.providers_whitelist,
                    append_addrs,
                    remove_addrs,
                    "update_providers_whitelist",
                );
                CONFIG.save(deps.storage, &config)?;

                result
            }
        }
        ExecuteMsg::UpdatePoolParameters { params } => update_pool_params(deps, info, params),
        ExecuteMsg::UpdateOracles {
            append_addrs,
            remove_addrs,
        } => {
            let mut config = CONFIG.load(deps.storage)?;
            // Authorization check
            if info.sender != config.owner {
                Err(ContractError::Unauthorized {})
            } else {
                let result = update_addr_list(
                    deps.api,
                    &mut config.pool_params.oracles,
                    append_addrs,
                    remove_addrs,
                    "update_oracles",
                );
                // TODO: validate oracle queries?
                CONFIG.save(deps.storage, &config)?;

                result
            }
        }
        ExecuteMsg::ProposeNewOwner {
            new_owner,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;
            propose_new_owner(
                deps,
                info,
                env,
                new_owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;
                Ok(())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::UpdateConfig { .. } => Err(ContractError::NonSupported {}),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise it returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
        } => {
            let config = CONFIG.load(deps.storage)?;

            // Only asset contract can execute this message
            let authorized =
                config
                    .pair_info
                    .asset_infos
                    .iter()
                    .any(|asset_info| match asset_info {
                        AssetInfo::Token { contract_addr } if contract_addr == &info.sender => true,
                        _ => false,
                    });

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let user = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
            let to_addr = to
                .map(|addr| addr_validate_to_lower(deps.api, &addr))
                .transpose()?
                .unwrap_or(user);

            let sender_addr = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;

            swap(
                deps,
                env,
                sender_addr,
                Asset {
                    info: AssetInfo::Token { contract_addr },
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
            )
        }
        Cw20HookMsg::WithdrawLiquidity {} => {
            let sender_addr = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
            withdraw_liquidity(deps, info, sender_addr, cw20_msg.amount)
        }
    }
}

/// ## Description
/// Provides liquidity in the pair with the specified input parameters.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **assets** is an array with two objects of type [`Asset`]. These are the assets available in the pool.
///
/// * **slippage_tolerance** is an [`Option`] field of type [`Decimal`]. It is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
/// * **auto_stake** is an [`Option`] field of type [`bool`]. Determines whether the LP tokens minted after
/// liquidity provision are automatically staked in the Generator contract on behalf of the LP token receiver.
///
/// * **receiver** is an [`Option`] field of type [`String`]. This is the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    assets.validate(deps.api)?;

    let config: Config = CONFIG.load(deps.storage)?;

    if !config.providers_whitelist.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    let pools = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address.clone())?;

    let pool_infos = pools
        .iter()
        .map(|asset| asset.info.clone())
        .collect::<Vec<_>>();

    let btc_asset = assets
        .iter()
        .find_map(|asset| match &asset.info {
            AssetInfo::Token { contract_addr } if pool_infos.contains(&asset.info) => {
                Some(Cw20CoinVerified {
                    address: contract_addr.clone(),
                    amount: asset.amount,
                })
            }
            _ => None,
        })
        .ok_or(StdError::generic_err(
            "Provided token does not belong to the pair",
        ))?
        .clone();

    if btc_asset.amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let transfer_msg = SubMsg::new(WasmMsg::Execute {
        contract_addr: btc_asset.address.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: info.sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount: btc_asset.amount,
        })?,
        funds: vec![],
    });

    let total_share = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;
    let share = if total_share.is_zero() {
        btc_asset.amount
    } else {
        let btc_pool = pools
            .into_iter()
            .find(|asset| !asset.is_native_token())
            .unwrap();

        btc_asset
            .amount
            .multiply_ratio(total_share, btc_pool.amount)
    };

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = receiver
        .map(|addr| addr_validate_to_lower(deps.api, &addr))
        .transpose()?
        .unwrap_or_else(|| info.sender.clone());
    let mint_msg = SubMsg::new(WasmMsg::Execute {
        contract_addr: config.pair_info.liquidity_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: share,
        })?,
        funds: vec![],
    });

    Ok(Response::new()
        .add_submessages(vec![transfer_msg, mint_msg])
        .add_attributes(vec![
            attr("action", "provide_liquidity"),
            attr("sender", info.sender),
            attr("receiver", receiver),
            attr("assets", format!("{}, {}", assets[0], assets[1])),
            attr("share", share),
        ]))
}

/// ## Description
/// Withdraw liquidity from the pool. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **sender** is an object of type [`Addr`]. This is the address that will receive assets back from the pair contract.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to burn.
pub fn withdraw_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let (pools, total_share) = pool_info(&deps.querier, &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    let mut messages = vec![];
    for asset in refund_assets.iter() {
        if !asset.amount.is_zero() {
            messages.push(asset.clone().into_msg(&deps.querier, sender.clone())?)
        }
    }

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: config.pair_info.liquidity_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
        funds: vec![],
    }));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr(
            "refund_assets",
            format!("{}, {}", refund_assets[0], refund_assets[1]),
        ),
    ]))
}

pub fn update_addr_list(
    api: &dyn Api,
    addr_list_ref: &mut Vec<Addr>,
    append_addrs: Vec<String>,
    remove_addrs: Vec<String>,
    action: &str,
) -> Result<Response, ContractError> {
    let append: Vec<_> = validate_addresses(api, &append_addrs)?
        .into_iter()
        .filter(|addr| !addr_list_ref.contains(addr))
        .collect();
    let remove: Vec<_> = validate_addresses(api, &remove_addrs)?
        .into_iter()
        .filter(|addr| addr_list_ref.contains(addr))
        .collect();

    if append.is_empty() && remove.is_empty() {
        return Err(StdError::generic_err("Append and remove arrays are empty").into());
    }

    addr_list_ref.retain(|addr| !remove.contains(addr));
    addr_list_ref.extend(append);

    let mut attrs = vec![attr("action", action)];
    if !append_addrs.is_empty() {
        attrs.push(attr("added_addresses", append_addrs.join(",")))
    }
    if !remove_addrs.is_empty() {
        attrs.push(attr("removed_addresses", remove_addrs.join(",")))
    }

    Ok(Response::default().add_attributes(attrs))
}

pub fn update_flow_params(flow_params: &mut FlowParams, update_params: Option<UpdateFlowParams>) {
    if let Some(update_params) = update_params {
        flow_params.recovery_period = update_params.recovery_period;
        flow_params.base_pool = update_params.base_pool;
        flow_params.min_spread = update_params.min_spread;
    }
}

pub fn update_pool_params(
    deps: DepsMut,
    info: MessageInfo,
    update_params_msg: UpdateParams,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    update_flow_params(&mut config.pool_params.entry, update_params_msg.entry);
    update_flow_params(&mut config.pool_params.exit, update_params_msg.exit);

    config.pool_params.validate(None)?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "update_pool_params")]))
}

/// ## Description
/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **sender** is an object of type [`Addr`]. This is the sender of the swap operation.
///
/// * **offer_asset** is an object of type [`Asset`]. Proposed asset for swapping.
///
/// * **belief_price** is an object of type [`Option<Decimal>`]. Used to calculate the maximum swap spread.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. Sets the maximum spread of the swap operation.
///
/// * **to** is an object of type [`Addr`]. Sets the recipient of the swap operation.
/// NOTE - the address that wants to swap should approve the pair contract to pull the offer token.
pub fn swap(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    receiver: Addr,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let ask_asset = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address.clone())?
        .into_iter()
        .find(|pool| pool.info.ne(&offer_asset.info))
        .ok_or_else(|| StdError::generic_err("Asset was not found"))?;

    if ask_asset.amount.is_zero() {
        return Err(ContractError::AskPoolEmpty {});
    }

    replenish_pools(&mut config.pool_params, env.block.height)?;
    let mut swap_result = compute_swap(&deps.querier, &offer_asset, &mut config.pool_params)?;
    assert_max_spread(offer_asset.amount, swap_result, belief_price, max_spread)?;

    let mut messages = vec![];

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;
    let mut maker_fee = Uint128::zero();
    if let Some(fee_addr) = fee_info.fee_address {
        maker_fee = fee_info.maker_fee_rate.checked_mul(swap_result.amount)?;
        if !maker_fee.is_zero() {
            messages.push(
                Asset {
                    info: ask_asset.info.clone(),
                    amount: maker_fee,
                }
                .into_msg(&deps.querier, fee_addr)?,
            );
            swap_result.amount = swap_result.amount.checked_sub(maker_fee)?;
        }
    }

    let ask_asset = Asset {
        info: ask_asset.info.clone(),
        amount: swap_result.amount,
    };
    messages.push(
        ask_asset
            .clone()
            .into_msg(&deps.querier, receiver.clone())?,
    );

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap"),
        attr("sender", sender),
        attr("receiver", receiver),
        attr("offer_asset", offer_asset.to_string()),
        attr("ask_asset", ask_asset.to_string()),
        attr("spread", swap_result.spread.to_string()),
        attr("spread_ust_fee", swap_result.spread_ust_fee),
        attr("maker_fee", maker_fee),
    ]))
}

/// ## Description
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Pair {}** Returns information about the pair in an object of type [`PairInfo`].
///
/// * **QueryMsg::Pool {}** Returns information about the amount of assets in the pair contract as
/// well as the amount of LP tokens issued using an object of type [`PoolResponse`].
///
/// * **QueryMsg::Share { amount }** Returns the amount of assets that could be withdrawn from the pool
/// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **QueryMsg::Simulation { offer_asset }** Returns the result of a swap simulation using a [`SimulationResponse`] object.
///
/// * **QueryMsg::ReverseSimulation { ask_asset }** Returns the result of a reverse swap simulation  using
/// a [`ReverseSimulationResponse`] object.
///
/// * **QueryMsg::CumulativePrices {}** Returns information about cumulative prices for the assets in the
/// pool using a [`CumulativePricesResponse`] object.
///
/// * **QueryMsg::Config {}** Returns the configuration for the pair contract using a [`ConfigResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&query_pair_info(deps)?),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation { offer_asset } => {
            to_binary(&query_simulation(deps, env, offer_asset)?)
        }
        QueryMsg::ReverseSimulation { ask_asset } => {
            to_binary(&query_reverse_simulation(deps, env, ask_asset)?)
        }
        QueryMsg::CumulativePrices {} => to_binary(&query_prices(deps)?),
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
    }
}

/// ## Description
/// Returns information about the pair contract in an object of type [`PairInfo`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_pair_info(deps: Deps) -> StdResult<PairInfo> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config.pair_info)
}

/// ## Description
/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(&deps.querier, &config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

/// ## Description
/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens for which we calculate associated amounts of assets.
pub fn query_share(deps: Deps, amount: Uint128) -> StdResult<Vec<Asset>> {
    let config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(&deps.querier, &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(refund_assets)
}

/// ## Description
/// Returns information about a swap simulation in a [`SimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset to swap as well as an amount of the said asset.
pub fn query_simulation(deps: Deps, env: Env, offer_asset: Asset) -> StdResult<SimulationResponse> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    replenish_pools(&mut config.pool_params, env.block.height)?;
    let swap_result = compute_swap(&deps.querier, &offer_asset, &mut config.pool_params)
        .map_err(|err| StdError::generic_err(err.to_string()))?;

    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;
    let commission_amount = if fee_info.fee_address.is_some() {
        fee_info.maker_fee_rate.checked_mul(swap_result.amount)?
    } else {
        Uint128::zero()
    };

    Ok(SimulationResponse {
        return_amount: swap_result.amount - commission_amount,
        spread_amount: swap_result.spread_ust_fee,
        commission_amount,
    })
}

/// ## Description
/// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **ask_asset** is an object of type [`Asset`]. This is the asset to swap to as well as the desired
/// amount of ask assets to receive from the swap.
pub fn query_reverse_simulation(
    deps: Deps,
    env: Env,
    mut ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;
    // after_fee = before_fee_amount * (1 - fee_rate), thus
    // return_amount = after_fee / (1 - fee_rate)
    let mut conversion_rate = Decimal::one();
    if fee_info.fee_address.is_some() {
        conversion_rate = conversion_rate - fee_info.maker_fee_rate
    }
    // after_fee / (x/y) = after_fee * y / x
    ask_asset.amount = conversion_rate
        .inv()
        .unwrap()
        .checked_mul(ask_asset.amount)?;

    replenish_pools(&mut config.pool_params, env.block.height)?;
    let swap_result = compute_reverse_swap(&deps.querier, &ask_asset, &config.pool_params)
        .map_err(|err| StdError::generic_err(err.to_string()))?;

    Ok(ReverseSimulationResponse {
        offer_amount: swap_result.amount,
        spread_amount: swap_result.spread_ust_fee,
        commission_amount: fee_info.maker_fee_rate.checked_mul(swap_result.amount)?,
    })
}

pub fn query_prices(deps: Deps) -> StdResult<CumulativePricesResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(&deps.querier, &config)?;

    let price_closure = |asset: &Asset| -> StdResult<Uint128> {
        let direction = if asset.is_native_token() {
            RateDirection::USD2BTC
        } else {
            RateDirection::BTC2USD
        };
        let price = get_oracle_price(&deps.querier, direction, &config.pool_params.oracles)
            .map_err(|err| StdError::generic_err(err.to_string()))?;
        Ok(price * asset.amount)
    };

    Ok(CumulativePricesResponse {
        assets: assets.clone(),
        total_share,
        price0_cumulative_last: price_closure(&assets[0])?,
        price1_cumulative_last: price_closure(&assets[1])?,
    })
}

/// ## Description
/// Used for the contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
