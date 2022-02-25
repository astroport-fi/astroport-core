use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, Uint64,
    WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, MinterResponse};

use crate::error::ContractError;
use crate::migration;
use crate::state::{
    get_pools, update_user_balance, Config, ExecuteOnReply, PoolInfo, UserInfo, CONFIG,
    DEFAULT_LIMIT, MAX_LIMIT, OWNERSHIP_PROPOSAL, POOL_INFO, TMP_USER_ACTION, USER_INFO,
};
use astroport::asset::addr_validate_to_lower;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::generator::StakerResponse;
use astroport::querier::{query_supply, query_token_balance};
use astroport::DecimalCheckedOps;
use astroport::{
    generator::{
        ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingTokenResponse,
        PoolInfoResponse, PoolLengthResponse, QueryMsg, RewardInfoResponse,
    },
    generator_proxy::{
        Cw20HookMsg as ProxyCw20HookMsg, ExecuteMsg as ProxyExecuteMsg, QueryMsg as ProxyQueryMsg,
    },
    vesting::ExecuteMsg as VestingExecuteMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::{Bound, PrimaryKey};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-generator";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`] struct.
/// Returns a default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters used for creating the contract.
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

/// ## Description
/// Exposes execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig { vesting_contract }** Changes the address of the Generator vesting contract.
///
/// * **ExecuteMsg::Add {
///             lp_token,
///             alloc_point,
///             reward_proxy,
///         }** Create a new generator and add it in the [`POOL_INFO`] mapping if it does not exist yet. This also updates the
/// total allocation point variable in the [`Config`] object.
///
/// * **ExecuteMsg::Set {
///             lp_token,
///             alloc_point,
///         }** Updates the given generator's ASTRO allocation points.
///
/// * **ExecuteMsg::MassUpdatePools {}** Updates reward variables (accrued rewards) for all generators.
///
/// * **ExecuteMsg::UpdatePool { lp_token }** Snapshots the given generator's state by computing the new amount of
/// rewards that were already distributed,
///
/// * **ExecuteMsg::Withdraw { lp_token, amount }** Withdraw LP tokens from the Generator.
///
/// * **ExecuteMsg::EmergencyWithdraw { lp_token }** Withdraw LP tokens without caring about reward claiming.
/// TO BE USED IN EMERGENCY SITUATIONS ONLY.
///
/// * **ExecuteMsg::SetAllowedRewardProxies { proxies }** Sets the list of allowed reward proxy contracts
/// that can interact with the Generator contract.
///
/// * **ExecuteMsg::SendOrphanProxyReward {
///             recipient,
///             lp_token,
///         }** Sends orphan proxy rewards to another address.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::SetTokensPerBlock { amount }** Sets a new amount of ASTRO that's distributed per block among all active generators.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
/// Only the current owner can call this.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership. Only the current owner can call this.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership. Only the newly proposed owner can call this.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::MoveToProxy { lp_token, proxy } => move_to_proxy(deps, env, lp_token, proxy),
        ExecuteMsg::UpdateAllowedProxies { add, remove } => {
            update_allowed_proxies(deps, add, remove)
        }
        ExecuteMsg::UpdateConfig { vesting_contract } => {
            execute_update_config(deps, info, vesting_contract)
        }
        ExecuteMsg::Add {
            lp_token,
            alloc_point,
            has_asset_rewards,
            reward_proxy,
        } => {
            let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.owner {
                return Err(ContractError::Unauthorized {});
            }

            // Check if LP token exists
            query_supply(&deps.querier, lp_token.clone())?;

            update_rewards_and_execute(
                deps,
                env,
                None,
                ExecuteOnReply::Add {
                    lp_token,
                    alloc_point,
                    has_asset_rewards,
                    reward_proxy,
                },
            )
        }
        ExecuteMsg::Set {
            lp_token,
            alloc_point,
            has_asset_rewards,
        } => {
            let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

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
                    has_asset_rewards,
                },
            )
        }
        ExecuteMsg::MassUpdatePools {} => {
            update_rewards_and_execute(deps, env, None, ExecuteOnReply::MassUpdatePools {})
        }
        ExecuteMsg::UpdatePool { lp_token } => {
            let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

            update_rewards_and_execute(
                deps,
                env,
                Some(lp_token.clone()),
                ExecuteOnReply::UpdatePool { lp_token },
            )
        }
        ExecuteMsg::Withdraw { lp_token, amount } => {
            let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

            update_rewards_and_execute(
                deps,
                env,
                Some(lp_token.clone()),
                ExecuteOnReply::Withdraw {
                    lp_token,
                    account: info.sender,
                    amount,
                },
            )
        }
        ExecuteMsg::EmergencyWithdraw { lp_token } => emergency_withdraw(deps, env, info, lp_token),
        ExecuteMsg::SetAllowedRewardProxies { proxies } => {
            set_allowed_reward_proxies(deps, info, proxies)
        }
        ExecuteMsg::SendOrphanProxyReward {
            recipient,
            lp_token,
        } => send_orphan_proxy_rewards(deps, info, recipient, lp_token),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::SetTokensPerBlock { amount } => {
            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.owner {
                return Err(ContractError::Unauthorized {});
            }

            update_rewards_and_execute(
                deps,
                env,
                None,
                ExecuteOnReply::SetTokensPerBlock { amount },
            )
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
    }
}

