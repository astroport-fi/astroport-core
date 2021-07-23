use crate::error::ContractError;
use crate::msg::{
    ExecuteMsg, GetMultiplierResponse, InstantiateMsg, PendingTokenResponse, PoolLengthResponse,
    QueryMsg,
};

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Storage, Uint128, WasmMsg,
};

use crate::state::{Config, PoolInfo, CONFIG, POOL_INFO, USER_INFO};

use cw20::{BalanceResponse, Cw20ExecuteMsg};
use std::cmp::max;
use std::ops::{Add, Mul, Sub};

// Bonus muliplier for early xASTRO makers.
const BONUS_MULTIPLIER: u64 = 10;
const AMOUNT: Uint128 = Uint128::new(1000);

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let config = Config {
        x_astro_token: msg.token,
        dev_addr: msg.dev_addr,
        bonus_end_block: msg.bonus_end_block,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: 0,
        owner: info.sender,
        start_block: msg.start_block,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Add {
            alloc_point,
            token,
            with_update,
        } => add(deps, env, info, alloc_point, token, with_update),
        ExecuteMsg::Set {
            token,
            alloc_point,
            with_update,
        } => set(deps, env, info, token, alloc_point, with_update),
        ExecuteMsg::MassUpdatePool {} => mass_update_pool(deps.storage, env, info),
        ExecuteMsg::UpdatePool { token } => update_pool(deps.storage, env, info, token),
        ExecuteMsg::Deposit { token, amount } => deposit(deps, env, info, token, amount),
        ExecuteMsg::Withdraw { token, amount } => withdraw(deps, env, info, token, amount),
        ExecuteMsg::EmergencyWithdraw { token } => emergency_withdraw(deps, env, info, token),
        ExecuteMsg::SetDev { dev_address } => set_dev(deps, info, dev_address),
    }
}

// Add a new lp to the pool. Can only be called by the owner.
// XXX DO NOT add the same LP token more than once. Rewards will be messed up if you do.
pub fn add(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    alloc_point: u64,
    token: Addr,
    with_update: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if POOL_INFO.load(deps.storage, &token).is_ok() {
        return Err(ContractError::TokenPoolAlreadyExists {});
    }

    let mut response = Response::default();

    if with_update {
        let update_pool_result = mass_update_pool(deps.storage, env.clone(), info).unwrap();
        if !update_pool_result.messages.is_empty() {
            for msg in update_pool_result.messages {
                response.messages.push(msg);
            }
            cfg = CONFIG.load(deps.storage)?;
        }
    }

    cfg.total_alloc_point = cfg.total_alloc_point.checked_add(alloc_point).unwrap();

    let pool_info = PoolInfo {
        alloc_point,
        last_reward_block: max(cfg.start_block, env.block.height),
        acc_per_share: Uint128::zero(),
    };

    POOL_INFO.save(deps.storage, &token, &pool_info)?;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(response)
}

// Update the given pool's xASTRO allocation point. Can only be called by the owner.
pub fn set(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
    alloc_point: u64,
    with_update: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    let pool_info = POOL_INFO.load(deps.storage, &token)?;
    let mut response = Response::default();

    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if with_update {
        let update_pool_result = mass_update_pool(deps.storage, env, info).unwrap();
        if !update_pool_result.messages.is_empty() {
            for msg in update_pool_result.messages {
                response.messages.push(msg);
            }
            cfg = CONFIG.load(deps.storage)?;
        }
    }

    cfg.total_alloc_point
        .checked_sub(pool_info.alloc_point)
        .unwrap()
        .checked_add(alloc_point)
        .unwrap();
    CONFIG.save(deps.storage, &cfg)?;
    Ok(response)
}

// Test that binaries of Addr and String are equal
// it's used for getting Addr from Map.keys() method via String
// may be in future CanonicalAddr will implement PrimaryKey or Addr will be got from Vec[u8]
#[test]
fn binaries_of_addr_and_string_are_equal() {
    let a = String::from("addr0001");
    let b = Addr::unchecked(a.clone());
    assert_eq!(a.as_bytes().to_vec(), b.as_bytes().to_vec());
}

