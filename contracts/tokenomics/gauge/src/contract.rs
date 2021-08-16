use std::cmp::{max, min};
use std::ops::Add;

use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, ReplyOn,
    Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg};

use crate::error::ContractError;
use crate::state::{Config, PoolInfo, CONFIG, POOL_INFO, USER_INFO};
use gauge_proxy_interface::msg::{Cw20HookMsg as ProxyCw20HookMsg, ExecuteMsg as ProxyExecuteMsg};
use terraswap::gauge::{
    ExecuteMsg, GetMultiplierResponse, InstantiateMsg, MigrateMsg, PendingTokenResponse,
    PoolLengthResponse, QueryMsg,
};

// Bonus multiplier for early ASTRO makers.
const BONUS_MULTIPLIER: u64 = 10;
const PRECISION_MULTIPLIER: Uint128 = Uint128::new(100_000_000_000);

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let mut allowed_reward_proxies: Vec<Addr> = vec![];
    for proxy in msg.allowed_reward_proxies {
        allowed_reward_proxies.push(deps.api.addr_validate(&proxy)?);
    }

    let config = Config {
        astro_token: msg.token,
        dev_addr: msg.dev_addr,
        bonus_end_block: msg.bonus_end_block,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: 0,
        owner: info.sender,
        start_block: msg.start_block,
        allowed_reward_proxies,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Add {
            alloc_point,
            token,
            reward_proxy,
            with_update,
        } => add(
            deps,
            env,
            info,
            alloc_point,
            token,
            reward_proxy,
            with_update,
        ),
        ExecuteMsg::Set {
            token,
            alloc_point,
            with_update,
        } => set(deps, env, info, token, alloc_point, with_update),
        ExecuteMsg::MassUpdatePools {} => mass_update_pools(&mut deps, env),
        ExecuteMsg::UpdatePool { token } => update_pool(deps, env, token),
        ExecuteMsg::Deposit { token, amount } => deposit(deps, env, info, token, amount),
        ExecuteMsg::Withdraw { token, amount } => withdraw(deps, env, info, token, amount),
        ExecuteMsg::EmergencyWithdraw { token } => emergency_withdraw(deps, env, info, token),
        ExecuteMsg::SetDev { dev_address } => set_dev(deps, info, dev_address),
        ExecuteMsg::SetAllowedRewardProxies { proxies } => {
            Ok(set_allowed_reward_proxies(deps, proxies)?)
        }
    }
}

// Add a new lp to the pool. Can only be called by the owner.
// XXX DO NOT add the same LP token more than once. Rewards will be messed up if you do.
pub fn add(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    alloc_point: u64,
    token: Addr,
    reward_proxy: Option<String>,
    with_update: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let reward_proxy = reward_proxy
        .map(|v| deps.api.addr_validate(&v))
        .transpose()?;

    if let Some(proxy) = &reward_proxy {
        if !cfg.allowed_reward_proxies.contains(&proxy) {
            return Err(ContractError::RewardProxyNotAllowed {});
        }
    }

    let response = if !with_update {
        Response::default()
    } else {
        mass_update_pools(&mut deps, env.clone())?
    };

    if POOL_INFO.load(deps.storage, &token).is_ok() {
        return Err(ContractError::TokenPoolAlreadyExists {});
    }

    cfg.total_alloc_point = cfg.total_alloc_point.checked_add(alloc_point).unwrap();

    let pool_info = PoolInfo {
        alloc_point,
        last_reward_block: max(cfg.start_block, env.block.height),
        acc_per_share: Uint128::zero(),
        reward_proxy,
    };

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &token, &pool_info)?;

    Ok(response)
}

// Update the given pool's ASTRO allocation point. Can only be called by the owner.
pub fn set(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
    alloc_point: u64,
    with_update: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let response = if !with_update {
        Response::default()
    } else {
        mass_update_pools(&mut deps, env)?
    };

    let mut pool_info = POOL_INFO.load(deps.storage, &token)?;

    cfg.total_alloc_point = cfg
        .total_alloc_point
        .checked_sub(pool_info.alloc_point)
        .unwrap()
        .checked_add(alloc_point)
        .unwrap();
    pool_info.alloc_point = alloc_point;

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &token, &pool_info)?;

    Ok(response)
}