/// ## Description
/// Sets a new Generator vesting contract address. Returns a [`ContractError`] on failure or the [`CONFIG`]
/// data will be updated with the new vesting contract address.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **vesting_contract** is an [`Option`] field object of type [`String`]. This is the new vesting contract address.
/// ##Executor
/// Only the owner can execute this.
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    vesting_contract: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(vesting_contract) = vesting_contract {
        config.vesting_contract = addr_validate_to_lower(deps.api, vesting_contract.as_str())?;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("action", "update_config"))
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise it creates a new generator and adds it to [`POOL_INFO`]
/// (if it does not exist yet) and updates total allocation points (in [`Config`]).
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token for which to create a generator.
///
/// * **alloc_point** is an object of type [`Uint64`]. This is the portion of ASTRO emissions that the generator gets.
///
/// * **has_asset_rewards** is the field of type [`bool`]. This flag indicates whether the generator receives dual rewards.
///
/// * **reward_proxy** is an [`Option`] field object of type [`String`]. This is the address of the dual rewards proxy.
///
/// ##Executor
/// Can only be called by the owner.
pub fn add(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    alloc_point: Uint64,
    has_asset_rewards: bool,
    reward_proxy: Option<String>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

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
        accumulated_rewards_per_share: Decimal::zero(),
        reward_proxy,
        accumulated_proxy_rewards_per_share: Decimal::zero(),
        proxy_reward_balance_before_update: Uint128::zero(),
        orphan_proxy_rewards: Uint128::zero(),
        has_asset_rewards,
    };

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::new()
        .add_attribute("action", "add_pool")
        .add_attribute("lp_token", lp_token))
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise updates the given generator's ASTRO allocation points and
/// returns a [`Response`] with the specified attributes.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token whose generator allocation points we update.
///
/// * **alloc_point** is an object of type [`Uint64`]. This is the new share of ASTRO emissions this generator gets.
///
/// * **has_asset_rewards** is the field of type [`bool`]. This flag indicates whether the generator receives dual rewards.
///
/// ##Executor
/// Can only be called by the owner.
pub fn set(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    alloc_point: Uint64,
    has_asset_rewards: bool,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    mass_update_pools(deps.branch(), env)?;

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token)?;

    cfg.total_alloc_point = cfg
        .total_alloc_point
        .checked_sub(pool_info.alloc_point)?
        .checked_add(alloc_point)?;
    pool_info.alloc_point = alloc_point;

    pool_info.has_asset_rewards = has_asset_rewards;

    CONFIG.save(deps.storage, &cfg)?;
    POOL_INFO.save(deps.storage, &lp_token, &pool_info)?;

    Ok(Response::new()
        .add_attribute("action", "set_pool")
        .add_attribute("lp_token", lp_token))
}

/// ## Description
/// Updates the amount of accrued rewards for a specific generator (if specified in input parameters), otherwise updates rewards for
/// all pools that are in [`POOL_INFO`]. Returns a [`ContractError`] on failure, otherwise returns a [`Response`] object with
/// the specified attributes.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **update_single_pool** is an [`Option`] field object of type [`Addr`]. This indicates whether a single generator
/// should be updated and if yes, which one.
///
/// * **on_reply** is an object of type [`ExecuteOnReply`]. This is the action to be performed on reply.
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

    let pools: Vec<(Addr, PoolInfo)>;
    match update_single_pool {
        Some(lp_token) => {
            let pool = POOL_INFO.load(deps.storage, &lp_token)?;
            pools = vec![(lp_token, pool)];
        }
        None => {
            pools = get_pools(deps.storage);
        }
    }

    let mut messages: Vec<SubMsg> = vec![];
    for (lp_token, mut pool) in pools {
        if let Some(reward_proxy) = pool.reward_proxy.clone() {
            messages.append(&mut get_proxy_rewards(
                deps.branch(),
                &lp_token,
                &mut pool,
                &reward_proxy,
            )?);
        }
    }

    if let Some(last) = messages.last_mut() {
        last.reply_on = ReplyOn::Success;
        Ok(Response::new().add_submessages(messages))
    } else {
        process_after_update(deps, env)
    }
}

