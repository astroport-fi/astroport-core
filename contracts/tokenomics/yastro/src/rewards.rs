use cosmwasm_std::{Addr, Coin, Decimal256, Order, StdError, StdResult, Storage, Uint128, Uint256};
use cw20_base::state::{BALANCES, TOKEN_INFO};

use astroport::incentives::{EPOCHS_START, EPOCH_LENGTH};

use crate::state::{Stream, PENDING, STREAMS, USER_INDEX};

/// When the epoch `now` falls in ends. Epochs are Astroport's weeks, starting Monday 00:00 UTC.
pub fn epoch_end(now: u64) -> u64 {
    EPOCHS_START + (now.saturating_sub(EPOCHS_START) / EPOCH_LENGTH + 1) * EPOCH_LENGTH
}

fn seconds(s: u64) -> Decimal256 {
    Decimal256::from_ratio(s, 1u8)
}

fn div(a: Decimal256, b: Decimal256) -> StdResult<Decimal256> {
    a.checked_div(b)
        .map_err(|e| StdError::generic_err(e.to_string()))
}

impl Stream {
    pub fn new(now: u64) -> Self {
        Stream {
            index: Decimal256::zero(),
            rate: Decimal256::zero(),
            queued: Decimal256::zero(),
            epoch_end: epoch_end(now),
            last_update: now,
            total_deposited: Uint128::zero(),
        }
    }

    /// Pays the stream out to `supply` yASTRO up to `now`. Must run before the supply changes.
    pub fn advance(&mut self, now: u64, supply: Uint128) -> StdResult<()> {
        while self.last_update < now {
            if supply.is_zero() && now > self.epoch_end {
                // Nobody held yASTRO through the epoch boundary: everything not yet paid out
                // streams again over the epoch `now` is in
                let unpaid = self
                    .rate
                    .checked_mul(seconds(self.epoch_end - self.last_update))?
                    .checked_add(self.queued)?;
                self.epoch_end = epoch_end(now);
                self.last_update = self.epoch_end - EPOCH_LENGTH;
                self.rate = div(unpaid, seconds(EPOCH_LENGTH))?;
                self.queued = Decimal256::zero();
                continue;
            }

            let until = now.min(self.epoch_end);
            let accrued = self.rate.checked_mul(seconds(until - self.last_update))?;
            if supply.is_zero() {
                // Nobody to pay: stream it again next epoch
                self.queued = self.queued.checked_add(accrued)?;
            } else {
                self.index = self
                    .index
                    .checked_add(div(accrued, Decimal256::from_ratio(supply, 1u8))?)?;
            }
            self.last_update = until;

            if until == self.epoch_end {
                self.rate = div(self.queued, seconds(EPOCH_LENGTH))?;
                self.queued = Decimal256::zero();
                if self.rate.is_zero() {
                    // Idle: jump to the epoch `now` is in
                    self.epoch_end = epoch_end(now);
                    self.last_update = self.last_update.max(self.epoch_end - EPOCH_LENGTH);
                } else {
                    self.epoch_end += EPOCH_LENGTH;
                }
            }
        }
        Ok(())
    }
}

/// Every stream, paid out up to `now`, without saving
pub fn streams_at(storage: &dyn Storage, now: u64) -> StdResult<Vec<(String, Stream)>> {
    let supply = TOKEN_INFO.load(storage)?.total_supply;
    STREAMS
        .range(storage, None, None, Order::Ascending)
        .map(|item| {
            let (denom, mut stream) = item?;
            stream.advance(now, supply)?;
            Ok((denom, stream))
        })
        .collect()
}

/// Pays every stream out up to `now` and saves them
fn sync(storage: &mut dyn Storage, now: u64) -> StdResult<Vec<(String, Stream)>> {
    let streams = streams_at(storage, now)?;
    for (denom, stream) in &streams {
        STREAMS.save(storage, denom, stream)?;
    }
    Ok(streams)
}

/// Rewards `balance` earned between two index values
fn earned(balance: Uint128, from: Decimal256, to: Decimal256) -> StdResult<Decimal256> {
    if to <= from || balance.is_zero() {
        return Ok(Decimal256::zero());
    }
    Ok(Decimal256::from_ratio(balance, 1u8).checked_mul(to - from)?)
}

