use std::collections::{HashMap, HashSet};

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, CosmosMsg, Decimal, Deps, DepsMut,
    Env, MessageInfo, Order, QuerierWrapper, Reply, Response, StdError, StdResult, SubMsg, Uint128,
    Uint64, WasmMsg,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, MinterResponse};
use cw_storage_plus::{Bound, PrimaryKey};
use protobuf::Message;

use crate::error::ContractError;
use crate::migration;

use astroport::asset::{
    addr_opt_validate, addr_validate_to_lower, pair_info_by_pool, token_asset_info, Asset,
    AssetInfo, PairInfo,
};

use astroport::common::{
    claim_ownership, drop_ownership_proposal, propose_new_owner, validate_addresses,
};
use astroport::factory::PairType;
use astroport::generator::{Config, ExecuteOnReply, PoolInfo};
use astroport::generator::{StakerResponse, UserInfoV2};
use astroport::querier::query_token_balance;
use astroport::restricted_vector::RestrictedVector;
use astroport::DecimalCheckedOps;
use astroport::{
    factory::{ConfigResponse as FactoryConfigResponse, QueryMsg as FactoryQueryMsg},
    generator::{
        Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PendingTokenResponse,
        PoolInfoResponse, QueryMsg, RewardInfoResponse,
    },
    generator_proxy::{
        Cw20HookMsg as ProxyCw20HookMsg, ExecuteMsg as ProxyExecuteMsg, QueryMsg as ProxyQueryMsg,
    },
    vesting::ExecuteMsg as VestingExecuteMsg,
};

use crate::response::MsgInstantiateContractResponse;
use crate::state::{
    accumulate_pool_proxy_rewards, update_proxy_asset, update_user_balance, update_virtual_amount,
    CompatibleLoader, CHECKPOINT_GENERATORS_LIMIT, CONFIG, DEFAULT_LIMIT, MAX_LIMIT,
    OWNERSHIP_PROPOSAL, POOL_INFO, PROXY_REWARDS_HOLDER, PROXY_REWARD_ASSET, USER_INFO,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-generator";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const INIT_REWARDS_HOLDER_ID: u64 = 1;

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
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let allowed_reward_proxies = validate_addresses(deps.api, &msg.allowed_reward_proxies)?;

    let generator_controller = addr_opt_validate(deps.api, &msg.generator_controller)?;
    let guardian = addr_opt_validate(deps.api, &msg.guardian)?;
    let voting_escrow = addr_opt_validate(deps.api, &msg.voting_escrow)?;

    let config = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        factory: addr_validate_to_lower(deps.api, &msg.factory)?,
        generator_controller,
        guardian,
        astro_token: addr_validate_to_lower(deps.api, &msg.astro_token)?,
        tokens_per_block: msg.tokens_per_block,
        total_alloc_point: Uint128::zero(),
        start_block: msg.start_block,
        allowed_reward_proxies,
        vesting_contract: addr_validate_to_lower(deps.api, &msg.vesting_contract)?,
        active_pools: vec![],
        blocked_tokens_list: vec![],
        voting_escrow,
        checkpoint_generator_limit: None,
    };

    CONFIG.save(deps.storage, &config)?;

    let init_reward_holder_msg =
        init_proxy_rewards_holder(&config.owner, &env.contract.address, msg.whitelist_code_id)?;

    Ok(Response::default().add_submessage(init_reward_holder_msg))
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
///             voting_escrow,
///             checkpoint_generator_limit,
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
///
/// * **ExecuteMsg::CheckpointUserBoost { user, generators }** Updates the boost emissions for
/// specified user and generators
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CheckpointUserBoost { generators, user } => {
            checkpoint_user_boost(deps, env, info, generators, user)
        }
        ExecuteMsg::DeactivatePools { pair_types } => deactivate_pools(deps, env, pair_types),
        ExecuteMsg::DeactivatePool { lp_token } => {
            let cfg = CONFIG.load(deps.storage)?;
            if info.sender != cfg.factory {
                return Err(ContractError::Unauthorized {});
            }
            let lp_token_addr = addr_validate_to_lower(deps.api, &lp_token)?;
            let active_pools: Vec<_> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
            mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;
            deactivate_pool(deps, cfg, lp_token_addr)
        }
        ExecuteMsg::UpdateBlockedTokenslist { add, remove } => {
            update_blocked_tokens_list(deps, env, info, add, remove)
        }
        ExecuteMsg::MoveToProxy { lp_token, proxy } => {
            move_to_proxy(deps, env, info, lp_token, proxy)
        }
        ExecuteMsg::MigrateProxy {
            lp_token,
            new_proxy,
        } => migrate_proxy(deps, env, info, lp_token, new_proxy),
        ExecuteMsg::UpdateAllowedProxies { add, remove } => {
            update_allowed_proxies(deps, info, add, remove)
        }
        ExecuteMsg::UpdateConfig {
            vesting_contract,
            generator_controller,
            guardian,
            voting_escrow,
            checkpoint_generator_limit,
        } => execute_update_config(
            deps,
            info,
            vesting_contract,
            generator_controller,
            guardian,
            voting_escrow,
            checkpoint_generator_limit,
        ),
        ExecuteMsg::SetupPools { pools } => execute_setup_pools(deps, env, info, pools),
        ExecuteMsg::UpdatePool {
            lp_token,
            has_asset_rewards,
        } => execute_update_pool(deps, info, lp_token, has_asset_rewards),
        ExecuteMsg::ClaimRewards { lp_tokens } => {
            let lp_tokens_addr = validate_addresses(deps.api, &lp_tokens)?;

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
                deps.branch(),
                env,
                Some(lp_token.clone()),
                ExecuteOnReply::Withdraw {
                    lp_token,
                    account: info.sender,
                    amount,
                },
            )
        }
        ExecuteMsg::EmergencyWithdraw { lp_token } => emergency_withdraw(deps, info, lp_token),
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
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::Callback { action } => {
            if info.sender != env.contract.address {
                return Err(ContractError::Unauthorized {});
            }

            handle_callback(deps, env, action)
        }
    }
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise updates user virtual amount boost for specified generators and
/// returns a [`Response`] with the specified attributes.
///
/// ## Params
/// * **generators** is an object of type [`Addr`]. These are the addresses of the generators for
/// which the amount will be recalculated.
///
/// * **user** is an object of type [`Addr`]. This is an address for
/// which the virtual amount will be recalculated.
fn checkpoint_user_boost(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    generators: Vec<String>,
    user: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let recipient_addr = if let Some(user) = user {
        addr_validate_to_lower(deps.api, &user)?
    } else {
        info.sender
    };

    if generators.len()
        > config
            .checkpoint_generator_limit
            .unwrap_or(CHECKPOINT_GENERATORS_LIMIT) as usize
    {
        return Err(ContractError::GeneratorsLimitExceeded {});
    }

    let mut send_rewards_msg: Vec<WasmMsg> = vec![];
    for generator in generators {
        let generator_addr = addr_validate_to_lower(deps.api, &generator)?;

        // calculates the emission boost  only for user who has LP in generator
        if USER_INFO.has(deps.storage, (&generator_addr, &recipient_addr)) {
            let user_info =
                USER_INFO.compatible_load(deps.storage, (&generator_addr, &recipient_addr))?;

            let mut pool = POOL_INFO.load(deps.storage, &generator_addr)?;
            accumulate_rewards_per_share(&deps.querier, &env, &generator_addr, &mut pool, &config)?;

            send_rewards_msg.append(&mut send_pending_rewards(
                deps.as_ref(),
                &config,
                &pool,
                &user_info,
                &recipient_addr,
            )?);

            // Update user's amount
            let amount = user_info.amount;
            let mut user_info = update_user_balance(user_info, &pool, amount)?;

            // Update user's virtual amount
            update_virtual_amount(
                deps.querier,
                &env.contract.address,
                &config.voting_escrow,
                &mut pool,
                &mut user_info,
                &recipient_addr,
                &generator_addr,
            )?;

            USER_INFO.save(deps.storage, (&generator_addr, &recipient_addr), &user_info)?;
            POOL_INFO.save(deps.storage, &generator_addr, &pool)?;
        }
    }

    Ok(Response::new()
        .add_attribute("action", "checkpoint_user_boost")
        .add_messages(send_rewards_msg))
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
            "Pair type duplicate!",
        )));
    }

    let blacklisted_pair_types: Vec<PairType> = deps
        .querier
        .query_wasm_smart(&cfg.factory, &FactoryQueryMsg::BlacklistedPairTypes {})?;

    // checks if each pair type is blacklisted
    for pair_type in &pair_types {
        if !blacklisted_pair_types.contains(pair_type) {
            return Err(ContractError::Std(StdError::generic_err(format!(
                "Pair type ({}) is not blacklisted!",
                pair_type
            ))));
        }
    }

    let active_pools: Vec<_> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
    mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;

    // find active pools with blacklisted pair type
    for pool in &mut cfg.active_pools {
        if !pool.1.is_zero() {
            let pair_info = pair_info_by_pool(&deps.querier, &pool.0)?;
            if pair_types.contains(&pair_info.pair_type) {
                // recalculate total allocation point before resetting the allocation point of pool
                cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(pool.1)?;
                // sets allocation point to zero for each pool with blacklisted pair type
                pool.1 = Uint128::zero();
            }
        }
    }

    CONFIG.save(deps.storage, &cfg)?;
    Ok(Response::new().add_attribute("action", "deactivate_pools"))
}