/// ## Description
/// Fetches accrued proxy rewards. Snapshots the old amount of rewards that are still unclaimed. Returns a [`ContractError`]
/// on failure, otherwise returns a vector that contains objects of type [`SubMsg`].
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token for which we fetch the latest amount of accrued proxy rewards.
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator associated with the `lp_token`.
///
/// * **reward_proxy** is an object of type [`Addr`]. This is the address of the dual rewards proxy for the target LP/generator.
fn get_proxy_rewards(
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

/// # Description
/// The entry point to the contract for processing replies from submessages.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`Reply`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, _msg: Reply) -> Result<Response, ContractError> {
    process_after_update(deps, env)
}

/// # Description
/// Loads an action from [`TMP_USER_ACTION`] and executes it. Returns a [`ContractError`]
/// on failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
fn process_after_update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    match TMP_USER_ACTION.load(deps.storage)? {
        Some(action) => {
            TMP_USER_ACTION.save(deps.storage, &None)?;
            match action {
                ExecuteOnReply::MassUpdatePools {} => mass_update_pools(deps, env),
                ExecuteOnReply::Add {
                    lp_token,
                    alloc_point,
                    has_asset_rewards,
                    reward_proxy,
                } => add(
                    deps,
                    env,
                    lp_token,
                    alloc_point,
                    has_asset_rewards,
                    reward_proxy,
                ),
                ExecuteOnReply::Set {
                    lp_token,
                    alloc_point,
                    has_asset_rewards,
                } => set(deps, env, lp_token, alloc_point, has_asset_rewards),
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
                ExecuteOnReply::SetTokensPerBlock { amount } => {
                    set_tokens_per_block(deps, env, amount)
                }
            }
        }
        None => Ok(Response::default()),
    }
}

/// # Description
/// Sets a new amount of ASTRO distributed per block among all active generators. Before that, we
/// will need to update all pools in order to correctly account for accrued rewards. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **amount** is an object of type [`Uint128`]. This is the new total amount of ASTRO distributed per block.
fn set_tokens_per_block(
    mut deps: DepsMut,
    env: Env,
    amount: Uint128,
) -> Result<Response, ContractError> {
    mass_update_pools(deps.branch(), env)?;

    CONFIG.update::<_, ContractError>(deps.storage, |mut v| {
        v.tokens_per_block = amount;
        Ok(v)
    })?;

    Ok(Response::new().add_attribute("action", "set_tokens_per_block"))
}

/// # Description
/// Updates the amount of accrued rewards for all generators. Returns a [`ContractError`] on failure, otherwise
/// returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
pub fn mass_update_pools(mut deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
    let pools = get_pools(deps.storage);

    if pools.is_empty() {
        return Ok(response);
    }

    for (lp_token, mut pool) in pools {
        accumulate_rewards_per_share(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;
        POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    }

    Ok(response.add_attribute("action", "mass_update_pools"))
}

/// # Description
/// Updates the amount of accrued rewards for a specific generator. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token whose accrued rewards we update.
pub fn update_pool(mut deps: DepsMut, env: Env, lp_token: Addr) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    accumulate_rewards_per_share(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(response.add_attribute("action", "update_pool"))
}

/// # Description
/// Accrues the amount of rewards distributed for each staked LP token in a specific generator.
/// Update reward variables of the given pool to be up-to-date.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token whose rewards per share we update.
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator associated with the `lp_token`
///
/// * **cfg** is an object of type [`Config`]. This is the contract config.
///
/// * **deposited** is an [`Option`] field object of type [`Uint128`]. This is the total amount of LP
/// tokens deposited in the target generator.
pub fn accumulate_rewards_per_share(
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
                pool.accumulated_proxy_rewards_per_share = pool
                    .accumulated_proxy_rewards_per_share
                    .checked_add(share)?;
                pool.proxy_reward_balance_before_update = reward_amount;
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
                // On deposit, the contract's LP token balance is already increased, so we need to subtract the
                lp_supply = res.balance.checked_sub(amount)?;
            } else {
                lp_supply = res.balance;
            }
        }
    };

    if env.block.height > pool.last_reward_block.u64() {
        if !lp_supply.is_zero() {
            let token_rewards = calculate_rewards(env, pool, cfg)?;

            let share = Decimal::from_ratio(token_rewards, lp_supply);
            pool.accumulated_rewards_per_share =
                pool.accumulated_rewards_per_share.checked_add(share)?;
        }

        pool.last_reward_block = Uint64::from(env.block.height);
    }

    Ok(())
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message to process.
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

