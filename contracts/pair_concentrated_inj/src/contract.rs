use std::vec;

use cosmwasm_std::{
    attr, entry_point, from_binary, wasm_execute, wasm_instantiate, Addr, Binary, CustomMsg,
    Decimal, Decimal256, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg,
    SubMsgResponse, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use cw_utils::parse_instantiate_response_data;
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQuerier, InjectiveQueryWrapper};
use itertools::Itertools;

use crate::consts::OBSERVATIONS_SIZE;
use astroport::asset::{
    addr_opt_validate, format_lp_token_name, Asset, AssetInfo, AssetInfoExt, CoinsExt,
    Decimal256Ext, PairInfo, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::cosmwasm_ext::{AbsDiff, DecimalToInteger, IntegerToDecimal};
use astroport::factory::PairType;
use astroport::pair::{Cw20HookMsg, InstantiateMsg};
use astroport::pair_concentrated::UpdatePoolParams;
use astroport::pair_concentrated_inj::{
    ConcentratedInjObParams, ConcentratedObPoolUpdateParams, ExecuteMsg,
};
use astroport::querier::{query_factory_config, query_fee_info, query_supply};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport_circular_buffer::BufferManager;

use crate::error::ContractError;
use crate::math::{calc_d, get_xcp};
use crate::orderbook::state::OrderbookState;
use crate::orderbook::utils::{
    get_subaccount_balances, is_contract_active, leave_orderbook, process_cumulative_trade,
};
use crate::state::{
    store_precisions, AmpGamma, Config, PoolParams, PoolState, Precisions, PriceState, CONFIG,
    OBSERVATIONS, OWNERSHIP_PROPOSAL,
};
use crate::utils::{
    accumulate_swap_sizes, assert_max_spread, assert_slippage_tolerance, before_swap_check,
    calc_last_prices, calc_provide_fee, check_asset_infos, check_assets, check_pair_registered,
    compute_swap, get_share_in_assets, mint_liquidity_token_message, query_contract_balances,
    query_pools,
};

/// Contract name that is used for migration.
pub(crate) const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;
/// An LP token's precision.
pub(crate) const LP_TOKEN_PRECISION: u8 = 6;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    check_asset_infos(&msg.asset_infos)?;

    if msg.asset_infos.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }

    let orderbook_params: ConcentratedInjObParams = from_binary(
        &msg.init_params
            .ok_or(ContractError::InitParamsNotFound {})?,
    )?;

    let params = &orderbook_params.main_params;

    if params.price_scale.is_zero() {
        return Err(StdError::generic_err("Initial price scale can not be zero").into());
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;

    store_precisions(deps.branch(), &msg.asset_infos, &factory_addr)?;

    let ob_state = OrderbookState::new(
        deps.querier,
        &env,
        &orderbook_params.orderbook_config.market_id,
        orderbook_params.orderbook_config.orders_number,
        orderbook_params.orderbook_config.min_trades_to_avg,
        &msg.asset_infos,
    )?;
    ob_state.save(deps.storage)?;

    BufferManager::init(deps.storage, OBSERVATIONS, OBSERVATIONS_SIZE)?;

    let mut pool_params = PoolParams::default();
    pool_params.update_params(UpdatePoolParams {
        mid_fee: Some(params.mid_fee),
        out_fee: Some(params.out_fee),
        fee_gamma: Some(params.fee_gamma),
        repeg_profit_threshold: Some(params.repeg_profit_threshold),
        min_price_scale_delta: Some(params.min_price_scale_delta),
        ma_half_time: Some(params.ma_half_time),
    })?;

    let pool_state = PoolState {
        initial: AmpGamma::default(),
        future: AmpGamma::new(params.amp, params.gamma)?,
        future_time: env.block.time.seconds(),
        initial_time: 0,
        price_state: PriceState {
            oracle_price: params.price_scale.into(),
            last_price: params.price_scale.into(),
            price_scale: params.price_scale.into(),
            last_price_update: env.block.time.seconds(),
            xcp_profit: Decimal256::zero(),
            xcp: Decimal256::zero(),
        },
    };

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Custom("concentrated_inj_orderbook".to_string()),
        },
        factory_addr,
        block_time_last: env.block.time.seconds(),
        pool_params,
        pool_state,
        owner: None,
    };

    CONFIG.save(deps.storage, &config)?;

    let token_name = format_lp_token_name(&msg.asset_infos, &deps.querier)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        wasm_instantiate(
            msg.token_code_id,
            &TokenInstantiateMsg {
                name: token_name,
                symbol: "uLP".to_string(),
                decimals: LP_TOKEN_PRECISION,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            vec![],
            String::from("Astroport LP token"),
        )?,
        INSTANTIATE_TOKEN_REPLY_ID,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(
    deps: DepsMut<InjectiveQueryWrapper>,
    _env: Env,
    msg: Reply,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match msg {
        Reply {
            id: INSTANTIATE_TOKEN_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let mut config = CONFIG.load(deps.storage)?;

            if config.pair_info.liquidity_token != Addr::unchecked("") {
                return Err(ContractError::Unauthorized {});
            }

            let init_response = parse_instantiate_response_data(data.as_slice())
                .map_err(|e| StdError::generic_err(format!("{e}")))?;
            config.pair_info.liquidity_token =
                deps.api.addr_validate(&init_response.contract_address)?;
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::new()
                .add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Updates contract parameters.
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
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
        ),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
            ..
        } => {
            offer_asset.info.check(deps.api)?;
            if !config.pair_info.asset_infos.contains(&offer_asset.info) {
                return Err(ContractError::InvalidAsset(offer_asset.info.to_string()));
            }
            offer_asset.assert_sent_native_token_balance(&info)?;

            let to_addr = addr_opt_validate(deps.api, &to)?;

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
        ExecuteMsg::UpdateConfig { params } => update_config(deps, env, info, params),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let factory_config = query_factory_config(&deps.querier, config.factory_addr)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let factory_config = query_factory_config(&deps.querier, config.factory_addr)?;

            drop_ownership_proposal(
                deps,
                info,
                config.owner.unwrap_or(factory_config.owner),
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut config| {
                    config.owner = Some(new_owner);
                    Ok(config)
                })?;

                Ok(())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::WithdrawFromOrderbook {} => orderbook_emergency_withdraw(deps, env),
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 receive message to process.
fn receive_cw20(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::WithdrawLiquidity { assets } => {
            let sender = deps.api.addr_validate(&cw20_msg.sender)?;
            withdraw_liquidity(deps, env, info, sender, cw20_msg.amount, assets)
        }
        _ => Err(ContractError::NotSupported {}),
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
/// liquidity provision are automatically staked in the Generator contract on behalf of the LP token receiver.
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
///
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
pub fn provide_liquidity<T>(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    mut assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
) -> Result<Response<T>, ContractError>
where
    T: CustomMsg,
{
    let mut config = CONFIG.load(deps.storage)?;

    if !check_pair_registered(
        deps.querier,
        &config.factory_addr,
        &config.pair_info.asset_infos,
    )? {
        return Err(ContractError::PairIsNotRegistered {});
    }

    match assets.len() {
        0 => {
            return Err(StdError::generic_err("Nothing to provide").into());
        }
        1 => {
            // Append omitted asset with explicit zero amount
            let (given_ind, _) = config
                .pair_info
                .asset_infos
                .iter()
                .find_position(|pool| pool.equal(&assets[0].info))
                .ok_or_else(|| ContractError::InvalidAsset(assets[0].info.to_string()))?;
            assets.push(Asset {
                info: config.pair_info.asset_infos[1 ^ given_ind].clone(),
                amount: Uint128::zero(),
            });
        }
        2 => {}
        _ => {
            return Err(ContractError::InvalidNumberOfAssets(
                config.pair_info.asset_infos.len(),
            ))
        }
    }

    check_assets(&assets)?;

    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let mut pools = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_state,
        &precisions,
        None,
    )?;

    if pools[0].info.equal(&assets[1].info) {
        assets.swap(0, 1);
    }

    // precisions.get_precision() also validates that the asset belongs to the pool
    let deposits = [
        Decimal256::with_precision(assets[0].amount, precisions.get_precision(&assets[0].info)?)?,
        Decimal256::with_precision(assets[1].amount, precisions.get_precision(&assets[1].info)?)?,
    ];

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    // Initial provide can not be one-sided
    if total_share.is_zero() && (deposits[0].is_zero() || deposits[1].is_zero()) {
        return Err(ContractError::InvalidZeroAmount {});
    }

    for (i, pool) in pools.iter_mut().enumerate() {
        match &pool.info {
            AssetInfo::Token { .. } => unreachable!("CW20 tokens are prohibited"),
            AssetInfo::NativeToken { .. } => {
                // If the asset is native token, the pool balance is already increased
                // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
                pool.amount = pool.amount.checked_sub(deposits[i])?;
            }
        }
    }

    let mut xs = pools.iter().map(|asset| asset.amount).collect_vec();

    let mut messages = vec![];
    let subacc_balances = get_subaccount_balances(
        &config.pair_info.asset_infos,
        &InjectiveQuerier::new(&deps.querier),
        &ob_state.subaccount,
    )?;
    // In case begin blocker logic wasn't executed, we need to update price and send maker fees
    if ob_state.last_balances != subacc_balances {
        let base_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[0])?;
        let quote_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[1])?;
        let maker_fee_message = process_cumulative_trade(
            deps.querier,
            &env,
            &ob_state,
            &mut config,
            &mut xs,
            &subacc_balances,
            base_asset_precision,
            quote_asset_precision,
        )
        .map_err(StdError::from)?;

        ob_state.last_balances = subacc_balances;

        messages.extend(maker_fee_message);
    }

    let mut new_xp = xs
        .iter()
        .enumerate()
        .map(|(ind, pool)| pool + deposits[ind])
        .collect_vec();
    new_xp[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let new_d = calc_d(&new_xp, &amp_gamma)?;
    let xcp = get_xcp(new_d, config.pool_state.price_state.price_scale);
    let (mut old_price, _) = (
        config.pool_state.price_state.last_price,
        config.pool_state.price_state.last_price,
    );

    let share = if total_share.is_zero() {
        let mint_amount = xcp
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT.to_decimal256(LP_TOKEN_PRECISION)?)
            .map_err(|_| ContractError::MinimumLiquidityAmountError {})?;

        messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);

        // share cannot become zero after minimum liquidity subtraction
        if mint_amount.is_zero() {
            return Err(ContractError::MinimumLiquidityAmountError {});
        }

        config.pool_state.price_state.xcp_profit = Decimal256::one();

        mint_amount
    } else {
        let mut old_xp = xs.clone();
        old_price = calc_last_prices(&old_xp, &config, &env)?.0;
        old_xp[1] *= config.pool_state.price_state.price_scale;
        let old_d = calc_d(&old_xp, &amp_gamma)?;
        let share = (total_share * new_d / old_d).saturating_sub(total_share);

        let mut ideposits = deposits;
        ideposits[1] *= config.pool_state.price_state.price_scale;

        share * (Decimal256::one() - calc_provide_fee(&ideposits, &new_xp, &config.pool_params))
    };

    // calculate accrued share
    let share_ratio = share / (total_share + share);
    let balanced_share = vec![
        new_xp[0] * share_ratio,
        new_xp[1] * share_ratio / config.pool_state.price_state.price_scale,
    ];
    let assets_diff = vec![
        deposits[0].diff(balanced_share[0]),
        deposits[1].diff(balanced_share[1]),
    ];

    let tmp_xp = vec![
        new_xp[0],
        new_xp[1] / config.pool_state.price_state.price_scale,
    ];
    let (new_price, _) = calc_last_prices(&tmp_xp, &config, &env)?;

    // if assets_diff[1] is zero then deposits are balanced thus no need to update price
    if !assets_diff[1].is_zero() {
        let last_price = assets_diff[0] / assets_diff[1];

        assert_slippage_tolerance(old_price, new_price, slippage_tolerance)?;

        config.pool_state.update_price(
            &config.pool_params,
            &env,
            total_share + share,
            &new_xp,
            last_price,
        )?;
    }

    let share_uint128 = share.to_uint(LP_TOKEN_PRECISION)?;

    config.pool_state.price_state.xcp = xcp;

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    let auto_stake = auto_stake.unwrap_or(false);
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        share_uint128,
        auto_stake,
    )?);

    ob_state.reconcile(deps.storage)?;
    CONFIG.save(deps.storage, &config)?;

    let attrs = vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", format!("{}, {}", &assets[0], &assets[1])),
        attr("share", share_uint128),
    ];

    Ok(Response::new().add_messages(messages).add_attributes(attrs))
}

