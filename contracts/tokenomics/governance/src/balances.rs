use std::convert::TryFrom;
use std::ops::Div;

use cosmwasm_std::{
    to_binary, Addr, Deps, DepsMut, Env, Event, MessageInfo, ReplyOn, Response, StdResult, Storage,
    SubMsg, Timestamp, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use cw_storage_plus::U64Key;

use crate::error::ContractError;
use crate::state::{
    LockedBalance, Point, CONFIG, GOVERNANCE_SATE, LOCKED, SLOPE_CHANGES, USER_POINT_EPOCH,
    USER_POINT_HISTORY,
};

//all future times are rounded by week
pub const WEEK: u64 = 7 * 86400;
//2 years
pub const MAX_TIME: u64 = 2 * 365 * 86400;
pub const MULTIPLIER: u64 = 1_000_000_000_000_000_000;

pub fn create_lock(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
    unlock_time: Timestamp,
) -> Result<Response, ContractError> {
    // Deposit `amount` tokens for `msg.sender` and lock until `unlock_time`
    // param amount  - Amount to deposit
    // param unlock_time - Epoch time when tokens unlock, rounded down to whole weeks
    let unlock = (unlock_time.nanos().div(1_000_000_000) / WEEK) * WEEK; // Locktime is rounded down to weeks
    let locked: LockedBalance = LOCKED
        .load(deps.storage, &info.sender)
        .unwrap_or(LockedBalance {
            amount: Uint128::zero(),
            end: _env.block.time.nanos().div(1_000_000_000),
        });
    if amount.is_zero() {
        return Err(ContractError::balance_err("Amount to small"));
    }
    if !locked.amount.is_zero() {
        return Err(ContractError::balance_err("Withdraw old tokens first"));
    }
    if unlock_time <= _env.block.time {
        return Err(ContractError::balance_err(
            "Can only lock until time in the future",
        ));
    }
    if unlock_time > _env.block.time.plus_seconds(MAX_TIME) {
        return Err(ContractError::balance_err("Voting lock can be 2 years max"));
    }
    deposit_for(deps, _env, info.sender, amount, unlock, locked)
}

pub fn increase_amount(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // Deposit `amount` additional tokens for `msg.sender` without modifying the unlock time
    // param amount Amount of tokens to deposit and add to the lock
    let locked: LockedBalance = LOCKED
        .load(deps.storage, &info.sender)
        .unwrap_or(LockedBalance {
            amount: Uint128::zero(),
            end: _env.block.time.nanos().div(1_000_000_000),
        });
    if amount.is_zero() {
        return Err(ContractError::balance_err("Amount to small"));
    }
    if locked.amount.is_zero() {
        return Err(ContractError::balance_err("No existing lock found"));
    }
    if locked.end <= _env.block.time.nanos().div(1_000_000_000) {
        return Err(ContractError::balance_err(
            "Cannot add to expired lock. Withdraw",
        ));
    }
    deposit_for(deps, _env, info.sender, amount, 0, locked)
}

pub fn increase_unlock_time(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    unlock_time: Timestamp,
) -> Result<Response, ContractError> {
    // Extend the unlock time for `msg.sender` to `_unlock_time`
    // param unlock_time New epoch time for unlocking
    let locked: LockedBalance = LOCKED
        .load(deps.storage, &info.sender)
        .unwrap_or(LockedBalance {
            amount: Uint128::zero(),
            end: _env.block.time.nanos().div(1_000_000_000),
        });
    let unlock = (unlock_time.nanos().div(1_000_000_000) / WEEK) * WEEK; //Locktime is rounded down to weeks
    if locked.end <= _env.block.time.nanos().div(1_000_000_000) {
        return Err(ContractError::balance_err("Lock expired"));
    }
    if locked.amount.is_zero() {
        return Err(ContractError::balance_err("Nothing is locked"));
    }
    if unlock <= locked.end {
        return Err(ContractError::balance_err(
            "Can only increase lock duration",
        ));
    }
    if unlock_time > _env.block.time.plus_seconds(MAX_TIME) {
        return Err(ContractError::balance_err("Voting lock can be 2 years max"));
    }
    deposit_for(deps, _env, info.sender, Uint128::zero(), unlock, locked)
}

pub fn deposit(
    deps: DepsMut,
    _env: Env,
    addr: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    // Deposit `_amount` tokens for `msg.sender` and add to the lock
    // Anyone (even a smart contract) can deposit for someone else, but
    // cannot extend their locktime and deposit for a brand new user
    // param amount Amount to add to user's lock
    let locked: LockedBalance = LOCKED.load(deps.storage, &addr).unwrap_or(LockedBalance {
        amount: Uint128::zero(),
        end: _env.block.time.nanos().div(1_000_000_000),
    });
    if amount.is_zero() {
        return Err(ContractError::balance_err("Amount to small"));
    }
    if locked.amount.is_zero() {
        return Err(ContractError::balance_err("No existing lock found"));
    }
    if locked.end <= _env.block.time.nanos().div(1_000_000_000) {
        return Err(ContractError::balance_err(
            "Cannot add to expired lock. Withdraw",
        ));
    }
    deposit_for(deps, _env, addr, amount, 0, locked)
}

pub fn withdraw(deps: DepsMut, _env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    // Withdraw all tokens for `msg.sender`
    // Only possible if the lock has expired
    let mut response = Response::default();
    //response.add_attribute("Action", "Withdraw");
    let mut locked: LockedBalance =
        LOCKED
            .load(deps.storage, &info.sender)
            .unwrap_or(LockedBalance {
                amount: Uint128::zero(),
                end: _env.block.time.nanos().div(1_000_000_000),
            });
    if _env.block.time.nanos().div(1_000_000_000) < locked.end {
        return Err(ContractError::balance_err("The lock didn't expire"));
    }
    if locked.amount.is_zero() {
        return Err(ContractError::balance_err("Nothing staked"));
    }
    let mut state = GOVERNANCE_SATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let amount = locked.amount;

    let old_locked: LockedBalance = locked.clone();
    locked.end = 0;
    locked.amount = Uint128::zero();
    LOCKED.update(deps.storage, &info.sender, |loked| -> StdResult<_> {
        let mut val = loked.unwrap_or_default();
        val.amount = locked.amount;
        val.end = locked.end;
        Ok(val)
    })?;

    //response.add_attribute("SupplyBefore", state.supply.to_string());
    state.supply = state.supply.checked_sub(amount).unwrap();
    //response.add_attribute("Supply", state.supply.to_string());

    // old_locked can have either expired <= timestamp or zero end
    // locked has only 0 end
    // Both can have >= 0 amount
    try_checkpoint(
        deps.storage,
        _env.clone(),
        info.sender.clone(),
        old_locked,
        locked,
    )?;

    response.messages.push(SubMsg {
        id: 0,
        msg: WasmMsg::Execute {
            contract_addr: config.xtrs_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount,
            })?,
            funds: vec![],
        }
        .into(),
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });
    let event = Event::new("Withdraw")
        .attr("Sender", info.sender.to_string())
        .attr("Amount", amount.to_string())
        .attr("Timestamp", _env.block.time.to_string());
    response.add_event(event);
    GOVERNANCE_SATE.save(deps.storage, &state)?;
    Ok(response)
}

