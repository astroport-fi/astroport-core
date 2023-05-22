use cosmwasm_std::{
    entry_point, to_binary, Binary, CustomQuery, Decimal, Decimal256, Deps, Env, StdError,
    StdResult, Storage, Uint128,
};
use injective_cosmwasm::InjectiveQueryWrapper;
use itertools::Itertools;

use astroport::asset::Asset;
use astroport::cosmwasm_ext::{AbsDiff, DecimalToInteger, IntegerToDecimal};
use astroport::pair::{
    ConfigResponse, PoolResponse, ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::pair_concentrated_inj::{OracleObservation, OrderbookStateResponse, QueryMsg};
use astroport::querier::{query_factory_config, query_fee_info, query_supply};
use astroport_circular_buffer::BufferManager;

use crate::contract::LP_TOKEN_PRECISION;
use crate::error::ContractError;
use crate::math::calc_d;
use crate::orderbook::state::OrderbookState;
use crate::state::{Observation, Precisions, CONFIG, OBSERVATIONS};
use crate::utils::{
    before_swap_check, compute_offer_amount, compute_swap, get_share_in_assets, query_pools,
};

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
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<InjectiveQueryWrapper>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => {
            to_binary(&query_pool(deps, env).map_err(|err| StdError::generic_err(err.to_string()))?)
        }
        QueryMsg::Share { amount } => to_binary(
            &query_share(deps, amount).map_err(|err| StdError::generic_err(err.to_string()))?,
        ),
        QueryMsg::Simulation { offer_asset, .. } => to_binary(
            &query_simulation(deps, env, offer_asset)
                .map_err(|err| StdError::generic_err(format!("{err}")))?,
        ),
        QueryMsg::ReverseSimulation { ask_asset, .. } => to_binary(
            &query_reverse_simulation(deps, env, ask_asset)
                .map_err(|err| StdError::generic_err(format!("{err}")))?,
        ),
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::LpPrice {} => to_binary(&query_lp_price(deps)?),
        QueryMsg::ComputeD {} => to_binary(&query_compute_d(deps, env)?),
        QueryMsg::CumulativePrices {} => Err(StdError::generic_err(
            stringify!(Not implemented. Use {"observe": {"seconds_ago": ... }} instead.),
        )),
        QueryMsg::Observe { seconds_ago } => to_binary(&query_observation(deps, env, seconds_ago)?),
        QueryMsg::OrderbookState {} => {
            let resp: OrderbookStateResponse = OrderbookState::load(deps.storage)?.into();
            to_binary(&resp)
        }
    }
}

/// Returns the amounts of assets in the pair contract and its subaccount as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
fn query_pool(deps: Deps<InjectiveQueryWrapper>, env: Env) -> Result<PoolResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let ob_state = OrderbookState::load(deps.storage)?;

    let assets = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_state,
        &precisions,
        None,
    )?
    .into_iter()
    .map(|asset| {
        let prec = precisions.get_precision(&asset.info)?;
        asset.into_asset(prec).map_err(Into::into)
    })
    .collect::<Result<Vec<_>, ContractError>>()?;
    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;

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
fn query_share(
    deps: Deps<InjectiveQueryWrapper>,
    amount: Uint128,
) -> Result<Vec<Asset>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let ob_config = OrderbookState::load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let pools = query_pools(
        deps.querier,
        &config.pair_info.contract_addr,
        &config,
        &ob_config,
        &precisions,
        None,
    )?;
    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share)?;

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

    Ok(refund_assets)
}

