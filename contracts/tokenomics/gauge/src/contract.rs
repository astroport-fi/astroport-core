use cosmwasm_std::{
    entry_point, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, Event, MessageInfo, Reply,
    Response, StdError, StdResult, SubMsg, Uint128, Uint64, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg};

use crate::error::ContractError;
use crate::state::{
    Config, ExecuteOnReply, PoolInfo, CONFIG, POOL_INFO, TMP_USER_ACTION, USER_INFO,
};
use astroport::{
    gauge::{
        ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingTokenResponse,
        PoolLengthResponse, QueryMsg,
    },
    vesting::ExecuteMsg as VestingExecuteMsg,
};
use gauge_proxy_interface::msg::{
    Cw20HookMsg as ProxyCw20HookMsg, ExecuteMsg as ProxyExecuteMsg, QueryMsg as ProxyQueryMsg,
};

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
        astro_token: deps.api.addr_validate(&msg.astro_token)?,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: Uint64::from(0u64),
        owner: info.sender,
        start_block: msg.start_block,
        allowed_reward_proxies,
        vesting_contract: deps.api.addr_validate(&msg.vesting_contract)?,
    };
    CONFIG.save(deps.storage, &config)?;

    TMP_USER_ACTION.save(deps.storage, &None)?;

    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Add {
            lp_token,
            alloc_point,
            with_update,
            reward_proxy,
        } => add(
            deps,
            env,
            info,
            lp_token,
            alloc_point,
            with_update,
            reward_proxy,
        ),
        ExecuteMsg::Set {
            lp_token,
            alloc_point,
            with_update,
        } => set(deps, env, info, lp_token, alloc_point, with_update),
        ExecuteMsg::MassUpdatePools {} => {
            update_rewards_and_execute(deps, None, ExecuteOnReply::MassUpdatePools {})
        }
        ExecuteMsg::UpdatePool { lp_token } => update_rewards_and_execute(
            deps,
            Some(lp_token.clone()),
            ExecuteOnReply::UpdatePool { lp_token },
        ),
        ExecuteMsg::Deposit { lp_token, amount } => update_rewards_and_execute(
            deps,
            Some(lp_token.clone()),
            ExecuteOnReply::Deposit {
                lp_token,
                account: info.sender,
                amount,
            },
        ),
        ExecuteMsg::Withdraw { lp_token, amount } => update_rewards_and_execute(
            deps,
            Some(lp_token.clone()),
            ExecuteOnReply::Withdraw {
                lp_token,
                account: info.sender,
                amount,
            },
        ),
        ExecuteMsg::EmergencyWithdraw { lp_token } => emergency_withdraw(deps, env, info, lp_token),
        ExecuteMsg::SetAllowedRewardProxies { proxies } => {
            Ok(set_allowed_reward_proxies(deps, proxies)?)
        }
        ExecuteMsg::SendOrphanReward {
            recipient,
            lp_token,
            amount,
        } => before_send_orphan_rewards(deps, info, recipient, lp_token, amount),
        ExecuteMsg::SetTokensPerBlock { amount } => set_tokens_per_block(deps, amount),
    }
}

// Add a new lp to the pool. Can only be called by the owner.
pub fn add(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: Addr,
    alloc_point: Uint64,
    with_update: bool,
    reward_proxy: Option<String>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if POOL_INFO.load(deps.storage, &lp_token).is_ok() {
        return Err(ContractError::TokenPoolAlreadyExists {});
    }

    let reward_proxy = reward_proxy
        .map(|v| deps.api.addr_validate(&v))
        .transpose()?;

    if let Some(proxy) = &reward_proxy {
        if !cfg.allowed_reward_proxies.contains(proxy) {
            return Err(ContractError::RewardProxyNotAllowed {});
        }
    }

    cfg.total_alloc_point = cfg.total_alloc_point.checked_add(alloc_point)?;

    let pool_info = PoolInfo {
        alloc_point,
        last_reward_block: (cfg.start_block).max(Uint64::from(env.block.height)),
        acc_per_share: Decimal::zero(),
        reward_proxy,
        acc_per_share_on_proxy: Decimal::zero(),
        proxy_reward_balance_before_update: Uint128::zero(),
    };

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    let mut response = Response::new()
        .add_event(Event::new(String::from("Added pool")).add_attribute("lp_token", lp_token));

    if with_update {
        response.messages.append(
            &mut update_rewards_and_execute(deps, None, ExecuteOnReply::MassUpdatePools {})?
                .messages,
        );
    }

    Ok(response)
}