pub fn checkpoint(deps: DepsMut, _env: Env) -> Result<Response, ContractError> {
    try_checkpoint(
        deps.storage,
        _env.clone(),
        _env.contract.address,
        LockedBalance::default(),
        LockedBalance::default(),
    )
}

pub fn query_balance_of(deps: Deps, _env: Env, addr: Addr) -> StdResult<Uint128> {
    // Get the current voting power for `msg.sender`
    // Adheres to the ERC20 `balanceOf` interface for compatibility
    // param addr User wallet address
    // return User voting power
    // _t Epoch time to return voting power at
    let _t = _env.block.time.nanos().div(1_000_000_000);
    let _epoch = USER_POINT_EPOCH.load(deps.storage, &addr).unwrap_or(0);
    if _epoch == 0 {
        Ok(Uint128::zero())
    } else {
        let mut last_point = USER_POINT_HISTORY.load(deps.storage, &addr).unwrap()[_epoch];
        last_point.bias = last_point
            .bias
            .checked_sub(
                last_point
                    .slope
                    .checked_mul(Uint128::from(_t - last_point.ts))
                    .unwrap(),
            )
            .unwrap_or_else(|_| Uint128::zero());
        Ok(last_point.bias)
    }
}

pub fn query_balance_of_at(deps: Deps, _env: Env, addr: Addr, _block: u64) -> StdResult<Uint128> {
    // Measure voting power of `addr` at block height `_block`
    // Adheres to MiniMe `balanceOfAt` interface: https://github.com/Giveth/minime
    // param addr User's wallet address
    // param _block Block to calculate the voting power at
    // return Voting power
    let state = GOVERNANCE_SATE.load(deps.storage)?;
    // Copying and pasting totalSupply code because Vyper cannot pass by
    // reference yet
    //     assert _block <= block.number
    if _block > _env.block.height {
        return Ok(Uint128::zero());
    }
    // Binary search
    let mut _min = 0;
    let mut _max = USER_POINT_EPOCH.load(deps.storage, &addr).unwrap(); //self.user_point_epoch[addr];
    for _ in 0..128 {
        // Will be always enough for 128-bit numbers
        if _min >= _max {
            break;
        }
        let _mid = (_min + _max + 1) / 2;
        if USER_POINT_HISTORY.load(deps.storage, &addr).unwrap()[_mid].blk <= _block {
            _min = _mid;
        } else {
            _max = _mid - 1;
        }
    }
    let mut upoint: Point = USER_POINT_HISTORY.load(deps.storage, &addr).unwrap()[_min];

    let max_epoch = state.epoch;
    let _epoch = find_block_epoch(deps.storage, _block, max_epoch);
    let point_0: Point = state.point_history[_epoch];
    let d_block;
    let d_t;
    if _epoch < max_epoch {
        let point_1: Point = state.point_history[_epoch + 1];
        d_block = point_1.blk - point_0.blk;
        d_t = point_1.ts - point_0.ts;
    } else {
        d_block = _env.block.height - point_0.blk;
        d_t = _env.block.time.nanos().div(1_000_000_000) - point_0.ts;
    }
    let mut block_time = point_0.ts;
    if d_block != 0 {
        block_time += d_t * (_block - point_0.blk) / d_block
    }
    upoint.bias = upoint
        .bias
        .checked_sub(
            upoint
                .slope
                .checked_mul(Uint128::from(block_time - upoint.ts))
                .unwrap(),
        )
        .unwrap();
    Ok(Uint128::from(u128::try_from(upoint.bias).unwrap()))
}

