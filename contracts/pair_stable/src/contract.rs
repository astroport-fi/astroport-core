use std::collections::HashMap;
use std::str::FromStr;
use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure_eq, from_json, to_json_binary, Addr, Binary, Coin, CosmosMsg, Decimal,
    Decimal256, Deps, DepsMut, Empty, Env, Fraction, MessageInfo, QuerierWrapper, Reply, Response,
    StdError, StdResult, SubMsg, SubMsgResponse, SubMsgResult, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};
use cw_utils::{one_coin, PaymentError};
use itertools::Itertools;

use astroport::asset::{
    addr_opt_validate, check_swap_parameters, Asset, AssetInfo, CoinsExt, Decimal256Ext,
    DecimalAsset, PairInfo, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner, LP_SUBDENOM};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::observation::{query_observation, PrecommitObservation, OBSERVATIONS_SIZE};
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, FeeShareConfig, InstantiateMsg, StablePoolParams,
    StablePoolUpdateParams, DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, MAX_FEE_SHARE_BPS,
    MIN_TRADE_SIZE,
};
use astroport::pair::{
    Cw20HookMsg, ExecuteMsg, PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse,
    StablePoolConfig,
};
use astroport::querier::{query_factory_config, query_fee_info, query_native_supply};
use astroport::token_factory::{tf_burn_msg, tf_create_denom_msg, MsgCreateDenomResponse};
use astroport::DecimalCheckedOps;
use astroport_circular_buffer::BufferManager;

use crate::error::ContractError;
use crate::math::{
    calc_y, compute_d, AMP_PRECISION, MAX_AMP, MAX_AMP_CHANGE, MIN_AMP_CHANGING_TIME,
};
use crate::state::{
    get_precision, store_precisions, Config, CONFIG, OBSERVATIONS, OWNERSHIP_PROPOSAL,
};
use crate::utils::{
    accumulate_prices, accumulate_swap_sizes, adjust_precision, calculate_shares,
    check_asset_infos, check_cw20_in_pool, compute_current_amp, compute_swap,
    determine_base_quote_amount, get_assets_collection, get_share_in_assets,
    mint_liquidity_token_message, select_pools, SwapResult,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-pair-stable";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Reply ID for create denom reply
const CREATE_DENOM_REPLY_ID: u64 = 1;
/// Number of assets in the pool.
const N_COINS: usize = 2;

/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    check_asset_infos(deps.api, &msg.asset_infos)?;

    if msg.asset_infos.len() != N_COINS {
        return Err(ContractError::InvalidNumberOfAssets(N_COINS));
    }

    if msg.init_params.is_none() {
        return Err(ContractError::InitParamsNotFound {});
    }

    let params: StablePoolParams = from_json(msg.init_params.unwrap())?;

    if params.amp == 0 || params.amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp {});
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;
    let greatest_precision = store_precisions(deps.branch(), &msg.asset_infos, &factory_addr)?;

    // Initializing cumulative prices
    let mut cumulative_prices = vec![];
    for from_pool in &msg.asset_infos {
        for to_pool in &msg.asset_infos {
            if !from_pool.eq(to_pool) {
                cumulative_prices.push((from_pool.clone(), to_pool.clone(), Uint128::zero()))
            }
        }
    }

    let config = Config {
        owner: addr_opt_validate(deps.api, &params.owner)?,
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: "".to_owned(),
            asset_infos: msg.asset_infos.clone(),
            pair_type: msg.pair_type,
        },
        factory_addr,
        block_time_last: 0,
        init_amp: params.amp * AMP_PRECISION,
        init_amp_time: env.block.time.seconds(),
        next_amp: params.amp * AMP_PRECISION,
        next_amp_time: env.block.time.seconds(),
        greatest_precision,
        cumulative_prices,
        fee_share: None,
        tracker_addr: None,
    };

    CONFIG.save(deps.storage, &config)?;
    BufferManager::init(deps.storage, OBSERVATIONS, OBSERVATIONS_SIZE)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        tf_create_denom_msg(env.contract.address.to_string(), LP_SUBDENOM),
        CREATE_DENOM_REPLY_ID,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: CREATE_DENOM_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let MsgCreateDenomResponse { new_token_denom } = data.try_into()?;

            CONFIG.update(deps.storage, |mut config| {
                if !config.pair_info.liquidity_token.is_empty() {
                    return Err(StdError::generic_err(
                        "Liquidity token is already set in the config",
                    ));
                }
                config.pair_info.liquidity_token = new_token_denom.clone();
                Ok(config)
            })?;

            Ok(Response::new().add_attribute("lp_denom", new_token_denom))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Updates the contract configuration with the specified