fn settle_one(
    storage: &mut dyn Storage,
    streams: &[(String, Stream)],
    addr: &Addr,
) -> StdResult<()> {
    let balance = BALANCES.may_load(storage, addr)?.unwrap_or_default();
    for (denom, stream) in streams {
        let user = USER_INDEX
            .may_load(storage, (addr, denom))?
            .unwrap_or_default();
        if stream.index == user {
            continue;
        }
        let amount = earned(balance, user, stream.index)?;
        if !amount.is_zero() {
            PENDING.update::<_, StdError>(storage, (addr, denom), |p| {
                Ok(p.unwrap_or_default().checked_add(amount)?)
            })?;
        }
        USER_INDEX.save(storage, (addr, denom), &stream.index)?;
    }
    Ok(())
}

/// Brings the streams and `addrs`' rewards up to `now` for their current balances. Must run
/// before any change to those balances or to the supply.
pub fn settle(storage: &mut dyn Storage, now: u64, addrs: &[&Addr]) -> StdResult<()> {
    let streams = sync(storage, now)?;
    for addr in addrs {
        settle_one(storage, &streams, addr)?;
    }
    Ok(())
}

/// What `addr` could claim at `now`, without writing anything
pub fn pending(storage: &dyn Storage, now: u64, addr: &Addr) -> StdResult<Vec<Coin>> {
    let balance = BALANCES.may_load(storage, addr)?.unwrap_or_default();
    let mut coins = vec![];
    for (denom, stream) in streams_at(storage, now)? {
        let user = USER_INDEX
            .may_load(storage, (addr, &denom))?
            .unwrap_or_default();
        let settled = PENDING
            .may_load(storage, (addr, &denom))?
            .unwrap_or_default();
        let amount = to_units(settled.checked_add(earned(balance, user, stream.index)?)?)?;
        if !amount.is_zero() {
            coins.push(Coin { denom, amount });
        }
    }
    Ok(coins)
}

fn to_units(amount: Decimal256) -> StdResult<Uint128> {
    let units: Uint256 = amount.to_uint_floor();
    Ok(units.try_into()?)
}

/// Settles `addr` and removes its whole units of reward, returning them. Fractions stay.
pub fn take(storage: &mut dyn Storage, now: u64, addr: &Addr) -> StdResult<Vec<Coin>> {
    settle(storage, now, &[addr])?;
    let owed: Vec<(String, Decimal256)> = PENDING
        .prefix(addr)
        .range(storage, None, None, Order::Ascending)
        .collect::<StdResult<_>>()?;
    let mut coins = vec![];
    for (denom, amount) in owed {
        let units = to_units(amount)?;
        if units.is_zero() {
            continue;
        }
        let rest = amount.checked_sub(Decimal256::from_ratio(units, 1u8))?;
        if rest.is_zero() {
            PENDING.remove(storage, (addr, &denom));
        } else {
            PENDING.save(storage, (addr, &denom), &rest)?;
        }
        coins.push(Coin {
            denom,
            amount: units,
        });
    }
    Ok(coins)
}

/// Gives `coin` back to `addr` as pending reward
pub fn restore(storage: &mut dyn Storage, addr: &Addr, coin: &Coin) -> StdResult<()> {
    PENDING.update::<_, StdError>(storage, (addr, &coin.denom), |p| {
        Ok(p.unwrap_or_default()
            .checked_add(Decimal256::from_ratio(coin.amount, 1u8))?)
    })?;
    Ok(())
}

/// Queues a deposit to stream over the next epoch
pub fn deposit(storage: &mut dyn Storage, now: u64, coin: &Coin) -> StdResult<Stream> {
    sync(storage, now)?;
    let mut stream = STREAMS
        .may_load(storage, &coin.denom)?
        .unwrap_or_else(|| Stream::new(now));
    stream.queued = stream
        .queued
        .checked_add(Decimal256::from_ratio(coin.amount, 1u8))?;
    stream.total_deposited = stream.total_deposited.checked_add(coin.amount)?;
    STREAMS.save(storage, &coin.denom, &stream)?;
    Ok(stream)
}