/// # Description
/// Distributes pending proxy rewards for a specific staker.
/// Returns a [`ContractError`] on failure, otherwise returns a vector that
/// contains objects of type [`SubMsg`].
/// # Params
/// * **cfg** is an object of type [`Config`].
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator where the staker is staked.
///
/// * **user** is an object of type [`UserInfo`]. This is the staker for which we claim accrued proxy rewards.
///
/// * **to** is an object of type [`Addr`]. This is the address that will receive the proxy rewards.
pub fn send_pending_rewards(
    cfg: &Config,
    pool: &PoolInfo,
    user: &UserInfo,
    to: &Addr,
) -> Result<Vec<WasmMsg>, ContractError> {
    if user.amount.is_zero() {
        return Ok(vec![]);
    }

    let mut messages = vec![];

    let pending_rewards = pool
        .accumulated_rewards_per_share
        .checked_mul(user.amount)?
        .checked_sub(user.reward_debt)?;

    if !pending_rewards.is_zero() {
        messages.push(WasmMsg::Execute {
            contract_addr: cfg.vesting_contract.to_string(),
            msg: to_binary(&VestingExecuteMsg::Claim {
                recipient: Some(to.to_string()),
                amount: Some(pending_rewards),
            })?,
            funds: vec![],
        });
    }

    if let Some(proxy) = &pool.reward_proxy {
        let pending_proxy_rewards = pool
            .accumulated_proxy_rewards_per_share
            .checked_mul(user.amount)?
            .checked_sub(user.reward_debt_proxy)?;

        if !pending_proxy_rewards.is_zero() {
            messages.push(WasmMsg::Execute {
                contract_addr: proxy.to_string(),
                funds: vec![],
                msg: to_binary(&ProxyExecuteMsg::SendRewards {
                    account: to.to_string(),
                    amount: pending_proxy_rewards,
                })?,
            });
        }
    }

    Ok(messages)
}

/// # Description
/// Deposit LP tokens in a generator to receive token emissions. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token to deposit.
///
/// * **beneficiary** is an object of type [`Addr`]. This is the address that will take ownership of the staked LP tokens.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to deposit.
pub fn deposit(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    beneficiary: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let user = USER_INFO
        .load(deps.storage, (&lp_token, &beneficiary))
        .unwrap_or_default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    accumulate_rewards_per_share(
        deps.branch(),
        &env,
        &lp_token,
        &mut pool,
        &cfg,
        Some(amount),
    )?;

    // Send pending rewards (if any) to the depositor
    let send_rewards_msg = send_pending_rewards(&cfg, &pool, &user, &beneficiary)?;

    // If a reward proxy is set - send LP tokens to the proxy
    let transfer_msg = if !amount.is_zero() && pool.reward_proxy.is_some() {
        vec![WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pool.reward_proxy.clone().unwrap().to_string(),
                msg: to_binary(&ProxyCw20HookMsg::Deposit {})?,
                amount,
            })?,
            funds: vec![],
        }]
    } else {
        vec![]
    };

    let reward_msg = build_claim_pools_asset_reward_messages(
        deps.as_ref(),
        &env,
        &lp_token,
        &pool,
        &beneficiary,
        user.amount,
        amount,
    )?;

    // Update user's LP token balance
    let updated_amount = user.amount.checked_add(amount)?;
    let user = update_user_balance(user, &pool, updated_amount)?;

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;
    USER_INFO.save(deps.storage, (&lp_token, &beneficiary), &user)?;

    Ok(Response::new()
        .add_messages(send_rewards_msg)
        .add_messages(transfer_msg)
        .add_messages(reward_msg)
        .add_attribute("action", "deposit")
        .add_attribute("amount", amount))
}

/// # Description
/// Withdraw LP tokens from a generator. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token to withdraw.
///
/// * **account** is an object of type [`Addr`]. This is the user whose LP tokens we withdraw.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to withdraw.
pub fn withdraw(
    mut deps: DepsMut,
    env: Env,
    lp_token: Addr,
    account: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let user = USER_INFO
        .load(deps.storage, (&lp_token, &account))
        .unwrap_or_default();
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    accumulate_rewards_per_share(deps.branch(), &env, &lp_token, &mut pool, &cfg, None)?;

    // Send pending rewards to the user
    let send_rewards_msg = send_pending_rewards(&cfg, &pool, &user, &account)?;

    // Instantiate the transfer call for the LP token
    let transfer_msg = if !amount.is_zero() {
        vec![match &pool.reward_proxy {
            Some(proxy) => WasmMsg::Execute {
                contract_addr: proxy.to_string(),
                funds: vec![],
                msg: to_binary(&ProxyExecuteMsg::Withdraw {
                    account: account.to_string(),
                    amount,
                })?,
            },
            None => WasmMsg::Execute {
                contract_addr: lp_token.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: account.to_string(),
                    amount,
                })?,
                funds: vec![],
            },
        }]
    } else {
        vec![]
    };

    let reward_msg = build_claim_pools_asset_reward_messages(
        deps.as_ref(),
        &env,
        &lp_token,
        &pool,
        &account,
        user.amount,
        Uint128::zero(),
    )?;

    // Update user's balance
    let updated_amount = user.amount.checked_sub(amount)?;
    let user = update_user_balance(user, &pool, updated_amount)?;

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    if !user.amount.is_zero() {
        USER_INFO.save(deps.storage, (&lp_token, &account), &user)?;
    } else {
        USER_INFO.remove(deps.storage, (&lp_token, &account));
    }

    Ok(Response::new()
        .add_messages(send_rewards_msg)
        .add_messages(transfer_msg)
        .add_messages(reward_msg)
        .add_attribute("action", "withdraw")
        .add_attribute("amount", amount))
}