/// Returns information about a swap simulation.
pub fn query_simulation(
    deps: Deps<InjectiveQueryWrapper>,
    env: Env,
    offer_asset: Asset,
) -> Result<SimulationResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;
    let ob_config = OrderbookState::load(deps.storage)?;

    let pools = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_config,
        &precisions,
        None,
    )?;

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

    let swap_result = compute_swap(
        &xs,
        offer_asset_dec.amount,
        ask_ind,
        &config,
        &env,
        maker_fee_share,
    )?;

    Ok(SimulationResponse {
        return_amount: swap_result.dy.to_uint(ask_asset_prec)?,
        spread_amount: swap_result.spread_fee.to_uint(ask_asset_prec)?,
        commission_amount: swap_result.total_fee.to_uint(ask_asset_prec)?,
    })
}

/// Returns information about a reverse swap simulation.
pub fn query_reverse_simulation(
    deps: Deps<InjectiveQueryWrapper>,
    env: Env,
    ask_asset: Asset,
) -> Result<ReverseSimulationResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let ask_asset_prec = precisions.get_precision(&ask_asset.info)?;
    let ask_asset_dec = ask_asset.to_decimal_asset(ask_asset_prec)?;
    let ob_config = OrderbookState::load(deps.storage)?;

    let pools = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_config,
        &precisions,
        None,
    )?;

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

/// Compute the current LP token virtual price.
pub fn query_lp_price<C>(deps: Deps<C>) -> StdResult<Decimal256>
where
    C: CustomQuery,
{
    let config = CONFIG.load(deps.storage)?;
    let total_lp = query_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;
    let vlp_price = config
        .pool_state
        .price_state
        .xcp
        .checked_div(total_lp)
        .unwrap_or_else(|_| Decimal256::zero());
    Ok(vlp_price)
}

/// Returns the pair contract configuration.
pub fn query_config<C>(deps: Deps<C>, env: Env) -> StdResult<ConfigResponse>
where
    C: CustomQuery,
{
    let config = CONFIG.load(deps.storage)?;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let dec256_price_scale = config.pool_state.price_state.price_scale;
    let price_scale = Decimal::from_atomics(
        Uint128::try_from(dec256_price_scale.atomics())?,
        dec256_price_scale.decimal_places(),
    )
    .map_err(|e| StdError::generic_err(format!("{e}")))?;

    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;
    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_binary(&ConcentratedPoolParams {
            amp: amp_gamma.amp,
            gamma: amp_gamma.gamma,
            mid_fee: config.pool_params.mid_fee,
            out_fee: config.pool_params.out_fee,
            fee_gamma: config.pool_params.fee_gamma,
            repeg_profit_threshold: config.pool_params.repeg_profit_threshold,
            min_price_scale_delta: config.pool_params.min_price_scale_delta,
            price_scale,
            ma_half_time: config.pool_params.ma_half_time,
            track_asset_balances: None,
        })?),
        owner: config.owner.unwrap_or(factory_config.owner),
        factory_addr: config.factory_addr,
    })
}

/// Compute the current pool D value.
pub fn query_compute_d(deps: Deps<InjectiveQueryWrapper>, env: Env) -> StdResult<Decimal256> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let ob_config = OrderbookState::load(deps.storage)?;

    let mut xs = query_pools(
        deps.querier,
        &env.contract.address,
        &config,
        &ob_config,
        &precisions,
        None,
    )
    .map_err(|e| StdError::generic_err(e.to_string()))?
    .into_iter()
    .map(|a| a.amount)
    .collect_vec();

    if xs[0].is_zero() || xs[1].is_zero() {
        return Err(StdError::generic_err("Pools are empty"));
    }

    xs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    calc_d(&xs, &amp_gamma)
}

