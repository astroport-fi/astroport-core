use astroport::cosmwasm_ext::{AbsDiff, DecimalToInteger, IntegerToDecimal};
use astroport_pcl_common::utils::{
    assert_slippage_tolerance, calc_provide_fee, check_assets, check_pair_registered,
};
use astroport_pcl_common::{calc_d, get_xcp};
use cosmwasm_std::{
    Addr, Decimal, Decimal256, Deps, Env, QuerierWrapper, StdError, StdResult, Storage, Uint128,
};

use astroport::asset::{Asset, Decimal256Ext, DecimalAsset, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::observation::{safe_sma_buffer_not_full, safe_sma_calculation};
use astroport::observation::{Observation, PrecommitObservation};
use astroport::pair::MIN_TRADE_SIZE;
use astroport::querier::query_native_supply;
use astroport_circular_buffer::error::BufferResult;
use astroport_circular_buffer::BufferManager;
use astroport_pcl_common::state::{Config, Precisions};
use itertools::Itertools;

use crate::contract::LP_TOKEN_PRECISION;
use crate::error::ContractError;
use crate::state::OBSERVATIONS;

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub(crate) fn pool_info(
    querier: QuerierWrapper,
    config: &Config,
) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;

    let total_share = query_native_supply(&querier, config.pair_info.liquidity_token.to_string())?;

    Ok((pools, total_share))
}

/// Returns current pool's volumes where amount is in [`Decimal256`] form.
pub(crate) fn query_pools(
    querier: QuerierWrapper,
    addr: &Addr,
    config: &Config,
    precisions: &Precisions,
) -> Result<Vec<DecimalAsset>, ContractError> {
    config
        .pair_info
        .query_pools(&querier, addr)?
        .into_iter()
        .map(|asset| {
            asset
                .to_decimal_asset(precisions.get_precision(&asset.info)?)
                .map_err(Into::into)
        })
        .collect()
}

/// Calculate and save price moving average
pub fn accumulate_swap_sizes(storage: &mut dyn Storage, env: &Env) -> BufferResult<()> {
    if let Some(PrecommitObservation {
        base_amount,
        quote_amount,
        precommit_ts,
    }) = PrecommitObservation::may_load(storage)?
    {
        let mut buffer = BufferManager::new(storage, OBSERVATIONS)?;
        let observed_price = Decimal::from_ratio(base_amount, quote_amount);

        let new_observation;
        if let Some(last_obs) = buffer.read_last(storage)? {
            // Skip saving observation if it has been already saved
            if last_obs.ts < precommit_ts {
                // Since this is circular buffer the next index contains the oldest value
                let count = buffer.capacity();
                if let Some(oldest_obs) = buffer.read_single(storage, buffer.head() + 1)? {
                    let price_sma = safe_sma_calculation(
                        last_obs.price_sma,
                        oldest_obs.price,
                        count,
                        observed_price,
                    )?;
                    new_observation = Observation {
                        ts: precommit_ts,
                        price: observed_price,
                        price_sma,
                    };
                } else {
                    // Buffer is not full yet
                    let count = buffer.head();
                    let price_sma =
                        safe_sma_buffer_not_full(last_obs.price_sma, count, observed_price)?;
                    new_observation = Observation {
                        ts: precommit_ts,
                        price: observed_price,
                        price_sma,
                    };
                }

                buffer.instant_push(storage, &new_observation)?
            }
        } else {
            // Buffer is empty
            if env.block.time.seconds() > precommit_ts {
                new_observation = Observation {
                    ts: precommit_ts,
                    price: observed_price,
                    price_sma: observed_price,
                };

                buffer.instant_push(storage, &new_observation)?
            }
        }
    }

    Ok(())
}

pub(crate) fn get_assets_with_precision(
    deps: Deps,
    config: &Config,
    assets: &mut Vec<Asset>,
    pools: Vec<DecimalAsset>,
    precisions: &Precisions,
) -> Result<Vec<Decimal256>, ContractError> {
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

    check_assets(deps.api, assets)?;

    if pools[0].info.equal(&assets[1].info) {
        assets.swap(0, 1);
    }

    // precisions.get_precision() also validates that the asset belongs to the pool
    Ok(vec![
        Decimal256::with_precision(assets[0].amount, precisions.get_precision(&assets[0].info)?)?,
        Decimal256::with_precision(assets[1].amount, precisions.get_precision(&assets[1].info)?)?,
    ])
}

