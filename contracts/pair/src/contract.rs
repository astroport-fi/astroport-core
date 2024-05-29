use std::convert::TryInto;
use std::str::FromStr;
use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure_eq, from_json, to_json_binary, wasm_execute, Addr, Binary, Coin, CosmosMsg,
    CustomMsg, CustomQuery, Decimal, Decimal256, Deps, DepsMut, Env, Event, Fraction, MessageInfo,
    QuerierWrapper, Reply, Response, StdError, StdResult, SubMsg, SubMsgResponse, SubMsgResult,
    Uint128, Uint256, Uint64, WasmMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

use astroport::asset::{
    addr_opt_validate, check_swap_parameters, Asset, AssetInfo, CoinsExt, PairInfo,
    MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::factory::PairType;
use astroport::incentives::ExecuteMsg as IncentiveExecuteMsg;
use astroport::pair::{
    ConfigResponse, FeeShareConfig, ReplyIds, XYKPoolConfig, XYKPoolParams, XYKPoolUpdateParams,
    DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, MAX_FEE_SHARE_BPS,
};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolResponse,
    QueryMsg, ReverseSimulationResponse, SimulationResponse, TWAP_PRECISION,
};

use astroport::querier::{
    query_factory_config, query_fee_info, query_native_supply, query_tracker_config,
};
use astroport::token_factory::{
    tf_before_send_hook_msg, tf_burn_msg, tf_create_denom_msg, tf_mint_msg, MsgCreateDenomResponse,
};
use astroport::{tokenfactory_tracker, U256};
use cw_utils::{
    one_coin, parse_reply_instantiate_data, MsgInstantiateContractResponse, PaymentError,
};

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-pair";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Tokenfactory LP token subdenom
pub const LP_SUBDENOM: &str = "astroport/share";

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.asset_infos.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }

    msg.asset_infos[0].check(deps.api)?;
    msg.asset_infos[1].check(deps.api)?;

    if msg.asset_infos[0] == msg.asset_infos[1] {
        return Err(ContractError::DoublingAssets {});
    }

    let mut track_asset_balances = false;

    if let Some(init_params) = msg.init_params {
        let params: XYKPoolParams = from_json(init_params)?;
        track_asset_balances = params.track_asset_balances.unwrap_or_default();
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: "".to_owned(),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Xyk {},
        },
        factory_addr: deps.api.addr_validate(msg.factory_addr.as_str())?,
        block_time_last: 0,
        price0_cumulative_last: Uint128::zero(),
        price1_cumulative_last: Uint128::zero(),
        track_asset_balances,
        fee_share: None,
        tracker_addr: None,
    };

    if track_asset_balances {
        for asset in &config.pair_info.asset_infos {
            BALANCES.save(deps.storage, asset, &Uint128::zero(), env.block.height)?;
        }
    }

    CONFIG.save(deps.storage, &config)?;

    // Create LP token
    let sub_msg: SubMsg<_> = SubMsg::reply_on_success(
        tf_create_denom_msg(env.contract.address.to_string(), LP_SUBDENOM),
        ReplyIds::CreateDenom as u64,
    );

    Ok(Response::new().add_submessage(sub_msg).add_attribute(
        "asset_balances_tracking".to_owned(),
        if config.track_asset_balances {
            "enabled"
        } else {
            "disabled"
        }
        .to_owned(),
    ))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match ReplyIds::try_from(msg.id)? {
        ReplyIds::CreateDenom => {
            if let SubMsgResult::Ok(SubMsgResponse { data: Some(b), .. }) = msg.result {
                let MsgCreateDenomResponse { new_token_denom } = b.try_into()?;
                let config = CONFIG.load(deps.storage)?;

                let mut sub_msgs = vec![];
                if config.track_asset_balances {
                    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;
                    let tracker_config = query_tracker_config(&deps.querier, config.factory_addr)?;
                    // Instantiate tracking contract
                    let sub_msg: Vec<SubMsg> = vec![SubMsg::reply_on_success(
                        WasmMsg::Instantiate {
                            admin: Some(factory_config.owner.to_string()),
                            code_id: tracker_config.code_id,
                            msg: to_json_binary(&tokenfactory_tracker::InstantiateMsg {
                                tokenfactory_module_address: tracker_config
                                    .token_factory_addr
                                    .to_string(),
                                tracked_denom: new_token_denom.clone(),
                            })?,
                            funds: vec![],
                            label: format!("{new_token_denom} tracking contract"),
                        },
                        ReplyIds::InstantiateTrackingContract as u64,
                    )];

                    sub_msgs.extend(sub_msg);
                }

                CONFIG.update(deps.storage, |mut config| {
                    if !config.pair_info.liquidity_token.is_empty() {
                        return Err(StdError::generic_err(
                            "Liquidity token is already set in the config",
                        ));
                    }

                    config.pair_info.liquidity_token = new_token_denom.clone();
                    Ok(config)
                })?;

                Ok(Response::new()
                    .add_submessages(sub_msgs)
                    .add_attribute("lp_denom", new_token_denom))
            } else {
                Err(ContractError::FailedToParseReply {})
            }
        }
        ReplyIds::InstantiateTrackingContract => {
            let MsgInstantiateContractResponse {
                contract_address, ..
            } = parse_reply_instantiate_data(msg)?;

            let config = CONFIG.update::<_, StdError>(deps.storage, |mut c| {
                c.tracker_addr = Some(deps.api.addr_validate(&contract_address)?);
                Ok(c)
            })?;

            let set_hook_msg = tf_before_send_hook_msg(
                env.contract.address,
                config.pair_info.liquidity_token,
                contract_address.clone(),
            );

            Ok(Response::new()
                .add_message(set_hook_msg)
                .add_attribute("tracker_contract", contract_address))
        }
    }
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
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
///            min_lp_to_receive,
///         }** Provides liquidity in the pair with the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs a swap operation with the specified parameters.
/// * **ExecuteMsg::WithdrawLiquidity {
///            assets,
///           min_assets_to_receive,
///       }** Withdraws liquidity from the pool.
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
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        ),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
            ..
        } => {
            offer_asset.info.check(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Cw20DirectSwap {});
            }

            let to_addr = addr_opt_validate(deps.api, &to)?;

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
        ExecuteMsg::UpdateConfig { params } => update_config(deps, info, params),
        ExecuteMsg::WithdrawLiquidity {
            assets,
            min_assets_to_receive,
        } => withdraw_liquidity(deps, env, info, assets, min_assets_to_receive),
        _ => Err(ContractError::NonSupported {}),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 message that has to be processed.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
            ..
        } => {
            // Only asset contract can execute this message
            let mut authorized = false;
            let config = CONFIG.load(deps.storage)?;

            for pool in config.pair_info.asset_infos {
                if let AssetInfo::Token { contract_addr, .. } = &pool {
                    if contract_addr == info.sender {
                        authorized = true;
                    }
                }
            }

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = addr_opt_validate(deps.api, &to)?;
            let contract_addr = info.sender.clone();

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
    }
}