/// Add or remove tokens to and from the blocked list. Returns a [`ContractError`] on failure.
fn update_blocked_tokens_list(
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
                .blocked_tokens_list
                .iter()
                .position(|x| *x == asset_info)
                .ok_or_else(|| {
                    StdError::generic_err(
                        "Can't remove token. It is not found in the blocked list.",
                    )
                })?;
            cfg.blocked_tokens_list.remove(index);
        }
    }

    // Add tokens to the blacklist
    if let Some(asset_infos) = add {
        let active_pools: Vec<_> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();
        mass_update_pools(deps.branch(), &env, &cfg, &active_pools)?;
        let astro = token_asset_info(cfg.astro_token.clone());

        for asset_info in asset_infos {
            // ASTRO or Terra native assets (UST, LUNA etc) cannot be blacklisted
            if asset_info.is_native_token() || asset_info.eq(&astro) {
                return Err(ContractError::AssetCannotBeBlocked {});
            }

            if !cfg.blocked_tokens_list.contains(&asset_info) {
                cfg.blocked_tokens_list.push(asset_info.clone());

                // Find active pools with blacklisted tokens
                for pool in &mut cfg.active_pools {
                    let pair_info = pair_info_by_pool(&deps.querier, &pool.0)?;
                    if pair_info.asset_infos.contains(&asset_info) {
                        // Recalculate total allocation points before resetting the pool allocation points
                        cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(pool.1)?;
                        // Sets allocation points to zero for each pool with blacklisted tokens
                        pool.1 = Uint128::zero();
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
    voting_escrow: Option<String>,
    checkpoint_generator_limit: Option<u32>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(vesting_contract) = vesting_contract {
        config.vesting_contract = addr_validate_to_lower(deps.api, &vesting_contract)?;
    }

    if let Some(generator_controller) = generator_controller {
        config.generator_controller =
            Some(addr_validate_to_lower(deps.api, &generator_controller)?);
    }

    if let Some(guardian) = guardian {
        config.guardian = Some(addr_validate_to_lower(deps.api, guardian.as_str())?);
    }

    if let Some(voting_escrow) = voting_escrow {
        config.voting_escrow = Some(addr_validate_to_lower(deps.api, voting_escrow.as_str())?);
    }

    if let Some(generator_limit) = checkpoint_generator_limit {
        config.checkpoint_generator_limit = Some(generator_limit);
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
    pools: Vec<(String, Uint128)>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;
    if info.sender != cfg.owner && Some(info.sender) != cfg.generator_controller {
        return Err(ContractError::Unauthorized {});
    }

    let pools_set: HashSet<String> = pools.clone().into_iter().map(|pc| pc.0).collect();
    if pools_set.len() != pools.len() {
        return Err(ContractError::PoolDuplicate {});
    }

    let mut setup_pools: Vec<(Addr, Uint128)> = vec![];

    let blacklisted_pair_types: Vec<PairType> = deps
        .querier
        .query_wasm_smart(&cfg.factory, &FactoryQueryMsg::BlacklistedPairTypes {})?;

    for (addr, alloc_point) in pools {
        let pool_addr = addr_validate_to_lower(deps.api, &addr)?;
        let pair_info = pair_info_by_pool(&deps.querier, &pool_addr)?;

        // check if assets in the blocked list
        for asset in &pair_info.asset_infos {
            if cfg.blocked_tokens_list.contains(asset) {
                return Err(StdError::generic_err(format!("Token {} is blocked!", asset)).into());
            }
        }

        // check if pair type is blacklisted
        if blacklisted_pair_types.contains(&pair_info.pair_type) {
            return Err(StdError::generic_err(format!(
                "Pair type ({}) is blacklisted!",
                pair_info.pair_type
            ))
            .into());
        }

        // If a pair gets deregistered from the factory, we should raise error.
        let _: PairInfo = deps
            .querier
            .query_wasm_smart(
                &cfg.factory,
                &FactoryQueryMsg::Pair {
                    asset_infos: pair_info.asset_infos.clone(),
                },
            )
            .map_err(|_| {
                ContractError::Std(StdError::generic_err(format!(
                    "The pair is not registered: {}-{}",
                    pair_info.asset_infos[0], pair_info.asset_infos[1]
                )))
            })?;

        setup_pools.push((pool_addr, alloc_point));
    }
    let prev_pools: Vec<_> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();

    mass_update_pools(deps.branch(), &env, &cfg, &prev_pools)?;

    for (lp_token, _) in &setup_pools {
        if !POOL_INFO.has(deps.storage, lp_token) {
            create_pool(deps.branch(), &env, lp_token, &cfg)?;
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

    Ok(Response::new().add_attributes(vec![
        attr("action", "update_pool"),
        attr("lp_token", lp_token),
        attr("has_asset_rewards", pool_info.has_asset_rewards.to_string()),
    ]))
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
    deps: DepsMut,
    env: Env,
    update_single_pool: Option<Addr>,
    action_on_reply: ExecuteOnReply,
) -> Result<Response, ContractError> {
    let pools = match update_single_pool {
        Some(lp_token) => {
            let pool = POOL_INFO.load(deps.storage, &lp_token)?;
            vec![(lp_token, pool)]
        }
        None => {
            let config = CONFIG.load(deps.storage)?;

            config
                .active_pools
                .iter()
                .map(|(lp_token, _)| {
                    Ok((lp_token.clone(), POOL_INFO.load(deps.storage, lp_token)?))
                })
                .collect::<StdResult<Vec<_>>>()?
        }
    };

    let mut messages = vec![];
    for (lp_token, mut pool) in pools {
        if let Some(reward_proxy) = pool.reward_proxy.clone() {
            let msg_opt = get_proxy_rewards(deps.querier, &mut pool, &reward_proxy)?;
            POOL_INFO.save(deps.storage, &lp_token, &pool)?;
            if let Some(msg) = msg_opt {
                messages.push(msg);
            }
        }
    }

    if !messages.is_empty() {
        messages.push(action_on_reply.into_submsg(&env)?);
        Ok(Response::new().add_submessages(messages))
    } else {
        handle_callback(deps, env, action_on_reply)
    }
}

/// ## Description
/// Fetches accrued proxy rewards. Snapshots the old amount of rewards that are still unclaimed. Returns a [`ContractError`]
/// on failure. Otherwise returns object of type [`Some(SubMsg)`] if there is pending tokens
/// or returns [`None`] in opposite case.
///
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **pool** is an object of type [`PoolInfo`]. This is the generator associated with the `lp_token`.
///
/// * **reward_proxy** is an object of type [`Addr`]. This is the address of the dual rewards proxy for the target LP/generator.
fn get_proxy_rewards(
    querier: QuerierWrapper,
    pool: &mut PoolInfo,
    reward_proxy: &Addr,
) -> Result<Option<SubMsg>, ContractError> {
    let reward_amount: Uint128 =
        querier.query_wasm_smart(reward_proxy, &ProxyQueryMsg::Reward {})?;

    pool.proxy_reward_balance_before_update = reward_amount;

    let res: Uint128 = querier.query_wasm_smart(reward_proxy, &ProxyQueryMsg::PendingToken {})?;

    Ok(if !res.is_zero() {
        Some(SubMsg::new(WasmMsg::Execute {
            contract_addr: reward_proxy.to_string(),
            funds: vec![],
            msg: to_binary(&ProxyExecuteMsg::UpdateRewards {})?,
        }))
    } else {
        None
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
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INIT_REWARDS_HOLDER_ID => {
            let data = msg
                .result
                .into_result()
                .map_err(|_| StdError::generic_err("Failed to get reply"))?
                .data
                .ok_or_else(|| StdError::generic_err("No data in reply"))?;
            let res: MsgInstantiateContractResponse = Message::parse_from_bytes(data.as_slice())
                .map_err(|_| {
                    StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
                })?;
            let rewards_holder = addr_validate_to_lower(deps.api, &res.contract_address)?;
            PROXY_REWARDS_HOLDER.save(deps.storage, &rewards_holder)?;

            Ok(Response::new().add_attribute("action", "init_rewards_holder"))
        }
        _ => Err(StdError::generic_err("Unknown reply id").into()),
    }
}

/// ## Description
/// Processes callback. Returns a [`ContractError`]
/// on failure, otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
fn handle_callback(
    deps: DepsMut,
    env: Env,
    action: ExecuteOnReply,
) -> Result<Response, ContractError> {
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
        ExecuteOnReply::SetTokensPerBlock { amount } => set_tokens_per_block(deps, env, amount),
        ExecuteOnReply::MigrateProxy {
            lp_addr,
            new_proxy_addr,
        } => migrate_proxy_callback(deps, env, lp_addr, new_proxy_addr),
        ExecuteOnReply::MigrateProxyDepositLP {
            lp_addr,
            prev_proxy_addr,
            amount,
        } => migrate_proxy_deposit_lp(deps, lp_addr, prev_proxy_addr, amount),
    }
}

/// ## Description
/// Sets the allocation points to zero for the generator associated with the specified LP token. Recalculates total allocation points.
pub fn deactivate_pool(
    deps: DepsMut,
    mut cfg: Config,
    lp_token: Addr,
) -> Result<Response, ContractError> {
    // Get old allocation points for the pool and subtract them from the total allocation points
    let old_alloc_point = get_alloc_point(&cfg.active_pools, &lp_token);
    cfg.total_alloc_point = cfg.total_alloc_point.checked_sub(old_alloc_point)?;

    // Set the pool allocation points to zero
    for pool in &mut cfg.active_pools {
        if pool.0 == lp_token {
            pool.1 = Uint128::zero();
            break;
        }
    }

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::new().add_attribute("action", "deactivate_pool"))
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

    let pools: Vec<_> = cfg.active_pools.iter().map(|pool| pool.0.clone()).collect();

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
    deps: DepsMut,
    env: &Env,
    cfg: &Config,
    lp_tokens: &[Addr],
) -> Result<(), ContractError> {
    for lp_token in lp_tokens {
        let mut pool = POOL_INFO.load(deps.storage, lp_token)?;
        accumulate_rewards_per_share(&deps.querier, env, lp_token, &mut pool, cfg)?;
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
    let cfg = CONFIG.load(deps.storage)?;

    mass_update_pools(deps.branch(), &env, &cfg, &lp_tokens)?;

    let mut send_rewards_msg = vec![];
    for lp_token in &lp_tokens {
        let mut pool = POOL_INFO.load(deps.storage, lp_token)?;
        let user = USER_INFO.compatible_load(deps.storage, (lp_token, &account))?;

        send_rewards_msg.append(&mut send_pending_rewards(
            deps.as_ref(),
            &cfg,
            &pool,
            &user,
            &account,
        )?);

        // Update user's amount
        let amount = user.amount;
        let mut user = update_user_balance(user, &pool, amount)?;

        // Update user's virtual amount
        update_virtual_amount(
            deps.querier,
            &env.contract.address,
            &cfg.voting_escrow,
            &mut pool,
            &mut user,
            &account,
            lp_token,
        )?;

        USER_INFO.save(deps.storage, (lp_token, &account), &user)?;
        POOL_INFO.save(deps.storage, lp_token, &pool)?;
    }

    Ok(Response::default()
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
    querier: &QuerierWrapper,
    env: &Env,
    lp_token: &Addr,
    pool: &mut PoolInfo,
    cfg: &Config,
) -> StdResult<()> {
    if let Some(proxy) = &pool.reward_proxy {
        let proxy_lp_supply: Uint128 =
            querier.query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

        if !proxy_lp_supply.is_zero() {
            let reward_amount: Uint128 =
                querier.query_wasm_smart(proxy, &ProxyQueryMsg::Reward {})?;

            let token_rewards =
                reward_amount.checked_sub(pool.proxy_reward_balance_before_update)?;

            let share = Decimal::from_ratio(token_rewards, proxy_lp_supply);
            pool.accumulated_proxy_rewards_per_share
                .update(proxy, share)?;
            pool.proxy_reward_balance_before_update = reward_amount;
        }
    }

    // we should calculate rewards by previous virtual amount
    let lp_supply = pool.total_virtual_supply;

    if env.block.height > pool.last_reward_block.u64() {
        if !lp_supply.is_zero() {
            let alloc_point = get_alloc_point(&cfg.active_pools, lp_token);

            let token_rewards = calculate_rewards(
                env.block.height - pool.last_reward_block.u64(),
                &alloc_point,
                cfg,
            )?;

            let share = Decimal::from_ratio(token_rewards, lp_supply);
            pool.reward_global_index = pool.reward_global_index.checked_add(share)?;
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

    if !POOL_INFO.has(deps.storage, &lp_token) {
        create_pool(deps.branch(), &env, &lp_token, &cfg)?;
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Deposit {} => {
            let account = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
            update_rewards_and_execute(
                deps,
                env,
                Some(lp_token.clone()),
                ExecuteOnReply::Deposit {
                    lp_token,
                    account,
                    amount,
                },
            )
        }
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
    deps: Deps,
    cfg: &Config,
    pool: &PoolInfo,
    user: &UserInfoV2,
    to: &Addr,
) -> Result<Vec<WasmMsg>, ContractError> {
    if user.amount.is_zero() {
        return Ok(vec![]);
    }

    let mut messages = vec![];

    let pending_rewards =
        (pool.reward_global_index - user.reward_user_index).checked_mul(user.virtual_amount)?;

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

    let proxy_rewards = accumulate_pool_proxy_rewards(pool, user)?;

    let proxy_rewards_holder = PROXY_REWARDS_HOLDER.load(deps.storage)?;
    for (proxy, pending_proxy_rewards) in proxy_rewards {
        if !pending_proxy_rewards.is_zero() {
            match &pool.reward_proxy {
                Some(reward_proxy) if reward_proxy == &proxy => {
                    messages.push(WasmMsg::Execute {
                        contract_addr: proxy.to_string(),
                        funds: vec![],
                        msg: to_binary(&ProxyExecuteMsg::SendRewards {
                            account: to.to_string(),
                            amount: pending_proxy_rewards,
                        })?,
                    });
                }
                _ => {
                    // Old proxy rewards are paid from reward holder
                    let asset_info = PROXY_REWARD_ASSET.load(deps.storage, &proxy)?;
                    messages.push(WasmMsg::Execute {
                        contract_addr: proxy_rewards_holder.to_string(),
                        funds: vec![],
                        msg: to_binary(&astroport::whitelist::ExecuteMsg::Execute {
                            msgs: vec![Asset {
                                info: asset_info,
                                amount: pending_proxy_rewards,
                            }
                            .into_msg(&deps.querier, to.clone())?],
                        })?,
                    });
                }
            }
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
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
    beneficiary: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let user = USER_INFO
        .compatible_load(deps.storage, (&lp_token, &beneficiary))
        .unwrap_or_default();

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    accumulate_rewards_per_share(&deps.querier, &env, &lp_token, &mut pool, &cfg)?;

    // Send pending rewards (if any) to the depositor
    let send_rewards_msg = send_pending_rewards(deps.as_ref(), &cfg, &pool, &user, &beneficiary)?;

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
    let mut user = update_user_balance(user, &pool, updated_amount)?;

    update_virtual_amount(
        deps.querier,
        &env.contract.address,
        &cfg.voting_escrow,
        &mut pool,
        &mut user,
        &beneficiary,
        &lp_token,
    )?;

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
    deps: DepsMut,
    env: Env,
    lp_token: Addr,
    account: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let user = USER_INFO
        .compatible_load(deps.storage, (&lp_token, &account))
        .unwrap_or_default();
    if user.amount < amount {
        return Err(ContractError::BalanceTooSmall {});
    }

    let cfg = CONFIG.load(deps.storage)?;
    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;

    accumulate_rewards_per_share(&deps.querier, &env, &lp_token, &mut pool, &cfg)?;

    // Send pending rewards to the user
    let send_rewards_msg = send_pending_rewards(deps.as_ref(), &cfg, &pool, &user, &account)?;

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
    let mut user = update_user_balance(user, &pool, updated_amount)?;

    update_virtual_amount(
        deps.querier,
        &env.contract.address,
        &cfg.voting_escrow,
        &mut pool,
        &mut user,
        &account,
        &lp_token,
    )?;

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
            None => query_token_balance(&deps.querier, lp_token, &env.contract.address)? - deposit,
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
/// * **info** is an object of type [`MessageInfo`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token to withdraw.
pub fn emergency_withdraw(
    deps: DepsMut,
    info: MessageInfo,
    lp_token: String,
) -> Result<Response, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

    let mut pool = POOL_INFO.load(deps.storage, &lp_token)?;
    let user = USER_INFO.compatible_load(deps.storage, (&lp_token, &info.sender))?;

    // Instantiate the transfer call for the LP token
    let transfer_msg;
    if let Some(proxy) = &pool.reward_proxy {
        let accumulated_proxy_rewards: HashMap<_, _> = accumulate_pool_proxy_rewards(&pool, &user)?
            .into_iter()
            .collect();
        // All users' proxy rewards become orphaned
        pool.orphan_proxy_rewards = pool
            .orphan_proxy_rewards
            .inner_ref()
            .iter()
            .map(|(addr, amount)| {
                let user_amount = accumulated_proxy_rewards
                    .get(addr)
                    .cloned()
                    .unwrap_or_default();
                let amount = amount.checked_add(user_amount)?;
                Ok((addr.clone(), amount))
            })
            .collect::<StdResult<Vec<_>>>()?
            .into();

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

    if pool.orphan_proxy_rewards.inner_ref().is_empty() {
        return Err(ContractError::ZeroOrphanRewards {});
    }

    let proxy_rewards_holder = PROXY_REWARDS_HOLDER.load(deps.storage)?;
    let submessages = pool
        .orphan_proxy_rewards
        .inner_ref()
        .iter()
        .filter(|(_, value)| !value.is_zero())
        .map(|(proxy, amount)| {
            let msg = match &pool.reward_proxy {
                Some(reward_proxy) if reward_proxy == proxy => SubMsg::new(WasmMsg::Execute {
                    contract_addr: reward_proxy.to_string(),
                    funds: vec![],
                    msg: to_binary(&ProxyExecuteMsg::SendRewards {
                        account: recipient.to_string(),
                        amount: *amount,
                    })?,
                }),
                _ => {
                    let asset_info = PROXY_REWARD_ASSET.load(deps.storage, proxy)?;
                    SubMsg::new(WasmMsg::Execute {
                        contract_addr: proxy_rewards_holder.to_string(),
                        funds: vec![],
                        msg: to_binary(&astroport::whitelist::ExecuteMsg::Execute {
                            msgs: vec![Asset {
                                info: asset_info,
                                amount: *amount,
                            }
                            .into_msg(&deps.querier, &recipient)?],
                        })?,
                    })
                }
            };

            Ok(msg)
        })
        .collect::<StdResult<Vec<_>>>()?;

    // Clear the orphaned proxy rewards
    pool.orphan_proxy_rewards = Default::default();

    POOL_INFO.save(deps.storage, &lp_token, &pool)?;

    Ok(Response::new()
        .add_submessages(submessages)
        .add_attribute("action", "send_orphan_rewards")
        .add_attribute("recipient", recipient)
        .add_attribute("lp_token", lp_token.to_string()))
}

/// ## Description
/// Return a message object that can help claim bLUNA rewards for an account.
/// Returns an [`ContractError`] on failure, otherwise returns the object
/// of type [`SubMsg`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
fn init_proxy_rewards_holder(
    owner: &Addr,
    admin: &Addr,
    whitelist_code_id: u64,
) -> Result<SubMsg, ContractError> {
    let msg = SubMsg::reply_on_success(
        CosmosMsg::Wasm(WasmMsg::Instantiate {
            admin: Some(owner.to_string()),
            code_id: whitelist_code_id,
            funds: vec![],
            label: "Proxy rewards holder".to_string(),
            msg: to_binary(&astroport::whitelist::InstantiateMsg {
                admins: vec![admin.to_string()],
                mutable: false,
            })?,
        }),
        INIT_REWARDS_HOLDER_ID,
    );

    Ok(msg)
}

fn migrate_proxy(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    lp_token: String,
    new_proxy: String,
) -> Result<Response, ContractError> {
    let lp_addr = addr_validate_to_lower(deps.api, &lp_token)?;
    let new_proxy_addr = addr_validate_to_lower(deps.api, &new_proxy)?;

    let cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    if !cfg.allowed_reward_proxies.contains(&new_proxy_addr) {
        return Err(ContractError::RewardProxyNotAllowed {});
    }

    // Check the pool has reward proxy
    let pool_info = POOL_INFO.load(deps.storage, &lp_addr)?;
    if let Some(proxy) = &pool_info.reward_proxy {
        if proxy == &new_proxy_addr {
            return Err(StdError::generic_err("Can not migrate to the same proxy").into());
        }
    } else {
        return Err(StdError::generic_err("Pool does not have proxy").into());
    }

    update_rewards_and_execute(
        deps,
        env,
        Some(lp_addr.clone()),
        ExecuteOnReply::MigrateProxy {
            lp_addr,
            new_proxy_addr,
        },
    )
}

fn migrate_proxy_callback(
    mut deps: DepsMut,
    env: Env,
    lp_addr: Addr,
    new_proxy_addr: Addr,
) -> Result<Response, ContractError> {
    let mut pool_info = POOL_INFO.load(deps.storage, &lp_addr)?;
    let cfg = CONFIG.load(deps.storage)?;
    accumulate_rewards_per_share(&deps.querier, &env, &lp_addr, &mut pool_info, &cfg)?;

    // We've checked this before the callback so it's safe to unwrap here
    let prev_proxy_addr = pool_info.reward_proxy.clone().unwrap();

    let proxy_lp_balance: Uint128 = deps
        .querier
        .query_wasm_smart(&prev_proxy_addr, &ProxyQueryMsg::Deposit {})?;

    // Since we migrate to another proxy the proxy reward balance becomes 0.
    pool_info.proxy_reward_balance_before_update = Uint128::zero();
    // Save a new index and orphan rewards for the new proxy
    pool_info
        .accumulated_proxy_rewards_per_share
        .update(&new_proxy_addr, Decimal::zero())?;
    pool_info
        .orphan_proxy_rewards
        .update(&new_proxy_addr, Uint128::zero())?;
    // Set new proxy
    pool_info.reward_proxy = Some(new_proxy_addr.clone());

    POOL_INFO.save(deps.storage, &lp_addr, &pool_info)?;

    update_proxy_asset(deps.branch(), &new_proxy_addr)?;

    let mut response = Response::new();

    // Transfer whole proxy reward balance to the rewards holder
    let rewards_amount: Uint128 = deps
        .querier
        .query_wasm_smart(&prev_proxy_addr, &ProxyQueryMsg::Reward {})?;
    if !rewards_amount.is_zero() {
        let rewards_holder = PROXY_REWARDS_HOLDER.load(deps.storage)?;
        let trasfer_rewards_msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: prev_proxy_addr.to_string(),
            msg: to_binary(&ProxyExecuteMsg::SendRewards {
                account: rewards_holder.to_string(),
                amount: rewards_amount,
            })?,
            funds: vec![],
        });
        response = response.add_submessage(trasfer_rewards_msg);
    }

    // Migrate all LP tokens to new proxy contract
    if !proxy_lp_balance.is_zero() {
        // Firstly, transfer LP tokens to the generator address
        let transfer_lp_msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: prev_proxy_addr.to_string(),
            msg: to_binary(&ProxyExecuteMsg::Withdraw {
                account: env.contract.address.to_string(),
                amount: proxy_lp_balance,
            })?,
            funds: vec![],
        });
        // Secondly, depositing them to new proxy through generator balance
        let proxy_deposit_msg = ExecuteOnReply::MigrateProxyDepositLP {
            lp_addr,
            prev_proxy_addr,
            amount: proxy_lp_balance,
        }
        .into_submsg(&env)?;
        Ok(response.add_submessages(vec![transfer_lp_msg, proxy_deposit_msg]))
    } else {
        // Nothing to migrate
        Ok(response.add_attributes([
            attr("action", "migrate_proxy"),
            attr("lp_token", lp_addr),
            attr("from", prev_proxy_addr),
            attr("to", new_proxy_addr),
        ]))
    }
}

fn migrate_proxy_deposit_lp(
    deps: DepsMut,
    lp_addr: Addr,
    prev_proxy: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let pool_info = POOL_INFO.load(deps.storage, &lp_addr)?;
    // We've set it before the callback so it's safe to unwrap here
    let new_proxy = pool_info.reward_proxy.unwrap();

    // Depositing LP tokens to new proxy
    let deposit_msg = WasmMsg::Execute {
        contract_addr: lp_addr.to_string(),
        msg: to_binary(&Cw20ExecuteMsg::Send {
            contract: new_proxy.to_string(),
            msg: to_binary(&ProxyCw20HookMsg::Deposit {})?,
            amount,
        })?,
        funds: vec![],
    };

    Ok(Response::new().add_message(deposit_msg).add_attributes([
        attr("action", "migrate_proxy"),
        attr("lp_token", lp_addr),
        attr("from", prev_proxy),
        attr("to", new_proxy),
    ]))
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

    if !POOL_INFO.has(deps.storage, &lp_addr) {
        create_pool(deps.branch(), &env, &lp_addr, &cfg)?;
    }

    let mut pool_info = POOL_INFO.load(deps.storage, &lp_addr)?;
    if pool_info.reward_proxy.is_some() {
        return Err(ContractError::PoolAlreadyHasRewardProxyContract {});
    }

    update_proxy_asset(deps.branch(), &proxy_addr)?;
    pool_info
        .orphan_proxy_rewards
        .update(&proxy_addr, Uint128::zero())?;
    pool_info
        .accumulated_proxy_rewards_per_share
        .update(&proxy_addr, Decimal::zero())?;
    pool_info.reward_proxy = Some(proxy_addr);

    let res: BalanceResponse = deps.querier.query_wasm_smart(
        &lp_addr,
        &Cw20QueryMsg::Balance {
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
        QueryMsg::TotalVirtualSupply { generator } => {
            Ok(to_binary(&total_virtual_supply(deps, generator)?)?)
        }
        QueryMsg::ActivePoolLength {} => {
            let config = CONFIG.load(deps.storage)?;
            Ok(to_binary(&config.active_pools.len())?)
        }
        QueryMsg::PoolLength {} => {
            let length = POOL_INFO
                .keys(deps.storage, None, None, Order::Ascending)
                .count();
            Ok(to_binary(&length)?)
        }
        QueryMsg::UserVirtualAmount { lp_token, user } => {
            Ok(to_binary(&query_virtual_amount(deps, lp_token, user)?)?)
        }
        QueryMsg::Deposit { lp_token, user } => {
            Ok(to_binary(&query_deposit(deps, lp_token, user)?)?)
        }
        QueryMsg::PendingToken { lp_token, user } => {
            Ok(to_binary(&pending_token(deps, env, lp_token, user)?)?)
        }
        QueryMsg::Config {} => Ok(to_binary(&CONFIG.load(deps.storage)?)?),
        QueryMsg::RewardInfo { lp_token } => Ok(to_binary(&query_reward_info(deps, lp_token)?)?),
        QueryMsg::OrphanProxyRewards { lp_token } => {
            Ok(to_binary(&query_orphan_proxy_rewards(deps, lp_token)?)?)
        }
        QueryMsg::PoolInfo { lp_token } => Ok(to_binary(&query_pool_info(deps, env, lp_token)?)?),
        QueryMsg::SimulateFutureReward {
            lp_token,
            future_block,
        } => {
            let cfg = CONFIG.load(deps.storage)?;
            let alloc_point = get_alloc_point(
                &cfg.active_pools,
                &addr_validate_to_lower(deps.api, &lp_token)?,
            );

            Ok(to_binary(&calculate_rewards(
                future_block - env.block.height,
                &alloc_point,
                &cfg,
            )?)?)
        }
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
        QueryMsg::BlockedTokensList {} => {
            Ok(to_binary(&CONFIG.load(deps.storage)?.blocked_tokens_list)?)
        }
    }
}

/// ## Description
/// Return total virtual supply by pool
pub fn total_virtual_supply(deps: Deps, generator: String) -> Result<Uint128, ContractError> {
    let generator_addr = addr_validate_to_lower(deps.api, &generator)?;
    let pool = POOL_INFO.load(deps.storage, &generator_addr)?;

    Ok(pool.total_virtual_supply)
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
        .compatible_load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();
    Ok(user_info.amount)
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the user virtual amount in a specific generator.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token for which we query the user's emission rewards for.
///
/// * **user** is an object of type [`String`]. This is the user whose virtual amount we're query.
pub fn query_virtual_amount(
    deps: Deps,
    lp_token: String,
    user: String,
) -> Result<Uint128, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;
    let user = addr_validate_to_lower(deps.api, &user)?;

    let user_info = USER_INFO
        .compatible_load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();
    Ok(user_info.virtual_amount)
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
        .compatible_load(deps.storage, (&lp_token, &user))
        .unwrap_or_default();

    let mut pending_on_proxy = None;

    if let Some(proxy) = &pool.reward_proxy {
        let proxy_lp_supply: Uint128 = deps
            .querier
            .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?;

        if !proxy_lp_supply.is_zero() {
            let proxy_rewards = accumulate_pool_proxy_rewards(&pool, &user_info)?
                .into_iter()
                .map(|(proxy_addr, mut reward)| {
                    // Add reward pending on proxy
                    if proxy_addr.eq(proxy) {
                        let res: Option<Uint128> = deps
                            .querier
                            .query_wasm_smart(proxy, &ProxyQueryMsg::PendingToken {})?;
                        if let Some(token_rewards) = res {
                            let share = user_info
                                .amount
                                .multiply_ratio(token_rewards, proxy_lp_supply);
                            reward = reward.checked_add(share)?;
                        }
                    }
                    let info = PROXY_REWARD_ASSET.load(deps.storage, &proxy_addr)?;
                    Ok(Asset {
                        info,
                        amount: reward,
                    })
                })
                .collect::<StdResult<Vec<_>>>()?;

            pending_on_proxy = Some(proxy_rewards);
        }
    }

    let lp_supply = pool.total_virtual_supply;

    let mut acc_per_share = pool.reward_global_index;
    if env.block.height > pool.last_reward_block.u64() && !lp_supply.is_zero() {
        let alloc_point = get_alloc_point(&cfg.active_pools, &lp_token);

        let token_rewards = calculate_rewards(
            env.block.height - pool.last_reward_block.u64(),
            &alloc_point,
            &cfg,
        )?;
        let share = Decimal::from_ratio(token_rewards, lp_supply);
        acc_per_share = pool.reward_global_index.checked_add(share)?;
    }

    // we should calculate rewards by virtual amount
    let pending =
        (acc_per_share - user_info.reward_user_index).checked_mul(user_info.virtual_amount)?;

    Ok(PendingTokenResponse {
        pending,
        pending_on_proxy,
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

    let proxy_reward_token = pool
        .reward_proxy
        .map(|proxy| {
            deps.querier
                .query_wasm_smart(&proxy, &ProxyQueryMsg::RewardInfo {})
        })
        .transpose()?;

    Ok(RewardInfoResponse {
        base_reward_token: config.astro_token,
        proxy_reward_token,
    })
}

/// Returns a [`ContractError`] on failure, otherwise returns a vector of pairs (asset, amount),
/// where 'asset' is an object of type [`AssetInfo`] and 'amount' is amount of orphaned proxy rewards for a specific generator.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **lp_token** is an object of type [`String`]. This is the LP token whose generator we query for orphaned rewards.
fn query_orphan_proxy_rewards(
    deps: Deps,
    lp_token: String,
) -> Result<Vec<(AssetInfo, Uint128)>, ContractError> {
    let lp_token = addr_validate_to_lower(deps.api, &lp_token)?;

    let pool = POOL_INFO.load(deps.storage, &lp_token)?;
    if pool.reward_proxy.is_some() {
        let orphan_rewards = pool
            .orphan_proxy_rewards
            .inner_ref()
            .iter()
            .map(|(proxy, amount)| {
                let asset = PROXY_REWARD_ASSET.load(deps.storage, proxy)?;
                Ok((asset, *amount))
            })
            .collect::<StdResult<Vec<_>>>()?;
        Ok(orphan_rewards)
    } else {
        Err(ContractError::PoolDoesNotHaveAdditionalRewards {})
    }
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
        pending_astro_rewards = calculate_rewards(
            env.block.height - pool.last_reward_block.u64(),
            &alloc_point,
            &config,
        )?;
    }

    // Calculate ASTRO tokens being distributed per block to this LP token pool
    let astro_tokens_per_block = config
        .tokens_per_block
        .checked_mul(alloc_point)?
        .checked_div(config.total_alloc_point)
        .unwrap_or_else(|_| Uint128::zero());

    Ok(PoolInfoResponse {
        alloc_point,
        astro_tokens_per_block,
        last_reward_block: pool.last_reward_block.u64(),
        current_block: env.block.height,
        pending_astro_rewards,
        reward_proxy: pool.reward_proxy,
        pending_proxy_rewards: pending_on_proxy,
        accumulated_proxy_rewards_per_share: pool
            .accumulated_proxy_rewards_per_share
            .inner_ref()
            .clone(),
        proxy_reward_balance_before_update: pool.proxy_reward_balance_before_update,
        orphan_proxy_rewards: pool.orphan_proxy_rewards.inner_ref().clone(),
        lp_supply,
        global_reward_index: pool.reward_global_index,
    })
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
    let lp_addr = addr_validate_to_lower(deps.api, &lp_token)?;
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
/// * **alloc_point** is the object of type [`Uint64`].
///
/// * **cfg** is an object of type [`Config`]. This is the Generator contract configuration.
pub fn calculate_rewards(n_blocks: u64, alloc_point: &Uint128, cfg: &Config) -> StdResult<Uint128> {
    let r = Uint128::from(n_blocks)
        .checked_mul(cfg.tokens_per_block)?
        .checked_mul(*alloc_point)?
        .checked_div(cfg.total_alloc_point)
        .unwrap_or_else(|_| Uint128::zero());

    Ok(r)
}

/// ## Description
/// Gets allocation point of the pool.
/// ## Params
/// * **pools** is a vector of set that contains LP token address and allocation point.
///
/// * **lp_token** is an object of type [`Addr`].
pub fn get_alloc_point(pools: &[(Addr, Uint128)], lp_token: &Addr) -> Uint128 {
    pools
        .iter()
        .find_map(|(addr, alloc_point)| {
            if addr == lp_token {
                return Some(*alloc_point);
            }
            None
        })
        .unwrap_or_else(Uint128::zero)
}

/// ## Description
/// Creates pool if it is allowed in the factory.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **lp_token** is an object of type [`Addr`].
///
/// * **cfg** is an object of type [`Config`].
pub fn create_pool(
    deps: DepsMut,
    env: &Env,
    lp_token: &Addr,
    cfg: &Config,
) -> Result<(), ContractError> {
    let factory_cfg: FactoryConfigResponse = deps
        .querier
        .query_wasm_smart(&cfg.factory, &FactoryQueryMsg::Config {})?;

    let pair_info = pair_info_by_pool(&deps.querier, lp_token)?;
    let pair_config = factory_cfg
        .pair_configs
        .into_iter()
        .find(|pair| pair.pair_type == pair_info.pair_type)
        .ok_or(ContractError::PairNotRegistered {})?;

    if pair_config.is_disabled || pair_config.is_generator_disabled {
        return Err(ContractError::GeneratorIsDisabled {});
    }

    POOL_INFO.save(
        deps.storage,
        lp_token,
        &PoolInfo {
            last_reward_block: cfg.start_block.max(Uint64::from(env.block.height)),
            reward_proxy: None,
            accumulated_proxy_rewards_per_share: Default::default(),
            proxy_reward_balance_before_update: Uint128::zero(),
            orphan_proxy_rewards: Default::default(),
            has_asset_rewards: false,
            reward_global_index: Decimal::zero(),
            total_virtual_supply: Default::default(),
        },
    )?;

    Ok(())
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
pub fn migrate(mut deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-generator" => {
            let keys = POOL_INFO
                .keys(deps.storage, None, None, Order::Ascending {})
                .map(|v| String::from_utf8(v).map_err(StdError::from))
                .collect::<Result<Vec<String>, StdError>>()?;

            match contract_version.version.as_ref() {
                "1.0.0" => {
                    let mut active_pools: Vec<(Addr, Uint64)> = vec![];

                    for key in keys {
                        let pool_info_v100 = migration::POOL_INFOV100
                            .load(deps.storage, &Addr::unchecked(key.clone()))?;

                        if !pool_info_v100.alloc_point.is_zero() {
                            active_pools.push((Addr::unchecked(&key), pool_info_v100.alloc_point));
                        }

                        let mut accumulated_proxy_rewards_per_share = Default::default();
                        let mut orphan_proxy_rewards = Default::default();
                        if let Some(proxy) = &pool_info_v100.reward_proxy {
                            accumulated_proxy_rewards_per_share = RestrictedVector::new(
                                proxy.clone(),
                                pool_info_v100.accumulated_proxy_rewards_per_share,
                            );
                            orphan_proxy_rewards = RestrictedVector::new(
                                proxy.clone(),
                                pool_info_v100.orphan_proxy_rewards,
                            );
                            update_proxy_asset(deps.branch(), proxy)?;
                        }

                        let lp_balance = if let Some(proxy) = &pool_info_v100.reward_proxy {
                            deps.querier
                                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?
                        } else {
                            let res: BalanceResponse = deps.querier.query_wasm_smart(
                                &key,
                                &Cw20QueryMsg::Balance {
                                    address: env.contract.address.to_string(),
                                },
                            )?;
                            res.balance
                        };

                        let pool_info = PoolInfo {
                            has_asset_rewards: false,
                            accumulated_proxy_rewards_per_share,
                            orphan_proxy_rewards,
                            last_reward_block: pool_info_v100.last_reward_block,
                            proxy_reward_balance_before_update: pool_info_v100
                                .proxy_reward_balance_before_update,
                            reward_proxy: pool_info_v100.reward_proxy,
                            reward_global_index: pool_info_v100.accumulated_rewards_per_share,
                            total_virtual_supply: lp_balance,
                        };
                        POOL_INFO.save(deps.storage, &Addr::unchecked(key), &pool_info)?;
                    }

                    migration::migrate_configs_from_v100(&mut deps, active_pools, &msg)?;
                }
                "1.2.0" => {
                    for key in keys {
                        let pool_info_v120 = migration::POOL_INFOV120
                            .load(deps.storage, &Addr::unchecked(key.clone()))?;

                        let mut accumulated_proxy_rewards_per_share = Default::default();
                        let mut orphan_proxy_rewards = Default::default();
                        if let Some(proxy) = &pool_info_v120.reward_proxy {
                            accumulated_proxy_rewards_per_share = RestrictedVector::new(
                                proxy.clone(),
                                pool_info_v120.accumulated_proxy_rewards_per_share,
                            );
                            orphan_proxy_rewards = RestrictedVector::new(
                                proxy.clone(),
                                pool_info_v120.orphan_proxy_rewards,
                            );
                            update_proxy_asset(deps.branch(), proxy)?;
                        }

                        let lp_balance = if let Some(proxy) = &pool_info_v120.reward_proxy {
                            deps.querier
                                .query_wasm_smart(proxy, &ProxyQueryMsg::Deposit {})?
                        } else {
                            let res: BalanceResponse = deps.querier.query_wasm_smart(
                                &key,
                                &Cw20QueryMsg::Balance {
                                    address: env.contract.address.to_string(),
                                },
                            )?;
                            res.balance
                        };

                        let pool_info = PoolInfo {
                            has_asset_rewards: pool_info_v120.has_asset_rewards,
                            accumulated_proxy_rewards_per_share,
                            orphan_proxy_rewards,
                            last_reward_block: pool_info_v120.last_reward_block,
                            proxy_reward_balance_before_update: pool_info_v120
                                .proxy_reward_balance_before_update,
                            reward_proxy: pool_info_v120.reward_proxy,
                            reward_global_index: pool_info_v120.accumulated_rewards_per_share,
                            total_virtual_supply: lp_balance,
                        };
                        POOL_INFO.save(deps.storage, &Addr::unchecked(key), &pool_info)?;
                    }
                    migration::migrate_configs_from_v120(&mut deps, &msg)?;
                }
                _ => return Err(ContractError::MigrationError {}),
            }
        }
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let mut response = Response::new();
    // Initialize the contract if it is not already initialized
    if PROXY_REWARDS_HOLDER.may_load(deps.storage)?.is_none() {
        let config = CONFIG.load(deps.storage)?;
        let init_reward_holder_msg = init_proxy_rewards_holder(
            &config.owner,
            &env.contract.address,
            msg.whitelist_code_id.unwrap(),
        )?;
        response = response.add_submessage(init_reward_holder_msg);
    }

    Ok(response
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
