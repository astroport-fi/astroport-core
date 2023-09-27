use std::cmp::Ordering;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, to_binary, BankMsg, Binary, BlockInfo, Coin, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use astroport::asset::{addr_opt_validate, validate_native_denom, Asset, AssetInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};

use astroport::shared_multisig::{
    Config, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, MultisigRole, PoolType,
    ProvideParams, QueryMsg, DEFAULT_WEIGHT, TOTAL_WEIGHT,
};

use astroport::generator::{
    Cw20HookMsg, ExecuteMsg as GeneratorExecuteMsg, QueryMsg as GeneratorQueryMsg,
};

use astroport::querier::{query_balance, query_token_balance};
use cw2::set_contract_version;
use cw3::{
    Proposal, ProposalListResponse, ProposalResponse, Status, Vote, VoteInfo, VoteListResponse,
    VoteResponse, Votes,
};
use cw_storage_plus::Bound;
use cw_utils::{Duration, Expiration, Threshold};

use crate::error::ContractError;
use crate::state::{
    load_vote, next_id, update_distributed_rewards, BALLOTS, CONFIG, DEFAULT_LIMIT,
    MANAGER1_PROPOSAL, MANAGER2_PROPOSAL, MAX_LIMIT, PROPOSALS,
};
use crate::utils::{
    check_generator_deposit, check_pool, check_provide_assets, get_pool_info,
    prepare_provide_after_withdraw_msg, prepare_provide_msg, prepare_withdraw_msg,
};