/// Provides liquidity in the pair with the specified input parameters.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
/// * **auto_stake** is an optional parameter which determines whether the LP tokens minted after
/// liquidity provision are automatically staked in the Incentives contract on behalf of the LP token receiver.
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
///
/// * **min_lp_to_receive** is an optional parameter which specifies the minimum amount of LP tokens to receive.
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
#[allow(clippy::too_many_arguments)]
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
    min_lp_to_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let mut pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let deposits = get_deposits_from_assets(deps.as_ref(), &assets, &pools)?;

    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let auto_stake = auto_stake.unwrap_or(false);

    let mut messages = vec![];
    let mut events = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the asset is a token contract, then we need to execute a TransferFrom msg to receive assets
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: deposits[i],
                })?,
                funds: vec![],
            }));
        } else {
            // If the asset is native token, the pool balance is already increased
            // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let share = calculate_shares(&deposits, &pools, total_share, slippage_tolerance)?;

    if total_share.is_zero() {
        messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);

        events.insert(
            0,
            Event::new("astroport-pool.v1.Mint").add_attributes([
                attr("action", "mint"),
                attr("to", env.contract.address.as_str()),
                attr("amount", MINIMUM_LIQUIDITY_AMOUNT.to_string()),
            ]),
        );
    }

    let min_amount_lp = min_lp_to_receive.unwrap_or(Uint128::zero());

    if share < min_amount_lp {
        return Err(ContractError::ProvideSlippageViolation(
            share,
            min_amount_lp,
        ));
    }

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        share,
        auto_stake,
    )?);

    events.insert(
        events.len(),
        Event::new("astroport-pool.v1.Mint").add_attributes([
            attr("action", "mint"),
            attr("to", receiver.clone()),
            attr("amount", share),
        ]),
    );

    if config.track_asset_balances {
        for (i, pool) in pools.iter().enumerate() {
            BALANCES.save(
                deps.storage,
                &pool.info,
                &pool.amount.checked_add(deposits[i])?,
                env.block.height,
            )?;
        }
    }

    // Accumulate prices for the assets in the pool
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) =
        accumulate_prices(env, &config, pools[0].amount, pools[1].amount)?
    {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    events.insert(
        0,
        Event::new("astroport-pool.v1.ProvideLiqudity").add_attributes([
            attr("action", "provide_liquidity"),
            attr("sender", info.sender),
            attr("receiver", receiver),
            attr("assets", format!("{}, {}", assets[0], assets[1])),
            attr("share", share),
        ]),
    );

    Ok(Response::new().add_messages(messages).add_events(events))
}

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Incentive contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **coin** denom and amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
/// be automatically staked in the Incentives contract on behalf of the recipient.
pub fn mint_liquidity_token_message<T, C>(
    querier: QuerierWrapper<C>,
    config: &Config,
    contract_address: &Addr,
    recipient: &Addr,
    amount: Uint128,
    auto_stake: bool,
) -> Result<Vec<CosmosMsg<T>>, ContractError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    let coin = coin(amount.into(), config.pair_info.liquidity_token.to_string());

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(tf_mint_msg(contract_address, coin, recipient));
    }

    // Mint for the pair contract and stake into the Incentives contract
    let incentives_addr = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    if let Some(address) = incentives_addr {
        let mut msgs = tf_mint_msg(contract_address, coin.clone(), contract_address);
        msgs.push(
            wasm_execute(
                address,
                &IncentiveExecuteMsg::Deposit {
                    recipient: Some(recipient.to_string()),
                },
                vec![coin],
            )?
            .into(),
        );
        Ok(msgs)
    } else {
        Err(ContractError::AutoStakeError {})
    }
}