// Update reward variables for all pools.
pub fn mass_update_pool(
    storage: &mut dyn Storage,
    env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    let token_keys: Vec<Addr> = POOL_INFO
        .keys(storage, None, None, cosmwasm_std::Order::Ascending)
        .map(|v| Addr::unchecked(String::from_utf8(v).unwrap()))
        .collect();
    for token in token_keys {
        let update_pool_result =
            update_pool(storage, env.clone(), info.clone(), token.clone()).unwrap();
        if !update_pool_result.messages.is_empty() {
            for msg in update_pool_result.messages {
                response.messages.push(msg);
            }
        }
    }
    Ok(response)
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool(
    storage: &mut dyn Storage,
    env: Env,
    info: MessageInfo,
    token: Addr,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(storage)?;
    let mut pool = POOL_INFO.load(storage, &token)?;
    let mut response = Response::default();

    if env.block.height <= pool.last_reward_block {
        return Ok(response);
    }

    //check local balances lp token
    let lp_supply = USER_INFO
        .load(storage, (&token, &info.sender))
        .unwrap_or_default()
        .amount;
    if lp_supply.is_zero() {
        pool.last_reward_block = env.block.height;
        POOL_INFO.save(storage, &token, &pool)?;
        return Ok(response);
    }

    let multiplier = get_multiplier(storage, pool.last_reward_block, env.block.height).unwrap();
    let token_rewards = Uint128::from(multiplier.reward_multiplier_over)
        .checked_mul(cfg.tokens_per_block)
        .unwrap()
        .checked_mul(Uint128::from(pool.alloc_point))
        .unwrap()
        .checked_div(Uint128::from(cfg.total_alloc_point))
        .unwrap();

    //calls to mint function for contract xASTRO token
    response.add_attribute("Rewards", token_rewards.to_string());
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cfg.x_astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: cfg.dev_addr.to_string(),
            amount: token_rewards.checked_div(Uint128(10)).unwrap(),
        })?,
        send: vec![],
    }));

    //TODO if not mint to info.sender.address ???
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cfg.x_astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Mint {
            recipient: env.contract.address.to_string(),
            amount: token_rewards,
        })?,
        send: vec![],
    }));
    let share = token_rewards
        .checked_mul(AMOUNT)
        .unwrap()
        .checked_div(lp_supply)
        .unwrap();
    pool.acc_per_share = pool.acc_per_share.checked_add(share).unwrap();
    pool.last_reward_block = env.block.height;
    POOL_INFO.save(storage, &token, &pool)?;
    Ok(response)
}

// Deposit LP tokens to MasterChef for xASTRO allocation.
pub fn deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "Deposit");
    let user = USER_INFO
        .load(deps.storage, (&token, &info.sender))
        .unwrap_or_default();

    let update_result =
        update_pool(deps.storage, env.clone(), info.clone(), token.clone()).unwrap();
    if !update_result.messages.is_empty() {
        for msg in update_result.messages {
            response.messages.push(msg);
        }
    }
    if !update_result.attributes.is_empty() {
        for attr in update_result.attributes {
            response.attributes.push(attr);
        }
    }
    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &token)?;

    if user.amount > Uint128::zero() {
        let pending = user
            .amount
            .checked_mul(pool.acc_per_share)
            .unwrap()
            .checked_div(AMOUNT)
            .unwrap()
            .checked_sub(user.reward_debt)
            .unwrap();

        let x_astro_balance: BalanceResponse = deps.querier.query_wasm_smart(
            cfg.x_astro_token.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )?;
        //call to transfer function for xASTRO token to:info.sender amount: safe (pending or x_astro_balance)
        let mut amout = pending;
        if pending > x_astro_balance.balance {
            amout = x_astro_balance.balance;
        }
        response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.x_astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: amout,
            })?,
            send: vec![],
        }));
        response.add_attribute("Action", "xASTROTransfer");
        response.add_attribute("pending", pending.to_string());
    }
    //call transfer function for lp token from: info.sender to: env.contract.address amount:_amount
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: info.sender.to_string(),
            recipient: env.contract.address.to_string(),
            amount,
        })?,
        send: vec![],
    }));

    //Change user balance
    USER_INFO.update(
        deps.storage,
        (&token, &info.sender),
        |user_info| -> StdResult<_> {
            let mut val = user_info.unwrap_or_default();
            val.amount = user.amount.checked_add(amount).unwrap();
            if pool.acc_per_share > Uint128::zero() {
                val.reward_debt = val
                    .amount
                    .checked_mul(pool.acc_per_share)
                    .unwrap()
                    .checked_sub(AMOUNT)
                    .unwrap();
            }
            Ok(val)
        },
    )?;
    Ok(response)
}