// Update the given pool's ASTRO allocation point. Can only be called by the owner.
pub fn set(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
    alloc_point: Uint64,
    with_update: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;

    cfg.total_alloc_point = cfg
        .total_alloc_point
        .checked_sub(pool_info.alloc_point)?
        .checked_add(alloc_point)?;
    pool_info.alloc_point = alloc_point;

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    let mut response = Response::new().add_event(
        Event::new(String::from("Set pool")).add_attribute("lp_token", lp_token.clone()),
    );

    if with_update {
        response.messages.append(
            &mut update_rewards_and_execute(
                deps,
                Some(lp_token.clone()),
                ExecuteOnReply::UpdatePool { lp_token },
            )?
            .messages,
        )
    }

    Ok(response)
}

fn update_rewards_and_execute(
    deps: DepsMut,
    only_lp_token: Option<Addr>,
    on_reply: ExecuteOnReply,
) -> Result<Response, ContractError> {
    TMP_USER_ACTION.update(deps.storage, |v| {
        if v.is_some() {
            Err(StdError::GenericErr {
                msg: String::from("Repeated reply definition!"),
            })
        } else {
            Ok(Some(on_reply))
        }
    })?;

    let mut response = Response::default();

    match only_lp_token {
        Some(lp_token) => {
            let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
            if let Some(reward_proxy) = &pool.reward_proxy {
                let reward_amount: Uint128 = deps
                    .querier
                    .query_wasm_smart(reward_proxy, &ProxyQueryMsg::Reward {})?;

                pool.proxy_reward_balance_before_update = reward_amount;
                POOL_INFO.save(deps.storage, &lp_token, &pool)?;

                let msg = ProxyQueryMsg::PendingToken {};
                let res: Option<Uint128> = deps.querier.query_wasm_smart(reward_proxy, &msg)?;

                let update_rewards = if let Some(amount) = res {
                    !amount.is_zero()
                } else {
                    true
                };

                if update_rewards {
                    response.messages.push(SubMsg::new(WasmMsg::Execute {
                        contract_addr: reward_proxy.to_string(),
                        funds: vec![],
                        msg: to_binary(&ProxyExecuteMsg::UpdateRewards {})?,
                    }));
                }
            }
        }
        None => {
            let pools: Vec<(Addr, PoolInfo)> = POOL_INFO
                .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
                .filter_map(|v| {
                    v.ok()
                        .map(|v| (Addr::unchecked(String::from_utf8(v.0).unwrap()), v.1))
                })
                .collect();
            for (lp_token, mut pool) in pools {
                if let Some(reward_proxy) = &pool.reward_proxy {
                    let reward_amount: Uint128 = deps
                        .querier
                        .query_wasm_smart(reward_proxy, &ProxyQueryMsg::Reward {})?;

                    pool.proxy_reward_balance_before_update = reward_amount;
                    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

                    let msg = ProxyQueryMsg::PendingToken {};
                    let res: Option<Uint128> = deps.querier.query_wasm_smart(reward_proxy, &msg)?;

                    let update_rewards = if let Some(amount) = res {
                        !amount.is_zero()
                    } else {
                        true
                    };

                    if update_rewards {
                        response.messages.push(SubMsg::new(WasmMsg::Execute {
                            contract_addr: reward_proxy.to_string(),
                            funds: vec![],
                            msg: to_binary(&ProxyExecuteMsg::UpdateRewards {})?,
                        }));
                    }
                }
            }
        }
    }

    let cfg = CONFIG.load(deps.storage)?;
    response.messages.push(SubMsg::reply_on_success(
        WasmMsg::Execute {
            contract_addr: cfg.vesting_contract.to_string(),
            funds: vec![],
            msg: to_binary(&VestingExecuteMsg::Claim {})?,
        },
        0,
    ));

    Ok(response)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    match TMP_USER_ACTION.load(deps.storage)? {
        Some(action) => {
            TMP_USER_ACTION.save(deps.storage, &None)?;
            match action {
                ExecuteOnReply::MassUpdatePools {} => mass_update_pools(deps, env),
                ExecuteOnReply::UpdatePool { lp_token } => update_pool(deps, env, lp_token),
                ExecuteOnReply::Deposit {
                    lp_token,
                    account,
                    amount,
                } => deposit(deps, env, lp_token, account, amount),
                ExecuteOnReply::Withdraw {
                    lp_token,
                    account,
                    amount,
                } => withdraw(deps, env, lp_token, account, amount),
                ExecuteOnReply::SendOrphanReward {
                    recipient,
                    lp_token,
                    amount,
                } => send_orphan_rewards(deps, env, recipient, lp_token, amount),
            }
        }
        None => Ok(Response::default()),
    }
}

