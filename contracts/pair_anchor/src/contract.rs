use crate::error::ContractError;
use crate::state::{Config, CONFIG};

use cosmwasm_bignumber:: {Decimal256, Uint256};

use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, Uint128,
    
    WasmMsg,
};

use crate::response::MsgInstantiateContractResponse;
use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;

use astroport::pair_anchor::{
    AnchorExecuteMsg, ConfigResponse, DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, AnchorQueryMsg, StateResponse, AnchorPoolParams,
};
use astroport::pair_anchor::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolResponse,
    QueryMsg, ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair::{
    InstantiateMsg
};


use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use protobuf::Message;
use std::str::FromStr;
use std::vec;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-pair-anchor";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// contract
// terra1sepfj7s0aeg5967uxnfk4thzlerrsktkpelm5s
// execute_msg
// {
//   "deposit_stable": {}
// }
// coins
// [{"denom":"uusd","amount":"1000000"}]

// contract
// terra1hzh9vpxhsk8253se0vv5jj6etdvxu3nv8z07zu
// execute_msg
// {
//   "send": {
//     "amount": "822048",
//     "contract": "terra1sepfj7s0aeg5967uxnfk4thzlerrsktkpelm5s",
//     "msg": "eyJyZWRlZW1fc3RhYmxlIjp7fX0="
//   }
// }
// {"redeem_stable":{}}
// coins
// []

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
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.asset_infos[0].check(deps.api)?;
    msg.asset_infos[1].check(deps.api)?;

    if msg.asset_infos[0] == msg.asset_infos[1] {
        return Err(ContractError::DoublingAssets {});
    }

    let params: AnchorPoolParams = from_binary(&msg.init_params.unwrap())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address,
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Xyk {},
        },
        factory_addr: addr_validate_to_lower(deps.api, msg.factory_addr.as_str())?,
        anchor_market_addr: addr_validate_to_lower(deps.api, params.anchor_market_addr.as_str())?,
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new())
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
/// 
/// * **ExecuteMsg::AssertAndSend {
///             offer_asset,
///             ask_asset_info,
///             receiver,
///             sender,
///             belief_price,
///             max_spread,
///         }** (internal) Is used as a sub-execution to send received tokens to the receiver and check the spread/price.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { .. } => Err(ContractError::NonSupported {}),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity { .. } => Err(ContractError::NonSupported {}),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => {
            offer_asset.info.check(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(addr_validate_to_lower(deps.api, &to_addr)?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
        ExecuteMsg::AssertAndSend {
            offer_asset,
            ask_asset_info,
            receiver,
            sender,
            belief_price,
            max_spread,
        } => assert_receive_and_send(
            deps,
            env,
            info,
            sender,
            offer_asset,
            ask_asset_info,
            receiver,
            belief_price,
            max_spread,
        ),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If no template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 receive message to process.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
        }) => {
            // Only an asset (token) contract can execute this message
            let mut authorized: bool = false;
            let config: Config = CONFIG.load(deps.storage)?;

            for pool in config.pair_info.asset_infos {
                if let AssetInfo::Token { contract_addr, .. } = &pool {
                    if contract_addr == &info.sender {
                        authorized = true;
                    }
                }
            }

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(addr_validate_to_lower(deps.api, to_addr.as_str())?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token { contract_addr },
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
            )
        }
        Ok(Cw20HookMsg::WithdrawLiquidity {}) => Err(ContractError::NonSupported {}),
        Err(err) => Err(ContractError::Std(err)),
    }
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
/// * **to** is an object of type [`Option<Addr>`]. Sets the recipient of the swap operation.
/// NOTE - the address that wants to swap should approve the pair contract to pull the offer token.
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;

    let config: Config = CONFIG.load(deps.storage)?;

    // If the asset balance is already increased, we should subtract the user deposit from the pool amount
    let pools: Vec<Asset> = config
        .pair_info
        .query_pools(&deps.querier, env.clone().contract.address)?
        .iter()
        .map(|p| {
            p.clone();
        })
        .collect();

    let offer_pool: Asset;
    let ask_pool: Asset;

    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    if ask_pool.amount > Uint128::from(0u128) {
        // send to community? Otherwise it would be sent to the swapper
    }

    let mut messages: Vec<CosmosMsg> = vec![];

    match offer_asset.info {
        AssetInfo::Token { contract_addr, .. } => {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: config.anchor_market_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&AnchorExecuteMsg::RedeemStable {})?,
                })?,
            }))
        }

        AssetInfo::NativeToken { denom, .. } => {
            let amount = offer_asset.amount;
            let asset = Asset {
                info: AssetInfo::NativeToken {
                    denom: denom.clone(),
                },
                amount,
            };
            let amount = amount.checked_sub(asset.compute_tax(&deps.querier)?)?;

            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.anchor_market_addr.to_string(),
                funds: vec![Coin {
                    denom,
                    amount,
                }],
                msg: to_binary(&AnchorExecuteMsg::DepositStable {})?,
            }));
        }
    }

    let receiver = to.unwrap_or_else(|| sender.clone());

    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: env.contract.address.to_string(),
        funds: vec![],
        msg: to_binary(&ExecuteMsg::AssertAndSend {
            offer_asset: Asset {
                amount: offer_asset.amount,
                info: offer_pool.info
            },
            ask_asset_info: ask_pool.info,
            sender: sender.to_string(),
            receiver: receiver.to_string(),
            belief_price,
            max_spread
        })?,
    }));

    Ok(
        Response::new()
            .add_messages(
                // 1. send collateral tokens from the contract to a user
                // 2. send inactive commission fees to the Maker ontract
                messages,
            )
            .add_attribute("action", "orchestrate"),
    )
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
/// * **to** is an object of type [`Option<Addr>`]. Sets the recipient of the swap operation.
/// NOTE - the address that wants to swap should approve the pair contract to pull the offer token.
#[allow(clippy::too_many_arguments)]
pub fn assert_receive_and_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: String,
    offer_asset: Asset,
    ask_asset_info: AssetInfo,
    receiver: String,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    
    if env.contract.address != info.sender {
        // only allowed to be sent by the contract itself
        return Err(ContractError::Unauthorized {});
    }

    let offer_amount = offer_asset.amount;
    let return_amount = ask_asset_info.query_pool(&deps.querier, env.contract.address)?;
    let spread_amount = Uint128::from(0u128);
    let commission_amount = Uint128::from(0u128);

    // println!("Contract->Offer: {:?}", offer_amount);    
    // println!("Contract->Return: {:?}", return_amount);

    // Check the max spread limit (if it was specified)
    assert_max_spread(
        belief_price,
        max_spread,
        offer_amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    // Compute the tax for the receiving asset (if it is a native one)
    let return_asset = Asset {
        info: ask_asset_info.clone(),
        amount: return_amount
    };
    // println!("Contract->Return-Asset: {:?}", return_asset);

    let tax_amount = return_asset.compute_tax(&deps.querier)?;
    let receiver_adr = addr_validate_to_lower(deps.api, receiver.as_str())?;

    // println!("Contract->Receiver: {:?}", receiver_adr);

    let messages: Vec<CosmosMsg> =
        vec![return_asset.into_msg(&deps.querier, receiver_adr)?];

    // No Maker fee
    let maker_fee_amount = Uint128::new(0);

    Ok(Response::new()
        .add_messages(
            // 1. send collateral tokens from the contract to a user
            // 2. send inactive commission fees to the Maker ontract
            messages,
        )
        .add_attribute("action", "swap")
        .add_attribute("sender", sender)
        .add_attribute("receiver", receiver.as_str())
        .add_attribute("offer_asset", offer_asset.info.to_string())
        .add_attribute("ask_asset", ask_asset_info.to_string())
        .add_attribute("offer_amount", offer_amount.to_string())
        .add_attribute("return_amount", return_amount.to_string())
        .add_attribute("tax_amount", tax_amount.to_string())
        .add_attribute("spread_amount", spread_amount.to_string())
        .add_attribute("commission_amount", commission_amount.to_string())
        .add_attribute("maker_fee_amount", maker_fee_amount.to_string()))
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
        QueryMsg::Simulation { offer_asset } => to_binary(&query_simulation(deps, offer_asset)?),
        QueryMsg::ReverseSimulation { ask_asset } => {
            to_binary(&query_reverse_simulation(deps, ask_asset)?)
        }
        QueryMsg::CumulativePrices {} => to_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

/// ## Description
/// Returns information about the pair contract in an object of type [`PairInfo`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_pair_info(deps: Deps) -> StdResult<PairInfo> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(config.pair_info)
}