/// Withdraw liquidity from the pool.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage).unwrap();

    let Coin { amount, denom } = one_coin(&info)?;

    ensure_eq!(
        denom,
        config.pair_info.liquidity_token,
        PaymentError::MissingDenom(config.pair_info.liquidity_token.to_string())
    );

    let (pools, total_share) = pool_info(deps.querier, &config)?;

    let refund_assets = if assets.is_empty() {
        // Usual withdraw (balanced)
        get_share_in_assets(&pools, amount, total_share)
    } else {
        return Err(StdError::generic_err("Imbalanced withdraw is currently disabled").into());
    };

    ensure_min_assets_to_receive(&config, refund_assets.clone(), min_assets_to_receive)?;

    if config.track_asset_balances {
        for (i, pool) in pools.iter().enumerate() {
            BALANCES.save(
                deps.storage,
                &pool.info,
                &(pool.amount - refund_assets[i].amount),
                env.block.height,
            )?;
        }
    }

    // Accumulate prices for the pair assets
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) =
        accumulate_prices(env.clone(), &config, pools[0].amount, pools[1].amount)?
    {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    // Update the pool info
    let mut messages = refund_assets
        .clone()
        .into_iter()
        .map(|asset| asset.into_msg(&info.sender))
        .collect::<StdResult<Vec<_>>>()?;

    messages.push(tf_burn_msg(
        env.contract.address,
        coin(amount.u128(), config.pair_info.liquidity_token.to_string()),
    ));

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", &info.sender),
        attr("withdrawn_share", amount),
        attr(
            "refund_assets",
            format!("{}, {}", refund_assets[0], refund_assets[1]),
        ),
    ]))
}

/// Returns the amount of pool assets that correspond to an amount of LP tokens.
///
/// * **pools** is the array with assets in the pool.
///
/// * **amount** is amount of LP tokens to compute a corresponding amount of assets for.
///
/// * **total_share** is the total amount of LP tokens currently minted.
pub fn get_share_in_assets(pools: &[Asset], amount: Uint128, total_share: Uint128) -> Vec<Asset> {
    let mut share_ratio = Decimal::zero();
    if !total_share.is_zero() {
        share_ratio = Decimal::from_ratio(amount, total_share);
    }

    pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect()
}

