#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_binary, Binary, Decimal, Decimal256, Deps, Env, StdError, StdResult, Uint128, Uint64,
};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo};
use astroport::cosmwasm_ext::{DecimalToInteger, IntegerToDecimal};
use astroport::observation::query_observation;
use astroport::pair::{
    ConfigResponse, PoolResponse, ReverseSimulationResponse, SimulationResponse,
};

use astroport::pair_concentrated::{ConcentratedPoolConfig, QueryMsg};
use astroport::querier::{query_factory_config, query_fee_info, query_supply};

use crate::contract::LP_TOKEN_PRECISION;
use crate::error::ContractError;
use astroport_pcl_common::state::Precisions;
use astroport_pcl_common::utils::{
    before_swap_check, compute_offer_amount, compute_swap, get_share_in_assets,
};
use astroport_pcl_common::{calc_d, get_xcp};

use crate::state::{BALANCES, CONFIG, OBSERVATIONS};

use crate::utils::{pool_info, query_pools};

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
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
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
        QueryMsg::CumulativePrices {} => Err(StdError::generic_err(
            stringify!(Not implemented. Use {"observe": {"seconds_ago": ... }} instead.),
        )),
        QueryMsg::Observe { seconds_ago } => {
            to_binary(&query_observation(deps, env, OBSERVATIONS, seconds_ago)?)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::LpPrice {} => to_binary(&query_lp_price(deps, env)?),
        QueryMsg::ComputeD {} => to_binary(&query_compute_d(deps, env)?),
        QueryMsg::AssetBalanceAt {
            asset_info,
            block_height,
        } => to_binary(&query_asset_balances_at(deps, asset_info, block_height)?),
    }
}

/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
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
fn query_share(deps: Deps, amount: Uint128) -> Result<Vec<Asset>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let pools = query_pools(
        deps.querier,
        &config.pair_info.contract_addr,
        &config,
        &precisions,
    )?;
    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let refund_assets =
        get_share_in_assets(&pools, amount.saturating_sub(Uint128::one()), total_share);

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
    deps: Deps,
    env: Env,
    offer_asset: Asset,
) -> Result<SimulationResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;

    let pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

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
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;
    let ask_asset_prec = precisions.get_precision(&ask_asset.info)?;
    let ask_asset_dec = ask_asset.to_decimal_asset(ask_asset_prec)?;

    let pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

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
pub fn query_lp_price(deps: Deps, env: Env) -> StdResult<Decimal256> {
    let config = CONFIG.load(deps.storage)?;
    let total_lp = query_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;
    if !total_lp.is_zero() {
        let precisions = Precisions::new(deps.storage)?;
        let mut ixs = query_pools(deps.querier, &env.contract.address, &config, &precisions)
            .map_err(|err| StdError::generic_err(err.to_string()))?
            .into_iter()
            .map(|asset| asset.amount)
            .collect_vec();
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
    let dec256_price_scale = config.pool_state.price_state.price_scale;
    let price_scale = Decimal::from_atomics(
        Uint128::try_from(dec256_price_scale.atomics())?,
        dec256_price_scale.decimal_places(),
    )
    .map_err(|e| StdError::generic_err(format!("{e}")))?;

    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    Ok(ConfigResponse {
        block_time_last: 0, // keeping this field for backwards compatibility
        params: Some(to_binary(&ConcentratedPoolConfig {
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
    })
}

/// Compute the current pool D value.
pub fn query_compute_d(deps: Deps, env: Env) -> StdResult<Decimal256> {
    let config = CONFIG.load(deps.storage)?;
    let precisions = Precisions::new(deps.storage)?;

    let mut xs = query_pools(deps.querier, &env.contract.address, &config, &precisions)
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

/// Returns the balance of the specified asset that was in the pool
/// just preceding the moment of the specified block height creation.
/// It will return None (null) if the balance was not tracked up to the specified block height
pub fn query_asset_balances_at(
    deps: Deps,
    asset_info: AssetInfo,
    block_height: Uint64,
) -> StdResult<Option<Uint128>> {
    BALANCES.may_load_at_height(deps.storage, &asset_info, block_height.u64())
}

#[cfg(test)]
mod testing {
    use std::error::Error;
    use std::str::FromStr;

    use astroport::observation::{query_observation, Observation, OracleObservation};
    use astroport_circular_buffer::BufferManager;
    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::Timestamp;

    use super::*;

    pub fn f64_to_dec<T>(val: f64) -> T
    where
        T: FromStr,
        T::Err: Error,
    {
        T::from_str(&val.to_string()).unwrap()
    }

    #[test]
    fn observations_full_buffer() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(100_000);
        BufferManager::init(&mut deps.storage, OBSERVATIONS, 20).unwrap();

        let mut buffer = BufferManager::new(&deps.storage, OBSERVATIONS).unwrap();

        let err = query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 11000).unwrap_err();
        assert_eq!(err.to_string(), "Generic error: Buffer is empty");

        let array = (1..=30)
            .into_iter()
            .map(|i| Observation {
                ts: env.block.time.seconds() + i * 1000,
                price_sma: Decimal::from_ratio(i, i * i),
                price: Default::default(),
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
            query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 10000).unwrap()
        );

        assert_eq!(
            OracleObservation {
                timestamp: 124_411,
                price: f64_to_dec(0.04098166666666694),
            },
            query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 5589).unwrap()
        );

        let err = query_observation(deps.as_ref(), env, OBSERVATIONS, 35_000).unwrap_err();
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

        let err = query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 11000).unwrap_err();
        assert_eq!(err.to_string(), "Generic error: Buffer is empty");

        let array = (1..=30)
            .into_iter()
            .map(|i| Observation {
                ts: env.block.time.seconds() + i * 1000,
                price: Default::default(),
                price_sma: Decimal::from_ratio(i, i * i),
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
            query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 10000).unwrap()
        );

        assert_eq!(
            OracleObservation {
                timestamp: 124_411,
                price: f64_to_dec(0.04098166666666694),
            },
            query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, 5589).unwrap()
        );
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
                ts: ts + i as u64 * 1000,
                price: Default::default(),
                price_sma: Decimal::from_ratio(i * i, i),
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
                        query_observation(
                            deps.as_ref(),
                            env.clone(),
                            OBSERVATIONS,
                            shift * 1000 - 500
                        )
                        .unwrap()
                    );
                }
                assert_eq!(
                    OracleObservation {
                        timestamp: ts + i as u64 * 1000,
                        price: f64_to_dec(i as f64),
                    },
                    query_observation(deps.as_ref(), env.clone(), OBSERVATIONS, shift * 1000)
                        .unwrap()
                );
            }
        }
    }
}