// Update reward variables for all pools.
pub fn mass_update_pools(deps: &mut DepsMut, env: Env) -> Result<Response, ContractError> {
    let mut response = Response::default();

    let pools: Vec<(Addr, PoolInfo)> = POOL_INFO
        .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .filter_map(|v| {
            v.ok()
                .map(|v| (Addr::unchecked(String::from_utf8(v.0).unwrap()), v.1))
        })
        .collect();

    if pools.is_empty() {
        return Ok(response);
    }
    let cfg = CONFIG.load(deps.storage)?;
    for (token, pool) in pools {
        let (_, messages, updated_pool) =
            update_pool_rewards(deps.as_ref(), env.clone(), token.clone(), pool, cfg.clone())?;

        if let Some(msgs) = messages {
            for msg in msgs {
                response.messages.push(SubMsg {
                    msg: CosmosMsg::Wasm(msg),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                });
            }
        }

        if let Some(p) = updated_pool {
            POOL_INFO.save(deps.storage, &token, &p)?;
        }
    }
    Ok(response.add_attribute("Action", "MassUpdatePools"))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool(deps: DepsMut, env: Env, token: Addr) -> Result<Response, ContractError> {
    let mut response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &token)?;

    let (_, messages, pool) = update_pool_rewards(deps.as_ref(), env, token.clone(), pool, cfg)?;
    if let Some(msgs) = messages {
        for msg in msgs {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(msg),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }

    if let Some(p) = pool {
        POOL_INFO.save(deps.storage, &token, &p)?;
    }

    Ok(response.add_attribute("Action", "UpdatePool"))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool_rewards(
    deps: Deps,
    env: Env,
    token: Addr,
    pool: PoolInfo,
    cfg: Config,
) -> StdResult<(Uint128, Option<Vec<WasmMsg>>, Option<PoolInfo>)> {
    if env.block.height <= pool.last_reward_block {
        return Ok((Uint128::zero(), None, None));
    }

    let lp_supply: BalanceResponse = deps.querier.query_wasm_smart(
        token,
        &cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    if lp_supply.balance.is_zero() {
        let updated_pool = PoolInfo {
            alloc_point: pool.alloc_point,
            last_reward_block: env.block.height,
            acc_per_share: pool.acc_per_share,
            reward_proxy: pool.reward_proxy,
        };
        return Ok((Uint128::zero(), None, Some(updated_pool)));
    }

    let token_rewards = calculate_rewards(env.clone(), pool.clone(), cfg.clone())?;
    let dev_token_rewards = token_rewards.checked_div(Uint128::from(10u128)).unwrap();
    let messages = vec![
        // mint dev rewards
        WasmMsg::Execute {
            contract_addr: cfg.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: cfg.dev_addr.to_string(),
                amount: dev_token_rewards,
            })?,
            funds: vec![],
        },
        // mint rewards
        WasmMsg::Execute {
            contract_addr: cfg.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: env.contract.address.to_string(),
                amount: token_rewards,
            })?,
            funds: vec![],
        },
    ];

    let share = token_rewards
        .checked_mul(PRECISION_MULTIPLIER)
        .unwrap()
        .checked_div(lp_supply.balance)
        .unwrap();
    // update pool info
    let updated_pool = PoolInfo {
        alloc_point: pool.alloc_point,
        last_reward_block: env.block.height,
        acc_per_share: pool.acc_per_share.checked_add(share).unwrap(),
        reward_proxy: pool.reward_proxy,
    };

    Ok((
        token_rewards.add(dev_token_rewards),
        Some(messages),
        Some(updated_pool),
    ))
}

// generates safe transfer msg: min(amount, astro_token amount)
fn safe_reward_transfer_message(
    deps: Deps,
    env: Env,
    cfg: Config,
    to: String,
    amount: Uint128,
    mint_rewards: Uint128, //need to be taken into account when calculating reward for safe transfer
) -> WasmMsg {
    let astro_balance: BalanceResponse = deps
        .querier
        .query_wasm_smart(
            cfg.astro_token.clone(),
            &cw20::Cw20QueryMsg::Balance {
                address: env.contract.address.to_string(),
            },
        )
        .unwrap();

    WasmMsg::Execute {
        contract_addr: cfg.astro_token.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Transfer {
            recipient: to,
            amount: min(amount, astro_balance.balance.add(mint_rewards)),
        })
        .unwrap(),
        funds: vec![],
    }
}