/// input parameters.
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
///         }** Provides liquidity in the pair using the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs an swap using the specified parameters.
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
        ExecuteMsg::UpdateConfig { params } => update_config(deps, env, info, params),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets,
            auto_stake,
            receiver,
            min_lp_to_receive,
            ..
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            auto_stake,
            receiver,
            min_lp_to_receive,
        ),
        ExecuteMsg::Swap {
            offer_asset,
            ask_asset_info,
            belief_price,
            max_spread,
            to,
            ..
        } => {
            offer_asset.info.check(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Cw20DirectSwap {});
            }
            offer_asset.assert_sent_native_token_balance(&info)?;

            let to_addr = addr_opt_validate(deps.api, &to)?;

            swap(
                deps,
                env,
                info.sender,
                offer_asset,
                ask_asset_info,
                belief_price,
                max_spread,
                to_addr,
            )
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let cfg = CONFIG.load(deps.storage)?;
            let factory_config = query_factory_config(&deps.querier, cfg.factory_addr.clone())?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                cfg.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let cfg = CONFIG.load(deps.storage)?;
            let factory_config = query_factory_config(&deps.querier, cfg.factory_addr.clone())?;

            drop_ownership_proposal(
                deps,
                info,
                cfg.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut config| {
                    config.owner = Some(new_owner);
                    Ok(config)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
        ExecuteMsg::WithdrawLiquidity {
            assets,
            min_assets_to_receive,
        } => withdraw_liquidity(deps, env, info, assets, min_assets_to_receive),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is the CW20 receive message to process.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::Swap {
            ask_asset_info,
            belief_price,
            max_spread,
            to,
        } => {
            let config = CONFIG.load(deps.storage)?;

            // Only asset contract can execute this message
            check_cw20_in_pool(&config, &info.sender)?;

            let to_addr = addr_opt_validate(deps.api, &to)?;
            swap(
                deps,
                env,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: info.sender,
                    },
                    amount: cw20_msg.amount,
                },
                ask_asset_info,
                belief_price,
                max_spread,
                to_addr,
            )
        }
    }
}

