use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    CustomQuery, Decimal, Decimal256, Deps, Env, Fraction, StdError, StdResult, Storage, Uint128,
    Uint256,
};
use cw_storage_plus::Item;

use astroport_circular_buffer::{BufferManager, CircularBuffer};

use crate::cosmwasm_ext::AbsDiff;

/// Circular buffer size which stores observations
pub const OBSERVATIONS_SIZE: u32 = 3000;

/// Stores trade size observations. We use it in orderbook integration
/// and derive prices for external contracts/users.
#[cw_serde]
#[derive(Copy, Default)]
pub struct Observation {
    /// Timestamp of the observation
    pub ts: u64,
    /// Observed price at this point
    pub price: Decimal,
    /// Price simple moving average (mean)
    pub price_sma: Decimal,
}

#[cw_serde]
pub struct OracleObservation {
    pub timestamp: u64,
    pub price: Decimal,
}

/// Returns price observation at point that was 'seconds_ago' seconds ago.
pub fn query_observation<C>(
    deps: Deps<C>,
    env: Env,
    observations: CircularBuffer<Observation>,
    seconds_ago: u64,
) -> StdResult<OracleObservation>
where
    C: CustomQuery,
{
    let buffer = BufferManager::new(deps.storage, observations)?;
    let target = env.block.time.seconds() - seconds_ago;

    let mut oldest_ind = buffer.head();
    let mut newest_ind = buffer.head() + buffer.capacity() - 1;

    if !buffer.exists(deps.storage, oldest_ind) {
        if buffer.head() > 0 {
            oldest_ind = 0;
            newest_ind %= buffer.capacity();
        } else {
            return match PrecommitObservation::may_load(deps.storage)? {
                // First observation after pool initialization could be captured but not committed yet
                Some(obs) if obs.precommit_ts <= target => Ok(OracleObservation {
                    timestamp: target,
                    price: Decimal::from_ratio(obs.base_amount, obs.quote_amount),
                }),
                Some(_) => Err(StdError::generic_err(format!(
                    "Requested observation is too old. Last known observation is at {}",
                    target
                ))),
                None => Err(StdError::generic_err("Buffer is empty")),
            };
        }
    }

    let newest_obs = buffer.read_single(deps.storage, newest_ind)?.unwrap();
    if target >= newest_obs.ts {
        return Ok(OracleObservation {
            timestamp: target,
            price: newest_obs.price_sma,
        });
    }
    let oldest_obs = buffer.read_single(deps.storage, oldest_ind)?.unwrap();
    if target == oldest_obs.ts {
        return Ok(OracleObservation {
            timestamp: target,
            price: oldest_obs.price_sma,
        });
    }
    if target < oldest_obs.ts {
        return Err(StdError::generic_err(format!(
            "Requested observation is too old. Last known observation is at {}",
            oldest_obs.ts
        )));
    }

    let (left, right) = binary_search(deps.storage, &buffer, target, oldest_ind, newest_ind)?;

    let price_left = left.price_sma;
    let price_right = right.price_sma;
    let price = if left.ts == target {
        price_left
    } else if right.ts == target {
        price_right
    } else if price_left == price_right {
        price_left
    } else {
        // Interpolate.
        let price_slope =
            price_right.diff(price_left) * Decimal::from_ratio(1u8, right.ts - left.ts);
        let time_interval = Decimal::from_ratio(target - left.ts, 1u8);
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

        if leftward_or_hit.ts <= target && target <= rightward_or_hit.ts {
            break Ok((leftward_or_hit, rightward_or_hit));
        }
        if leftward_or_hit.ts > target {
            end = mid - 1;
        } else {
            start = mid + 1;
        }
    }
}

#[cw_serde]
pub struct PrecommitObservation {
    pub base_amount: Uint128,
    pub quote_amount: Uint128,
    pub precommit_ts: u64,
}

impl<'a> PrecommitObservation {
    /// Temporal storage for observation which should be committed in the next block
    const PRECOMMIT_OBSERVATION: Item<'a, PrecommitObservation> =
        Item::new("precommit_observation");

    pub fn save(
        storage: &mut dyn Storage,
        env: &Env,
        base_amount: Uint128,
        quote_amount: Uint128,
    ) -> StdResult<()> {
        let next_obs = match Self::may_load(storage)? {
            // Accumulating observations at the same block
            Some(mut prev_obs) if env.block.time.seconds() == prev_obs.precommit_ts => {
                prev_obs.base_amount += base_amount;
                prev_obs.quote_amount += quote_amount;
                prev_obs
            }
            _ => PrecommitObservation {
                base_amount,
                quote_amount,
                precommit_ts: env.block.time.seconds(),
            },
        };

        Self::PRECOMMIT_OBSERVATION.save(storage, &next_obs)
    }

    #[inline]
    pub fn may_load(storage: &dyn Storage) -> StdResult<Option<Self>> {
        Self::PRECOMMIT_OBSERVATION.may_load(storage)
    }
}

pub fn try_dec256_into_dec(val: Decimal256) -> StdResult<Decimal> {
    let numerator: Uint128 = val.numerator().try_into()?;

    Ok(Decimal::from_ratio(numerator, Decimal::one().denominator()))
}

/// Internal function to calculate new moving average using Uint256.
/// Overflow is possible only if new average price is greater than 2^128 - 1 which is unlikely.
/// Formula: (sma * count + new_price - oldest_price) / count
pub fn safe_sma_calculation(
    price_sma: Decimal,
    oldest_price: Decimal,
    count: u32,
    new_price: Decimal,
) -> StdResult<Decimal> {
    let sma_times_count = price_sma.numerator().full_mul(count);
    let res = Decimal256::from_ratio(
        sma_times_count + Uint256::from(new_price.numerator())
            - Uint256::from(oldest_price.numerator()),
        price_sma.denominator().full_mul(count),
    );

    try_dec256_into_dec(res)
}

/// Same as [`safe_sma_calculation`] but is being used when buffer is not full yet.
/// Formula: (sma * count + new_price) / (count + 1)
pub fn safe_sma_buffer_not_full(
    price_sma: Decimal,
    count: u32,
    new_price: Decimal,
) -> StdResult<Decimal> {
    let sma_times_count = price_sma.numerator().full_mul(count);
    let res = Decimal256::from_ratio(
        sma_times_count + Uint256::from(new_price.numerator()),
        price_sma.denominator().full_mul(count + 1),
    );

    try_dec256_into_dec(res)
}

#[cfg(test)]
mod test {
    use cosmwasm_std::to_json_binary;

    use crate::observation::Observation;

    #[test]
    fn check_observation_size() {
        // Checking [`Observation`] object size to estimate gas cost

        let obs = Observation {
            ts: 0,
            price: Default::default(),
            price_sma: Default::default(),
        };

        let storage_bytes = to_json_binary(&obs).unwrap().len();
        assert_eq!(storage_bytes, 36); // in storage

        // https://github.com/cosmos/cosmos-sdk/blob/47f46643affd7ec7978329c42bac47275ac7e1cc/store/types/gas.go#L199
        println!("sdk gas cost per read {}", 1000 + storage_bytes * 3);
        println!("sdk gas cost per write {}", 2000 + storage_bytes * 30)
    }
}