pub(crate) fn calculate_shares(
    env: &Env,
    config: &mut Config,
    pools: &mut [DecimalAsset],
    total_share: Decimal256,
    deposits: Vec<Decimal256>,
    slippage_tolerance: Option<Decimal>,
) -> Result<(Uint128, Decimal256), ContractError> {
    // Initial provide can not be one-sided
    if total_share.is_zero() && (deposits[0].is_zero() || deposits[1].is_zero()) {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut new_xp = pools
        .iter()
        .enumerate()
        .map(|(ind, pool)| pool.amount + deposits[ind])
        .collect_vec();
    new_xp[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let new_d = calc_d(&new_xp, &amp_gamma)?;

    let share = if total_share.is_zero() {
        let xcp = get_xcp(new_d, config.pool_state.price_state.price_scale);
        let mint_amount = xcp
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT.to_decimal256(LP_TOKEN_PRECISION)?)
            .map_err(|_| ContractError::MinimumLiquidityAmountError {})?;

        // share cannot become zero after minimum liquidity subtraction
        if mint_amount.is_zero() {
            return Err(ContractError::MinimumLiquidityAmountError {});
        }

        config.pool_state.price_state.xcp_profit_real = Decimal256::one();
        config.pool_state.price_state.xcp_profit = Decimal256::one();

        mint_amount
    } else {
        let mut old_xp = pools.iter().map(|a| a.amount).collect_vec();
        old_xp[1] *= config.pool_state.price_state.price_scale;
        let old_d = calc_d(&old_xp, &amp_gamma)?;
        let share = (total_share * new_d / old_d).saturating_sub(total_share);

        let mut ideposits = deposits.clone();
        ideposits[1] *= config.pool_state.price_state.price_scale;

        share * (Decimal256::one() - calc_provide_fee(&ideposits, &new_xp, &config.pool_params))
    };

    // calculate accrued share
    let share_ratio = share / (total_share + share);
    let balanced_share = [
        new_xp[0] * share_ratio,
        new_xp[1] * share_ratio / config.pool_state.price_state.price_scale,
    ];
    let assets_diff = [
        deposits[0].diff(balanced_share[0]),
        deposits[1].diff(balanced_share[1]),
    ];

    let mut slippage = Decimal256::zero();

    // If deposit doesn't diverge too much from the balanced share, we don't update the price
    if assets_diff[0] >= MIN_TRADE_SIZE && assets_diff[1] >= MIN_TRADE_SIZE {
        slippage = assert_slippage_tolerance(
            &deposits,
            share,
            &config.pool_state.price_state,
            slippage_tolerance,
        )?;

        let last_price = assets_diff[0] / assets_diff[1];
        config.pool_state.update_price(
            &config.pool_params,
            env,
            total_share + share,
            &new_xp,
            last_price,
        )?;
    }

    Ok((share.to_uint(LP_TOKEN_PRECISION)?, slippage))
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;
    use std::str::FromStr;

    use cosmwasm_std::testing::{mock_env, MockStorage};
    use cosmwasm_std::{BlockInfo, Timestamp};

    use super::*;

    pub fn dec_to_f64(val: impl Display) -> f64 {
        f64::from_str(&val.to_string()).unwrap()
    }

    #[test]
    fn test_swap_observations() {
        let mut store = MockStorage::new();
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(1);

        let next_block = |block: &mut BlockInfo| {
            block.height += 1;
            block.time = block.time.plus_seconds(1);
        };

        BufferManager::init(&mut store, OBSERVATIONS, 10).unwrap();

        for _ in 0..=50 {
            accumulate_swap_sizes(&mut store, &env).unwrap();
            PrecommitObservation::save(&mut store, &env, 1000u128.into(), 500u128.into()).unwrap();
            next_block(&mut env.block);
        }

        let buffer = BufferManager::new(&store, OBSERVATIONS).unwrap();

        let obs = buffer.read_last(&store).unwrap().unwrap();
        assert_eq!(obs.ts, 50);
        assert_eq!(buffer.head(), 0);
        assert_eq!(dec_to_f64(obs.price_sma), 2.0);
        assert_eq!(dec_to_f64(obs.price), 2.0);
    }
}