/// # Description
/// Builds claim reward messages for a specific generator (if the messages are supported)
pub fn build_claim_pools_asset_reward_messages(
    deps: Deps,
    env: &Env,
    lp_token: &Addr,
    pool: &PoolInfo,
    account: &Addr,
    user_amount: Uint128,
    deposit: Uint128,
) -> Result<Vec<WasmMsg>, ContractError> {
    Ok(if pool.has_asset_rewards {
        let total_share = match &pool.reward_proxy {
            Some(proxy) => deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?,
            None => {
                query_token_balance(
                    &deps.querier,
                    lp_token.clone(),
                    env.contract.address.clone(),
                )? - deposit
            }
        };

        let minter_response: MinterResponse = deps
            .querier
            .query_wasm_smart(lp_token, &Cw20QueryMsg::Minter {})?;

        vec![WasmMsg::Execute {
            contract_addr: minter_response.minter,
            funds: vec![],
            msg: to_binary(
                &astroport::pair_stable_bluna::ExecuteMsg::ClaimRewardByGenerator {
                    user: account.to_string(),
                    user_share: user_amount,
                    total_share,
                },
            )?,
        }]
    } else {
        vec![]
    })
}

/// # Description
/// Withdraw LP tokens without caring about rewards. TO BE USED IN EMERGENCY SITUATIONS ONLY.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the
/// specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token to withdraw.
pub fn emergency_withdraw(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    lp_token: String,
) -> Result<Response, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let user = USER_INFO.load(deps.storage, (&lp_token, &info.sender))?;

    pool.orphan_proxy_rewards = pool.orphan_proxy_rewards.checked_add(
        pool.accumulated_proxy_rewards_per_share
            .checked_mul(user.amount)?
            .saturating_sub(user.reward_debt_proxy),
    )?;

    // Instantiate the transfer call for the LP token
    let transfer_msg: WasmMsg;
    if let Some(proxy) = &pool.reward_proxy {
        transfer_msg = WasmMsg::Execute {
            contract_addr: proxy.to_string(),
            msg: to_binary(&ProxyExecuteMsg::EmergencyWithdraw {
                account: info.sender.to_string(),
                amount: user.amount,
            })?,
            funds: vec![],
        };
    } else {
        transfer_msg = WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: info.sender.to_string(),
                amount: user.amount,
            })?,
            funds: vec![],
        };
    }

    // Change the user's balance
    USER_INFO.remove(deps.storage, (&lp_token, &info.sender));
    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(Response::new()
        .add_message(transfer_msg)
        .add_attribute("action", "emergency_withdraw")
        .add_attribute("amount", user.amount))
}

/// # Description
/// Sets the allowed reward proxies taht can interact with the Generator contract. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **proxies** is an array that contains objects of type [`String`].
/// This is the full list of allowed proxies that can interact with the Generator.
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

/// # Description
/// Sends orphaned proxy rewards (which are left behind by emergency withdrawals) to another address.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] with the specified
/// attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **recipient** is an object of type [`String`]. This is the recipient of the orphaned rewards.
///
/// * **lp_token** is an object of type [`String`]. This is the LP token whose orphaned rewards we send out.
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

    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let proxy = match &pool.reward_proxy {
        Some(proxy) => proxy.clone(),
        None => return Err(ContractError::PoolDoesNotHaveAdditionalRewards {}),
    };

    let amount = pool.orphan_proxy_rewards;
    if amount.is_zero() {
        return Err(ContractError::OrphanRewardsTooSmall {});
    }

    pool.orphan_proxy_rewards = Uint128::zero();
    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(Response::new()
        .add_message(WasmMsg::Execute {
            contract_addr: proxy.to_string(),
            funds: vec![],
            msg: to_binary(&ProxyExecuteMsg::SendRewards {
                account: recipient.to_string(),
                amount,
            })?,
        })
        .add_attribute("action", "send_orphan_rewards")
        .add_attribute("recipient", recipient)
        .add_attribute("lp_token", lp_token.to_string())
        .add_attribute("amount", amount))
}

