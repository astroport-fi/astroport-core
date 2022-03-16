use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, Uint64,
    WasmMsg,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, MinterResponse};
use std::collections::HashSet;

use crate::error::ContractError;
use crate::migration;
use crate::state::{
    update_user_balance, Config, ExecuteOnReply, UserInfo, CONFIG, DEFAULT_LIMIT, MAX_LIMIT,
    OWNERSHIP_PROPOSAL, POOL_INFO, TMP_USER_ACTION, USER_INFO,
};
use astroport::asset::{
    addr_validate_to_lower, pair_info_by_pool, token_asset_info, AssetInfo, PairInfo,
};

use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::factory::{PairConfig, PairType};
use astroport::generator::PoolInfo;
use astroport::generator::StakerResponse;
use astroport::querier::query_token_balance;
use astroport::DecimalCheckedOps;
use astroport::{
    factory::{ConfigResponse as FactoryConfigResponse, QueryMsg as FactoryQueryMsg},
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

    let mut config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        factory: addr_validate_to_lower(deps.api, &msg.factory)?,
        generator_controller: None,
        guardian: None,
        astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: Uint64::from(0u64),
        start_block: msg.start_block,
        allowed_reward_proxies,
        vesting_contract: addr_validate_to_lower(deps.api, &msg.vesting_contract)?,
        active_pools: vec![],
        blocked_list_tokens: vec![],
    };

    if let Some(generator_controller) = msg.generator_controller {
        config.generator_controller =
            Some(addr_validate_to_lower(deps.api, &generator_controller)?);
    }
    if let Some(guardian) = msg.guardian {
        config.guardian = Some(addr_validate_to_lower(deps.api, &guardian)?);
    }

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
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig {
///             vesting_contract,
///             generator_controller,
///             guardian,
///         }** Changes the address of the Generator vesting contract, Generator controller contract or Generator guardian.
///
/// * **ExecuteMsg::SetupPools { pools }** Setting up a new list of pools with allocation points.
///
/// * **UpdatePool {
///             lp_token,
///             has_asset_rewards,
///         }** Update the given pool's has_asset_rewards parameter.
///
/// * **ExecuteMsg::ClaimRewards { lp_token }** Updates reward and returns it to user.
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
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
/// Only the current owner can call this.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership. Only the newly proposed owner
/// can call this.
///
/// * **ExecuteMsg::DeactivatePool { lp_token }** Sets the allocation point to zero for specified
/// LP token.
///
/// * **ExecuteMsg::DeactivatePools { pair_types }** Sets the allocation point to zero for each pool
/// by the pair type
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::DeactivatePools { pair_types } => deactivate_pools(deps, env, pair_types),
        ExecuteMsg::DeactivatePool { lp_token } => {
            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.factory {
                return Err(ContractError::Unauthorized {});
            }
            let lp_token_addr = addr_validate_to_lower(deps.api, &lp_token)?;
            let active_pools: Vec<Addr> =
                cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
            mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;
            deactivate_pool(deps, lp_token_addr)
        }
        ExecuteMsg::UpdateTokensBlockedlist { add, remove } => {
            update_tokens_blockedlist(deps, env, info, add, remove)
        }
        ExecuteMsg::MoveToProxy { lp_token, proxy } => {
            move_to_proxy(deps, env, info, lp_token, proxy)
        }
        ExecuteMsg::UpdateAllowedProxies { add, remove } => {
            update_allowed_proxies(deps, info, add, remove)
        }
        ExecuteMsg::UpdateConfig {
            vesting_contract,
            generator_controller,
            guardian,
        } => execute_update_config(deps, info, vesting_contract, generator_controller, guardian),
        ExecuteMsg::SetupPools { pools } => execute_setup_pools(deps, env, info, pools),
        ExecuteMsg::UpdatePool {
            lp_token,
            has_asset_rewards,
        } => execute_update_pool(deps, info, lp_token, has_asset_rewards),
        ExecuteMsg::ClaimRewards { lp_tokens } => {
            let mut lp_tokens_addr: Vec<Addr> = vec![];
            for lp_token in &lp_tokens {
                lp_tokens_addr.push(addr_validate_to_lower(deps.api, lp_token)?);
            }

            update_rewards_and_execute(
                deps,
                env,
                None,
                ExecuteOnReply::ClaimRewards {
                    lp_tokens: lp_tokens_addr,
                    account: info.sender,
                },
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
/// Sets the allocation point to zero for each pool by the pair type
fn deactivate_pools(
    mut deps: DepsMut,
    env: Env,
    pair_types: Vec<PairType>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    // Check for duplicate pair types
    let mut uniq: HashSet<String> = HashSet::new();
    if !pair_types
        .clone()
        .into_iter()
        .all(|a| uniq.insert(a.to_string()))
    {
        return Err(ContractError::Std(StdError::generic_err(
            "Duplicate of pair type!",
        )));
    }

    let blacklisted_pair_types: Vec<PairType> = deps.querier.query_wasm_smart(
        cfg.factory.clone(),
        &FactoryQueryMsg::BlacklistedPairTypes {},
    )?;

    // checks if each pair type is blacklisted
    for pair_type in pair_types.clone() {
        if !blacklisted_pair_types.contains(&pair_type) {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Pair type ({}) is not blacklisted!",
                pair_type
            ))));
        }
    }

    let active_pools: Vec<Addr> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
    mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;

    // find active pools with blacklisted pair type
    for pool in &mut cfg.active_pools {
        if !pool.1.is_zero() {
            let pair_info = pair_info_by_pool(deps.as_ref(), pool.0.clone())?;
            if pair_types.contains(&pair_info.pair_type) {
                // recalculate total allocation point before resetting the allocation point of pool
                cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(pool.1)?;
                // sets allocation point to zero for each pool with blacklisted pair type
                pool.1 = Uint64::zero();
            }
        }
    }

    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "deactivate_pools"))
}