// Deposit LP tokens to MasterChef for ASTRO allocation.
pub fn deposit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    token: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut response = Response::new().add_attribute("Action", "Deposit");

    let mut user = USER_INFO
        .load(deps.storage, (&token, &info.sender))
        .unwrap_or_default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &token)?;

    let (mint_rewards, messages, updated_pool) = update_pool_rewards(
        deps.as_ref(),
        env.clone(),
        token.clone(),
        pool.clone(),
        cfg.clone(),
    )?;

    if let Some(msgs) = messages {
        for msg in msgs {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(msg),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }

    if let Some(p) = updated_pool {
        pool = p;
        POOL_INFO.save(deps.storage, &token, &pool)?;
    }
    //let mut pending = Uint128::zero();
    if user.amount > Uint128::zero() {
        let pending = user
            .amount
            .checked_mul(pool.acc_per_share)
            .unwrap()
            .checked_div(PRECISION_MULTIPLIER)
            .unwrap()
            .checked_sub(user.reward_debt)
            .unwrap();
        if !pending.is_zero() {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(safe_reward_transfer_message(
                    deps.as_ref(),
                    env.clone(),
                    cfg,
                    info.sender.to_string(),
                    pending,
                    mint_rewards,
                )),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }
    //call transfer function for lp token from: info.sender to: env.contract.address amount:_amount
    if !amount.is_zero() {
        response
            .messages
            .push(if let Some(receiver) = pool.reward_proxy {
                SubMsg {
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: token.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::SendFrom {
                            owner: info.sender.to_string(),
                            contract: receiver.to_string(),
                            msg: to_binary(&ProxyCw20HookMsg::Deposit {
                                account: info.sender.clone(),
                            })?,
                            amount,
                        })?,
                        funds: vec![],
                    }),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                }
            } else {
                SubMsg {
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: token.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                            owner: info.sender.to_string(),
                            recipient: env.contract.address.to_string(),
                            amount,
                        })?,
                        funds: vec![],
                    }),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                }
            })
    }
    //Change user balance
    user.amount = user.amount.checked_add(amount)?;
    if pool.acc_per_share > Uint128::zero() {
        user.reward_debt = user
            .amount
            .checked_mul(pool.acc_per_share)?
            .checked_div(PRECISION_MULTIPLIER)
            .map_err(|e| ContractError::Std(e.into()))?;
    };
    USER_INFO.save(deps.storage, (&token, &info.sender), &user)?;

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
    let mut response = Response::new().add_attribute("Action", "Withdraw");
    let user = USER_INFO.load(deps.storage, (&token, &info.sender))?;
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }
    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &token)?;
    let (mint_rewards, messages, updated_pool) = update_pool_rewards(
        deps.as_ref(),
        env.clone(),
        token.clone(),
        pool.clone(),
        cfg.clone(),
    )?;
    if let Some(msgs) = messages {
        for msg in msgs {
            response.messages.push(SubMsg {
                msg: CosmosMsg::Wasm(msg),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        }
    }
    if let Some(p) = updated_pool {
        pool = p;
        POOL_INFO.save(deps.storage, &token, &pool)?;
    }
    let pending = user
        .amount
        .checked_mul(pool.acc_per_share)
        .unwrap()
        .checked_div(PRECISION_MULTIPLIER)
        .unwrap()
        .checked_sub(user.reward_debt)
        .unwrap();
    if !pending.is_zero() {
        response.messages.push(SubMsg {
            msg: CosmosMsg::Wasm(safe_reward_transfer_message(
                deps.as_ref(),
                env,
                cfg,
                info.sender.to_string(),
                pending,
                mint_rewards,
            )),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }
    // Update user balance
    user.amount = user.amount.checked_sub(amount)?;
    user.reward_debt = user
        .amount
        .checked_mul(pool.acc_per_share)?
        .checked_div(PRECISION_MULTIPLIER)
        .map_err(StdError::from)?;
    USER_INFO.save(deps.storage, (&token, &info.sender), &user)?;

    // call to transfer function for lp token
    if !amount.is_zero() {
        response
            .messages
            .push(if let Some(proxy) = pool.reward_proxy {
                SubMsg {
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: proxy.to_string(),
                        msg: to_binary(&ProxyExecuteMsg::Withdraw {
                            account: info.sender,
                            amount,
                        })?,
                        funds: vec![],
                    }),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                }
            } else {
                SubMsg {
                    msg: CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: token.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::Transfer {
                            recipient: info.sender.to_string(),
                            amount,
                        })?,
                        funds: vec![],
                    }),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                }
            });
    }
    Ok(response)
}