// version info for migration info
const CONTRACT_NAME: &str = "astroport-shared-multisig";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    validate_native_denom(msg.denom1.as_str())?;
    validate_native_denom(msg.denom2.as_str())?;

    let cfg = Config {
        threshold: Threshold::AbsoluteCount {
            weight: TOTAL_WEIGHT,
        },
        total_weight: TOTAL_WEIGHT,
        max_voting_period: msg.max_voting_period,
        factory_addr: deps.api.addr_validate(&msg.factory_addr)?,
        generator_addr: deps.api.addr_validate(&msg.generator_addr)?,
        manager1: deps.api.addr_validate(&msg.manager1)?,
        manager2: deps.api.addr_validate(&msg.manager2)?,
        target_pool: addr_opt_validate(deps.api, &msg.target_pool)?,
        migration_pool: None,
        rage_quit_started: false,
        denom1: msg.denom1,
        denom2: msg.denom2,
    };

    if let Some(target_pool) = &cfg.target_pool {
        check_pool(&deps.querier, target_pool, &cfg)?;
    }

    CONFIG.save(deps.storage, &cfg)?;

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
        ExecuteMsg::UpdateConfig { factory, generator } => {
            update_config(deps, env, info, factory, generator)
        }
        ExecuteMsg::DepositGenerator { amount } => deposit_generator(deps, env, info, amount),
        ExecuteMsg::ClaimGeneratorRewards {} => claim_generator_rewards(deps),
        ExecuteMsg::WithdrawGenerator { amount } => withdraw_generator(deps, env, info, amount),
        ExecuteMsg::SetupMaxVotingPeriod { max_voting_period } => {
            setup_max_voting_period(deps, info, env, max_voting_period)
        }
        ExecuteMsg::SetupPools {
            target_pool,
            migration_pool,
        } => setup_pools(deps, env, info, target_pool, migration_pool),
        ExecuteMsg::WithdrawTargetPoolLP {
            withdraw_amount,
            provide_params,
        } => withdraw_target_pool(deps, env, info, withdraw_amount, provide_params),
        ExecuteMsg::WithdrawRageQuitLP {
            pool_type,
            withdraw_amount,
        } => withdraw_ragequit(deps, env, info, pool_type, withdraw_amount),
        ExecuteMsg::Transfer { asset, recipient } => transfer(deps, info, env, &asset, recipient),
        ExecuteMsg::ProvideLiquidity {
            pool_type,
            assets,
            slippage_tolerance,
            auto_stake,
            ..
        } => provide(
            deps,
            env,
            info,
            pool_type,
            assets,
            slippage_tolerance,
            auto_stake,
        ),
        ExecuteMsg::StartRageQuit {} => start_rage_quit(deps, info),
        ExecuteMsg::CompleteTargetPoolMigration {} => end_target_pool_migration(deps, info, env),
        ExecuteMsg::Propose {
            title,
            description,
            msgs,
            latest,
        } => execute_propose(deps, env, info, title, description, msgs, latest),
        ExecuteMsg::Vote { proposal_id, vote } => execute_vote(deps, env, info, proposal_id, vote),
        ExecuteMsg::Execute { proposal_id } => execute_execute(deps, env, proposal_id),
        ExecuteMsg::Close { proposal_id } => execute_close(deps, env, proposal_id),
        ExecuteMsg::ProposeNewManager2 {
            new_manager,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;
            propose_new_owner(
                deps,
                info,
                env,
                new_manager,
                expires_in,
                config.manager2,
                MANAGER2_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropManager2Proposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.manager2, MANAGER2_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimManager2 {} => {
            claim_ownership(deps, info, env, MANAGER2_PROPOSAL, |deps, new_manager| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.manager2 = new_manager;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::ProposeNewManager1 {
            new_manager,
            expires_in,
        } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                new_manager,
                expires_in,
                config.manager1,
                MANAGER1_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropManager1Proposal {} => {
            let config = CONFIG.load(deps.storage)?;
            drop_ownership_proposal(deps, info, config.manager1, MANAGER1_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimManager1 {} => {
            claim_ownership(deps, info, env, MANAGER1_PROPOSAL, |deps, new_manager| {
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.manager1 = new_manager;
                        Ok(v)
                    })
                    .map(|_| ())
            })
            .map_err(Into::into)
        }
    }
}

pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    factory: Option<String>,
    generator: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "update_config")];

    // we need to approve from both managers
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if config.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    if let Some(factory) = factory {
        config.factory_addr = deps.api.addr_validate(&factory)?;
        attributes.push(attr("factory", factory));
    }

    if let Some(new_generator) = generator {
        let (_, lp_token) = get_pool_info(&deps.querier, &config, PoolType::Target)?;

        // checks if all LP tokens have been withdrawn from the generator for the target pool
        check_generator_deposit(
            &deps.querier,
            &config.generator_addr,
            &lp_token,
            &env.contract.address,
        )?;

        if config.migration_pool.is_some() {
            let (_, lp_token) = get_pool_info(&deps.querier, &config, PoolType::Migration)?;

            // checks if all LP tokens have been withdrawn from the generator for the migration pool
            check_generator_deposit(
                &deps.querier,
                &config.generator_addr,
                &lp_token,
                &env.contract.address,
            )?;
        }

        config.generator_addr = deps.api.addr_validate(&new_generator)?;
        attributes.push(attr("generator", new_generator));
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// Stakes the target LP tokens in the Generator contract.
pub fn deposit_generator(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if cfg.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    if cfg.migration_pool.is_some() {
        return Err(ContractError::MigrationNotCompleted {});
    }

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    let (_, lp_token) = get_pool_info(&deps.querier, &cfg, PoolType::Target)?;

    let total_lp_amount = query_token_balance(&deps.querier, &lp_token, &env.contract.address)?;
    let deposit_amount = amount.unwrap_or(total_lp_amount);

    if deposit_amount.is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    if deposit_amount > total_lp_amount {
        return Err(ContractError::BalanceToSmall(
            env.contract.address.to_string(),
            total_lp_amount.to_string(),
        ));
    }

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: cfg.generator_addr.to_string(),
                amount: deposit_amount,
                msg: to_binary(&Cw20HookMsg::Deposit {})?,
            })?,
            funds: vec![],
        }))
        .add_attributes([attr("action", "deposit_generator")]))
}

/// Updates generator rewards and return it to Multisig
pub fn claim_generator_rewards(deps: DepsMut) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    let (_, lp_token) = get_pool_info(&deps.querier, &cfg, PoolType::Target)?;

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.generator_addr.to_string(),
            msg: to_binary(&GeneratorExecuteMsg::ClaimRewards {
                lp_tokens: vec![lp_token.to_string()],
            })?,
            funds: vec![],
        }))
        .add_attributes([attr("action", "claim_generator_rewards")]))
}