/// ## Description
/// Sets the reward proxy contract for a specifi generator. Returns a [`ContractError`] on failure, otherwise
/// returns a [`Response`] with the specified attributes if the operation was successful.
fn move_to_proxy(
    deps: DepsMut,
    env: Env,
    lp_token: String,
    proxy: String,
) -> Result<Response, ContractError> {
    let lp_addr = addr_validate_to_lower(deps.api, &lp_token)?;
    let proxy_addr = addr_validate_to_lower(deps.api, &proxy)?;

    let cfg = CONFIG.load(deps.storage)?;

    if !cfg.allowed_reward_proxies.contains(&proxy_addr) {
        return Err(ContractError::RewardProxyNotAllowed {});
    }

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_addr.clone())?;
    if pool_info.reward_proxy.is_some() {
        return Err(ContractError::PoolAlreadyHasRewardProxyContract {});
    }
    pool_info.reward_proxy = Some(proxy_addr);

    let res: BalanceResponse = deps.querier.query_wasm_smart(
        lp_addr.clone(),
        &cw20::Cw20QueryMsg::Balance {
            address: env.contract.address.to_string(),
        },
    )?;

    let messages = if !res.balance.is_zero() {
        vec![WasmMsg::Execute {
            contract_addr: lp_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: pool_info.reward_proxy.clone().unwrap().to_string(),
                msg: to_binary(&ProxyCw20HookMsg::Deposit {})?,
                amount: res.balance,
            })?,
            funds: vec![],
        }]
    } else {
        vec![]
    };

    POOL_INFO.save(deps.storage, &lp_addr, &pool_info)?;

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(vec![attr("action", "move_to_proxy"), attr("proxy", proxy)]))
}

/// ## Description
/// Add or remove proxy contracts to and from the proxy contract whitelist. Returns a [`ContractError`] on failure.
fn update_allowed_proxies(
    deps: DepsMut,
    add: Option<Vec<String>>,
    remove: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    if add.is_none() && remove.is_none() {
        return Err(ContractError::Std(StdError::generic_err(
            "Need to provide add or remove parameters",
        )));
    }

    let mut cfg = CONFIG.load(deps.storage)?;

    // Remove proxies
    if let Some(remove_proxies) = remove {
        for remove_proxy in remove_proxies {
            let index = cfg
                .allowed_reward_proxies
                .iter()
                .position(|x| *x.as_str() == remove_proxy.as_str().to_lowercase())
                .ok_or_else(|| {
                    StdError::generic_err(
                        "Can't remove proxy contract. It is not found in allowed list.",
                    )
                })?;
            cfg.allowed_reward_proxies.remove(index);
        }
    }

    // Add new proxies
    if let Some(add_proxies) = add {
        for add_proxy in add_proxies {
            let proxy_addr = addr_validate_to_lower(deps.api, &add_proxy)?;
            if !cfg.allowed_reward_proxies.contains(&proxy_addr) {
                cfg.allowed_reward_proxies.push(proxy_addr);
            }
        }
    }

    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::default().add_attribute("action", "update_allowed_proxies"))
}

/// ## Description
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::PoolLength {}** Returns the amount of instantiated generators using a [`PoolLengthResponse`] object.
///
/// * **QueryMsg::Deposit { lp_token, user }** Returns the amount of LP tokens staked by a user in a specific generator.
///
/// * **QueryMsg::PendingToken { lp_token, user }** Returns the amount of pending rewards a user earned using
/// a [`PendingTokenResponse`] object.
///
/// * **QueryMsg::Config {}** Returns the Generator contract configuration using a [`ConfigResponse`] object.
///
/// * **QueryMsg::RewardInfo { lp_token }** Returns reward information about a specific generator
/// using a [`RewardInfoResponse`] object.
///
/// * **QueryMsg::OrphanProxyRewards { lp_token }** Returns the amount of orphaned proxy rewards for a specific generator.
///
/// * **QueryMsg::PoolInfo { lp_token }** Returns general information about a generator using a [`PoolInfoResponse`] object.
///
/// * **QueryMsg::SimulateFutureReward { lp_token, future_block }** Returns the amount of token rewards a generator will
/// distribute up to a future block.
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
        QueryMsg::PoolInfo { lp_token } => Ok(to_binary(&query_pool_info(deps, env, lp_token)?)?),
        QueryMsg::SimulateFutureReward {
            lp_token,
            future_block,
        } => Ok(to_binary(&query_simulate_future_reward(
            deps,
            env,
            lp_token,
            future_block,
        )?)?),
        QueryMsg::PoolStakers {
            lp_token,
            start_after,
            limit,
        } => Ok(to_binary(&query_list_of_stakers(
            deps,
            lp_token,
            start_after,
            limit,
        )?)?),
    }
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the amount of instantiated generators
/// using a [`PoolLengthResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn pool_length(deps: Deps) -> Result<PoolLengthResponse, ContractError> {
    let length = POOL_INFO
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .count();
    Ok(PoolLengthResponse { length })
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the amount of LP tokens a user staked in a specific generator.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token for which we query the user's balance for.
///
/// * **user** is an object of type [`String`]. This is the user whose balance we query.
pub fn query_deposit(deps: Deps, lp_token: String, user: String) -> Result<Uint128, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;

    let user_info = USER_INFO
        .load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();
    Ok(user_info.amount)
}