/// ## Description
/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, config)?;

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
pub fn query_share(_deps: Deps, _amount: Uint128) -> StdResult<Vec<Asset>> {
    // let config: Config = CONFIG.load(deps.storage)?;
    // let (pools, total_share) = pool_info(deps, config)?;
    // let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(vec![])
}

/// ## Description
/// Returns information about a swap simulation in a [`SimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset to swap as well as an amount of the said asset.
pub fn query_simulation(deps: Deps, offer_asset: Asset) -> StdResult<SimulationResponse> {
    let config: Config = CONFIG.load(deps.storage)?;        
    let pools: [AssetInfo; 2] = config.pair_info.asset_infos;

    if offer_asset.info.equal(&pools[0]) || offer_asset.info.equal(&pools[1]) {
    } else {
        return Err(StdError::generic_err(
            "Given offer asset doesn't belong to pairs",
        ));
    }

    let result: StateResponse = deps.querier.query_wasm_smart(
        config.anchor_market_addr, 
        &AnchorQueryMsg::State { block_height: None }
    )?;

    let return_amount;
    let offer_amount: Uint256 = offer_asset.amount.into();

    if offer_asset.is_native_token() {
        return_amount = Uint128::from(offer_amount / result.prev_exchange_rate);
    } else {
        return_amount = Uint128::from(offer_amount * result.prev_exchange_rate);
    }

    let spread_amount = Uint128::from(0u128);
    let commission_amount = Uint128::from(0u128);

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
}

