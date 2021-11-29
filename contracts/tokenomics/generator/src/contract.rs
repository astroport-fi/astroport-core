use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo,
    Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, Uint64, WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20ReceiveMsg};

use crate::error::ContractError;
use crate::state::{
    Config, ExecuteOnReply, PoolInfo, CONFIG, POOL_INFO, TMP_USER_ACTION, USER_INFO,
};
use astroport::asset::addr_validate_to_lower;
use astroport::{
    generator::{
        ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingTokenResponse,
        PoolLengthResponse, QueryMsg, RewardInfoResponse,
    },
    generator_proxy::{
        Cw20HookMsg as ProxyCw20HookMsg, ExecuteMsg as ProxyExecuteMsg, QueryMsg as ProxyQueryMsg,
    },
    vesting::ExecuteMsg as VestingExecuteMsg,
};
use cw2::set_contract_version;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-generator";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut allowed_reward_proxies: Vec<Addr> = vec![];
    for proxy in msg.allowed_reward_proxies {
        allowed_reward_proxies.push(addr_validate_to_lower(deps.api, &proxy)?);
    }

    let config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: Uint64::from(0u64),
        start_block: msg.start_block,
        allowed_reward_proxies,
        vesting_contract: addr_validate_to_lower(deps.api, &msg.vesting_contract)?,
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
        ExecuteMsg::UpdateConfig {
            owner,
            tokens_per_block,
            vesting_contract,
        } => execute_update_config(deps, info, owner, tokens_per_block, vesting_contract),
        ExecuteMsg::Add {
            lp_token,
            alloc_point,
            reward_proxy,
        } => {
            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.owner {
                return Err(ContractError::Unauthorized {});
            }

            update_rewards_and_execute(
                deps,
                env,
                None,
                ExecuteOnReply::Add {
                    lp_token,
                    alloc_point,
                    reward_proxy,
                },
            )
        }
        ExecuteMsg::Set {
            lp_token,
            alloc_point,
        } => {
            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.owner {
                return Err(ContractError::Unauthorized {});
            }

            update_rewards_and_execute(
                deps,
                env,
                None,
                ExecuteOnReply::Set {
                    lp_token,
                    alloc_point,
                },
            )
        }
        ExecuteMsg::MassUpdatePools {} => {
            update_rewards_and_execute(deps, env, None, ExecuteOnReply::MassUpdatePools {})
        }
        ExecuteMsg::UpdatePool { lp_token } => update_rewards_and_execute(
            deps,
            env,
            Some(lp_token.clone()),
            ExecuteOnReply::UpdatePool { lp_token },
        ),
        ExecuteMsg::Withdraw { lp_token, amount } => update_rewards_and_execute(
            deps,
            env,
            Some(lp_token.clone()),
            ExecuteOnReply::Withdraw {
                lp_token,
                account: info.sender,
                amount,
            },
        ),
        ExecuteMsg::EmergencyWithdraw { lp_token } => emergency_withdraw(deps, env, info, lp_token),
        ExecuteMsg::SetAllowedRewardProxies { proxies } => {
            set_allowed_reward_proxies(deps, info, proxies)
        }
        ExecuteMsg::SendOrphanProxyReward {
            recipient,
            lp_token,
        } => send_orphan_proxy_rewards(deps, info, recipient, lp_token),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
    }
}

// Only owner can execute it
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
    tokens_per_block: Option<Uint128>,
    vesting_contract: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(owner) = owner {
        config.owner = addr_validate_to_lower(deps.api, owner.as_str())?;
    }

    if let Some(tokens_per_block) = tokens_per_block {
        config.tokens_per_block = tokens_per_block;
    }

    if let Some(vesting_contract) = vesting_contract {
        config.vesting_contract = addr_validate_to_lower(deps.api, vesting_contract.as_str())?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

// Add a new lp to the pool. Can only be called by the owner.
pub fn add(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    alloc_point: Uint64,
    reward_proxy: Option<String>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    if POOL_INFO.load(deps.storage, &lp_token).is_ok() {
        return Err(ContractError::TokenPoolAlreadyExists {});
    }

    let reward_proxy = reward_proxy
        .map(|v| addr_validate_to_lower(deps.api, &v))
        .transpose()?;

    if let Some(proxy) = &reward_proxy {
        if !cfg.allowed_reward_proxies.contains(proxy) {
            return Err(ContractError::RewardProxyNotAllowed {});
        }
    }

    mass_update_pools(deps.branch(), env.clone())?;

    cfg.total_alloc_point = cfg.total_alloc_point.checked_add(alloc_point)?;

    let pool_info = PoolInfo {
        alloc_point,
        last_reward_block: (cfg.start_block).max(Uint64::from(env.block.height)),
        acc_per_share: Decimal::zero(),
        reward_proxy,
        acc_per_share_on_proxy: Decimal::zero(),
        proxy_reward_balance_before_update: Uint128::zero(),
        orphan_proxy_rewards: Uint128::zero(),
    };

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::new()
        .add_attribute("action", "add_pool")
        .add_attribute("lp_token", lp_token))
}