/// Withdraws the LP tokens from the specified pool
pub fn withdraw_generator(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if cfg.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    // We should complete the migration from the target pool
    if cfg.migration_pool.is_some() {
        return Err(ContractError::MigrationNotCompleted {});
    }

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    let (_, lp_token) = get_pool_info(&deps.querier, &cfg, PoolType::Target)?;

    let total_amount: Uint128 = deps.querier.query_wasm_smart(
        &cfg.generator_addr,
        &GeneratorQueryMsg::Deposit {
            lp_token: lp_token.to_string(),
            user: env.contract.address.to_string(),
        },
    )?;

    let burn_amount = amount.unwrap_or(total_amount);
    if burn_amount > total_amount {
        return Err(ContractError::BalanceToSmall(
            env.contract.address.to_string(),
            total_amount.to_string(),
        ));
    }

    Ok(Response::new()
        .add_message(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: cfg.generator_addr.to_string(),
            msg: to_binary(&GeneratorExecuteMsg::Withdraw {
                lp_token: lp_token.to_string(),
                amount: burn_amount,
            })?,
            funds: vec![],
        }))
        .add_attributes([attr("action", "withdraw_generator")]))
}

/// Withdraw liquidity from the pool.
/// * **withdraw_amount** is the amount of LP tokens to burn.
///
/// * **provide_params** is the parameters to LP tokens in the same transaction to migration_pool
pub fn withdraw_target_pool(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Option<Uint128>,
    provide_params: Option<ProvideParams>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if cfg.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    if cfg.migration_pool.is_none() {
        return Err(ContractError::MigrationPoolError {});
    }

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    let (pair, lp_token) = get_pool_info(&deps.querier, &cfg, PoolType::Target)?;

    let mut attributes = vec![attr("action", "withdraw_target_pool")];
    let mut messages = vec![];

    let (withdraw_msg, burn_amount) = prepare_withdraw_msg(
        &deps.querier,
        &env.contract.address,
        &pair,
        &lp_token,
        amount,
    )?;

    messages.push(withdraw_msg);

    if let Some(provide_params) = provide_params {
        messages.push(prepare_provide_after_withdraw_msg(
            &deps.querier,
            &cfg,
            burn_amount,
            &pair,
            provide_params,
            &mut attributes,
        )?);
    }

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

/// Withdraws the LP tokens from the specified pool
pub fn withdraw_ragequit(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_type: PoolType,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if !cfg.rage_quit_started {
        return Err(ContractError::RageQuitIsNotStarted {});
    }

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    let (pair, lp_token) = get_pool_info(&deps.querier, &cfg, pool_type)?;
    let (withdraw_msg, _) = prepare_withdraw_msg(
        &deps.querier,
        &env.contract.address,
        &pair,
        &lp_token,
        amount,
    )?;

    Ok(Response::new()
        .add_message(withdraw_msg)
        .add_attributes([attr("action", "withdraw_ragequit")]))
}

pub fn provide(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool_type: PoolType,
    assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    if cfg.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    if pool_type == PoolType::Target {
        // we cannot provide to the target pool if migration pool is set
        if cfg.migration_pool.is_some() {
            return Err(ContractError::MigrationPoolIsAlreadySet {});
        }
    }

    check_provide_assets(&deps.querier, &env.contract.address, &assets, &cfg)?;

    let (pair, _) = get_pool_info(&deps.querier, &cfg, pool_type)?;
    let message = prepare_provide_msg(&pair, assets, slippage_tolerance, auto_stake)?;

    Ok(Response::new()
        .add_message(message)
        .add_attribute("action", "shared_multisig_provide"))
}

fn transfer(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    asset: &Asset,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    if asset.amount.is_zero() {
        return Err(StdError::generic_err("Can't send 0 amount").into());
    }

    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.manager1 && info.sender != config.manager2 {
        return Err(ContractError::Unauthorized {});
    }

    let recipient = recipient.unwrap_or(info.sender.to_string());

    let message = match &asset.info {
        AssetInfo::Token { contract_addr } => {
            let (_, lp_token) = get_pool_info(&deps.querier, &config, PoolType::Target)?;
            if lp_token == *contract_addr {
                return Err(ContractError::UnauthorizedTransfer(
                    info.sender.to_string(),
                    lp_token.to_string(),
                ));
            }

            if config.migration_pool.is_some() {
                let (_, lp_token) = get_pool_info(&deps.querier, &config, PoolType::Migration)?;
                if lp_token == *contract_addr {
                    return Err(ContractError::UnauthorizedTransfer(
                        info.sender.to_string(),
                        lp_token.to_string(),
                    ));
                }
            }

            let total_amount =
                query_token_balance(&deps.querier, contract_addr, &env.contract.address)?;
            update_distributed_rewards(
                deps.storage,
                &contract_addr.to_string(),
                asset.amount,
                total_amount,
                &info.sender,
                &config,
            )?;

            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.clone(),
                    amount: asset.amount,
                })?,
                funds: vec![],
            })
        }
        AssetInfo::NativeToken { denom } => {
            // Either manager cannot transfer his coin specified in the config before rage quit is not started
            if (*denom == config.denom1 || *denom == config.denom2) && !config.rage_quit_started {
                return Err(ContractError::RageQuitIsNotStarted {});
            }

            // Either manager can transfer only his coin specified in the config. Also, either manager can
            // transfer any coins that aren't set in the config
            if (*denom == config.denom1 && info.sender != config.manager1)
                || (*denom == config.denom2 && info.sender != config.manager2)
            {
                return Err(ContractError::UnauthorizedTransfer(
                    info.sender.to_string(),
                    denom.clone(),
                ));
            }

            let total_amount = query_balance(&deps.querier, &env.contract.address, denom)?;
            if *denom != config.denom1 && *denom != config.denom2 {
                update_distributed_rewards(
                    deps.storage,
                    denom,
                    asset.amount,
                    total_amount,
                    &info.sender,
                    &config,
                )?;
            }

            CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient.clone(),
                amount: vec![Coin {
                    denom: denom.to_string(),
                    amount: asset.amount,
                }],
            })
        }
    };

    Ok(Response::default().add_message(message).add_attributes([
        attr("action", "transfer"),
        attr("recipient", recipient),
        attr("amount", asset.amount),
        attr("denom", asset.info.to_string()),
    ]))
}