/// Performs binary search in circular buffer. Returns left and right bounds of target value.
/// Either left or right bound may hit in target value.
fn binary_search(
    storage: &dyn Storage,
    buffer: &BufferManager<Observation>,
    target: u64,
    mut start: u32,
    mut end: u32,
) -> StdResult<(Observation, Observation)> {
    loop {
        let mid = (start + end) / 2;

        // We've checked bounds before calling this function thus these errors should be impossible.
        let leftward_or_hit = buffer.read_single(storage, mid)?.ok_or_else(|| {
            StdError::generic_err(format!(
                "Unexpected error in binary_search: leftward_or_hit is None at index {mid}",
            ))
        })?;
        let rightward_or_hit = buffer.read_single(storage, mid + 1)?.ok_or_else(|| {
            StdError::generic_err(format!(
                "Unexpected error in binary_search: rightward_or_hit is None at index {}",
                mid + 1
            ))
        })?;

        if leftward_or_hit.timestamp <= target && target <= rightward_or_hit.timestamp {
            break Ok((leftward_or_hit, rightward_or_hit));
        }
        if leftward_or_hit.timestamp > target {
            end = mid - 1;
        } else {
            start = mid + 1;
        }
    }
}

/// Returns price observation at point that was 'seconds_ago' seconds ago.
pub fn query_observation<C>(
    deps: Deps<C>,
    env: Env,
    seconds_ago: u64,
) -> StdResult<OracleObservation>
where
    C: CustomQuery,
{
    let buffer = BufferManager::new(deps.storage, OBSERVATIONS)?;
    let target = env.block.time.seconds() - seconds_ago;

    let mut oldest_ind = buffer.head();
    let mut newest_ind = buffer.head() + buffer.capacity() - 1;
    if !buffer.exists(deps.storage, oldest_ind) {
        if buffer.head() > 0 {
            oldest_ind = 0;
            newest_ind %= buffer.capacity();
        } else {
            return Err(StdError::generic_err("Buffer is empty"));
        }
    }

    let newest_obs = buffer.read_single(deps.storage, newest_ind)?.unwrap();
    if target >= newest_obs.timestamp {
        return Ok(OracleObservation {
            timestamp: target,
            price: Decimal::from_ratio(newest_obs.base_amount, newest_obs.quote_amount),
        });
    }
    let oldest_obs = buffer.read_single(deps.storage, oldest_ind)?.unwrap();
    if target == oldest_obs.timestamp {
        return Ok(OracleObservation {
            timestamp: target,
            price: Decimal::from_ratio(oldest_obs.base_amount, oldest_obs.quote_amount),
        });
    }
    if target < oldest_obs.timestamp {
        return Err(StdError::generic_err(format!(
            "Requested observation is too old. Last known observation is at {}",
            oldest_obs.timestamp
        )));
    }

    let (left, right) = binary_search(deps.storage, &buffer, target, oldest_ind, newest_ind)?;

    let price_left = Decimal::from_ratio(left.base_amount, left.quote_amount);
    let price_right = Decimal::from_ratio(right.base_amount, right.quote_amount);
    let price = if left.timestamp == target {
        price_left
    } else if right.timestamp == target {
        price_right
    } else if price_left == price_right {
        price_left
    } else {
        // Interpolate.
        let price_slope = price_right.diff(price_left)
            * Decimal::from_ratio(1u8, right.timestamp - left.timestamp);
        let time_interval = Decimal::from_ratio(target - left.timestamp, 1u8);
        if price_left > price_right {
            price_left - price_slope * time_interval
        } else {
            price_left + price_slope * time_interval
        }
    };

    Ok(OracleObservation {
        timestamp: target,
        price,
    })
}

#[cfg(test)]
mod testing {
    use std::error::Error;
    use std::str::FromStr;

    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::Timestamp;

    use crate::state::Observation;

    use super::*;

    pub fn f64_to_dec<T>(val: f64) -> T
    where
        T: FromStr,
        T::Err: Error,
    {
        T::from_str(&val.to_string()).unwrap()
    }

    #[test]
    fn observations_checking_triple_capacity_step_by_step() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(100_000);
        const CAPACITY: u32 = 20;
        BufferManager::init(&mut deps.storage, OBSERVATIONS, CAPACITY).unwrap();

        let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

        let ts = env.block.time.seconds();