// Update reward variables for all pools.
pub fn mass_update_pools(mut deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
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
    for (lp_token, mut pool) in pools {
        update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;
        POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    }
    Ok(response.add_event(Event::new(String::from("Mass update pools"))))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool(mut deps: DepsMut, env: Env, lp_token: Addr) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(response.add_event(Event::new(String::from("Updated pool"))))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool_rewards(
    deps: DepsMut,
    env: &Env,
    lp_token: &Addr,
    pool: &mut PoolInfo,
    cfg: &Config,
) -> StdResult<()> {
    let lp_supply: Uint128;

    match &pool.reward_proxy {
        Some(proxy) => {
            lp_supply = deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

            let reward_amount: Uint128 = deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Reward {})?;

            if !lp_supply.is_zero() {
                let token_rewards =
                    reward_amount.checked_sub(pool.proxy_reward_balance_before_update)?;

                let share = Decimal::from_ratio(token_rewards, lp_supply);
                pool.acc_per_share_on_proxy = pool.acc_per_share_on_proxy + share;
            }
        }
        None => {
            let res: BalanceResponse = deps.querier.query_wasm_smart(
                lp_token,
                &cw20::Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            lp_supply = res.balance;
        }
    };

    if env.block.height > pool.last_reward_block.u64() {
        if !lp_supply.is_zero() {
            let token_rewards = calculate_rewards(env, pool, cfg)?;

            let share = Decimal::from_ratio(token_rewards, lp_supply);
            pool.acc_per_share = pool.acc_per_share + share;
        }

        pool.last_reward_block = Uint64::from(env.block.height);
    }

    Ok(())
}

// Deposit LP tokens to MasterChef for ASTRO allocation.
pub fn deposit(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    account: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut response = Response::new().add_attribute("Action", "Deposit");

    let mut user = USER_INFO
        .load(deps.storage, (&lp_token, &account))
        .unwrap_or_default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;

    if !user.amount.is_zero() {
        let pending = (user.amount * pool.acc_per_share).checked_sub(user.reward_debt)?;
        if !pending.is_zero() {
            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: cfg.astro_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: account.to_string(),
                    amount: pending,
                })?,
                funds: vec![],
            }));
        }
        if let Some(proxy) = &pool.reward_proxy {
            let pending_on_proxy =
                (user.amount * pool.acc_per_share_on_proxy).checked_sub(user.reward_debt_proxy)?;
            if !pending_on_proxy.is_zero() {
                response.messages.push(SubMsg::new(WasmMsg::Execute {
                    contract_addr: proxy.to_string(),
                    funds: vec![],
                    msg: to_binary(&ProxyExecuteMsg::SendRewards {
                        account: account.clone(),
                        amount: pending_on_proxy,
                    })?,
                }));
            }
        }
    }
    //call transfer function for lp token from: info.sender to: env.contract.address amount:_amount
    if !amount.is_zero() {
        match &pool.reward_proxy {
            Some(proxy) => {
                response.messages.push(SubMsg::new(WasmMsg::Execute {
                    contract_addr: lp_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::SendFrom {
                        owner: account.to_string(),
                        contract: proxy.to_string(),
                        msg: to_binary(&ProxyCw20HookMsg::Deposit {})?,
                        amount,
                    })?,
                    funds: vec![],
                }));
            }
            None => {
                response.messages.push(SubMsg::new(WasmMsg::Execute {
                    contract_addr: lp_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: account.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount,
                    })?,
                    funds: vec![],
                }));
            }
        }
    }
    //Change user balance
    user.amount = user.amount.checked_add(amount)?;
    if !pool.acc_per_share.is_zero() {
        user.reward_debt = user.amount * pool.acc_per_share;
    };
    if !pool.acc_per_share_on_proxy.is_zero() {
        user.reward_debt_proxy = user.amount * pool.acc_per_share_on_proxy;
    };

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    USER_INFO.save(deps.storage, (&lp_token, &account), &user)?;

    Ok(response.add_event(Event::new(String::from("Deposit")).add_attribute("amount", amount)))
}