/// Provides liquidity with the specified input parameters.
///
/// * **assets** vector with assets available in the pool.
///
/// * **auto_stake** determines whether the resulting LP tokens are automatically staked in
/// the Incentives contract to receive token incentives.
///
/// * **receiver** address that receives LP tokens. If this address isn't specified, the function will default to the caller.
///
/// * **min_lp_to_receive** is an optional parameter which specifies the minimum amount of LP tokens to receive.
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
    min_lp_to_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let pools = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut assets_collection =
        get_assets_collection(deps.as_ref(), &config, &pools, assets.clone())?;

    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let mut messages = vec![];

    for (deposit, pool) in assets_collection.iter_mut() {
        // We cannot put a zero amount into an empty pool.
        if deposit.amount.is_zero() && pool.is_zero() {
            return Err(ContractError::InvalidProvideLPsWithSingleToken {});
        }

        // Transfer only non-zero amount
        if !deposit.amount.is_zero() {
            // If the pool is a token contract, then we need to execute a TransferFrom msg to receive funds
            if let AssetInfo::Token { contract_addr } = &deposit.info {
                messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_json_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: deposit.amount,
                    })?,
                    funds: vec![],
                }))
            } else {
                // If the asset is a native token, the pool balance already increased
                // To calculate the pool balance properly, we should subtract the user deposit from the recorded pool token amount
                *pool = pool.checked_sub(deposit.amount)?;
            }
        }
    }

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;

    let auto_stake = auto_stake.unwrap_or(false);

    let share = calculate_shares(deps.as_ref(), &env, &config, total_share, assets_collection)?;

    if total_share.is_zero() {
        messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);
    }

    let min_amount_lp = min_lp_to_receive.unwrap_or(Uint128::zero());

    if share < min_amount_lp {
        return Err(ContractError::ProvideSlippageViolation(
            share,
            min_amount_lp,
        ));
    }

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        share,
        auto_stake,
    )?);

    let pools = pools
        .into_iter()
        .map(|(info, amount)| {
            let precision = get_precision(deps.storage, &info)?;
            Ok(DecimalAsset {
                info,
                amount: Decimal256::with_precision(amount, precision)?,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;

    if accumulate_prices(deps.storage, &env, &mut config, &pools)? {
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", assets.iter().join(", ")),
        attr("share", share),
    ]))
}

/// Withdraw liquidity from the pool.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

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

    let mut messages = refund_assets
        .clone()
        .into_iter()
        .map(|asset| asset.into_msg(&info.sender))
        .collect::<StdResult<Vec<_>>>()?;
    messages.push(tf_burn_msg(
        env.contract.address.to_string(),
        coin(amount.u128(), config.pair_info.liquidity_token.to_string()),
    ));

    let pools = pools
        .iter()
        .map(|pool| {
            let precision = get_precision(deps.storage, &pool.info)?;
            pool.to_decimal_asset(precision)
        })
        .collect::<StdResult<Vec<DecimalAsset>>>()?;

    if accumulate_prices(deps.storage, &env, &mut config, &pools)? {
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", info.sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}

/// Performs an swap operation with the specified parameters.
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
    sender: Addr,
    offer_asset: Asset,
    ask_asset_info: Option<AssetInfo>,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // If the asset balance already increased
    // We should subtract the user deposit from the pool offer asset amount
    let pools = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|mut pool| {
            if pool.info.equal(&offer_asset.info) {
                pool.amount = pool.amount.checked_sub(offer_asset.amount)?;
            }
            let token_precision = get_precision(deps.storage, &pool.info)?;
            Ok(DecimalAsset {
                info: pool.info,
                amount: Decimal256::with_precision(pool.amount, token_precision)?,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;

    let (offer_pool, ask_pool) =
        select_pools(Some(&offer_asset.info), ask_asset_info.as_ref(), &pools)?;

    let offer_precision = get_precision(deps.storage, &offer_pool.info)?;

    // Check if the liquidity is non-zero
    check_swap_parameters(
        pools
            .iter()
            .map(|pool| {
                pool.amount
                    .to_uint128_with_precision(get_precision(deps.storage, &pool.info)?)
            })
            .collect::<StdResult<Vec<Uint128>>>()?,
        offer_asset.amount,
    )?;

    let offer_asset_dec = offer_asset.to_decimal_asset(offer_precision)?;

    let SwapResult {
        return_amount,
        spread_amount,
    } = compute_swap(
        deps.storage,
        &env,
        &config,
        &offer_asset_dec,
        &offer_pool,
        &ask_pool,
        &pools,
    )?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let commission_amount = fee_info.total_fee_rate.checked_mul_uint128(return_amount)?;
    let return_amount = return_amount.saturating_sub(commission_amount);

    // Check the max spread limit (if it was specified)
    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset.amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    let receiver = to.unwrap_or_else(|| sender.clone());

    let return_asset = Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };

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
    if let Some(ref fee_share) = config.fee_share {
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
            .into_msg(&fee_share.recipient)?;
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

    if accumulate_prices(deps.storage, &env, &mut config, &pools)? {
        CONFIG.save(deps.storage, &config)?;
    }

    // Store observation from precommit data
    accumulate_swap_sizes(deps.storage, &env)?;

    // Store time series data in precommit observation.
    // Skipping small unsafe values which can seriously mess oracle price due to rounding errors.
    // This data will be reflected in observations on the next action.
    let ask_precision = get_precision(deps.storage, &ask_pool.info)?;
    if offer_asset_dec.amount >= MIN_TRADE_SIZE
        && return_amount.to_decimal256(ask_precision)? >= MIN_TRADE_SIZE
    {
        // Store time series data
        let (base_amount, quote_amount) =
            determine_base_quote_amount(&pools, &offer_asset, return_amount)?;
        PrecommitObservation::save(deps.storage, &env, base_amount, quote_amount)?;
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
            attr("offer_amount", offer_asset.amount),
            attr("return_amount", return_amount),
            attr("spread_amount", spread_amount),
            attr("commission_amount", commission_amount),
            attr("maker_fee_amount", maker_fee_amount),
            attr("fee_share_amount", fee_share_amount),
        ]))
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
/// * **QueryMsg::ReverseSimulation { ask_asset }** Returns the result of a reverse swap simulation using
/// a [`ReverseSimulationResponse`] object.
///
/// * **QueryMsg::CumulativePrices {}** Returns information about cumulative prices for the assets in the
/// pool using a [`CumulativePricesResponse`] object.
///
/// * **QueryMsg::Config {}** Returns the configuration for the pair contract using a [`ConfigResponse`] object.
/// * **QueryMsg::SimulateWithdraw { lp_amount }** Returns the amount of assets that could be withdrawn from the pool
/// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
/// * **QueryMsg::SimulateProvide { msg }** Simulates the liquidity provision in the pair contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_json_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => to_json_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_json_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation {
            offer_asset,
            ask_asset_info,
        } => to_json_binary(&query_simulation(deps, env, offer_asset, ask_asset_info)?),
        QueryMsg::ReverseSimulation {
            offer_asset_info,
            ask_asset,
        } => to_json_binary(&query_reverse_simulation(
            deps,
            env,
            ask_asset,
            offer_asset_info,
        )?),
        QueryMsg::CumulativePrices {} => to_json_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Observe { seconds_ago } => {
            to_json_binary(&query_observation(deps, env, OBSERVATIONS, seconds_ago)?)
        }
        QueryMsg::Config {} => to_json_binary(&query_config(deps, env)?),
        QueryMsg::SimulateWithdraw { lp_amount } => to_json_binary(&query_share(deps, lp_amount)?),
        QueryMsg::SimulateProvide { assets, .. } => to_json_binary(
            &query_simulate_provide(deps, env, assets)
                .map_err(|e| StdError::generic_err(e.to_string()))?,
        ),
        QueryMsg::QueryComputeD {} => to_json_binary(&query_compute_d(deps, env)?),
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
pub fn query_simulation(
    deps: Deps,
    env: Env,
    offer_asset: Asset,
    ask_asset_info: Option<AssetInfo>,
) -> StdResult<SimulationResponse> {
    let config = CONFIG.load(deps.storage)?;
    let pools = config.pair_info.query_pools_decimal(
        &deps.querier,
        &config.pair_info.contract_addr,
        &config.factory_addr,
    )?;

    let (offer_pool, ask_pool) =
        select_pools(Some(&offer_asset.info), ask_asset_info.as_ref(), &pools)
            .map_err(|err| StdError::generic_err(format!("{err}")))?;

    let offer_precision = get_precision(deps.storage, &offer_pool.info)?;

    if check_swap_parameters(
        pools
            .iter()
            .map(|pool| {
                pool.amount
                    .to_uint128_with_precision(get_precision(deps.storage, &pool.info)?)
            })
            .collect::<StdResult<Vec<Uint128>>>()?,
        offer_asset.amount,
    )
    .is_err()
    {
        return Ok(SimulationResponse {
            return_amount: Uint128::zero(),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        });
    }

    let SwapResult {
        return_amount,
        spread_amount,
    } = compute_swap(
        deps.storage,
        &env,
        &config,
        &offer_asset.to_decimal_asset(offer_precision)?,
        &offer_pool,
        &ask_pool,
        &pools,
    )
    .map_err(|err| StdError::generic_err(format!("{err}")))?;

    // Get fee info from factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    let commission_amount = fee_info.total_fee_rate.checked_mul_uint128(return_amount)?;
    let return_amount = return_amount.saturating_sub(commission_amount);

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
///
/// * **offer_asset_info** is optional field which specifies the asset to swap from.
/// May be omitted only in case the pool length is 2.
pub fn query_reverse_simulation(
    deps: Deps,
    env: Env,
    ask_asset: Asset,
    offer_asset_info: Option<AssetInfo>,
) -> StdResult<ReverseSimulationResponse> {
    let config = CONFIG.load(deps.storage)?;
    let pools = config.pair_info.query_pools_decimal(
        &deps.querier,
        &config.pair_info.contract_addr,
        &config.factory_addr,
    )?;
    let (offer_pool, ask_pool) =
        select_pools(offer_asset_info.as_ref(), Some(&ask_asset.info), &pools)
            .map_err(|err| StdError::generic_err(format!("{err}")))?;

    let offer_precision = get_precision(deps.storage, &offer_pool.info)?;
    let ask_precision = get_precision(deps.storage, &ask_asset.info)?;

    // Check the swap parameters are valid
    if check_swap_parameters(
        pools
            .iter()
            .map(|pool| {
                pool.amount
                    .to_uint128_with_precision(get_precision(deps.storage, &pool.info)?)
            })
            .collect::<StdResult<Vec<Uint128>>>()?,
        ask_asset.amount,
    )
    .is_err()
    {
        return Ok(ReverseSimulationResponse {
            offer_amount: Uint128::zero(),
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        });
    }

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let before_commission = (Decimal256::one()
        - Decimal256::new(fee_info.total_fee_rate.atomics().into()))
    .inv()
    .ok_or_else(|| StdError::generic_err("The pool must have less than 100% fee!"))?
    .checked_mul(Decimal256::with_precision(ask_asset.amount, ask_precision)?)?;

    let xp = pools.into_iter().map(|pool| pool.amount).collect_vec();
    let new_offer_pool_amount = calc_y(
        compute_current_amp(&config, &env)?,
        ask_pool.amount - before_commission,
        &xp,
        config.greatest_precision,
    )?;

    let offer_amount = new_offer_pool_amount.checked_sub(
        offer_pool
            .amount
            .to_uint128_with_precision(config.greatest_precision)?,
    )?;
    let offer_amount = adjust_precision(offer_amount, config.greatest_precision, offer_precision)?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount: offer_amount
            .saturating_sub(before_commission.to_uint128_with_precision(offer_precision)?),
        commission_amount: fee_info
            .total_fee_rate
            .checked_mul_uint128(before_commission.to_uint128_with_precision(ask_precision)?)?,
    })
}

/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
pub fn query_cumulative_prices(deps: Deps, env: Env) -> StdResult<CumulativePricesResponse> {
    let mut config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;
    let decimal_assets = assets
        .iter()
        .cloned()
        .map(|asset| {
            let precision = get_precision(deps.storage, &asset.info)?;
            asset.to_decimal_asset(precision)
        })
        .collect::<StdResult<Vec<DecimalAsset>>>()?;

    accumulate_prices(deps.storage, &env, &mut config, &decimal_assets)
        .map_err(|err| StdError::generic_err(format!("{err}")))?;

    Ok(CumulativePricesResponse {
        assets,
        total_share,
        cumulative_prices: config.cumulative_prices,
    })
}

/// Returns the pair contract configuration in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps, env: Env) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;
    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_json_binary(&StablePoolConfig {
            amp: Decimal::from_ratio(compute_current_amp(&config, &env)?, AMP_PRECISION),
            fee_share: config.fee_share,
        })?),
        owner: config.owner.unwrap_or(factory_config.owner),
        factory_addr: config.factory_addr,
        tracker_addr: config.tracker_addr,
    })
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
            * belief_price.inv().ok_or_else(|| {
                ContractError::Std(StdError::generic_err(
                    "Invalid belief_price. Check the input values.",
                ))
            })?;

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

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    unimplemented!("No safe path available for migration from cw20 to tokenfactory LP tokens")
}

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub fn pool_info(querier: QuerierWrapper, config: &Config) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;
    let total_share = query_native_supply(&querier, &config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values.
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender
        != if let Some(ref owner) = config.owner {
            owner.to_owned()
        } else {
            factory_config.owner
        }
    {
        return Err(ContractError::Unauthorized {});
    }

    let mut response = Response::default();

    match from_json::<StablePoolUpdateParams>(&params)? {
        StablePoolUpdateParams::StartChangingAmp {
            next_amp,
            next_amp_time,
        } => start_changing_amp(config, deps, env, next_amp, next_amp_time)?,
        StablePoolUpdateParams::StopChangingAmp {} => stop_changing_amp(config, deps, env)?,
        StablePoolUpdateParams::EnableFeeShare {
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
        StablePoolUpdateParams::DisableFeeShare => {
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

/// Start changing the AMP value.
///
/// * **next_amp** new value for AMP.
///
/// * **next_amp_time** end time when the pool amplification will be equal to `next_amp`.
fn start_changing_amp(
    mut config: Config,
    deps: DepsMut,
    env: Env,
    next_amp: u64,
    next_amp_time: u64,
) -> Result<(), ContractError> {
    if next_amp == 0 || next_amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp {});
    }

    let current_amp = compute_current_amp(&config, &env)?.u64();

    let next_amp_with_precision = next_amp * AMP_PRECISION;

    if next_amp_with_precision * MAX_AMP_CHANGE < current_amp
        || next_amp_with_precision > current_amp * MAX_AMP_CHANGE
    {
        return Err(ContractError::MaxAmpChangeAssertion {});
    }

    let block_time = env.block.time.seconds();

    if block_time < config.init_amp_time + MIN_AMP_CHANGING_TIME
        || next_amp_time < block_time + MIN_AMP_CHANGING_TIME
    {
        return Err(ContractError::MinAmpChangingTimeAssertion {});
    }

    config.init_amp = current_amp;
    config.next_amp = next_amp_with_precision;
    config.init_amp_time = block_time;
    config.next_amp_time = next_amp_time;

    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

/// Stop changing the AMP value.
fn stop_changing_amp(mut config: Config, deps: DepsMut, env: Env) -> StdResult<()> {
    let current_amp = compute_current_amp(&config, &env)?;
    let block_time = env.block.time.seconds();

    config.init_amp = current_amp.u64();
    config.next_amp = current_amp.u64();
    config.init_amp_time = block_time;
    config.next_amp_time = block_time;

    // now (block_time < next_amp_time) is always False, so we return the saved AMP
    CONFIG.save(deps.storage, &config)?;

    Ok(())
}
/// Compute the current pool D value.
fn query_compute_d(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let amp = compute_current_amp(&config, &env)?;
    let pools = config
        .pair_info
        .query_pools_decimal(&deps.querier, env.contract.address, &config.factory_addr)?
        .into_iter()
        .map(|pool| pool.amount)
        .collect::<Vec<_>>();

    compute_d(amp, &pools)
        .map_err(|_| StdError::generic_err("Failed to calculate the D"))?
        .to_uint128_with_precision(config.greatest_precision)
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

fn query_simulate_provide(
    deps: Deps,
    env: Env,
    assets: Vec<Asset>,
) -> Result<Uint128, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &config.pair_info.contract_addr)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let assets_collection = get_assets_collection(deps, &config, &pools, assets)?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let share = calculate_shares(deps, &env, &config, total_share, assets_collection)?;

    Ok(share)
}
