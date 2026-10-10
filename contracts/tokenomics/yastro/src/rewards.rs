use cosmwasm_std::{Addr, Coin, Decimal256, Order, StdResult, Storage, Uint128, Uint256};
use cw20_base::state::{BALANCES, TOKEN_INFO};

use crate::error::ContractError;
use crate::state::{GLOBAL_INDEX, PENDING, UNDISTRIBUTED, USER_INDEX};

/// Rewards `balance` earned between two index values
fn earned(balance: Uint128, from: Decimal256, to: Decimal256) -> StdResult<Uint128> {
    if to <= from || balance.is_zero() {
        return Ok(Uint128::zero());
    }
    let amount: Uint256 = Uint256::from(balance).mul_floor(to - from);
    Ok(amount.try_into()?)
}

fn indexes(storage: &dyn Storage) -> StdResult<Vec<(String, Decimal256)>> {
    GLOBAL_INDEX
        .range(storage, None, None, Order::Ascending)
        .collect()
}

/// Brings `addr`'s rewards up to the current index for its current balance. Must run before any
/// change to that balance.
pub fn settle(storage: &mut dyn Storage, addr: &Addr) -> StdResult<()> {
    let balance = BALANCES.may_load(storage, addr)?.unwrap_or_default();
    for (denom, global) in indexes(storage)? {
        let user = USER_INDEX
            .may_load(storage, (addr, &denom))?
            .unwrap_or_default();
        if global == user {
            continue;
        }
        let amount = earned(balance, user, global)?;
        if !amount.is_zero() {
            PENDING.update::<_, cosmwasm_std::StdError>(storage, (addr, &denom), |p| {
                Ok(p.unwrap_or_default().checked_add(amount)?)
            })?;
        }
        USER_INDEX.save(storage, (addr, &denom), &global)?;
    }
    Ok(())
}

/// What `addr` could claim right now, without writing anything
pub fn pending(storage: &dyn Storage, addr: &Addr) -> StdResult<Vec<Coin>> {
    let balance = BALANCES.may_load(storage, addr)?.unwrap_or_default();
    let mut coins = vec![];
    for (denom, global) in indexes(storage)? {
        let user = USER_INDEX
            .may_load(storage, (addr, &denom))?
            .unwrap_or_default();
        let settled = PENDING
            .may_load(storage, (addr, &denom))?
            .unwrap_or_default();
        let amount = settled.checked_add(earned(balance, user, global)?)?;
        if !amount.is_zero() {
            coins.push(Coin { denom, amount });
        }
    }
    Ok(coins)
}

/// Settles `addr` and removes all its settled rewards, returning them
pub fn take(storage: &mut dyn Storage, addr: &Addr) -> StdResult<Vec<Coin>> {
    settle(storage, addr)?;
    let coins: Vec<Coin> = PENDING
        .prefix(addr)
        .range(storage, None, None, Order::Ascending)
        .map(|r| r.map(|(denom, amount)| Coin { denom, amount }))
        .collect::<StdResult<_>>()?;
    for coin in &coins {
        PENDING.remove(storage, (addr, &coin.denom));
    }
    Ok(coins.into_iter().filter(|c| !c.amount.is_zero()).collect())
}

/// Adds a deposit to the denom's index, or holds it until someone holds yASTRO
pub fn distribute(storage: &mut dyn Storage, coin: &Coin) -> Result<(), ContractError> {
    let supply = TOKEN_INFO.load(storage)?.total_supply;
    let carried = UNDISTRIBUTED
        .may_load(storage, &coin.denom)?
        .unwrap_or_default();
    let total = coin.amount.checked_add(carried)?;
    if supply.is_zero() {
        UNDISTRIBUTED.save(storage, &coin.denom, &total)?;
        return Ok(());
    }
    let index = GLOBAL_INDEX
        .may_load(storage, &coin.denom)?
        .unwrap_or_default();
    let added = Decimal256::from_ratio(total, supply);
    GLOBAL_INDEX.save(storage, &coin.denom, &index.checked_add(added)?)?;
    UNDISTRIBUTED.remove(storage, &coin.denom);
    Ok(())
}