pub fn execute_propose(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    title: String,
    description: String,
    msgs: Vec<CosmosMsg>,
    // we ignore earliest
    latest: Option<Expiration>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if info.sender != cfg.manager2 && info.sender != cfg.manager1 {
        return Err(ContractError::Unauthorized {});
    }

    // max expires also used as default
    let max_expires = cfg.max_voting_period.after(&env.block);
    let mut expires = latest.unwrap_or(max_expires);
    let comp = expires.partial_cmp(&max_expires);
    if let Some(Ordering::Greater) = comp {
        expires = max_expires;
    } else if comp.is_none() {
        return Err(ContractError::WrongExpiration {});
    }

    let mut prop = Proposal {
        title,
        description,
        start_height: env.block.height,
        expires,
        msgs,
        status: Status::Open,
        votes: Votes::yes(DEFAULT_WEIGHT),
        threshold: cfg.threshold,
        total_weight: cfg.total_weight,
        proposer: info.sender.clone(),
        deposit: None,
    };
    prop.update_status(&env.block);
    let id = next_id(deps.storage)?;
    PROPOSALS.save(deps.storage, id, &prop)?;

    // add the first yes vote from voter
    if info.sender == cfg.manager1 {
        BALLOTS.save(deps.storage, (id, &MultisigRole::Manager1), &Vote::Yes)?;
    } else {
        BALLOTS.save(deps.storage, (id, &MultisigRole::Manager2), &Vote::Yes)?;
    }

    Ok(Response::new()
        .add_attribute("action", "propose")
        .add_attribute("proposal_id", id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_vote(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    proposal_id: u64,
    vote: Vote,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager1 && info.sender != config.manager2 {
        return Err(ContractError::Unauthorized {});
    }

    // ensure proposal exists and can be voted on
    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // Allow voting on Passed and Rejected proposals too
    if ![Status::Open, Status::Passed, Status::Rejected].contains(&prop.status) {
        return Err(ContractError::NotOpen {});
    }

    // if they are not expired
    if prop.expires.is_expired(&env.block) {
        return Err(ContractError::Expired {});
    }

    // store sender vote
    if info.sender == config.manager1 {
        BALLOTS.update(
            deps.storage,
            (proposal_id, &MultisigRole::Manager1),
            |bal| match bal {
                Some(_) => Err(ContractError::AlreadyVoted {}),
                None => Ok(vote),
            },
        )?;
    } else {
        BALLOTS.update(
            deps.storage,
            (proposal_id, &MultisigRole::Manager2),
            |bal| match bal {
                Some(_) => Err(ContractError::AlreadyVoted {}),
                None => Ok(vote),
            },
        )?;
    }

    // update vote tally
    prop.votes.add_vote(vote, DEFAULT_WEIGHT);
    prop.update_status(&env.block);
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "vote")
        .add_attribute("proposal_id", proposal_id.to_string())
        .add_attribute("status", format!("{:?}", prop.status)))
}

pub fn execute_execute(
    deps: DepsMut,
    env: Env,
    proposal_id: u64,
) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    // we allow execution even after the proposal "expiration" as long as all vote come in before
    // that point. If it was approved on time, it can be executed any time.
    prop.update_status(&env.block);
    if prop.status != Status::Passed {
        return Err(ContractError::WrongExecuteStatus {});
    }

    // set it to executed
    prop.status = Status::Executed;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    // dispatch all proposed messages
    Ok(Response::new()
        .add_messages(prop.msgs)
        .add_attribute("action", "execute")
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn execute_close(deps: DepsMut, env: Env, proposal_id: u64) -> Result<Response, ContractError> {
    // anyone can trigger this if the vote passed

    let mut prop = PROPOSALS.load(deps.storage, proposal_id)?;
    if [Status::Executed, Status::Rejected, Status::Passed].contains(&prop.status) {
        return Err(ContractError::WrongCloseStatus {});
    }

    // Avoid closing of Passed due to expiration proposals
    if prop.current_status(&env.block) == Status::Passed {
        return Err(ContractError::WrongCloseStatus {});
    }

    if !prop.expires.is_expired(&env.block) {
        return Err(ContractError::NotExpired {});
    }

    // set it to failed
    prop.status = Status::Rejected;
    PROPOSALS.save(deps.storage, proposal_id, &prop)?;

    Ok(Response::new()
        .add_attribute("action", "close")
        .add_attribute("proposal_id", proposal_id.to_string()))
}

pub fn setup_pools(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    target_pool: Option<String>,
    migration_pool: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "setup_pools")];

    // if we change target or migration pool, we need to approve from both managers
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if config.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    // change migration pool
    if let Some(migration_pool) = migration_pool {
        // we can change the migration pool if rage quit is not started and the migration pool is None
        if config.migration_pool.is_some() {
            return Err(ContractError::MigrationPoolIsAlreadySet {});
        }

        let migration_pool_addr = deps.api.addr_validate(&migration_pool)?;
        check_pool(&deps.querier, &migration_pool_addr, &config)?;

        config.migration_pool = Some(migration_pool_addr);
        attributes.push(attr("migration_pool", migration_pool));
    }

    // change target pool
    if let Some(target_pool) = target_pool {
        if config.target_pool.is_some() {
            return Err(ContractError::TargetPoolIsAlreadySet {});
        }

        let target_pool_addr = deps.api.addr_validate(&target_pool)?;
        check_pool(&deps.querier, &target_pool_addr, &config)?;

        config.target_pool = Some(target_pool_addr);
        attributes.push(attr("target_pool", target_pool));
    }

    if config.target_pool.eq(&config.migration_pool) {
        return Err(ContractError::PoolsError {});
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

pub fn setup_max_voting_period(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    max_voting_period: Duration,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "update_config")];

    if config.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    // we need to approve from both managers
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    config.max_voting_period = max_voting_period;
    attributes.push(attr("max_voting_period", max_voting_period.to_string()));

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

pub fn start_rage_quit(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.manager1 && info.sender != config.manager2 {
        return Err(ContractError::Unauthorized {});
    }

    if config.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    config.rage_quit_started = true;
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(vec![attr("action", "start_rage_quit")]))
}