/// Add or remove tokens to and from the blocked list. Returns a [`ContractError`] on failure.
fn update_tokens_blockedlist(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    add: Option<Vec<AssetInfo>>,
    remove: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {
    if add.is_none() && remove.is_none() {
        return Err(ContractError::Std(StdError::generic_err(
            "Need to provide add or remove parameters",
        )));
    }

    let mut cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner && Some(info.sender) != cfg.guardian {
        return Err(ContractError::Unauthorized {});
    }

    // Remove tokens from blacklist
    if let Some(asset_infos) = remove {
        for asset_info in asset_infos {
            let index = cfg
                .blocked_list_tokens
                .iter()
                .position(|x| *x == asset_info)
                .ok_or_else(|| {
                    StdError::generic_err(
                        "Can't remove token. It is not found in the blocked list.",
                    )
                })?;
            cfg.blocked_list_tokens.remove(index);
        }
    }

    // Add tokens to the blacklist
    if let Some(asset_infos) = add {
        let active_pools: Vec<Addr> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
        mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;
        let astro = token_asset_info(cfg.astro_token.clone());

        for asset_info in asset_infos {
            // ASTRO or Terra native assets (UST, LUNA etc) cannot be blacklisted
            if asset_info.is_native_token() || asset_info.eq(&astro) {
                return Err(ContractError::AssetCannotBeBlocked {});
            }

            if !cfg.blocked_list_tokens.contains(&asset_info) {
                cfg.blocked_list_tokens.push(asset_info.clone());

                // Find active pools with blacklisted tokens
                for pool in &mut cfg.active_pools {
                    let pair_info = pair_info_by_pool(deps.as_ref(), pool.0.clone())?;
                    if pair_info.asset_infos.contains(&asset_info) {
                        // Recalculate total allocation points before resetting the pool allocation points
                        cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(pool.1)?;
                        // Sets allocation points to zero for each pool with blacklisted tokens
                        pool.1 = Uint64::zero();
                    }
                }
            }
        }
    }

    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "update_tokens_blockedlist"))
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
///
/// * **generator_controller** is an [`Option`] field object of type [`String`].
/// This is the new generator controller contract address.
///
/// /// * **guardian** is an [`Option`] field object of type [`String`].
/// This is the new generator guardian address.
///
/// ##Executor
/// Only the owner can execute this.
pub fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    vesting_contract: Option<String>,
    generator_controller: Option<String>,
    guardian: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(vesting_contract) = vesting_contract {
        config.vesting_contract = addr_validate_to_lower(deps.api, vesting_contract.as_str())?;
    }

    if let Some(generator_controller) = generator_controller {
        config.generator_controller = Some(addr_validate_to_lower(
            deps.api,
            generator_controller.as_str(),
        )?);
    }

    if let Some(guardian) = guardian {
        config.guardian = Some(addr_validate_to_lower(deps.api, guardian.as_str())?);
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
/// * **pools** is a vector of set that contains LP token address and allocation point.
///
/// ##Executor
/// Can only be called by the owner or generator controller
pub fn execute_setup_pools(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pools: Vec<(String, Uint64)>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner && Some(info.sender) != cfg.generator_controller {
        return Err(ContractError::Unauthorized {});
    }

    let pools_set: HashSet<String> = pools.clone().into_iter().map(|pc| pc.0).collect();
    if pools_set.len() != pools.len() {
        return Err(ContractError::PoolDuplicate {});
    }

    let mut setup_pools: Vec<(Addr, Uint64)> = vec![];

    let blacklisted_pair_types: Vec<PairType> = deps.querier.query_wasm_smart(
        cfg.factory.clone(),
        &FactoryQueryMsg::BlacklistedPairTypes {},
    )?;

    for (addr, alloc_point) in pools {
        let pool_addr = addr_validate_to_lower(deps.api, &addr)?;
        let pair_info = pair_info_by_pool(deps.as_ref(), pool_addr.clone())?;

        // check if assets in the blocked list
        for asset in pair_info.asset_infos.clone() {
            if cfg.blocked_list_tokens.contains(&asset) {
                return Err(ContractError::Std(StdError::generic_err(format!(
                    "Token {} is blocked!",
                    asset
                ))));
            }
        }

        // check if pair type is blacklisted
        if blacklisted_pair_types.contains(&pair_info.pair_type) {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Pair type ({}) is blacklisted!",
                pair_info.pair_type
            ))));
        }

        // If a pair gets deregistered from the factory, we should raise error.
        let _: PairInfo = deps
            .querier
            .query_wasm_smart(
                cfg.factory.clone(),
                &FactoryQueryMsg::Pair {
                    asset_infos: pair_info.asset_infos.clone(),
                },
            )
            .map_err(|_| {
                ContractError::Std(StdError::generic_err(format!(
                    "The pair aren't registered: {}-{}",
                    pair_info.asset_infos[0], pair_info.asset_infos[1]
                )))
            })?;

        setup_pools.push((pool_addr, alloc_point));
    }

    let factory_cfg: FactoryConfigResponse = deps
        .querier
        .query_wasm_smart(cfg.factory.clone(), &FactoryQueryMsg::Config {})?;

    let prev_pools: Vec<Addr> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();

    mass_update_pools(deps.branch(), &env, &cfg, &prev_pools)?;

    for (lp_token, _) in &setup_pools {
        if POOL_INFO.may_load(deps.storage, lp_token)?.is_none() {
            create_pool(deps.branch(), &env, lp_token, &cfg, &factory_cfg)?;
        }
    }

    cfg.total_alloc_point = setup_pools.iter().map(|(_, alloc_point)| alloc_point).sum();
    cfg.active_pools = setup_pools;

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::new().add_attribute("action", "setup_pools"))
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise updates the given generator's ASTRO allocation points and
/// returns a [`Response`] with the specified attributes.
///
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the LP token whose generator allocation points we update.
///
/// * **has_asset_rewards** is the field of type [`bool`]. This flag indicates whether the generator receives dual rewards.
///
/// ##Executor
/// Can only be called by the owner.
pub fn execute_update_pool(
    deps: DepsMut,
    info: MessageInfo,
    lp_token: String,
    has_asset_rewards: bool,
) -> Result<Response, ContractError> {
    let lp_token_addr = addr_validate_to_lower(deps.api, &lp_token)?;

    let cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_token_addr)?;

    pool_info.has_asset_rewards = has_asset_rewards;

    POOL_INFO.save(deps.storage, &lp_token_addr, &pool_info)?;

    Ok(Response::new()
        .add_attribute("action", "update_pool")
        .add_attribute("lp_token", lp_token)
        .add_attribute("has_asset_rewards", pool_info.has_asset_rewards.to_string()))
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

    let mut pools: Vec<(Addr, PoolInfo)> = vec![];
    match update_single_pool {
        Some(lp_token) => {
            let pool = POOL_INFO.load(deps.storage, &lp_token)?;
            pools = vec![(lp_token, pool)];
        }
        None => {
            let config = CONFIG.load(deps.storage)?;

            for (lp_token, _) in config.active_pools {
                pools.push((lp_token.clone(), POOL_INFO.load(deps.storage, &lp_token)?))
            }
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

/// ## Description
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

/// ## Description
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
                ExecuteOnReply::ClaimRewards { lp_tokens, account } => {
                    claim_rewards(deps, env, lp_tokens, account)
                }
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

/// ## Description
/// Sets the allocation points to zero for the generator associated with the specified LP token. Recalculates total allocation points.
pub fn deactivate_pool(deps: DepsMut, lp_token: Addr) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    // Gets old allocation points for the pool and subtracts them from total allocation points
    let old_alloc_point = get_alloc_point(&cfg.active_pools, &lp_token);
    cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(old_alloc_point)?;

    // Sets the pool allocation points to zero
    for pool in &mut cfg.active_pools {
        if pool.0 == lp_token {
            pool.1 = Uint64::zero();
            break;
        }
    }

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::new().add_attribute("action", "setup_pool"))
}

