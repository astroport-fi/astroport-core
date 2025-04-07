use cosmwasm_std::{
    to_json_binary, Binary, Decimal, Decimal256, DecimalRangeExceeded, Deps, Env, StdError,
    StdResult, Uint128,
};
use itertools::Itertools;

use astroport::asset::Asset;
use astroport::cosmwasm_ext::{DecimalToInteger, IntegerToDecimal};
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, PoolResponse, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport::pair_concentrated::{ConcentratedPoolConfig, QueryMsg};
use astroport::querier::{query_factory_config, query_fee_info, query_native_supply};
use astroport_pcl_common::state::Precisions;
use astroport_pcl_common::utils::{
    accumulate_prices, before_swap_check, calc_last_prices, compute_offer_amount, compute_swap,
    get_share_in_assets,
};
use astroport_pcl_common::{calc_d, get_xcp};

use crate::error::ContractError;
use crate::instantiate::LP_TOKEN_PRECISION;
use crate::orderbook::execute::process_cumulative_trades;
use crate::orderbook::state::OrderbookState;
use crate::orderbook::utils::Liquidity;
use crate::state::CONFIG;
use crate::utils::{calculate_shares, get_assets_with_precision, pool_info};

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
/// * **QueryMsg::AssetBalanceAt { asset_info, block_height }** Returns the balance of the specified
/// asset that was in the pool just preceding the moment of the specified block height creation.
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Pair {} => Ok(to_json_binary(&CONFIG.load(deps.storage)?.pair_info)?),
        QueryMsg::Pool {} => Ok(to_json_binary(&query_pool(deps)?)?),
        QueryMsg::Share { amount } => Ok(to_json_binary(&query_share(deps, env, amount)?)?),
        QueryMsg::Simulation { offer_asset, .. } => {
            Ok(to_json_binary(&query_simulation(deps, env, offer_asset)?)?)
        }
        QueryMsg::ReverseSimulation { ask_asset, .. } => Ok(to_json_binary(
            &query_reverse_simulation(deps, env, ask_asset)?,
        )?),
        QueryMsg::CumulativePrices {} => Ok(to_json_binary(&query_cumulative_prices(deps, env)?)?),
        QueryMsg::Observe { .. } => unimplemented!(
            "Simple moving average observations has been removed from PCL for Duality"
        ),
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps, env)?)?),
        QueryMsg::LpPrice {} => Ok(to_json_binary(&query_lp_price(deps, env)?)?),
        QueryMsg::ComputeD {} => Ok(to_json_binary(&query_compute_d(deps, env)?)?),
        QueryMsg::AssetBalanceAt { .. } => {
            unimplemented!("PCL for Duality doesn't support balances tracking")
        }
        QueryMsg::SimulateProvide {
            assets,
            slippage_tolerance,
        } => Ok(to_json_binary(&query_simulate_provide(
            deps,
            env,
            assets,
            slippage_tolerance,
        )?)?),
        QueryMsg::SimulateWithdraw { lp_amount } => {
            Ok(to_json_binary(&query_share(deps, env, lp_amount)?)?)
        }
    }
}

/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let mut ob_state = OrderbookState::load(deps.storage)?;
    pool_info(deps.querier, &config, &mut ob_state).map(|(assets, total_share)| PoolResponse {
        assets,
        total_share,
    })
}

/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **amount** is the amount of LP tokens for which we calculate associated amounts of assets.
fn query_share(deps: Deps, env: Env, amount: Uint128) -> Result<Vec<Asset>, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let refund_assets =
        get_share_in_assets(&pools, amount.saturating_sub(Uint128::one()), total_share);

    refund_assets
        .into_iter()
        .map(|asset| {
            let prec = precisions.get_precision(&asset.info)?;

            Ok(Asset {
                info: asset.info,
                amount: asset.amount.to_uint(prec)?,
            })
        })
        .collect()
}

/// Returns information about a swap simulation.
pub fn query_simulation(
    deps: Deps,
    env: Env,
    offer_asset: Asset,
) -> Result<SimulationResponse, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    let (offer_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == offer_asset.info)
        .ok_or_else(|| ContractError::InvalidAsset(offer_asset_dec.info.to_string()))?;
    let ask_ind = 1 - offer_ind;
    let ask_asset_prec = precisions.get_precision(&pools[ask_ind].info)?;

    before_swap_check(&pools, offer_asset_dec.amount)?;

    let xs = pools.iter().map(|asset| asset.amount).collect_vec();

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
    // If this pool is configured to share fees
    let mut share_fee_share = Decimal256::zero();
    if let Some(fee_share) = config.fee_share.clone() {
        share_fee_share = Decimal256::from_ratio(fee_share.bps, 10000u16);
    }

    let swap_result = compute_swap(
        &xs,
        offer_asset_dec.amount,
        ask_ind,
        &config,
        &env,
        maker_fee_share,
        share_fee_share,
    )?;

    Ok(SimulationResponse {
        return_amount: swap_result.dy.to_uint(ask_asset_prec)?,
        spread_amount: swap_result.spread_fee.to_uint(ask_asset_prec)?,
        commission_amount: swap_result.total_fee.to_uint(ask_asset_prec)?,
    })
}