pub fn query_total_supply(deps: Deps, _env: Env) -> StdResult<Uint128> {
    // Calculate total voting power
    // Adheres to the ERC20 `totalSupply` interface for Aragon compatibility
    // return Total voting power
    let t = _env.block.time.nanos().div(1_000_000_000);
    let state = GOVERNANCE_SATE.load(deps.storage)?;
    let _epoch = state.epoch;
    let last_point: Point = state.point_history[_epoch];
    query_supply_at(deps, _env, last_point, t)
}

pub fn query_total_supply_at(deps: Deps, _env: Env, _block: u64) -> StdResult<Uint128> {
    // Calculate total voting power at some point in the past
    // param _block Block to calculate the total voting power at
    // return Total voting power at `_block`
    let state = GOVERNANCE_SATE.load(deps.storage)?;
    if _block > _env.block.height {
        return Ok(Uint128::zero());
    }
    let _epoch = state.epoch;
    let target_epoch = find_block_epoch(deps.storage, _block, _epoch);

    let point: Point = state.point_history[target_epoch];
    let mut dt = 0;
    if target_epoch < _epoch {
        let point_next: Point = state.point_history[target_epoch + 1];
        if point.blk != point_next.blk {
            dt = (_block - point.blk) * (point_next.ts - point.ts) / (point_next.blk - point.blk)
        }
    } else if point.blk != _env.block.height {
        dt = (_block - point.blk) * (_env.block.time.nanos().div(1_000_000_000) - point.ts)
            / (_env.block.height - point.blk);
    }
    //Now dt contains info on how far are we beyond point
    query_supply_at(deps, _env, point, point.ts + dt)
}

fn find_block_epoch(storage: &dyn Storage, _block: u64, max_epoch: usize) -> usize {
    // Binary search to estimate timestamp for block number
    // param _block Block to find
    // param max_epoch Don't go beyond this epoch
    // return Approximate timestamp for block
    let state = GOVERNANCE_SATE.load(storage).unwrap();
    //Binary search
    let mut _min = 0;
    let mut _max = max_epoch;
    for _ in 0..128 {
        // Will be always enough for 128-bit numbers
        if _min >= _max {
            break;
        }
        let _mid = (_min + _max + 1) / 2;
        if state.point_history[_mid].blk <= _block {
            _min = _mid;
        } else {
            _max = _mid - 1;
        }
    }
    _min
}