/// Sets a new amount of ASTRO distributed per block among all active generators. Before that, we
/// will need to update all pools in order to correctly account for accrued rewards. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **amount** is the object of type [`Uint128`]. Sets a new count of tokens per block.
fn set_tokens_per_block(
    mut deps: DepsMut,
    env: Env,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    let pools: Vec<Addr> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();

    mass_update_pools(deps.branch(), &env, &cfg, &pools)?;

    cfg.tokens_per_block = amount;
    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::new().add_attribute("action", "set_tokens_per_block"))
}

/// ## Description
/// Updates the amount of accrued rewards for all generators. Returns a [`ContractError`] on failure, otherwise
/// returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **cfg** is the object of type [`Config`].
///
/// * **lp_tokens** is the list of type [`Addr`].
pub fn mass_update_pools(
    mut deps: DepsMut,
    env: &Env,
    cfg: &Config,
    lp_tokens: &[Addr],
) -> Result<(), ContractError> {
    for lp_token in lp_tokens {
        let mut pool = POOL_INFO.load(deps.storage, lp_token)?;
        accumulate_rewards_per_share(deps.branch(), env, lp_token, &mut pool, cfg, None)?;
        POOL_INFO.save(deps.storage, lp_token, &pool)?;
    }

    Ok(())
}