/// ## Description
/// Calculates pending token rewards for a specific user. Returns a [`ContractError`] on failure, otherwise returns
/// information in a [`PendingTokenResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token staked by the user whose pending rewards we calculate.
///
/// * **user** is an object of type [`String`]. This is the user for which we fetch the amount of pending token rewards.
// View function to see pending ASTRO on frontend.
pub fn pending_token(
    deps: Deps,
    env: Env,
    lp_token: String,
    user: String,
) -> Result<PendingTokenResponse, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;

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

                let mut acc_per_share_on_proxy = pool.accumulated_proxy_rewards_per_share;
                if let Some(token_rewards) = res {
                    let share = Decimal::from_ratio(token_rewards, lp_supply);
                    acc_per_share_on_proxy = pool
                        .accumulated_proxy_rewards_per_share
                        .checked_add(share)?;
                }

                pending_on_proxy = Some(
                    acc_per_share_on_proxy
                        .checked_mul(user_info.amount)?
                        .checked_sub(user_info.reward_debt_proxy)?,
                );
            }
        }
        None => {
            lp_supply = query_token_balance(&deps.querier, lp_token, env.contract.address.clone())?;
        }
    }

    let mut acc_per_share = pool.accumulated_rewards_per_share;
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        let token_rewards = calculate_rewards(&env, &pool, &cfg)?;
        let share = Decimal::from_ratio(token_rewards, lp_supply);
        acc_per_share = pool.accumulated_rewards_per_share.checked_add(share)?;
    }

    let pending = acc_per_share
        .checked_mul(user_info.amount)?
        .checked_sub(user_info.reward_debt)?;

    Ok(PendingTokenResponse {
        pending,
        pending_on_proxy,
    })
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns information about a generator's
/// configuration using a [`ConfigResponse`] object .
/// ## Params
/// * **deps** is an object of type [`Deps`].
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

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns reward information for a specific generator
/// using a [`RewardInfoResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token whose generator we query for reward information.
fn query_reward_info(deps: Deps, lp_token: String) -> Result<RewardInfoResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

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

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the amount of orphaned proxy rewards for a specific generator.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token whose generator we query for orphaned rewards.
fn query_orphan_proxy_rewards(deps: Deps, lp_token: String) -> Result<Uint128, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    if pool.reward_proxy.is_none() {
        return Err(ContractError::PoolDoesNotHaveAdditionalRewards {});
    }

    Ok(pool.orphan_proxy_rewards)
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns a generator's
/// configuration using a [`PoolInfoResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token whose generator we query.
fn query_pool_info(
    deps: Deps,
    env: Env,
    lp_token: String,
) -> Result<PoolInfoResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let pool = POOL_INFO.load(deps.storage, &lp_token)?;

    let lp_supply: Uint128;
    let mut pending_on_proxy = None;
    let mut pending_astro_rewards = Uint128::zero();

    // If proxy rewards are live for this LP token, calculate its pending proxy rewards
    match &pool.reward_proxy {
        Some(proxy) => {
            lp_supply = deps
                .querier
                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

            // If LP tokens are staked via a proxy contract, fetch current pending proxy rewards
            if !lp_supply.is_zero() {
                let res: Uint128 = deps
                    .querier
                    .query_wasm_smart(proxy, &ProxyQueryMsg::PendingToken {})?;

                if !res.is_zero() {
                    pending_on_proxy = Some(res);
                }
            }
        }
        None => {
            lp_supply = query_token_balance(&deps.querier, lp_token, env.contract.address.clone())?;
        }
    }

    // Calculate pending ASTRO rewards
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        pending_astro_rewards = calculate_rewards(&env, &pool, &config)?;
    }

    // Calculate ASTRO tokens being distributed per block to this LP token pool
    let astro_tokens_per_block: Uint128;
    astro_tokens_per_block = config
        .tokens_per_block
        .checked_mul(Uint128::from(pool.alloc_point.u64()))?
        .checked_div(Uint128::from(config.total_alloc_point.u64()))
        .unwrap_or_else(|_| Uint128::zero());

    Ok(PoolInfoResponse {
        alloc_point: pool.alloc_point,
        astro_tokens_per_block,
        last_reward_block: pool.last_reward_block.u64(),
        current_block: env.block.height,
        accumulated_rewards_per_share: pool.accumulated_rewards_per_share,
        pending_astro_rewards,
        reward_proxy: pool.reward_proxy,
        pending_proxy_rewards: pending_on_proxy,
        accumulated_proxy_rewards_per_share: pool.accumulated_proxy_rewards_per_share,
        proxy_reward_balance_before_update: pool.proxy_reward_balance_before_update,
        orphan_proxy_rewards: pool.orphan_proxy_rewards,
        lp_supply,
    })
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the total amount of ASTRO tokens distributed for
/// a specific generator up to a certain block in the future.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token for which we query the amount of future ASTRO rewards.
pub fn query_simulate_future_reward(
    deps: Deps,
    env: Env,
    lp_token: String,
    future_block: u64,
) -> Result<Uint128, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let n_blocks = Uint128::from(future_block)
        .checked_sub(env.block.height.into())
        .unwrap_or_else(|_| Uint128::zero());

    let simulated_reward = n_blocks
        .checked_mul(cfg.tokens_per_block)?
        .checked_mul(Uint128::from(pool.alloc_point.u64()))?
        .checked_div(Uint128::from(cfg.total_alloc_point.u64()))
        .unwrap_or_else(|_| Uint128::zero());

    Ok(simulated_reward)
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns a list of stakers that currently
/// have funds in a specific generator.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the
/// LP token whose generator we query for stakers.
///
/// * **start_after** is an object of type [`Option<String>`]. This is an optional field
/// that specifies whether the function should return a list of stakers starting from a
/// specific address onward.
///
/// * **limit** is an object of type [`Option<u32>`]. This is the max amount of staker
/// addresses to return.
pub fn query_list_of_stakers(
    deps: Deps,
    lp_token: String,
    start_after: Option<String>,
    limit: Option<u32>,
) -> Result<Vec<StakerResponse>, ContractError> {
    let lp_addr = addr_validate_to_lower(deps.api, lp_token.as_str())?;
    let mut active_stakers: Vec<StakerResponse> = vec![];

    if POOL_INFO.has(deps.storage, &lp_addr) {
        let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
        let start = start_after
            .map(|start| start.joined_key())
            .map(Bound::Exclusive);

        active_stakers = USER_INFO
            .prefix(&lp_addr)
            .range(deps.storage, start, None, Order::Ascending)
            .filter_map(|stakers| {
                stakers
                    .ok()
                    .map(|staker| StakerResponse {
                        account: String::from_utf8(staker.0).unwrap(),
                        amount: staker.1.amount,
                    })
                    .filter(|active_staker| !active_staker.amount.is_zero())
            })
            .take(limit)
            .collect();
    }

    Ok(active_stakers)
}