/// Withdraw liquidity from the pool.
///
/// * **sender** address that will receive assets back from the pair contract
///
/// * **amount** amount of provided LP tokens
///
/// * **assets** defines number of coins a user wants to withdraw per each asset.
fn withdraw_liquidity(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
    assets: Vec<Asset>,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let precisions = Precisions::new(deps.storage)?;
    let ob_state = OrderbookState::load(deps.storage)?;
    let pools = query_pools(
        deps.querier,
        &config.pair_info.contract_addr,
        &config,
        &ob_state,
        &precisions,
        None,
    )?;
    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;

    let burn_amount;
    let refund_assets;
    let mut response = Response::new();
    let mut messages = vec![];

    if assets.is_empty() {
        // Usual withdraw (balanced)
        burn_amount = amount;
        refund_assets = get_share_in_assets(&pools, amount, total_share)?;
    } else {
        return Err(StdError::generic_err("Imbalanced withdraw is currently disabled").into());
    }

    let contract_balances =
        query_contract_balances(deps.querier, &env.contract.address, &config, &precisions)?;

    // If contract does not have enough liquidity - withdraw all from orderbook
    if refund_assets[0].amount > contract_balances[0].amount
        || refund_assets[1].amount > contract_balances[1].amount
    {
        let querier = InjectiveQuerier::new(&deps.querier);
        let orderbook_balances = get_subaccount_balances(
            &config.pair_info.asset_infos,
            &querier,
            &ob_state.subaccount,
        )?;
        response = leave_orderbook(&ob_state, orderbook_balances, &env).map_err(StdError::from)?;
    }

    // decrease XCP
    let mut xs = pools.into_iter().map(|a| a.amount).collect_vec();

    xs[0] -= refund_assets[0].amount;
    xs[1] -= refund_assets[1].amount;
    xs[1] *= config.pool_state.price_state.price_scale;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let d = calc_d(&xs, &amp_gamma)?;
    config.pool_state.price_state.xcp = get_xcp(d, config.pool_state.price_state.price_scale);

    let refund_assets = refund_assets
        .into_iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info).unwrap();

            Ok(Asset {
                info: asset.info,
                amount: asset.amount.to_uint(prec)?,
            })
        })
        .collect::<StdResult<Vec<_>>>()?;

    messages.extend(
        refund_assets
            .iter()
            .cloned()
            .map(|asset| asset.into_msg(&sender))
            .collect::<StdResult<Vec<_>>>()?,
    );
    messages.push(
        wasm_execute(
            &config.pair_info.liquidity_token,
            &Cw20ExecuteMsg::Burn {
                amount: burn_amount,
            },
            vec![],
        )?
        .into(),
    );

    CONFIG.save(deps.storage, &config)?;
    ob_state.reconcile(deps.storage)?;

    Ok(response.add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
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
fn swap<T>(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response<T>, ContractError>
where
    T: CustomMsg,
{
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;
    let mut config = CONFIG.load(deps.storage)?;
    let mut ob_state = OrderbookState::load(deps.storage)?;

    let mut pools = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_state,
        &precisions,
        None,
    )?;

    let (offer_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == offer_asset_dec.info)
        .ok_or_else(|| ContractError::InvalidAsset(offer_asset_dec.info.to_string()))?;
    let ask_ind = 1 ^ offer_ind;
    let ask_asset_prec = precisions.get_precision(&pools[ask_ind].info)?;

    pools[offer_ind].amount -= offer_asset_dec.amount;

    before_swap_check(&pools, offer_asset_dec.amount)?;

    let mut xs = pools.iter().map(|asset| asset.amount).collect_vec();

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let mut maker_fee_share = Decimal256::zero();
    if fee_info.fee_address.is_some() {
        maker_fee_share = fee_info.maker_fee_rate.into();
    }

    let mut messages = vec![];

    let subacc_balances = get_subaccount_balances(
        &config.pair_info.asset_infos,
        &InjectiveQuerier::new(&deps.querier),
        &ob_state.subaccount,
    )?;
    // In case begin blocker logic wasn't executed, we need to update price and send maker fees
    if ob_state.last_balances != subacc_balances {
        let base_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[0])?;
        let quote_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[1])?;
        let maker_fee_message = process_cumulative_trade(
            deps.querier,
            &env,
            &ob_state,
            &mut config,
            &mut xs,
            &subacc_balances,
            base_asset_precision,
            quote_asset_precision,
        )
        .map_err(StdError::from)?;

        ob_state.last_balances = subacc_balances;

        messages.extend(maker_fee_message);
    }

    let swap_result = compute_swap(
        &xs,
        offer_asset_dec.amount,
        ask_ind,
        &config,
        &env,
        maker_fee_share,
    )?;
    xs[offer_ind] += offer_asset_dec.amount;
    xs[ask_ind] -= swap_result.dy + swap_result.maker_fee;

    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset_dec.amount,
        swap_result.dy,
        swap_result.spread_fee,
    )?;
    let spread_amount = swap_result.spread_fee.to_uint(ask_asset_prec)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    let (last_price, _) = swap_result.calc_last_prices(offer_asset_dec.amount, offer_ind);

    // update_price() works only with internal representation
    xs[1] *= config.pool_state.price_state.price_scale;
    config
        .pool_state
        .update_price(&config.pool_params, &env, total_share, &xs, last_price)?;

    let receiver = to.unwrap_or_else(|| sender.clone());

    let return_amount = swap_result.dy.to_uint(ask_asset_prec)?;
    messages.push(
        pools[ask_ind]
            .info
            .with_balance(return_amount)
            .into_msg(&receiver)?,
    );

    let mut maker_fee = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        if !swap_result.maker_fee.is_zero() {
            maker_fee = swap_result.maker_fee.to_uint(ask_asset_prec)?;
            let fee = Asset {
                info: pools[ask_ind].info.clone(),
                amount: maker_fee,
            };
            messages.push(fee.into_msg(fee_address)?);
        }
    }

    // Store time series data
    let (base_amount, quote_amount) = if offer_ind == 0 {
        (offer_asset.amount, return_amount)
    } else {
        (return_amount, offer_asset.amount)
    };
    accumulate_swap_sizes(deps.storage, &env, &mut ob_state, base_amount, quote_amount)?;

    CONFIG.save(deps.storage, &config)?;
    ob_state.reconcile(deps.storage)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap"),
        attr("sender", sender),
        attr("receiver", receiver),
        attr("offer_asset", offer_asset_dec.info.to_string()),
        attr("ask_asset", pools[ask_ind].info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_amount),
        attr("spread_amount", spread_amount),
        attr(
            "commission_amount",
            swap_result.total_fee.to_uint(ask_asset_prec)?,
        ),
        attr("maker_fee_amount", maker_fee),
    ]))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values in [`Binary`] form.