/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **belief_price** is used to calculate the maximum swap spread.
///
/// * **max_spread** sets the maximum spread of the swap operation.
///
/// * **to** sets the recipient of the swap operation.
///
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

    let mut config = CONFIG.load(deps.storage)?;

    // If the asset balance is already increased, we should subtract the user deposit from the pool amount
    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?
        .into_iter()
        .map(|mut p| {
            if p.info.equal(&offer_asset.info) {
                p.amount = p.amount.checked_sub(offer_asset.amount)?;
            }
            Ok(p)
        })
        .collect::<StdResult<Vec<_>>>()?;

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

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    let offer_amount = offer_asset.amount;

    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_amount,
        fee_info.total_fee_rate,
    )?;

    // Check the max spread limit (if it was specified)
    assert_max_spread(
        belief_price,
        max_spread,
        offer_amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    let return_asset = Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };

    let receiver = to.unwrap_or_else(|| sender.clone());
    let mut messages = vec![];
    if !return_amount.is_zero() {
        messages.push(return_asset.into_msg(receiver.clone())?)
    }

    // If this pool is configured to share fees, calculate the amount to send
    // to the receiver and add the transfer message
    // The calculation works as follows: We take the share percentage first,
    // and the remainder is then split between LPs and maker
    let mut fees_commission_amount = commission_amount;
    let mut fee_share_amount = Uint128::zero();
    if let Some(fee_share) = config.fee_share.clone() {
        // Calculate the fee share amount from the full commission amount
        let share_fee_rate = Decimal::from_ratio(fee_share.bps, 10000u16);
        fee_share_amount = fees_commission_amount * share_fee_rate;

        if !fee_share_amount.is_zero() {
            // Subtract the fee share amount from the commission
            fees_commission_amount = fees_commission_amount.saturating_sub(fee_share_amount);

            // Build send message for the shared amount
            let fee_share_msg = Asset {
                info: ask_pool.info.clone(),
                amount: fee_share_amount,
            }
            .into_msg(fee_share.recipient)?;
            messages.push(fee_share_msg);
        }
    }

    // Compute the Maker fee
    let mut maker_fee_amount = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        if let Some(f) = calculate_maker_fee(
            &ask_pool.info,
            fees_commission_amount,
            fee_info.maker_fee_rate,
        ) {
            maker_fee_amount = f.amount;
            messages.push(f.into_msg(fee_address)?);
        }
    }

    if config.track_asset_balances {
        BALANCES.save(
            deps.storage,
            &offer_pool.info,
            &(offer_pool.amount + offer_amount),
            env.block.height,
        )?;
        BALANCES.save(
            deps.storage,
            &ask_pool.info,
            &(ask_pool.amount - return_amount - maker_fee_amount - fee_share_amount),
            env.block.height,
        )?;
    }

    // Accumulate prices for the assets in the pool
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) =
        accumulate_prices(env, &config, pools[0].amount, pools[1].amount)?
    {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new()
        .add_messages(
            // 1. send collateral tokens from the contract to a user
            // 2. send inactive commission fees to the Maker contract
            messages,
        )
        .add_attributes(vec![
            attr("action", "swap"),
            attr("sender", sender),
            attr("receiver", receiver),
            attr("offer_asset", offer_asset.info.to_string()),
            attr("ask_asset", ask_pool.info.to_string()),
            attr("offer_amount", offer_amount),
            attr("return_amount", return_amount),
            attr("spread_amount", spread_amount),
            attr("commission_amount", commission_amount),
            attr("maker_fee_amount", maker_fee_amount),
            attr("fee_share_amount", fee_share_amount),
        ]))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values.
pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender != factory_config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut response = Response::default();

    match from_json::<XYKPoolUpdateParams>(&params)? {
        XYKPoolUpdateParams::EnableFeeShare {
            fee_share_bps,
            fee_share_address,
        } => {
            // Enable fee sharing for this contract
            // If fee sharing is already enabled, we should be able to overwrite
            // the values currently set

            // Ensure the fee share isn't 0 and doesn't exceed the maximum allowed value
            if fee_share_bps == 0 || fee_share_bps > MAX_FEE_SHARE_BPS {
                return Err(ContractError::FeeShareOutOfBounds {});
            }

            // Set sharing config
            config.fee_share = Some(FeeShareConfig {
                bps: fee_share_bps,
                recipient: deps.api.addr_validate(&fee_share_address)?,
            });

            CONFIG.save(deps.storage, &config)?;

            response.attributes.push(attr("action", "enable_fee_share"));
            response
                .attributes
                .push(attr("fee_share_bps", fee_share_bps.to_string()));
            response
                .attributes
                .push(attr("fee_share_address", fee_share_address));
        }
        XYKPoolUpdateParams::DisableFeeShare => {
            // Disable fee sharing for this contract by setting bps and
            // address back to None
            config.fee_share = None;
            CONFIG.save(deps.storage, &config)?;
            response
                .attributes
                .push(attr("action", "disable_fee_share"));
        }
    }

    Ok(response)
}