        let array = (1..=CAPACITY * 3)
            .into_iter()
            .map(|i| Observation {
                timestamp: ts + i as u64 * 1000,
                base_sma: Default::default(),
                base_amount: (i * i).into(),
                quote_sma: Default::default(),
                quote_amount: i.into(),
            })
            .collect_vec();

        for (k, obs) in array.iter().enumerate() {
            env.block.time = env.block.time.plus_seconds(1000);

            buffer.push(&obs);
            buffer.commit(&mut deps.storage).unwrap();
            let k1 = k as u32 + 1;

            let from = k1.saturating_sub(CAPACITY) + 1;
            let to = k1;

            for i in from..=to {
                let shift = (to - i) as u64;
                if shift != 0 {
                    assert_eq!(
                        OracleObservation {
                            timestamp: ts + i as u64 * 1000 + 500,
                            price: f64_to_dec(i as f64 + 0.5),
                        },
                        query_observation(deps.as_ref(), env.clone(), shift * 1000 - 500).unwrap()
                    );
                }
                assert_eq!(
                    OracleObservation {
                        timestamp: ts + i as u64 * 1000,
                        price: f64_to_dec(i as f64),
                    },
                    query_observation(deps.as_ref(), env.clone(), shift * 1000).unwrap()
                );
            }
        }
    }

    #[test]
    fn observations_full_buffer() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(100_000);
        BufferManager::init(&mut deps.storage, OBSERVATIONS, 20).unwrap();

        let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

        let err = query_observation(deps.as_ref(), env.clone(), 11000).unwrap_err();
        assert_eq!(err.to_string(), "Generic error: Buffer is empty");

        let array = (1..=30)
            .into_iter()
            .map(|i| Observation {
                timestamp: env.block.time.seconds() + i * 1000,
                base_sma: Default::default(),
                base_amount: i.into(),
                quote_sma: Default::default(),
                quote_amount: (i * i).into(),
            })
            .collect_vec();
        buffer.push_many(&array);
        buffer.commit(&mut deps.storage).unwrap();

        env.block.time = env.block.time.plus_seconds(30_000);

        assert_eq!(
            OracleObservation {
                timestamp: 120_000,
                price: f64_to_dec(20.0 / 400.0),
            },
            query_observation(deps.as_ref(), env.clone(), 10000).unwrap()
        );

        assert_eq!(
            OracleObservation {
                timestamp: 124_411,
                price: f64_to_dec(0.04098166666666694),
            },
            query_observation(deps.as_ref(), env.clone(), 5589).unwrap()
        );

        let err = query_observation(deps.as_ref(), env, 35_000).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Requested observation is too old. Last known observation is at 111000"
        );
    }

    #[test]
    fn observations_incomplete_buffer() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(100_000);
        BufferManager::init(&mut deps.storage, OBSERVATIONS, 3000).unwrap();

        let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

        let err = query_observation(deps.as_ref(), env.clone(), 11000).unwrap_err();
        assert_eq!(err.to_string(), "Generic error: Buffer is empty");

        let array = (1..=30)
            .into_iter()
            .map(|i| Observation {
                timestamp: env.block.time.seconds() + i * 1000,
                base_sma: Default::default(),
                base_amount: i.into(),
                quote_sma: Default::default(),
                quote_amount: (i * i).into(),
            })
            .collect_vec();
        buffer.push_many(&array);
        buffer.commit(&mut deps.storage).unwrap();

        env.block.time = env.block.time.plus_seconds(30_000);

        assert_eq!(
            OracleObservation {
                timestamp: 120_000,
                price: f64_to_dec(20.0 / 400.0),
            },
            query_observation(deps.as_ref(), env.clone(), 10000).unwrap()
        );

        assert_eq!(
            OracleObservation {
                timestamp: 124_411,
                price: f64_to_dec(0.04098166666666694),
            },
            query_observation(deps.as_ref(), env.clone(), 5589).unwrap()
        );
    }
}