// Withdraw LP tokens from MasterChef.
pub fn withdraw(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    account: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut response = Response::new().add_attribute("Action", "Withdraw");
    let mut user = USER_INFO
        .load(deps.storage, (&lp_token, &account))
        .unwrap_or_default();
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }
    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;

    let pending = (user.amount * pool.acc_per_share).checked_sub(user.reward_debt)?;
    if !pending.is_zero() {
        response.messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.astro_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: account.to_string(),
                amount: pending,
            })?,
            funds: vec![],
        }));
    }

    if let Some(proxy) = &pool.reward_proxy {
        let pending_on_proxy =
            (user.amount * pool.acc_per_share_on_proxy).checked_sub(user.reward_debt_proxy)?;
        if !pending_on_proxy.is_zero() {
            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: proxy.to_string(),
                funds: vec![],
                msg: to_binary(&ProxyExecuteMsg::SendRewards {
                    account: account.clone(),
                    amount: pending_on_proxy,
                })?,
            }));
        }
    }

    // call to transfer function for lp token
    if !amount.is_zero() {
        match &pool.reward_proxy {
            Some(proxy) => {
                response.messages.push(SubMsg::new(WasmMsg::Execute {
                    contract_addr: proxy.to_string(),
                    funds: vec![],
                    msg: to_binary(&ProxyExecuteMsg::Withdraw {
                        account: account.clone(),
                        amount,
                    })?,
                }));
            }
            None => {
                response.messages.push(SubMsg::new(WasmMsg::Execute {
                    contract_addr: lp_token.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::Transfer {
                        recipient: account.to_string(),
                        amount,
                    })?,
                    funds: vec![],
                }));
            }
        };
    }

    // Update user balance
    user.amount = user.amount.checked_sub(amount)?;
    if !pool.acc_per_share.is_zero() {
        user.reward_debt = user.amount * pool.acc_per_share;
    }
    if !pool.acc_per_share_on_proxy.is_zero() {
        user.reward_debt_proxy = user.amount * pool.acc_per_share_on_proxy;
    }

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    if !user.amount.is_zero() {
        USER_INFO.save(deps.storage, (&lp_token, &account), &user)?;
    } else {
        USER_INFO.remove(deps.storage, (&lp_token, &account));
    }

    Ok(response.add_event(Event::new(String::from("Withdraw")).add_attribute("amount", amount)))
}