pub fn end_target_pool_migration(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let mut attributes = vec![attr("action", "end_target_pool_migration")];

    // the other options either manager can change alone
    if info.sender != config.manager1 && info.sender != config.manager2 {
        return Err(ContractError::Unauthorized {});
    }

    if config.rage_quit_started {
        return Err(ContractError::RageQuitStarted {});
    }

    let (target_pool, lp_token) = get_pool_info(&deps.querier, &config, PoolType::Target)?;

    // checks if all LP tokens have been withdrawn from the generator
    check_generator_deposit(
        &deps.querier,
        &config.generator_addr,
        &lp_token,
        &env.contract.address,
    )?;

    // we cannot set the target pool to None
    if config.migration_pool.is_none() {
        return Err(ContractError::MigrationPoolError {});
    }

    // checks if all LP tokens have been withdrawn from the target pool
    let total_amount = query_token_balance(&deps.querier, lp_token, env.contract.address)?;
    if !total_amount.is_zero() {
        return Err(ContractError::TargetPoolAmountError {});
    }

    attributes.push(attr("old_target_pool", target_pool.as_str()));
    attributes.push(attr(
        "old_migration_pool",
        config.migration_pool.clone().unwrap().as_str(),
    ));
    config.target_pool = config.migration_pool.clone();
    config.migration_pool = None;

    attributes.push(attr(
        "new_target_pool",
        config.target_pool.clone().unwrap().as_str(),
    ));
    attributes.push(attr("new_migration_pool", "None"));

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    Err(ContractError::MigrationError {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
        QueryMsg::Proposal { proposal_id } => to_binary(&query_proposal(deps, env, proposal_id)?),
        QueryMsg::Vote { proposal_id, voter } => to_binary(&query_vote(deps, proposal_id, voter)?),
        QueryMsg::ListProposals { start_after, limit } => {
            to_binary(&list_proposals(deps, env, start_after, limit)?)
        }
        QueryMsg::ReverseProposals {
            start_before,
            limit,
        } => to_binary(&reverse_proposals(deps, env, start_before, limit)?),
        QueryMsg::ListVotes { proposal_id } => to_binary(&list_votes(deps, proposal_id)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let cfg = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        threshold: cfg.threshold.to_response(cfg.total_weight),
        max_voting_period: cfg.max_voting_period,
        manager1: cfg.manager1.into(),
        manager2: cfg.manager2.into(),
        target_pool: cfg.target_pool,
        migration_pool: cfg.migration_pool,
        rage_quit_started: cfg.rage_quit_started,
        denom1: cfg.denom1,
        denom2: cfg.denom2,
        factory: cfg.factory_addr.into(),
        generator: cfg.generator_addr.to_string(),
    })
}

fn query_proposal(deps: Deps, env: Env, id: u64) -> StdResult<ProposalResponse> {
    let prop = PROPOSALS.load(deps.storage, id)?;
    let status = prop.current_status(&env.block);
    let threshold = prop.threshold.to_response(prop.total_weight);

    Ok(ProposalResponse {
        id,
        title: prop.title,
        description: prop.description,
        msgs: prop.msgs,
        status,
        expires: prop.expires,
        deposit: prop.deposit,
        proposer: prop.proposer,
        threshold,
    })
}

fn list_proposals(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let proposals = PROPOSALS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect::<StdResult<_>>()?;

    Ok(ProposalListResponse { proposals })
}

fn reverse_proposals(
    deps: Deps,
    env: Env,
    start_before: Option<u64>,
    limit: Option<u32>,
) -> StdResult<ProposalListResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let end = start_before.map(Bound::exclusive);

    let props: StdResult<Vec<_>> = PROPOSALS
        .range(deps.storage, None, end, Order::Descending)
        .take(limit)
        .map(|p| map_proposal(&env.block, p))
        .collect();

    Ok(ProposalListResponse { proposals: props? })
}