// Update the given pool's ASTRO allocation point. Can only be called by the owner.
pub fn set(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    alloc_point: Uint64,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;

    mass_update_pools(deps.branch(), env)?;

    cfg.total_alloc_point = cfg
        .total_alloc_point
        .checked_sub(pool_info.alloc_point)?
        .checked_add(alloc_point)?;
    pool_info.alloc_point = alloc_point;

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::new()
        .add_attribute("action", "set_pool")
        .add_attribute("lp_token", lp_token.clone()))
}

fn update_rewards_and_execute(
    mut deps: DepsMut,
    env: Env,
    update_single_pool: Option<Addr>,
    on_reply: ExecuteOnReply,
) -> Result<Response, ContractError> {
    TMP_USER_ACTION.update(deps.storage, |v| {
        if v.is_some() {
            Err(StdError::generic_err("Repetitive reply definition!"))
        } else {
            Ok(Some(on_reply))
        }
    })?;

    let mut messages: Vec<SubMsg> = vec![];

    match update_single_pool {
        Some(lp_token) => {
            let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

            let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
            if let Some(reward_proxy) = pool.reward_proxy.clone() {
                messages.append(&mut get_pool_rewards_from_proxy(
                    deps.branch(),
                    &lp_token,
                    &mut pool,
                    &reward_proxy,
                )?);
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
                if let Some(reward_proxy) = pool.reward_proxy.clone() {
                    messages.append(&mut get_pool_rewards_from_proxy(
                        deps.branch(),
                        &lp_token,
                        &mut pool,
                        &reward_proxy,
                    )?);
                }
            }
        }
    }

    if let Some(last) = messages.last_mut() {
        last.reply_on = ReplyOn::Success;
        Ok(Response::new().add_submessages(messages))
    } else {
        process_after_update(deps, env)
    }
}