/// Accumulate token prices for the assets in the pool.
/// Note that this function shifts **block_time** when any of the token prices is zero in order to not
/// fill an accumulator with a null price for that period.
///
/// * **x** is the balance of asset\[\0] in the pool.
///
/// * **y** is the balance of asset\[\1] in the pool.
pub fn accumulate_prices(
    env: Env,
    config: &Config,
    x: Uint128,
    y: Uint128,
) -> StdResult<Option<(Uint128, Uint128, u64)>> {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return Ok(None);
    }

    // We have to shift block_time when any price is zero in order to not fill an accumulator with a null price for that period
    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    let mut pcl0 = config.price0_cumulative_last;
    let mut pcl1 = config.price1_cumulative_last;

    if !x.is_zero() && !y.is_zero() {
        let price_precision = Uint128::from(10u128.pow(TWAP_PRECISION.into()));
        pcl0 = config.price0_cumulative_last.wrapping_add(
            time_elapsed
                .checked_mul(price_precision)?
                .multiply_ratio(y, x),
        );
        pcl1 = config.price1_cumulative_last.wrapping_add(
            time_elapsed
                .checked_mul(price_precision)?
                .multiply_ratio(x, y),
        );
    };

    Ok(Some((pcl0, pcl1, block_time)))
}

/// Calculates the amount of fees the Maker contract gets according to specified pair parameters.
/// Returns a [`None`] if the Maker fee is zero, otherwise returns a [`Asset`] struct with the specified attributes.
///
/// * **pool_info** contains information about the pool asset for which the commission will be calculated.
///
/// * **commission_amount** is the total amount of fees charged for a swap.
///
/// * **maker_commission_rate** is the percentage of fees that go to the Maker contract.
pub fn calculate_maker_fee(
    pool_info: &AssetInfo,
    commission_amount: Uint128,
    maker_commission_rate: Decimal,
) -> Option<Asset> {
    let maker_fee: Uint128 = commission_amount * maker_commission_rate;
    if maker_fee.is_zero() {
        return None;
    }

    Some(Asset {
        info: pool_info.clone(),
        amount: maker_fee,
    })
}

/// Exposes all the queries available in the contract.
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
///
/// * **QueryMsg::AssetBalanceAt { asset_info, block_height }** Returns the balance of the specified asset that was in the pool
/// just preceeding the moment of the specified block height creation.
/// * **QueryMsg::SimulateWithdraw { lp_amount }** Returns the amount of assets that could be withdrawn from the pool
/// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
/// * **QueryMsg::SimulateProvide { msg }** Simulates the liquidity provision in the pair contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_json_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => to_json_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_json_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation { offer_asset, .. } => {
            to_json_binary(&query_simulation(deps, offer_asset)?)
        }
        QueryMsg::ReverseSimulation { ask_asset, .. } => {
            to_json_binary(&query_reverse_simulation(deps, ask_asset)?)
        }
        QueryMsg::CumulativePrices {} => to_json_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::AssetBalanceAt {
            asset_info,
            block_height,
        } => to_json_binary(&query_asset_balances_at(deps, asset_info, block_height)?),
        QueryMsg::SimulateWithdraw { lp_amount } => to_json_binary(&query_share(deps, lp_amount)?),
        QueryMsg::SimulateProvide {
            assets,
            slippage_tolerance,
        } => to_json_binary(&query_simulate_provide(deps, assets, slippage_tolerance)?),
        _ => Err(StdError::generic_err("Query is not supported")),
    }
}

/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **amount** is the amount of LP tokens for which we calculate associated amounts of assets.
pub fn query_share(deps: Deps, amount: Uint128) -> StdResult<Vec<Asset>> {
    let config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(deps.querier, &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(refund_assets)
}

/// Returns information about a swap simulation in a [`SimulationResponse`] object.
///
/// * **offer_asset** is the asset to swap as well as an amount of the said asset.
pub fn query_simulation(deps: Deps, offer_asset: Asset) -> StdResult<SimulationResponse> {
    let config = CONFIG.load(deps.storage)?;

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given offer asset does not belong in the pair",
        ));
    }

    // Get fee info from the factory contract
    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr,
        config.pair_info.pair_type,
    )?;

    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        ask_pool.amount,
        offer_asset.amount,
        fee_info.total_fee_rate,
    )?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
}

/// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
///
/// * **ask_asset** is the asset to swap to as well as the desired amount of ask
/// assets to receive from the swap.
pub fn query_reverse_simulation(
    deps: Deps,
    ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let config = CONFIG.load(deps.storage)?;

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if ask_asset.info.equal(&pools[0].info) {
        ask_pool = pools[0].clone();
        offer_pool = pools[1].clone();
    } else if ask_asset.info.equal(&pools[1].info) {
        ask_pool = pools[1].clone();
        offer_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given ask asset doesn't belong to pairs",
        ));
    }

    // Get fee info from factory
    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr,
        config.pair_info.pair_type,
    )?;

    let (offer_amount, spread_amount, commission_amount) = compute_offer_amount(
        offer_pool.amount,
        ask_pool.amount,
        ask_asset.amount,
        fee_info.total_fee_rate,
    )?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
    })
}

/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
pub fn query_cumulative_prices(deps: Deps, env: Env) -> StdResult<CumulativePricesResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;

    let mut price0_cumulative_last = config.price0_cumulative_last;
    let mut price1_cumulative_last = config.price1_cumulative_last;

    if let Some((price0_cumulative_new, price1_cumulative_new, _)) =
        accumulate_prices(env, &config, assets[0].amount, assets[1].amount)?
    {
        price0_cumulative_last = price0_cumulative_new;
        price1_cumulative_last = price1_cumulative_new;
    }

    let cumulative_prices = vec![
        (
            assets[0].info.clone(),
            assets[1].info.clone(),
            price0_cumulative_last,
        ),
        (
            assets[1].info.clone(),
            assets[0].info.clone(),
            price1_cumulative_last,
        ),
    ];

    let resp = CumulativePricesResponse {
        assets,
        total_share,
        cumulative_prices,
    };

    Ok(resp)
}

/// Returns the pair contract configuration in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;

    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_json_binary(&XYKPoolConfig {
            track_asset_balances: config.track_asset_balances,
            fee_share: config.fee_share,
        })?),
        owner: factory_config.owner,
        factory_addr: config.factory_addr,
        tracker_addr: config.tracker_addr,
    })
}

/// Returns the amount of LP tokens that will be minted
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
fn query_simulate_provide(
    deps: Deps,
    assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?;

    let deposits = get_deposits_from_assets(deps, &assets, &pools)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let share = calculate_shares(&deposits, &pools, total_share, slippage_tolerance)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    Ok(share)
}

/// Returns the balance of the specified asset that was in the pool
/// just preceeding the moment of the specified block height creation.
/// It will return None (null) if the balance was not tracked up to the specified block height
pub fn query_asset_balances_at(
    deps: Deps,
    asset_info: AssetInfo,
    block_height: Uint64,
) -> StdResult<Option<Uint128>> {
    BALANCES.may_load_at_height(deps.storage, &asset_info, block_height.u64())
}

/// Returns the result of a swap.
///
/// * **offer_pool** total amount of offer assets in the pool.
///
/// * **ask_pool** total amount of ask assets in the pool.
///
/// * **offer_amount** amount of offer assets to swap.
///
/// * **commission_rate** total amount of fees charged for the swap.
pub fn compute_swap(
    offer_pool: Uint128,
    ask_pool: Uint128,
    offer_amount: Uint128,
    commission_rate: Decimal,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // offer => ask
    check_swap_parameters(vec![offer_pool, ask_pool], offer_amount)?;

    let offer_pool: Uint256 = offer_pool.into();
    let ask_pool: Uint256 = ask_pool.into();
    let offer_amount: Uint256 = offer_amount.into();
    let commission_rate = Decimal256::from(commission_rate);

    // ask_amount = (ask_pool - cp / (offer_pool + offer_amount))
    let cp: Uint256 = offer_pool * ask_pool;
    let return_amount: Uint256 = (Decimal256::from_ratio(ask_pool, 1u8)
        - Decimal256::from_ratio(cp, offer_pool + offer_amount))
        * Uint256::from(1u8);

    // Calculate spread & commission
    let spread_amount: Uint256 =
        (offer_amount * Decimal256::from_ratio(ask_pool, offer_pool)).saturating_sub(return_amount);
    let commission_amount: Uint256 = return_amount * commission_rate;

    // The commision (minus the part that goes to the Maker contract) will be absorbed by the pool
    let return_amount: Uint256 = return_amount - commission_amount;
    Ok((
        return_amount.try_into()?,
        spread_amount.try_into()?,
        commission_amount.try_into()?,
    ))
}