fn map_proposal(
    block: &BlockInfo,
    item: StdResult<(u64, Proposal)>,
) -> StdResult<ProposalResponse> {
    item.map(|(id, prop)| {
        let status = prop.current_status(block);
        let threshold = prop.threshold.to_response(prop.total_weight);
        ProposalResponse {
            id,
            title: prop.title,
            description: prop.description,
            msgs: prop.msgs,
            status,
            deposit: prop.deposit,
            proposer: prop.proposer,
            expires: prop.expires,
            threshold,
        }
    })
}

fn query_vote(deps: Deps, proposal_id: u64, voter: String) -> StdResult<VoteResponse> {
    let voter = deps.api.addr_validate(&voter)?;
    let cfg = CONFIG.load(deps.storage)?;

    let ballot;
    if voter == cfg.manager1 {
        ballot = BALLOTS.may_load(deps.storage, (proposal_id, &MultisigRole::Manager1))?;
    } else if voter == cfg.manager2 {
        ballot = BALLOTS.may_load(deps.storage, (proposal_id, &MultisigRole::Manager2))?;
    } else {
        return Err(StdError::generic_err(format!(
            "Vote not found for: {}",
            voter
        )));
    }

    let vote = ballot.map(|vote| VoteInfo {
        proposal_id,
        vote,
        voter: voter.to_string(),
        weight: DEFAULT_WEIGHT,
    });

    Ok(VoteResponse { vote })
}

fn list_votes(deps: Deps, proposal_id: u64) -> StdResult<VoteListResponse> {
    let mut votes = vec![];

    if let Some(vote_info) = load_vote(deps, (proposal_id, &MultisigRole::Manager1))? {
        votes.push(vote_info);
    }

    if let Some(vote_info) = load_vote(deps, (proposal_id, &MultisigRole::Manager2))? {
        votes.push(vote_info);
    }

    Ok(VoteListResponse { votes })
}