/// ## Description
/// Calculates the amount of accrued rewards since the last reward checkpoint for a specific generator.
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator for which we calculate accrued rewards.
///
/// * **cfg** is an object of type [`Config`]. This is the Generator contract configuration.
pub fn calculate_rewards(env: &Env, pool: &PoolInfo, cfg: &Config) -> StdResult<Uint128> {
    let n_blocks = Uint128::from(env.block.height).checked_sub(pool.last_reward_block.into())?;

    let r;
    if !cfg.total_alloc_point.is_zero() {
        r = n_blocks
            .checked_mul(cfg.tokens_per_block)?
            .checked_mul(Uint128::from(pool.alloc_point.u64()))?
            .checked_div(Uint128::from(cfg.total_alloc_point.u64()))?;
    } else {
        r = Uint128::zero();
    }

    Ok(r)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-generator" => match contract_version.version.as_ref() {
            "1.0.0" => {
                let keys = POOL_INFO
                    .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
                    .map(|v| String::from_utf8(v).map_err(StdError::from))
                    .collect::<Result<Vec<String>, StdError>>()?;

                for key in keys {
                    let pool_info_v100 = migration::POOL_INFOV100
                        .load(deps.storage, &Addr::unchecked(key.clone()))?;
                    let pool_info = PoolInfo {
                        has_asset_rewards: false,
                        accumulated_proxy_rewards_per_share: pool_info_v100
                            .accumulated_proxy_rewards_per_share,
                        alloc_point: pool_info_v100.alloc_point,
                        accumulated_rewards_per_share: pool_info_v100.accumulated_rewards_per_share,
                        last_reward_block: pool_info_v100.last_reward_block,
                        orphan_proxy_rewards: pool_info_v100.orphan_proxy_rewards,
                        proxy_reward_balance_before_update: pool_info_v100
                            .proxy_reward_balance_before_update,
                        reward_proxy: pool_info_v100.reward_proxy,
                    };
                    POOL_INFO.save(deps.storage, &Addr::unchecked(key), &pool_info)?;
                }
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