/// Returns an amount of offer assets for a specified amount of ask assets.
///
/// * **offer_pool** total amount of offer assets in the pool.
///
/// * **ask_pool** total amount of ask assets in the pool.
///
/// * **ask_amount** amount of ask assets to swap to.
///
/// * **commission_rate** total amount of fees charged for the swap.
pub fn compute_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
    commission_rate: Decimal,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer
    check_swap_parameters(vec![offer_pool, ask_pool], ask_amount)?;

    // offer_amount = cp / (ask_pool - ask_amount / (1 - commission_rate)) - offer_pool
    let cp = Uint256::from(offer_pool) * Uint256::from(ask_pool);
    let one_minus_commission = Decimal256::one() - Decimal256::from(commission_rate);
    let inv_one_minus_commission = Decimal256::one() / one_minus_commission;

    let offer_amount: Uint128 = cp
        .multiply_ratio(
            Uint256::from(1u8),
            Uint256::from(
                ask_pool.checked_sub(
                    (Uint256::from(ask_amount) * inv_one_minus_commission).try_into()?,
                )?,
            ),
        )
        .checked_sub(offer_pool.into())?
        .try_into()?;

    let before_commission_deduction = Uint256::from(ask_amount) * inv_one_minus_commission;
    let spread_amount = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool))
        .saturating_sub(before_commission_deduction.try_into()?);
    let commission_amount = before_commission_deduction * Decimal256::from(commission_rate);
    Ok((offer_amount, spread_amount, commission_amount.try_into()?))
}

/// Returns shares for the provided deposits.
///
/// * **deposits** is an array with asset amounts
///
/// * **pools** is an array with total amount of assets in the pool
///
/// * **total_share** is the total amount of LP tokens currently minted
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
pub fn calculate_shares(
    deposits: &[Uint128; 2],
    pools: &[Asset],
    total_share: Uint128,
    slippage_tolerance: Option<Decimal>,
) -> Result<Uint128, ContractError> {
    let share = if total_share.is_zero() {
        // Initial share = collateral amount
        let share = Uint128::new(
            (U256::from(deposits[0].u128()) * U256::from(deposits[1].u128()))
                .integer_sqrt()
                .as_u128(),
        )
        .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
        .map_err(|_| ContractError::MinimumLiquidityAmountError {})?;

        // share cannot become zero after minimum liquidity subtraction
        if share.is_zero() {
            return Err(ContractError::MinimumLiquidityAmountError {});
        }

        share
    } else {
        // Assert slippage tolerance
        assert_slippage_tolerance(slippage_tolerance, deposits, pools)?;

        // min(1, 2)
        // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_0))
        // == deposit_0 * total_share / pool_0
        // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
        // == deposit_1 * total_share / pool_1
        std::cmp::min(
            deposits[0].multiply_ratio(total_share, pools[0].amount),
            deposits[1].multiply_ratio(total_share, pools[1].amount),
        )
    };
    Ok(share)
}