fn get_pool_rewards_from_proxy(
    deps: DepsMut,
    lp_token: &Addr,
    pool: &mut PoolInfo,
    reward_proxy: &Addr,
) -> Result<Vec<SubMsg>, ContractError> {
    let reward_amount: Uint128 = deps
        .querier
        .query_wasm_smart(reward_proxy, &ProxyQueryMsg::Reward {})?;

    pool.proxy_reward_balance_before_update = reward_amount;
    POOL_INFO.save(deps.storage, lp_token, pool)?;

    let msg = ProxyQueryMsg::PendingToken {};
    let res: Uint128 = deps.querier.query_wasm_smart(reward_proxy, &msg)?;

    Ok(if !res.is_zero() {
        vec![SubMsg::new(WasmMsg::Execute {
            contract_addr: reward_proxy.to_string(),
            funds: vec![],
            msg: to_binary(&ProxyExecuteMsg::UpdateRewards {})?,
        })]
    } else {
        vec![]
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    process_after_update(deps, env)
}

fn process_after_update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    match TMP_USER_ACTION.load(deps.storage)? {
        Some(action) => {
            TMP_USER_ACTION.save(deps.storage, &None)?;
            match action {
                ExecuteOnReply::MassUpdatePools {} => mass_update_pools(deps, env),
                ExecuteOnReply::Add {
                    lp_token,
                    alloc_point,
                    reward_proxy,
                } => add(deps, env, lp_token, alloc_point, reward_proxy),
                ExecuteOnReply::Set {
                    lp_token,
                    alloc_point,
                } => set(deps, env, lp_token, alloc_point),
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
        update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;
        POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    }
    Ok(response.add_attribute("action", "mass_update_pools"))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool(mut deps: DepsMut, env: Env, lp_token: Addr) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(response.add_attribute("action", "update_pool"))
}

// Update reward variables of the given pool to be up-to-date.
pub fn update_pool_rewards(
    deps: DepsMut,
    env: &Env,
    lp_token: &Addr,
    pool: &mut PoolInfo,
    cfg: &Config,
    deposited: Option<Uint128>,
) -> StdResult<()> {
    let lp_supply: Uint128;

    match &pool.reward_proxy {
        Some(proxy) => {
            lp_supply = deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

            if !lp_supply.is_zero() {
                let reward_amount: Uint128 = deps
                    .querier
                    .query_wasm_smart(proxy, &ProxyQueryMsg::Reward {})?;

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

            if let Some(amount) = deposited {
                lp_supply = res.balance - amount;
            } else {
                lp_supply = res.balance;
            }
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

fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let amount = cw20_msg.amount;
    let lp_token = info.sender;

    if POOL_INFO.load(deps.storage, &lp_token).is_err() {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => update_rewards_and_execute(
            deps,
            env,
            Some(lp_token.clone()),
            ExecuteOnReply::Deposit {
                lp_token,
                account: Addr::unchecked(cw20_msg.sender),
                amount,
            },
        ),
        Cw20HookMsg::DepositFor(beneficiary) => update_rewards_and_execute(
            deps,
            env,
            Some(lp_token.clone()),
            ExecuteOnReply::Deposit {
                lp_token,
                account: beneficiary,
                amount,
            },
        ),
    }
}

// Deposit LP tokens to MasterChef for ASTRO allocation.
pub fn deposit(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    beneficiary: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;
    let beneficiary = addr_validate_to_lower(deps.api, beneficiary.as_str())?;

    let mut response = Response::new().add_attribute("Action", "Deposit");

    let mut user = USER_INFO
        .load(deps.storage, (&lp_token, &beneficiary))
        .unwrap_or_default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    update_pool_rewards(
        deps.branch(),
        &env,
        &lp_token,
        &mut pool,
        &cfg,
        Some(amount),
    )?;

    if !user.amount.is_zero() {
        let pending = (user.amount * pool.acc_per_share).checked_sub(user.reward_debt)?;
        if !pending.is_zero() {
            response.messages.push(SubMsg::new(WasmMsg::Execute {
                contract_addr: cfg.vesting_contract.to_string(),
                msg: to_binary(&VestingExecuteMsg::Claim {
                    recipient: Some(beneficiary.to_string()),
                    amount: Some(pending),
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
                        account: beneficiary.clone(),
                        amount: pending_on_proxy,
                    })?,
                }));
            }
        }
    }

    //call transfer function for lp token from: info.sender to: env.contract.address amount:_amount
    if !amount.is_zero() && pool.reward_proxy.is_some() {
        response.messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pool.reward_proxy.clone().unwrap().to_string(),
                msg: to_binary(&ProxyCw20HookMsg::Deposit {})?,
                amount,
            })?,
            funds: vec![],
        }));
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
    USER_INFO.save(deps.storage, (&lp_token, &beneficiary), &user)?;

    Ok(response
        .add_attribute("action", "deposit")
        .add_attribute("amount", amount))
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

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    let mut user = USER_INFO
        .load(deps.storage, (&lp_token, &account))
        .unwrap_or_default();
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }
    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    update_pool_rewards(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;

    let pending = (user.amount * pool.acc_per_share).checked_sub(user.reward_debt)?;
    if !pending.is_zero() {
        response.messages.push(SubMsg::new(WasmMsg::Execute {
            contract_addr: cfg.vesting_contract.to_string(),
            msg: to_binary(&VestingExecuteMsg::Claim {
                recipient: Some(account.to_string()),
                amount: Some(pending),
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

    Ok(response
        .add_attribute("action", "withdraw")
        .add_attribute("amount", amount))
}

// Withdraw without caring about rewards. EMERGENCY ONLY.
pub fn emergency_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: Addr,
) -> Result<Response, ContractError> {
    let mut response = Response::new().add_attribute("Action", "EmergencyWithdraw");

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let user = USER_INFO.load(deps.storage, (&lp_token, &info.sender))?;

    pool.orphan_proxy_rewards +=
        (user.amount * pool.acc_per_share_on_proxy).saturating_sub(user.reward_debt_proxy);

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
    POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    Ok(response
        .add_attribute("action", "emergency_withdraw")
        .add_attribute("amount", user.amount))
}

fn set_allowed_reward_proxies(
    deps: DepsMut,
    info: MessageInfo,
    proxies: Vec<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut allowed_reward_proxies: Vec<Addr> = vec![];
    for proxy in proxies {
        allowed_reward_proxies.push(addr_validate_to_lower(deps.api, &proxy)?);
    }

    CONFIG.update::<_, StdError>(deps.storage, |mut v| {
        v.allowed_reward_proxies = allowed_reward_proxies;
        Ok(v)
    })?;
    Ok(Response::new().add_attribute("action", "set_allowed_reward_proxies"))
}

fn send_orphan_proxy_rewards(
    deps: DepsMut,
    info: MessageInfo,
    recipient: String,
    lp_token: String,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    };

    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

    let recipient = addr_validate_to_lower(deps.api, &recipient)?;

    let mut response = Response::new();

    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let proxy = match &pool.reward_proxy {
        Some(proxy) => proxy.clone(),
        None => return Err(ContractError::PoolDoesNotHaveAdditionalRewards {}),
    };

    let amount = pool.orphan_proxy_rewards;
    pool.orphan_proxy_rewards = Uint128::zero();
    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    if amount.is_zero() {
        return Err(ContractError::OrphanRewardsTooSmall {});
    }

    let msg = ProxyExecuteMsg::SendRewards {
        account: recipient.clone(),
        amount,
    };

    response.messages.push(SubMsg::new(WasmMsg::Execute {
        contract_addr: proxy.to_string(),
        funds: vec![],
        msg: to_binary(&msg)?,
    }));

    Ok(response
        .add_attribute("action", "send_orphan_rewards")
        .add_attribute("recipient", recipient.to_string())
        .add_attribute("lp_token", lp_token)
        .add_attribute("amount", amount))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::PoolLength {} => Ok(to_binary(&pool_length(deps)?)?),
        QueryMsg::Deposit { lp_token, user } => {
            Ok(to_binary(&query_deposit(deps, lp_token, user)?)?)
        }
        QueryMsg::PendingToken { lp_token, user } => {
            Ok(to_binary(&pending_token(deps, env, lp_token, user)?)?)
        }
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::RewardInfo { lp_token } => Ok(to_binary(&query_reward_info(deps, lp_token)?)?),
        QueryMsg::OrphanProxyRewards { lp_token } => {
            Ok(to_binary(&query_orphan_proxy_rewards(deps, lp_token)?)?)
        }
    }
}

pub fn pool_length(deps: Deps) -> Result<PoolLengthResponse, ContractError> {
    let length = POOL_INFO
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();
    Ok(PoolLengthResponse { length })
}

pub fn query_deposit(deps: Deps, lp_token: Addr, user: Addr) -> Result<Uint128, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;
    let user = addr_validate_to_lower(deps.api, user.as_str())?;

    let user_info = USER_INFO
        .load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();
    Ok(user_info.amount)
}

// View function to see pending ASTRO on frontend.
pub fn pending_token(
    deps: Deps,
    env: Env,
    lp_token: Addr,
    user: Addr,
) -> Result<PendingTokenResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;
    let user = addr_validate_to_lower(deps.api, user.as_str())?;

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

fn query_config(deps: Deps) -> Result<ConfigResponse, ContractError> {
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

fn query_reward_info(deps: Deps, lp_token: Addr) -> Result<RewardInfoResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    let pool = POOL_INFO.load(deps.storage, &lp_token)?;

    let proxy_reward_token = match pool.reward_proxy {
        Some(proxy) => {
            let res: Addr = deps
                .querier
                .query_wasm_smart(&proxy, &ProxyQueryMsg::RewardInfo {})?;
            Some(res)
        }
        None => None,
    };

    Ok(RewardInfoResponse {
        base_reward_token: config.astro_token,
        proxy_reward_token,
    })
}

fn query_orphan_proxy_rewards(deps: Deps, lp_token: Addr) -> Result<Uint128, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, lp_token.as_str())?;

    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    if pool.reward_proxy.is_none() {
        return Err(ContractError::PoolDoesNotHaveAdditionalRewards {});
    }

    Ok(pool.orphan_proxy_rewards)
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
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Ok(Response::default())
}