/// ## Description
/// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **ask_asset** is an object of type [`Asset`]. This is the asset to swap to as well as the desired
/// amount of ask assets to receive from the swap.
pub fn query_reverse_simulation(
    deps: Deps,
    ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let pools: [AssetInfo; 2] = config.pair_info.asset_infos;

    if ask_asset.info.equal(&pools[0]) || ask_asset.info.equal(&pools[1]) {
    } else {
        return Err(StdError::generic_err(
            "Given ask asset doesn't belong to pairs",
        ));
    }
    
    let result: StateResponse = deps.querier.query_wasm_smart(
        config.anchor_market_addr, 
        &AnchorQueryMsg::State { block_height: None }
    )?;

    let offer_amount;
    let return_amount: Uint256 = ask_asset.amount.into();

    if ask_asset.is_native_token() {
        offer_amount = Uint128::from(return_amount / result.prev_exchange_rate);
    } else {
        offer_amount = Uint128::from(return_amount * result.prev_exchange_rate);
    }

    let spread_amount = Uint128::from(0u128);
    let commission_amount = Uint128::from(0u128);

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
    })
}

/// ## Description
/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
pub fn query_cumulative_prices(deps: Deps, _env: Env) -> StdResult<CumulativePricesResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, config)?;

    let price0_cumulative_last = Uint128::from(0u128);
    let price1_cumulative_last = Uint128::from(0u128);

    let resp = CumulativePricesResponse {
        assets,
        total_share,
        price0_cumulative_last,
        price1_cumulative_last,
    };

    Ok(resp)
}

/// ## Description
/// Returns the pair contract configuration in a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(_deps: Deps) -> StdResult<ConfigResponse> {
    // let config: Config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        block_time_last: 0u64,
        params: None,
    })
}

