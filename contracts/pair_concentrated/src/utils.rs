use cosmwasm_std::{Addr, Env, QuerierWrapper, StdResult, Storage, Uint128};

use astroport::asset::{Asset, DecimalAsset};
use astroport::observation::{Observation, PrecommitObservation};
use astroport::querier::query_supply;
use astroport_circular_buffer::error::BufferResult;
use astroport_circular_buffer::BufferManager;
use astroport_pcl_common::state::{Config, Precisions};
use astroport_pcl_common::utils::{safe_sma_buffer_not_full, safe_sma_calculation};

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
    let total_share = query_supply(&querier, &config.pair_info.liquidity_token)?;

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

/// Calculate and save moving averages of swap sizes.
pub fn accumulate_swap_sizes(storage: &mut dyn Storage, env: &Env) -> BufferResult<()> {
    if let Some(PrecommitObservation {
        base_amount,
        quote_amount,
        precommit_ts,
    }) = PrecommitObservation::may_load(storage)?
    {
        let mut buffer = BufferManager::new(storage, OBSERVATIONS)?;

        let new_observation;
        if let Some(last_obs) = buffer.read_last(storage)? {
            // Skip saving observation if it has been already saved
            if last_obs.timestamp < precommit_ts {
                // Since this is circular buffer the next index contains the oldest value
                let count = buffer.capacity();
                if let Some(oldest_obs) = buffer.read_single(storage, buffer.head() + 1)? {
                    let new_base_sma = safe_sma_calculation(
                        last_obs.base_sma,
                        oldest_obs.base_amount,
                        count,
                        base_amount,
                    )?;
                    let new_quote_sma = safe_sma_calculation(
                        last_obs.quote_sma,
                        oldest_obs.quote_amount,
                        count,
                        quote_amount,
                    )?;
                    new_observation = Observation {
                        base_amount,
                        quote_amount,
                        base_sma: new_base_sma,
                        quote_sma: new_quote_sma,
                        timestamp: precommit_ts,
                    };
                } else {
                    // Buffer is not full yet
                    let count = buffer.head();
                    let base_sma = safe_sma_buffer_not_full(last_obs.base_sma, count, base_amount)?;
                    let quote_sma =
                        safe_sma_buffer_not_full(last_obs.quote_sma, count, quote_amount)?;
                    new_observation = Observation {
                        base_amount,
                        quote_amount,
                        base_sma,
                        quote_sma,
                        timestamp: precommit_ts,
                    };
                }

                buffer.instant_push(storage, &new_observation)?
            }
        } else {
            // Buffer is empty
            if env.block.time.seconds() > precommit_ts {
                new_observation = Observation {
                    timestamp: precommit_ts,
                    base_sma: base_amount,
                    base_amount,
                    quote_sma: quote_amount,
                    quote_amount,
                };

                buffer.instant_push(storage, &new_observation)?
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_env, MockStorage};
    use cosmwasm_std::{BlockInfo, Timestamp};

    use super::*;

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
        assert_eq!(obs.timestamp, 50);
        assert_eq!(buffer.head(), 0);
        assert_eq!(obs.base_sma.u128(), 1000u128);
        assert_eq!(obs.quote_sma.u128(), 500u128);
    }
}