// Withdraw without caring about rewards. EMERGENCY ONLY.
pub fn emergency_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
) -> Result<Response, ContractError> {
    let mut response = Response::new().add_attribute("Action", "EmergencyWithdraw");

    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let user = USER_INFO.load(deps.storage, (&lp_token, &info.sender))?;

    //call to transfer function for lp token
    response
        .messages
        .push(if let Some(proxy) = &pool.reward_proxy {
            SubMsg::new(WasmMsg::Execute {
                contract_addr: proxy.to_string(),
                msg: to_binary(&ProxyExecuteMsg::EmergencyWithdraw {
                    account: info.sender.clone(),
                    amount: user.amount,
                })?,
                funds: vec![],
            })
        } else {
            SubMsg::new(WasmMsg::Execute {
                contract_addr: lp_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: info.sender.to_string(),
                    amount: user.amount,
                })?,
                funds: vec![],
            })
        });

    // Change user balance
    USER_INFO.remove(deps.storage, (&lp_token, &info.sender));
    Ok(response.add_event(
        Event::new(String::from("Emergency withdraw")).add_attribute("amount", user.amount),
    ))
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
    Ok(Response::new().add_event(Event::new(String::from("Set allowed reward proxies"))))
}

// lp_token:
//  None - for sending ASTRO
//  Some(lp_token) - fo sending additional reward from the pool
fn before_send_orphan_rewards(
    deps: DepsMut,
    info: MessageInfo,
    recipient: String,
    lp_token: Option<String>,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    };

    let lp_token = lp_token.map(|v| deps.api.addr_validate(&v)).transpose()?;

    let recipient = deps.api.addr_validate(&recipient)?;

    update_rewards_and_execute(
        deps,
        lp_token.clone(),
        ExecuteOnReply::SendOrphanReward {
            recipient,
            lp_token,
            amount,
        },
    )
}

fn send_orphan_rewards(
    mut deps: DepsMut,
    env: Env,
    recipient: Addr,
    lp_token: Option<Addr>,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let mut response = Response::new().add_event(
        Event::new("Sent orphan rewards")
            .add_attribute("recipient", recipient.to_string())
            .add_attribute(
                "lp_token",
                match &lp_token {
                    Some(s) => s.to_string(),
                    None => "None".to_string(),
                },
            )
            .add_attribute("amount", amount),
    );

    if let Some(lp_token) = lp_token {
        let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
        let proxy = match &pool.reward_proxy {
            Some(proxy) => proxy.clone(),
            None => return Err(ContractError::PoolDoesNotHaveAdditionalRewards {}),
        };

        update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;
        POOL_INFO.save(deps.storage, &lp_token, &pool)?;

        let msg = ProxyQueryMsg::Reward {};
        let balance: Uint128 = deps.querier.query_wasm_smart(proxy.to_string(), &msg)?;
        let mut to_distribute = Uint128::zero();

        for (_, user_info) in USER_INFO
            .prefix(&lp_token)
            .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .filter_map(|v| v.ok())
        {
            to_distribute = to_distribute.checked_add(
                (user_info.amount * pool.acc_per_share_on_proxy)
                    .checked_sub(user_info.reward_debt_proxy)?,
            )?;
        }

        if amount <= balance.checked_sub(to_distribute)? {
            let msg = ProxyExecuteMsg::SendRewards {
                account: recipient,
                amount,
            };

            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: proxy.to_string(),
                funds: vec![],
                msg: to_binary(&msg)?,
            }));
        } else {
            return Err(ContractError::OrphanRewardsTooSmall {});
        }
    } else {
        let msg = Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        };
        let res: BalanceResponse = deps
            .querier
            .query_wasm_smart(cfg.astro_token.to_string(), &msg)?;
        let mut to_distribute = Uint128::zero();

        let pools: Vec<(Addr, PoolInfo)> = POOL_INFO
            .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
            .filter_map(|v| {
                v.ok()
                    .map(|v| (Addr::unchecked(String::from_utf8(v.0).unwrap()), v.1))
            })
            .collect();

        for (lp_token, mut pool) in pools {
            update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg)?;
            POOL_INFO.save(deps.storage, &lp_token, &pool)?;

            for (_, user_info) in USER_INFO
                .prefix(&lp_token)
                .range(deps.storage, None, None, cosmwasm_std::Order::Ascending)
                .filter_map(|v| v.ok())
            {
                to_distribute = to_distribute.checked_add(
                    (user_info.amount * pool.acc_per_share).checked_sub(user_info.reward_debt)?,
                )?;
            }
        }
        if amount <= res.balance.checked_sub(to_distribute)? {
            let msg = Cw20ExecuteMsg::Transfer {
                recipient: recipient.to_string(),
                amount,
            };

            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: cfg.astro_token.to_string(),
                funds: vec![],
                msg: to_binary(&msg)?,
            }));
        } else {
            return Err(ContractError::OrphanRewardsTooSmall {});
        }
    }

    Ok(response)
}