fn deposit_for(
    deps: DepsMut,
    _env: Env,
    addr: Addr,
    amount: Uint128,
    unlock_time: u64,
    locked_balance: LockedBalance,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    let mut state = GOVERNANCE_SATE.load(deps.storage)?;
    let config = CONFIG.load(deps.storage)?;
    let mut _locked = locked_balance;

    state.supply = state.supply.checked_add(amount).unwrap();

    let old_locked = _locked.clone();
    // Adding to existing lock, or if a lock is expired - creating a new one
    _locked.amount = _locked.amount.checked_add(amount).unwrap();
    if unlock_time != 0 {
        _locked.end = unlock_time;
    }
    LOCKED.update(deps.storage, &addr, |loked| -> StdResult<_> {
        let mut val = loked.unwrap_or_default();
        val.amount = _locked.amount;
        val.end = _locked.end;
        Ok(val)
    })?;
    // Possibilities:
    // Both old_locked.end could be current or expired (>/< block.timestamp)
    // value == 0 (extend lock) or value > 0 (add to lock or extend lock)
    // _locked.end > block.timestamp (always)
    try_checkpoint(
        deps.storage,
        _env.clone(),
        addr.clone(),
        old_locked,
        _locked.clone(),
    )?;
    if !amount.is_zero() {
        response.messages.push(SubMsg {
            id: 0,
            msg: WasmMsg::Execute {
                contract_addr: config.xtrs_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: addr.to_string(),
                    recipient: _env.contract.address.to_string(),
                    amount,
                })?,
                funds: vec![],
            }
            .into(),
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }
    let event = Event::new("Deposit")
        .attr("addr", addr.to_string())
        .attr("amount", amount.to_string())
        .attr("end", _locked.end.to_string());
    response.add_event(event);
    GOVERNANCE_SATE.save(deps.storage, &state)?;
    Ok(response)
}

fn query_supply_at(deps: Deps, _env: Env, point: Point, t: u64) -> StdResult<Uint128> {
    // Calculate total voting power at some point in the past
    // param point The point (bias/slope) to start search from
    // param t Time to calculate the total voting power at
    // return Total voting power at that time
    let mut last_point: Point = point;
    let mut t_i = (last_point.ts / WEEK) * WEEK;
    for _ in 0..255 {
        t_i += WEEK;
        let mut d_slope = Uint128::zero();
        if t_i > t {
            t_i = t;
        } else {
            d_slope = SLOPE_CHANGES
                .load(deps.storage, U64Key::from(t_i))
                .unwrap_or_else(|_| Uint128::zero());
        }
        last_point.bias = last_point
            .bias
            .checked_sub(
                last_point
                    .slope
                    .checked_mul(Uint128::from(t_i - last_point.ts))
                    .unwrap(),
            )
            .unwrap();
        if t_i == t {
            break;
        }
        last_point.slope = last_point.slope.checked_add(d_slope).unwrap();
        last_point.ts = t_i;
    }
    Ok(Uint128::from(u128::try_from(last_point.bias).unwrap()))
}