// Withdraw LP tokens from MasterChef.
pub fn withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut response = Response::default();
    response.add_attribute("Action", "Withdraw");
    let user = &mut USER_INFO
        .load(deps.storage, (&token, &info.sender))
        .unwrap_or_default();
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }
    //check result update pool
    let update_result =
        update_pool(deps.storage, env.clone(), info.clone(), token.clone()).unwrap();
    if !update_result.messages.is_empty() {
        for msg in update_result.messages {
            response.messages.push(msg);
        }
    }
    if !update_result.attributes.is_empty() {
        for attr in update_result.attributes {
            response.attributes.push(attr);
        }
    }
    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &token)?;
    let pending = user
        .amount
        .checked_mul(pool.acc_per_share)
        .unwrap()
        .checked_div(AMOUNT)
        .unwrap()
        .checked_sub(user.reward_debt)
        .unwrap();

    let x_astro_balance: BalanceResponse = deps.querier.query_wasm_smart(
        cfg.x_astro_token.clone(),
        &cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    //xASTRO transfer to info.sender pending;
    let mut pending_rewards = pending;
    if pending > x_astro_balance.balance {
        pending_rewards = x_astro_balance.balance;
    }

    response.add_attribute("PendingRewards", pending_rewards.to_string());
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: cfg.x_astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: info.sender.to_string(),
            amount: pending_rewards,
        })?,
        send: vec![],
    }));
    //call to transfer function for lp token
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: env.contract.address.to_string(),
            recipient: info.sender.to_string(),
            amount,
        })?,
        send: vec![],
    }));

    //Change user balance
    USER_INFO.update(
        deps.storage,
        (&token, &info.sender),
        |user_info| -> StdResult<_> {
            let mut val = user_info.unwrap_or_default();
            val.amount = user.amount.checked_sub(amount).unwrap();
            val.reward_debt = user
                .reward_debt
                .checked_mul(pool.acc_per_share)
                .unwrap()
                .checked_div(AMOUNT)
                .unwrap();
            Ok(val)
        },
    )?;
    Ok(response)
}

// Withdraw without caring about rewards. EMERGENCY ONLY.
pub fn emergency_withdraw(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
) -> Result<Response, ContractError> {
    let user = &mut USER_INFO
        .load(deps.storage, (&token, &info.sender))
        .unwrap();
    let mut response = Response::default();
    response.add_attribute("Action", "EmergencyWithdraw");
    //call to transfer function for lp token
    response.messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
            owner: env.contract.address.to_string(),
            recipient: info.sender.to_string(),
            amount: user.amount,
        })?,
        send: vec![],
    }));

    //Change user balance
    USER_INFO.update(
        deps.storage,
        (&token, &info.sender),
        |user_info| -> StdResult<_> {
            let mut val = user_info.unwrap_or_default();
            val.amount = Uint128::zero();
            val.reward_debt = Uint128::zero();
            Ok(val)
        },
    )?;
    Ok(response)
}

// Update dev address by the previous dev.
pub fn set_dev(
    deps: DepsMut,
    info: MessageInfo,
    dev_address: Addr,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.dev_addr {
        return Err(ContractError::Unauthorized {});
    }
    cfg.dev_addr = dev_address;
    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::default())
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PoolLength {} => to_binary(&pool_length(deps)?),
        QueryMsg::PendingToken { token, user } => {
            to_binary(&pending_token(deps, env, token, user)?)
        }
        QueryMsg::GetMultiplier { from, to } => to_binary(&get_multiplier(deps.storage, from, to)?),
    }
}

pub fn pool_length(deps: Deps) -> StdResult<PoolLengthResponse> {
    let _length = POOL_INFO
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();
    Ok(PoolLengthResponse { length: _length })
}

// Return reward multiplier over the given _from to _to block.
fn get_multiplier(storage: &dyn Storage, from: u64, to: u64) -> StdResult<GetMultiplierResponse> {
    let cfg = CONFIG.load(storage)?;
    let mut _reward = 0_u64;
    if to <= cfg.bonus_end_block {
        _reward = to.sub(from).mul(BONUS_MULTIPLIER as u64)
    } else if from >= cfg.bonus_end_block {
        _reward = to.sub(from);
    } else {
        _reward = cfg
            .bonus_end_block
            .sub(from)
            .mul(BONUS_MULTIPLIER)
            .add(to.sub(cfg.bonus_end_block));
    }
    Ok(GetMultiplierResponse {
        reward_multiplier_over: _reward,
    })
}

// View function to see pending xASTRO on frontend.
pub fn pending_token(
    deps: Deps,
    env: Env,
    token: Addr,
    user: Addr,
) -> StdResult<PendingTokenResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &token)?;
    let user_info = &mut USER_INFO.load(deps.storage, (&token, &user))?;
    let acc_per_share = pool.acc_per_share;
    let lp_supply = USER_INFO
        .load(deps.storage, (&token, &user))
        .unwrap_or_default()
        .amount;
    if env.block.height > pool.last_reward_block && lp_supply != Uint128::zero() {
        let multiplier = get_multiplier(deps.storage, pool.last_reward_block, env.block.height)?;
        let mtpl = Uint128::from(multiplier.reward_multiplier_over);
        let token_rewards = mtpl
            .checked_mul(cfg.tokens_per_block)
            .unwrap()
            .checked_mul(Uint128::from(pool.alloc_point))
            .unwrap()
            .checked_div(Uint128::from(cfg.total_alloc_point))
            .unwrap();
        acc_per_share
            .add(token_rewards.checked_mul(AMOUNT).unwrap())
            .checked_div(lp_supply)
            .unwrap();
    }
    let _pending = user_info
        .amount
        .checked_mul(acc_per_share)
        .unwrap()
        .checked_div(AMOUNT)
        .unwrap()
        .checked_sub(user_info.reward_debt)
        .unwrap();
    Ok(PendingTokenResponse { pending: _pending })
}