fn set_tokens_per_block(deps: DepsMut, amount: Uint128) -> Result<Response, ContractError> {
    CONFIG.update::<_, ContractError>(deps.storage, |mut v| {
        v.tokens_per_block = amount;
        Ok(v)
    })?;
    Ok(Response::new()
        .add_event(Event::new("Set tokens per block").add_attribute("amount", amount)))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::PoolLength {} => to_binary(&pool_length(deps)?),
        QueryMsg::Deposit { lp_token, user } => to_binary(&query_deposit(deps, lp_token, user)),
        QueryMsg::PendingToken { lp_token, user } => {
            to_binary(&pending_token(deps, env, lp_token, user)?)
        }
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

pub fn pool_length(deps: Deps) -> StdResult<PoolLengthResponse> {
    let length = POOL_INFO
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();
    Ok(PoolLengthResponse { length })
}

pub fn query_deposit(deps: Deps, lp_token: Addr, user: Addr) -> Uint128 {
    let user_info = USER_INFO
        .load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();
    user_info.amount
}

// View function to see pending ASTRO on frontend.
pub fn pending_token(
    deps: Deps,
    env: Env,
    lp_token: Addr,
    user: Addr,
) -> StdResult<PendingTokenResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let user_info = USER_INFO
        .load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();

    let mut pending_on_proxy = None;

    let lp_supply: Uint128;

    match &pool.reward_proxy {
        Some(proxy) => {
            lp_supply = deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

            if !lp_supply.is_zero() {
                let res: Option<Uint128> = deps
                    .querier
                    .query_wasm_smart(proxy, &ProxyQueryMsg::PendingToken {})?;
                let mut acc_per_share_on_proxy = pool.acc_per_share_on_proxy;
                if let Some(token_rewards) = res {
                    let share = Decimal::from_ratio(token_rewards, lp_supply);
                    acc_per_share_on_proxy = pool.acc_per_share_on_proxy + share;
                }
                pending_on_proxy = Some(
                    (user_info.amount * acc_per_share_on_proxy)
                        .checked_sub(user_info.reward_debt_proxy)?,
                );
            }
        }
        None => {
            let res: BalanceResponse = deps.querier.query_wasm_smart(
                lp_token,
                &cw20::Cw20QueryMsg::Balance {
                    address: env.contract.address.to_string(),
                },
            )?;
            lp_supply = res.balance;
        }
    }

    let mut acc_per_share = pool.acc_per_share;
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        let token_rewards = calculate_rewards(&env, &pool, &cfg)?;
        let share = Decimal::from_ratio(token_rewards, lp_supply);
        acc_per_share = pool.acc_per_share + share;
    }
    let pending = (user_info.amount * acc_per_share).checked_sub(user_info.reward_debt)?;

    Ok(PendingTokenResponse {
        pending,
        pending_on_proxy,
    })
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        allowed_reward_proxies: config.allowed_reward_proxies,
        astro_token: config.astro_token,
        owner: config.owner,
        start_block: config.start_block,
        tokens_per_block: config.tokens_per_block,
        total_alloc_point: config.total_alloc_point,
        vesting_contract: config.vesting_contract,
    })
}

pub fn calculate_rewards(env: &Env, pool: &PoolInfo, cfg: &Config) -> StdResult<Uint128> {
    let n_blocks = Uint128::from(env.block.height).checked_sub(pool.last_reward_block.into())?;

    let r = n_blocks
        .checked_mul(cfg.tokens_per_block)?
        .checked_mul(Uint128::from(pool.alloc_point.u64()))?
        .checked_div(Uint128::from(cfg.total_alloc_point.u64()))?;

    Ok(r)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