fn try_checkpoint(
    storage: &mut dyn Storage,
    _env: Env,
    addr: Addr,
    old_locked: LockedBalance,
    new_locked: LockedBalance,
) -> Result<Response, ContractError> {
    let mut state = GOVERNANCE_SATE.load(storage)?;
    let block_timestamp = _env.block.time.nanos().checked_div(1_000_000_000).unwrap();
    let mut u_old = Point::default();
    let mut u_new: Point = Point::default();
    let mut old_dslope = Uint128::zero();
    let mut new_dslope = Uint128::zero();
    let mut _epoch = state.epoch;
    if addr != _env.contract.address {
        // Calculate slopes and biases
        // Kept at zero when they have to
        if old_locked.end > block_timestamp && !old_locked.amount.is_zero() {
            let old_amount = old_locked.amount;
            u_old.slope = old_amount.checked_div(Uint128::from(MAX_TIME)).unwrap();
            u_old.bias = u_old
                .slope
                .checked_mul(Uint128::from(old_locked.end - block_timestamp))
                .unwrap();
        }
        if new_locked.end > block_timestamp && !new_locked.amount.is_zero() {
            //println!("new_locked not zero");
            let new_amount = new_locked.amount;
            u_new.slope = new_amount.checked_div(Uint128::from(MAX_TIME)).unwrap();
            u_new.bias = u_new
                .slope
                .checked_mul(Uint128::from(new_locked.end - block_timestamp))
                .unwrap();
        }
        // Read values of scheduled changes in the slope
        // old_locked.end can be in the past and in the future
        // new_locked.end can ONLY by in the FUTURE unless everything expired: than zeros
        old_dslope = SLOPE_CHANGES
            .load(storage, U64Key::from(old_locked.end))
            .unwrap_or_else(|_| Uint128::zero());
        if new_locked.end != 0 {
            if new_locked.end == old_locked.end {
                new_dslope = old_dslope;
            } else {
                new_dslope = SLOPE_CHANGES
                    .load(storage, U64Key::from(new_locked.end))
                    .unwrap_or_else(|_| Uint128::zero());
            }
        }
    }
    let mut last_point = Point {
        bias: Uint128::zero(),
        slope: Uint128::zero(),
        ts: block_timestamp,
        blk: _env.block.height,
    };
    if _epoch > 0 {
        last_point = state.point_history[_epoch];
    }
    let mut last_checkpoint = last_point.ts;
    // initial_last_point is used for extrapolation to calculate block number
    // (approximately, for *At methods) and save them
    // as we cannot figure that out exactly from inside the contract
    let initial_last_point: Point = last_point;
    let mut block_slope = 0; // dblock/dt
    if block_timestamp > last_point.ts {
        block_slope =
            MULTIPLIER * (_env.block.height - last_point.blk) / (block_timestamp - last_point.ts);
    }
    // If last point is already recorded in this block, slope=0
    // But that's ok b/c we know the block in such case
    // Go over weeks to fill history and calculate what the current point is
    let mut t_i = (last_checkpoint / WEEK) * WEEK;
    for _ in 0..255 {
        // Hopefully it won't happen that this won't get used in 5 years!
        // If it does, users will be able to withdraw but vote weight will be broken
        t_i += WEEK;
        let mut d_slope = Uint128::zero();
        if t_i > block_timestamp {
            t_i = block_timestamp
        } else {
            d_slope = SLOPE_CHANGES
                .load(storage, U64Key::from(t_i))
                .unwrap_or_else(|_| Uint128::zero());
        }
        last_point.bias = last_point
            .bias
            .checked_sub(
                last_point
                    .slope
                    .checked_mul(Uint128::from(t_i - last_checkpoint))
                    .unwrap(),
            )
            .unwrap();
        last_point.slope = last_point
            .slope
            .checked_add(d_slope)
            .unwrap_or_else(|_| Uint128::zero());
        last_checkpoint = t_i;
        last_point.ts = t_i;
        last_point.blk =
            initial_last_point.blk + block_slope * (t_i - initial_last_point.ts) / MULTIPLIER;
        _epoch += 1;
        if t_i == block_timestamp {
            last_point.blk = _env.block.height;
            break;
        } else {
            state.point_history[_epoch] = last_point;
        }
    }
    state.epoch = _epoch;
    // Now point_history is filled until t=now
    if addr != _env.contract.address {
        // If last point was in this block, the slope change has been applied already
        //  But in such case we have 0 slope(s)
        last_point.slope += u_new.slope.checked_sub(u_old.slope).unwrap();
        last_point.bias += u_new.bias.checked_sub(u_old.bias).unwrap();
    }
    // Record the changed point into history
    if _epoch >= state.point_history.len() {
        state.point_history.resize(_epoch + 1, Point::default());
    }
    state.point_history[_epoch] = last_point;
    if addr != _env.contract.address {
        // Schedule the slope changes (slope is going down)
        // We subtract new_user_slope from [new_locked.end]
        // and add old_user_slope to [old_locked.end]
        if old_locked.end > block_timestamp {
            // old_dslope was <something> - u_old.slope, so we cancel that
            old_dslope = old_dslope.checked_add(u_old.slope).unwrap();
            if new_locked.end == old_locked.end {
                old_dslope = old_dslope
                    .checked_sub(u_new.slope)
                    .unwrap_or_else(|_| Uint128::zero()); // It was a new deposit, not extension
            }
            //self.slope_changes[old_locked.end] = old_dslope
            SLOPE_CHANGES.save(storage, U64Key::from(old_locked.end), &old_dslope)?;
        }
        if new_locked.end > block_timestamp && new_locked.end > old_locked.end {
            new_dslope = new_dslope
                .checked_sub(u_new.slope)
                .unwrap_or_else(|_| Uint128::zero()); // old slope disappeared at this point
            SLOPE_CHANGES.save(storage, U64Key::from(new_locked.end), &new_dslope)?;
            // else: we recorded it already in old_dslope
        }
        // Now handle user history
        let user_epoch = USER_POINT_EPOCH
            .load(storage, &addr)
            .unwrap_or(0)
            .checked_add(1)
            .unwrap();
        USER_POINT_EPOCH.update(storage, &addr, |_point| -> StdResult<_> {
            let val = user_epoch;
            Ok(val)
        })?;
        u_new.ts = block_timestamp;
        u_new.blk = _env.block.height;
        let mut history = USER_POINT_HISTORY
            .load(storage, &addr)
            .unwrap_or_else(|_| Vec::with_capacity(1_000_000_000));
        if history.len() <= user_epoch {
            history.resize(user_epoch + 1, Point::default());
        }
        history[user_epoch] = u_new;
        USER_POINT_HISTORY.save(storage, &addr, &history)?;
    }
    GOVERNANCE_SATE.save(storage, &state)?;
    Ok(Response::default())
}