fn update_config<T>(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response<T>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    let owner = config.owner.as_ref().unwrap_or(&factory_config.owner);
    if info.sender != *owner {
        return Err(ContractError::Unauthorized {});
    }

    let action = match from_binary::<ConcentratedObPoolUpdateParams>(&params)? {
        ConcentratedObPoolUpdateParams::Update(update_params) => {
            config.pool_params.update_params(update_params)?;
            "update_params"
        }
        ConcentratedObPoolUpdateParams::Promote(promote_params) => {
            config.pool_state.promote_params(&env, promote_params)?;
            "promote_params"
        }
        ConcentratedObPoolUpdateParams::StopChangingAmpGamma {} => {
            config.pool_state.stop_promotion(&env);
            "stop_changing_amp_gamma"
        }
        ConcentratedObPoolUpdateParams::UpdateOrderbookParams { orders_number } => {
            let mut ob_config = OrderbookState::load(deps.storage)?;
            ob_config.orders_number = orders_number;
            ob_config.save(deps.storage)?;
            "update_orderbook_params"
        }
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", action))
}

/// In case for some reason orderbook was disabled and liquidity left in the subaccount
/// this permissionless endpoint can be used to withdraw whole balance to the contract address.
pub fn orderbook_emergency_withdraw(
    deps: DepsMut<InjectiveQueryWrapper>,
    env: Env,
) -> Result<Response<InjectiveMsgWrapper>, ContractError> {
    let querier = InjectiveQuerier::new(&deps.querier);

    // Ask chain whether the pair contract is still active in begin blocker
    if is_contract_active(&querier, &env.contract.address)? {
        return Err(StdError::generic_err(
            "Failed to withdraw liquidity from orderbook: contract is active",
        )
        .into());
    }

    let ob_state = OrderbookState::load(deps.storage)?;
    let balances = get_subaccount_balances(&ob_state.asset_infos, &querier, &ob_state.subaccount)?;

    let mut response = if !(balances[0].amount + balances[1].amount).is_zero() {
        leave_orderbook(&ob_state, balances.clone(), &env).map_err(StdError::from)?
    } else {
        Response::new()
    };

    if ob_state.last_balances != balances {
        let mut config = CONFIG.load(deps.storage)?;
        let precisions = Precisions::new(deps.storage)?;
        let mut pools = query_pools(
            deps.querier,
            &env.contract.address,
            &config,
            &ob_state,
            &precisions,
            None,
        )?
        .iter()
        .map(|asset| asset.amount)
        .collect_vec();
        let base_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[0])?;
        let quote_asset_precision = precisions.get_precision(&config.pair_info.asset_infos[1])?;
        let maker_fee_message = process_cumulative_trade(
            deps.querier,
            &env,
            &ob_state,
            &mut config,
            &mut pools,
            &balances,
            base_asset_precision,
            quote_asset_precision,
        )
        .map_err(StdError::from)?;
        CONFIG.save(deps.storage, &config)?;

        response = response.add_messages(maker_fee_message);
    }

    let new_balances = vec![
        ob_state.asset_infos[0].with_balance(0u8),
        ob_state.asset_infos[1].with_balance(0u8),
    ];
    ob_state.reconciliation_done(deps.storage, new_balances)?;

    Ok(response.add_attributes(vec![
        attr("action", "emergency_withdraw"),
        attr("pair", env.contract.address),
    ]))
}