/// Returns information about a reverse swap simulation.
pub fn query_reverse_simulation(
    deps: Deps,
    env: Env,
    ask_asset: Asset,
) -> Result<ReverseSimulationResponse, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let ask_asset_prec = precisions.get_precision(&ask_asset.info)?;
    let ask_asset_dec = ask_asset.to_decimal_asset(ask_asset_prec)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    let (ask_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == ask_asset.info)
        .ok_or_else(|| ContractError::InvalidAsset(ask_asset.info.to_string()))?;
    let offer_ind = 1 - ask_ind;
    let offer_asset_prec = precisions.get_precision(&pools[offer_ind].info)?;

    let xs = pools.iter().map(|asset| asset.amount).collect_vec();
    let (offer_amount, spread_amount, commission_amount) =
        compute_offer_amount(&xs, ask_asset_dec.amount, ask_ind, &config, &env)?;

    Ok(ReverseSimulationResponse {
        offer_amount: offer_amount.to_uint(offer_asset_prec)?,
        spread_amount: spread_amount.to_uint(offer_asset_prec)?,
        commission_amount: commission_amount.to_uint(offer_asset_prec)?,
    })
}

/// Returns information about cumulative prices for the assets in the pool.
fn query_cumulative_prices(
    deps: Deps,
    env: Env,
) -> Result<CumulativePricesResponse, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    if pools[0].amount.is_zero() || pools[1].amount.is_zero() {
        return Err(StdError::generic_err("Pools are empty").into());
    }

    let xs = pools.iter().map(|asset| asset.amount).collect_vec();
    let last_real_price = calc_last_prices(&xs, &config, &env)?;

    accumulate_prices(&env, &mut config, last_real_price);

    let mut ob_state = OrderbookState::load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config, &mut ob_state)?;

    Ok(CumulativePricesResponse {
        assets,
        total_share,
        cumulative_prices: config.cumulative_prices,
    })
}

/// Compute the current LP token virtual price.
pub fn query_lp_price(deps: Deps, env: Env) -> Result<Decimal256, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let total_lp = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;
    let mut ob_state = OrderbookState::load(deps.storage)?;

    if !total_lp.is_zero() {
        let precisions = Precisions::new(deps.storage)?;

        let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
        let mut pools = liquidity
            .total_dec(&precisions)
            .map_err(|e| StdError::generic_err(e.to_string()))?;

        let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
        // Process all filled orders; send maker fees; repeg PCL
        if !cumulative_trades.is_empty() {
            // This non-trivial array of mutable refs allows us to keep balances updated
            // considering sent maker and share fees
            let mut balances = pools
                .iter_mut()
                .map(|asset| &mut asset.amount)
                .collect_vec();

            process_cumulative_trades(
                deps,
                &env,
                &cumulative_trades,
                &mut config,
                &mut balances,
                &precisions,
                None,
            )?;
        }

        let mut ixs = pools.into_iter().map(|asset| asset.amount).collect_vec();
        ixs[1] *= config.pool_state.price_state.price_scale;
        let amp_gamma = config.pool_state.get_amp_gamma(&env);
        let d = calc_d(&ixs, &amp_gamma)?;
        let xcp = get_xcp(d, config.pool_state.price_state.price_scale);

        Ok(xcp / total_lp)
    } else {
        Ok(Decimal256::zero())
    }
}

/// Returns the pair contract configuration.
pub fn query_config(deps: Deps, env: Env) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let price_scale = config
        .pool_state
        .price_state
        .price_scale
        .try_into()
        .map_err(|err: DecimalRangeExceeded| StdError::generic_err(err.to_string()))?;

    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_json_binary(&ConcentratedPoolConfig {
            amp: amp_gamma.amp,
            gamma: amp_gamma.gamma,
            mid_fee: config.pool_params.mid_fee,
            out_fee: config.pool_params.out_fee,
            fee_gamma: config.pool_params.fee_gamma,
            repeg_profit_threshold: config.pool_params.repeg_profit_threshold,
            min_price_scale_delta: config.pool_params.min_price_scale_delta,
            price_scale,
            ma_half_time: config.pool_params.ma_half_time,
            track_asset_balances: config.track_asset_balances,
            fee_share: config.fee_share,
        })?),
        owner: config.owner.unwrap_or(factory_config.owner),
        factory_addr: config.factory_addr,
        tracker_addr: config.tracker_addr,
    })
}

/// Compute the current pool D value.
pub fn query_compute_d(deps: Deps, env: Env) -> Result<Decimal256, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;
    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    let mut xs = pools.into_iter().map(|a| a.amount).collect_vec();

    if xs[0].is_zero() || xs[1].is_zero() {
        return Err(StdError::generic_err("Pools are empty").into());
    }

    xs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    Ok(calc_d(&xs, &amp_gamma)?)
}

pub fn query_simulate_provide(
    deps: Deps,
    env: Env,
    mut assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
) -> Result<Uint128, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    let total_share = query_native_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;

    let precisions = Precisions::new(deps.storage)?;

    let mut ob_state = OrderbookState::load(deps.storage)?;

    let liquidity = Liquidity::new(deps.querier, &config, &mut ob_state, false)?;

    let mut pools = liquidity
        .total_dec(&precisions)
        .map_err(|e| StdError::generic_err(e.to_string()))?;

    let cumulative_trades = ob_state.fetch_cumulative_trades(&precisions)?;
    // Process all filled orders; send maker fees; repeg PCL
    if !cumulative_trades.is_empty() {
        // This non-trivial array of mutable refs allows us to keep balances updated
        // considering sent maker and share fees
        let mut balances = pools
            .iter_mut()
            .map(|asset| &mut asset.amount)
            .collect_vec();

        process_cumulative_trades(
            deps,
            &env,
            &cumulative_trades,
            &mut config,
            &mut balances,
            &precisions,
            None,
        )?;
    }

    let deposits = get_assets_with_precision(deps, &config, &mut assets, &pools, &precisions)?;

    let (share_uint128, _) = calculate_shares(
        &env,
        &mut config,
        &pools,
        total_share,
        &deposits,
        slippage_tolerance,
    )?;

    Ok(share_uint128)
}