/// ## Description
/// Returns a [`ContractError`] on failure.
/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
/// ## Params
/// * **belief_price** is an object of type [`Option<Decimal>`]. This is the belief price used in the swap.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. This is the
/// max spread allowed so that the swap can be executed successfuly.
///
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of assets to swap.
///
/// * **return_amount** is an object of type [`Uint128`]. This is the amount of assets to receive from the swap.
///
/// * **spread_amount** is an object of type [`Uint128`]. This is the spread used in the swap.
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    let default_spread = Decimal::from_str(DEFAULT_SLIPPAGE)?;
    let max_allowed_spread = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;

    let max_spread = max_spread.unwrap_or(default_spread);
    if max_spread.gt(&max_allowed_spread) {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    if let Some(belief_price) = belief_price {
        let expected_return =
            offer_amount * Decimal::from(Decimal256::one() / Decimal256::from(belief_price));
        let spread_amount = expected_return
            .checked_sub(return_amount)
            .unwrap_or_else(|_| Uint128::zero());

        if return_amount < expected_return
            && Decimal::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if Decimal::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
        return Err(ContractError::MaxSpreadAssertion {});
    }

    Ok(())
}

/// ## Description
/// This is an internal function that enforces slippage tolerance for swaps.
/// Returns a [`ContractError`] on failure, otherwise returns [`Ok`].
/// ## Params
/// * **slippage_tolerance** is an object of type [`Option<Decimal>`]. This is the slippage tolerance to enforce.
///
/// * **deposits** are an array of [`Uint128`] type items. These are offer and ask amounts for a swap.
///
/// * **pools** are an array of [`Asset`] type items. These are total amounts of assets in the pool.
// fn assert_slippage_tolerance(
//     slippage_tolerance: Option<Decimal>,
//     deposits: &[Uint128; 2],
//     pools: &[Asset; 2],
// ) -> Result<(), ContractError> {
//     let default_slippage = Decimal::from_str(DEFAULT_SLIPPAGE)?;
//     let max_allowed_slippage = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;

//     let slippage_tolerance = slippage_tolerance.unwrap_or(default_slippage);
//     if slippage_tolerance.gt(&max_allowed_slippage) {
//         return Err(ContractError::AllowedSpreadAssertion {});
//     }

//     let slippage_tolerance: Decimal256 = slippage_tolerance.into();
//     let one_minus_slippage_tolerance = Decimal256::one() - slippage_tolerance;
//     let deposits: [Uint256; 2] = [deposits[0].into(), deposits[1].into()];
//     let pools: [Uint256; 2] = [pools[0].amount.into(), pools[1].amount.into()];

//     // Ensure each price does not change more than what the slippage tolerance allows
//     if Decimal256::from_ratio(deposits[0], deposits[1]) * one_minus_slippage_tolerance
//         > Decimal256::from_ratio(pools[0], pools[1])
//         || Decimal256::from_ratio(deposits[1], deposits[0]) * one_minus_slippage_tolerance
//             > Decimal256::from_ratio(pools[1], pools[0])
//     {
//         return Err(ContractError::MaxSlippageAssertion {});
//     }

//     Ok(())
// }

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

/// ## Description
/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **config** is an object of type [`Config`].
pub fn pool_info(_deps: Deps, config: Config) -> StdResult<([Asset; 2], Uint128)> {
    // let contract_addr = config.pair_info.contract_addr.clone();
    // let pools: [Asset; 2] = config.pair_info.query_pools(&deps.querier, contract_addr)?;
    // let total_share: Uint128 = query_supply(&deps.querier, config.pair_info.liquidity_token)?;
    
    let pools: [Asset; 2] = [
        Asset {
            amount: Uint128::from(0u128),
            info: config.pair_info.asset_infos[0].clone()
        },
        Asset {
            amount: Uint128::from(0u128),
            info: config.pair_info.asset_infos[1].clone()
        },
    ];
    
    let total_share = Uint128::from(0u128);

    Ok((pools, total_share))
}