/// Verify assets provided and returns deposit amounts.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **pools** is the array with assets in the pool.
pub fn get_deposits_from_assets(
    deps: Deps,
    assets: &[Asset],
    pools: &[Asset],
) -> Result<[Uint128; 2], ContractError> {
    if assets.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }
    assets[0].info.check(deps.api)?;
    assets[1].info.check(deps.api)?;

    let deposits = [
        assets
            .iter()
            .find(|a| a.info.equal(&pools[0].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
        assets
            .iter()
            .find(|a| a.info.equal(&pools[1].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
    ];

    if deposits[0].is_zero() || deposits[1].is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    Ok(deposits)
}

/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
///
/// * **belief_price** belief price used in the swap.
///
/// * **max_spread** max spread allowed so that the swap can be executed successfully.
///
/// * **offer_amount** amount of assets to swap.
///
/// * **return_amount** amount of assets to receive from the swap.
///
/// * **spread_amount** spread used in the swap.
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
        let expected_return = offer_amount
            * belief_price
                .inv()
                .ok_or_else(|| StdError::generic_err("Belief price must not be zero!"))?;
        let spread_amount = expected_return.saturating_sub(return_amount);

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

/// This is an internal function that enforces slippage tolerance for swaps.
///
/// * **slippage_tolerance** slippage tolerance to enforce.
///
/// * **deposits** array with offer and ask amounts for a swap.
///
/// * **pools** array with total amount of assets in the pool.
pub fn assert_slippage_tolerance(
    slippage_tolerance: Option<Decimal>,
    deposits: &[Uint128; 2],
    pools: &[Asset],
) -> Result<(), ContractError> {
    let default_slippage = Decimal::from_str(DEFAULT_SLIPPAGE)?;
    let max_allowed_slippage = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;

    let slippage_tolerance = slippage_tolerance.unwrap_or(default_slippage);
    if slippage_tolerance.gt(&max_allowed_slippage) {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    let slippage_tolerance: Decimal256 = Decimal256::from(slippage_tolerance);
    let one_minus_slippage_tolerance = Decimal256::one() - slippage_tolerance;
    let deposits: [Uint256; 2] = [deposits[0].into(), deposits[1].into()];
    let pools: [Uint256; 2] = [pools[0].amount.into(), pools[1].amount.into()];

    // Ensure each price does not change more than what the slippage tolerance allows
    if Decimal256::from_ratio(deposits[0], deposits[1]) * one_minus_slippage_tolerance
        > Decimal256::from_ratio(pools[0], pools[1])
        || Decimal256::from_ratio(deposits[1], deposits[0]) * one_minus_slippage_tolerance
            > Decimal256::from_ratio(pools[1], pools[0])
    {
        return Err(ContractError::MaxSlippageAssertion {});
    }

    Ok(())
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    use crate::migration;
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-pair" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" | "1.1.0" | "1.2.0" => {
                migration::add_asset_balances_tracking_flag(deps.storage)?;
            }
            "1.3.0" | "1.3.1" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default().add_attributes([
        ("previous_contract_name", contract_version.contract.as_str()),
        (
            "previous_contract_version",
            contract_version.version.as_str(),
        ),
        ("new_contract_name", CONTRACT_NAME),
        ("new_contract_version", CONTRACT_VERSION),
    ]))
}

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub fn pool_info(querier: QuerierWrapper, config: &Config) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;
    let total_share = query_native_supply(&querier, &config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

fn ensure_min_assets_to_receive(
    config: &Config,
    mut refund_assets: Vec<Asset>,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<(), ContractError> {
    if let Some(min_assets_to_receive) = min_assets_to_receive {
        if refund_assets.len() != min_assets_to_receive.len() {
            return Err(ContractError::WrongAssetLength {
                expected: refund_assets.len(),
                actual: min_assets_to_receive.len(),
            });
        }

        for asset in &min_assets_to_receive {
            if !config.pair_info.asset_infos.contains(&asset.info) {
                return Err(ContractError::AssetMismatch {});
            }
        }

        if refund_assets[0].info.ne(&min_assets_to_receive[0].info) {
            refund_assets.swap(0, 1)
        }

        if refund_assets[0].amount < min_assets_to_receive[0].amount {
            return Err(ContractError::WithdrawSlippageViolation {
                asset_name: refund_assets[0].info.to_string(),
                received: refund_assets[0].amount,
                expected: min_assets_to_receive[0].amount,
            });
        }

        if refund_assets[1].amount < min_assets_to_receive[1].amount {
            return Err(ContractError::WithdrawSlippageViolation {
                asset_name: refund_assets[1].info.to_string(),
                received: refund_assets[1].amount,
                expected: min_assets_to_receive[1].amount,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::{Decimal, Uint128};

    use crate::contract::compute_swap;

    #[test]
    fn compute_swap_does_not_panic_on_spread_calc() {
        let offer_pool = Uint128::from(u128::MAX / 2);
        let ask_pool = Uint128::from(u128::MAX / 1000000000);
        let offer_amount = Uint128::from(1000000000u128);
        let commission_rate = Decimal::permille(3);

        let (return_amount, spread_amount, commission_amount) =
            compute_swap(offer_pool, ask_pool, offer_amount, commission_rate).unwrap();
        assert_eq!(return_amount, Uint128::from(2u128));
        assert_eq!(spread_amount, Uint128::zero());
        assert_eq!(commission_amount, Uint128::zero());
    }
}