// Withdraw without caring about rewards. EMERGENCY ONLY.
pub fn emergency_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token: Addr,
) -> Result<Response, ContractError> {
    let user = USER_INFO
        .load(deps.storage, (&token, &info.sender))
        .unwrap();

    let mut response = Response::new().add_attribute("Action", "EmergencyWithdraw");

    let pool = POOL_INFO.load(deps.storage, &token)?;

    //call to transfer function for lp token
    response
        .messages
        .push(if let Some(proxy) = &pool.reward_proxy {
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: proxy.to_string(),
                    msg: to_binary(&ProxyExecuteMsg::EmergencyWithdraw {
                        account: info.sender.clone(),
                    })?,
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            }
        } else {
            SubMsg {
                msg: CosmosMsg::Wasm(WasmMsg::Execute {
                    contract_addr: token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: info.sender.to_string(),
                        amount: user.amount,
                    })?,
                    funds: vec![],
                }),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            }
        });
    // Change user balance
    USER_INFO.remove(deps.storage, (&token, &info.sender));
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

fn set_allowed_reward_proxies(deps: DepsMut, proxies: Vec<String>) -> StdResult<Response> {
    let mut allowed_reward_proxies: Vec<Addr> = vec![];
    for proxy in proxies {
        allowed_reward_proxies.push(deps.api.addr_validate(&proxy)?);
    }

    CONFIG.update::<_, StdError>(deps.storage, |mut v| {
        v.allowed_reward_proxies = allowed_reward_proxies;
        Ok(v)
    })?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PoolLength {} => to_binary(&pool_length(deps)?),
        QueryMsg::PendingToken { token, user } => {
            to_binary(&pending_token(deps, env, token, user)?)
        }
        QueryMsg::GetMultiplier { from, to } => {
            let cfg = CONFIG.load(deps.storage)?;
            to_binary(&get_multiplier(from, to, cfg.bonus_end_block))
        }
    }
}

pub fn pool_length(deps: Deps) -> StdResult<PoolLengthResponse> {
    let _length = POOL_INFO
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();
    Ok(PoolLengthResponse { length: _length })
}

// Return reward multiplier over the given _from to _to block.
fn get_multiplier(from: u64, to: u64, bonus_end_block: u64) -> GetMultiplierResponse {
    let reward: u64;
    if to <= bonus_end_block {
        reward = to
            .checked_sub(from)
            .unwrap()
            .checked_mul(BONUS_MULTIPLIER)
            .unwrap();
    } else if from >= bonus_end_block {
        reward = to.checked_sub(from).unwrap();
    } else {
        reward = bonus_end_block
            .checked_sub(from)
            .unwrap()
            .checked_mul(BONUS_MULTIPLIER)
            .unwrap()
            .checked_add(to.checked_sub(bonus_end_block).unwrap())
            .unwrap();
    }
    GetMultiplierResponse { multiplier: reward }
}

// View function to see pending ASTRO on frontend.
pub fn pending_token(
    deps: Deps,
    env: Env,
    token: Addr,
    user: Addr,
) -> StdResult<PendingTokenResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &token)?;
    let user_info = USER_INFO.load(deps.storage, (&token, &user))?;
    let mut acc_per_share = pool.acc_per_share;

    let lp_supply: BalanceResponse = deps.querier.query_wasm_smart(
        token,
        &cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;
    if env.block.height > pool.last_reward_block && !lp_supply.balance.is_zero() {
        let token_rewards = calculate_rewards(env, pool, cfg)?;
        acc_per_share = acc_per_share
            .checked_add(
                token_rewards
                    .checked_mul(PRECISION_MULTIPLIER)
                    .unwrap()
                    .checked_div(lp_supply.balance)
                    .unwrap(),
            )
            .unwrap();
    }
    let pending_amount = user_info
        .amount
        .checked_mul(acc_per_share)
        .unwrap()
        .checked_div(PRECISION_MULTIPLIER)
        .unwrap()
        .checked_sub(user_info.reward_debt)
        .unwrap();
    Ok(PendingTokenResponse {
        pending: pending_amount,
    })
}

pub fn calculate_rewards(env: Env, pool: PoolInfo, cfg: Config) -> StdResult<Uint128> {
    let m = get_multiplier(
        pool.last_reward_block,
        env.block.height,
        cfg.bonus_end_block,
    );

    let r = Uint128::from(m.multiplier)
        .checked_mul(cfg.tokens_per_block)?
        .checked_mul(Uint128::from(pool.alloc_point))?
        .checked_div(Uint128::from(cfg.total_alloc_point))?;

    Ok(r)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