/// ## Description
/// Updates the amount of accrued rewards for a specific generator. Returns a [`ContractError`] on
/// failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is the object of type [`Addr`]. Sets the liquidity pool to be updated.
///
/// * **account** is the object of type [`Addr`].
pub fn claim_rewards(
    mut deps: DepsMut,
    env: Env,
    lp_tokens: Vec<Addr>,
    account: Addr,
) -> Result<Response, ContractError> {
    let response = Response::default();

    let cfg = CONFIG.load(deps.storage)?;

    mass_update_pools(deps.branch(), &env, &cfg, &lp_tokens)?;

    let mut send_rewards_msg: Vec<WasmMsg> = vec![];
    for lp_token in &lp_tokens {
        let pool = POOL_INFO.load(deps.storage, lp_token)?;

        let user = USER_INFO.load(deps.storage, (lp_token, &account))?;

        send_rewards_msg.append(&mut send_pending_rewards(&cfg, &pool, &user, &account)?);
    }

    Ok(response
        .add_attribute("action", "claim_rewards")
        .add_messages(send_rewards_msg))
}

/// ## Description
/// Accrues the amount of rewards distributed for each staked LP token in a specific generator.
/// Also update reward variables for the given generator.
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
            let alloc_point = get_alloc_point(&cfg.active_pools, lp_token);

            let token_rewards = calculate_rewards(env, pool, &alloc_point, cfg)?;

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
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let amount = cw20_msg.amount;
    let lp_token = info.sender;

    let cfg = CONFIG.load(deps.storage)?;

    if POOL_INFO.may_load(deps.storage, &lp_token)?.is_none() {
        let factory_cfg: FactoryConfigResponse = deps
            .querier
            .query_wasm_smart(cfg.factory.clone(), &FactoryQueryMsg::Config {})?;

        create_pool(deps.branch(), &env, &lp_token, &cfg, &factory_cfg)?;
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

/// ## Description
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

/// ## Description
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

/// ## Description
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

/// ## Description
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

/// ## Description
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

/// ## Description
/// Sets the allowed reward proxies that can interact with the Generator contract. Returns a [`ContractError`] on
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

/// ## Description
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
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: String,
    proxy: String,
) -> Result<Response, ContractError> {
    let lp_addr = addr_validate_to_lower(deps.api, &lp_token)?;
    let proxy_addr = addr_validate_to_lower(deps.api, &proxy)?;

    let cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if !cfg.allowed_reward_proxies.contains(&proxy_addr) {
        return Err(ContractError::RewardProxyNotAllowed {});
    }

    if POOL_INFO.may_load(deps.storage, &lp_addr)?.is_none() {
        let factory_cfg: FactoryConfigResponse = deps
            .querier
            .query_wasm_smart(cfg.factory.clone(), &FactoryQueryMsg::Config {})?;

        create_pool(deps.branch(), &env, &lp_addr, &cfg, &factory_cfg)?;
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

/// Add or remove proxy contracts to and from the proxy contract whitelist. Returns a [`ContractError`] on failure.
fn update_allowed_proxies(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<String>>,
    remove: Option<Vec<String>>,
) -> Result<Response, ContractError> {
    if add.is_none() && remove.is_none() {
        return Err(ContractError::Std(StdError::generic_err(
            "Need to provide add or remove parameters",
        )));
    }

    let mut cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

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
        QueryMsg::ActivePoolLength {} => Ok(to_binary(&active_pool_length(deps)?)?),
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
        QueryMsg::BlockedListTokens {} => Ok(to_binary(&query_blocked_list_tokens(deps)?)?),
    }
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the blocked list of tokens.
fn query_blocked_list_tokens(deps: Deps) -> Result<Vec<AssetInfo>, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(config.blocked_list_tokens)
}

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
/// Returns a [`ContractError`] on failure, otherwise returns the amount of active generators
/// using a [`PoolLengthResponse`] object.
pub fn active_pool_length(deps: Deps) -> Result<PoolLengthResponse, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    Ok(PoolLengthResponse {
        length: config.active_pools.len(),
    })
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
/// Calculates and returns the pending token rewards for a specific user. Returns a [`ContractError`] on failure, otherwise returns
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
            lp_supply = query_token_balance(
                &deps.querier,
                lp_token.clone(),
                env.contract.address.clone(),
            )?;
        }
    }

    let mut acc_per_share = pool.accumulated_rewards_per_share;
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        let alloc_point = get_alloc_point(&cfg.active_pools, &lp_token);

        let token_rewards = calculate_rewards(&env, &pool, &alloc_point, &cfg)?;
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
        factory: config.factory,
        guardian: config.guardian,
        start_block: config.start_block,
        tokens_per_block: config.tokens_per_block,
        total_alloc_point: config.total_alloc_point,
        vesting_contract: config.vesting_contract,
        generator_controller: config.generator_controller,
        active_pools: config.active_pools,
        blocked_list_tokens: config.blocked_list_tokens,
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
/// * **lp_token** is an object of type [`String`]. This is the LP token whose generator we query.
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
            lp_supply = query_token_balance(
                &deps.querier,
                lp_token.clone(),
                env.contract.address.clone(),
            )?;
        }
    }

    let alloc_point = get_alloc_point(&config.active_pools, &lp_token);

    // Calculate pending ASTRO rewards
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        pending_astro_rewards = calculate_rewards(&env, &pool, &alloc_point, &config)?;
    }

    // Calculate ASTRO tokens being distributed per block to this LP token pool
    let astro_tokens_per_block: Uint128;
    astro_tokens_per_block = config
        .tokens_per_block
        .checked_mul(Uint128::from(alloc_point.u64()))?
        .checked_div(Uint128::from(config.total_alloc_point.u64()))
        .unwrap_or_else(|_| Uint128::zero());

    Ok(PoolInfoResponse {
        alloc_point,
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
    let alloc_point = get_alloc_point(&cfg.active_pools, &lp_token);
    let n_blocks = Uint128::from(future_block)
        .checked_sub(env.block.height.into())
        .unwrap_or_else(|_| Uint128::zero());

    let simulated_reward = n_blocks
        .checked_mul(cfg.tokens_per_block)?
        .checked_mul(Uint128::from(alloc_point.u64()))?
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
/// Calculates and returns the amount of accrued rewards since the last reward checkpoint for a specific generator.
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator for which we calculate accrued rewards.
///
/// * **alloc_point** is the object of type [`Uint64`].
///
/// * **cfg** is an object of type [`Config`]. This is the Generator contract configuration.
pub fn calculate_rewards(
    env: &Env,
    pool: &PoolInfo,
    alloc_point: &Uint64,
    cfg: &Config,
) -> StdResult<Uint128> {
    let n_blocks = Uint128::from(env.block.height).checked_sub(pool.last_reward_block.into())?;

    let r;
    if !cfg.total_alloc_point.is_zero() {
        r = n_blocks
            .checked_mul(cfg.tokens_per_block)?
            .checked_mul(Uint128::from(alloc_point.u64()))?
            .checked_div(Uint128::from(cfg.total_alloc_point.u64()))?;
    } else {
        r = Uint128::zero();
    }

    Ok(r)
}

/// ## Description
/// Gets allocation point of the pool.
/// ## Params
/// * **pools** is a vector of set that contains LP token address and allocation point.
///
/// * **lp_token** is an object of type [`Addr`].
pub fn get_alloc_point(pools: &[(Addr, Uint64)], lp_token: &Addr) -> Uint64 {
    pools
        .iter()
        .find_map(|(addr, alloc_point)| {
            if addr == lp_token {
                return Some(*alloc_point);
            }
            None
        })
        .unwrap_or_else(Uint64::zero)
}

/// ## Description
/// Creates pool if it is allowed in the factory.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`]. This is the
pub fn create_pool(
    deps: DepsMut,
    env: &Env,
    lp_token: &Addr,
    cfg: &Config,
    factory_cfg: &FactoryConfigResponse,
) -> Result<PoolInfo, ContractError> {
    let pair_info = pair_info_by_pool(deps.as_ref(), lp_token.clone())?;

    let mut pair_config: Option<PairConfig> = None;
    for factory_pair_config in &factory_cfg.pair_configs {
        if factory_pair_config.pair_type == pair_info.pair_type {
            pair_config = Some(factory_pair_config.clone());
        }
    }

    if let Some(pair_config) = pair_config {
        if pair_config.is_disabled || pair_config.is_generator_disabled {
            return Err(ContractError::GeneratorIsDisabled {});
        }
    } else {
        return Err(ContractError::PairNotRegistered {});
    }

    POOL_INFO.save(
        deps.storage,
        lp_token,
        &PoolInfo {
            last_reward_block: cfg.start_block.max(Uint64::from(env.block.height)),
            accumulated_rewards_per_share: Decimal::zero(),
            reward_proxy: None,
            accumulated_proxy_rewards_per_share: Decimal::zero(),
            proxy_reward_balance_before_update: Uint128::zero(),
            orphan_proxy_rewards: Uint128::zero(),
            has_asset_rewards: false,
        },
    )?;

    Ok(POOL_INFO.load(deps.storage, lp_token)?)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-generator" => match contract_version.version.as_ref() {
            "1.0.0" => {
                let msg: migration::MigrationMsgV120 = from_binary(&msg.params)?;

                let mut active_pools: Vec<(Addr, Uint64)> = vec![];

                let keys = POOL_INFO
                    .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
                    .map(|v| String::from_utf8(v).map_err(StdError::from))
                    .collect::<Result<Vec<String>, StdError>>()?;

                for key in keys {
                    let pool_info_v100 = migration::POOL_INFOV100
                        .load(deps.storage, &Addr::unchecked(key.clone()))?;

                    if !pool_info_v100.alloc_point.is_zero() {
                        active_pools.push((Addr::unchecked(&key), pool_info_v100.alloc_point));
                    }

                    let pool_info = PoolInfo {
                        has_asset_rewards: false,
                        accumulated_proxy_rewards_per_share: pool_info_v100
                            .accumulated_proxy_rewards_per_share,
                        accumulated_rewards_per_share: pool_info_v100.accumulated_rewards_per_share,
                        last_reward_block: pool_info_v100.last_reward_block,
                        orphan_proxy_rewards: pool_info_v100.orphan_proxy_rewards,
                        proxy_reward_balance_before_update: pool_info_v100
                            .proxy_reward_balance_before_update,
                        reward_proxy: pool_info_v100.reward_proxy,
                    };
                    POOL_INFO.save(deps.storage, &Addr::unchecked(key), &pool_info)?;
                }

                migration::migrate_configs_to_v120(&mut deps, active_pools, msg)?
            }
            "1.1.0" => {
                let msg: migration::MigrationMsgV120 = from_binary(&msg.params)?;

                let mut active_pools: Vec<(Addr, Uint64)> = vec![];

                let keys = POOL_INFO
                    .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending {})
                    .map(|v| String::from_utf8(v).map_err(StdError::from))
                    .collect::<Result<Vec<String>, StdError>>()?;

                for key in keys {
                    let pool_info_v110 = migration::POOL_INFOV110
                        .load(deps.storage, &Addr::unchecked(key.clone()))?;

                    if !pool_info_v110.alloc_point.is_zero() {
                        active_pools.push((Addr::unchecked(&key), pool_info_v110.alloc_point));
                    }

                    let pool_info = PoolInfo {
                        has_asset_rewards: pool_info_v110.has_asset_rewards,
                        accumulated_proxy_rewards_per_share: pool_info_v110
                            .accumulated_proxy_rewards_per_share,
                        accumulated_rewards_per_share: pool_info_v110.accumulated_rewards_per_share,
                        last_reward_block: pool_info_v110.last_reward_block,
                        orphan_proxy_rewards: pool_info_v110.orphan_proxy_rewards,
                        proxy_reward_balance_before_update: pool_info_v110
                            .proxy_reward_balance_before_update,
                        reward_proxy: pool_info_v110.reward_proxy,
                    };
                    POOL_INFO.save(deps.storage, &Addr::unchecked(key), &pool_info)?;
                }

                migration::migrate_configs_to_v120(&mut deps, active_pools, msg)?
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
