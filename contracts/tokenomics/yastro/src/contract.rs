use cosmwasm_std::{
    attr, coin, ensure, entry_point, to_json_binary, wasm_execute, Addr, BankMsg, Binary, Coin,
    Deps, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, SubMsgResult,
    Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::MinterResponse;
use cw20_base::allowances::{
    execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use cw20_base::contract::{
    execute_send, execute_transfer, execute_update_marketing, execute_upload_logo, query_balance,
    query_download_logo, query_marketing_info, query_token_info,
};
use cw20_base::enumerable::{query_all_accounts, query_owner_allowances, query_spender_allowances};
use cw20_base::state::{BALANCES, TOKEN_INFO};
use cw_utils::{must_pay, nonpayable};

use astroport::asset::{AssetInfo, AssetInfoExt, PairInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::incentives::{
    ExecuteMsg as IncentivesExecuteMsg, IncentivesSchedule, InputSchedule, PoolInfoResponse,
    QueryMsg as IncentivesQueryMsg,
};
use astroport::pair::QueryMsg as PairQueryMsg;

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, RewardState, StakingState};
use crate::rewards::{deposit, pending, restore, settle, streams_at, take};
use crate::state::{
    Config, Unbonding, CONFIG, FORWARDS, NEXT_FORWARD_ID, OWNERSHIP_PROPOSAL, REWARD_DENOMS,
    STREAMS, TOTAL_UNBONDING, UNBONDINGS,
};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Open unbondings per address, so withdrawing stays cheap
pub const MAX_UNBONDINGS: usize = 30;
/// Longest unbonding period the owner can set
pub const MAX_UNBONDING_PERIOD: u64 = 28 * 86400;
/// Reward denoms ever accepted, since every balance change settles each one
pub const MAX_REWARD_DENOMS: usize = 5;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    // Validates the token info and stores it with the marketing info, minting nothing and
    // leaving no minter
    cw20_base::contract::instantiate(
        deps.branch(),
        env,
        info,
        cw20_base::msg::InstantiateMsg {
            name: msg.name,
            symbol: msg.symbol,
            decimals: msg.decimals,
            initial_balances: vec![],
            mint: None,
            marketing: msg.marketing,
        },
    )?;
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    check_unbonding_period(msg.unbonding_period)?;
    let mut reward_denoms: Vec<String> = vec![];
    for denom in msg.reward_denoms {
        ensure!(
            denom != msg.astro_denom,
            ContractError::StakedDenomAsReward { denom }
        );
        if !reward_denoms.contains(&denom) {
            reward_denoms.push(denom);
        }
    }
    ensure!(
        reward_denoms.len() <= MAX_REWARD_DENOMS,
        ContractError::TooManyRewardDenoms {
            max: MAX_REWARD_DENOMS
        }
    );

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            astro_denom: msg.astro_denom,
            unbonding_period: msg.unbonding_period,
            incentives: msg
                .incentives
                .map(|a| deps.api.addr_validate(&a))
                .transpose()?,
        },
    )?;
    REWARD_DENOMS.save(deps.storage, &reward_denoms)?;
    TOTAL_UNBONDING.save(deps.storage, &Uint128::zero())?;
    NEXT_FORWARD_ID.save(deps.storage, &0)?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let now = env.block.time.seconds();
    match msg {
        ExecuteMsg::Stake { recipient } => stake(deps, env, info, recipient),
        ExecuteMsg::Unstake { amount } => unstake(deps, env, info, amount),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
        ExecuteMsg::DepositRewards {} => deposit_rewards(deps, env, info),
        ExecuteMsg::Claim { recipient } => {
            nonpayable(&info)?;
            let recipient = recipient
                .map(|r| deps.api.addr_validate(&r))
                .transpose()?
                .unwrap_or_else(|| info.sender.clone());
            claim(deps, now, info.sender, recipient)
        }
        ExecuteMsg::ClaimFor { address } => {
            nonpayable(&info)?;
            let address = deps.api.addr_validate(&address)?;
            ensure!(
                pool_lp_token(deps.as_ref(), &env, &address).is_none(),
                ContractError::IsAPool {
                    address: address.to_string()
                }
            );
            claim(deps, now, address.clone(), address)
        }
        ExecuteMsg::ForwardPoolRewards { pool } => forward_pool_rewards(deps, env, info, pool),

        ExecuteMsg::Transfer { recipient, amount } => {
            nonpayable(&info)?;
            let to = check_recipient(deps.as_ref(), &env, &recipient, amount)?;
            settle(deps.storage, now, &[&info.sender, &to])?;
            Ok(execute_transfer(deps, env, info, recipient, amount)?)
        }
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => {
            nonpayable(&info)?;
            let to = check_recipient(deps.as_ref(), &env, &contract, amount)?;
            settle(deps.storage, now, &[&info.sender, &to])?;
            Ok(execute_send(deps, env, info, contract, amount, msg)?)
        }
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => {
            nonpayable(&info)?;
            let from = deps.api.addr_validate(&owner)?;
            let to = check_recipient(deps.as_ref(), &env, &recipient, amount)?;
            settle(deps.storage, now, &[&from, &to])?;
            Ok(execute_transfer_from(
                deps, env, info, owner, recipient, amount,
            )?)
        }
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => {
            nonpayable(&info)?;
            let from = deps.api.addr_validate(&owner)?;
            let to = check_recipient(deps.as_ref(), &env, &contract, amount)?;
            settle(deps.storage, now, &[&from, &to])?;
            Ok(execute_send_from(
                deps, env, info, owner, contract, amount, msg,
            )?)
        }
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => {
            nonpayable(&info)?;
            Ok(execute_increase_allowance(
                deps, env, info, spender, amount, expires,
            )?)
        }
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => {
            nonpayable(&info)?;
            Ok(execute_decrease_allowance(
                deps, env, info, spender, amount, expires,
            )?)
        }
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => {
            nonpayable(&info)?;
            Ok(execute_update_marketing(
                deps,
                env,
                info,
                project,
                description,
                marketing,
            )?)
        }
        ExecuteMsg::UploadLogo(logo) => {
            nonpayable(&info)?;
            Ok(execute_upload_logo(deps, env, info, logo)?)
        }

        ExecuteMsg::UpdateConfig {
            add_reward_denoms,
            remove_reward_denoms,
            unbonding_period,
            incentives,
        } => update_config(
            deps,
            info,
            add_reward_denoms,
            remove_reward_denoms,
            unbonding_period,
            incentives,
        ),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;
            Ok(propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )?)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            nonpayable(&info)?;
            let config = CONFIG.load(deps.storage)?;
            Ok(drop_ownership_proposal(
                deps,
                info,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )?)
        }
        ExecuteMsg::ClaimOwnership {} => {
            nonpayable(&info)?;
            Ok(claim_ownership(
                deps,
                info,
                env,
                OWNERSHIP_PROPOSAL,
                |deps, new_owner| {
                    CONFIG.update::<_, StdError>(deps.storage, |mut c| {
                        c.owner = new_owner;
                        Ok(c)
                    })?;
                    Ok(())
                },
            )?)
        }
    }
}

/// A yASTRO recipient: valid, not this contract, and with a non-zero amount
fn check_recipient(
    deps: Deps,
    env: &Env,
    recipient: &str,
    amount: Uint128,
) -> Result<Addr, ContractError> {
    ensure!(!amount.is_zero(), ContractError::ZeroAmount {});
    let addr = deps.api.addr_validate(recipient)?;
    ensure!(
        addr != env.contract.address,
        ContractError::SelfRecipient {}
    );
    Ok(addr)
}

fn check_unbonding_period(period: u64) -> Result<(), ContractError> {
    ensure!(
        period > 0 && period <= MAX_UNBONDING_PERIOD,
        ContractError::InvalidUnbondingPeriod {
            max: MAX_UNBONDING_PERIOD
        }
    );
    Ok(())
}

/// The LP token of `address` if it's a pool holding yASTRO: a contract answering Astroport's
/// `pair {}` query with yASTRO among its assets
fn pool_lp_token(deps: Deps, env: &Env, address: &Addr) -> Option<String> {
    let pair: PairInfo = deps
        .querier
        .query_wasm_smart(address, &PairQueryMsg::Pair {})
        .ok()?;
    let yastro = AssetInfo::Token {
        contract_addr: env.contract.address.clone(),
    };
    pair.asset_infos
        .contains(&yastro)
        .then_some(pair.liquidity_token)
}

fn stake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.astro_denom)?;
    let recipient = match recipient {
        Some(r) => check_recipient(deps.as_ref(), &env, &r, amount)?,
        None => info.sender.clone(),
    };

    settle(deps.storage, env.block.time.seconds(), &[&recipient])?;
    BALANCES.update::<_, StdError>(deps.storage, &recipient, |b| {
        Ok(b.unwrap_or_default().checked_add(amount)?)
    })?;
    TOKEN_INFO.update::<_, StdError>(deps.storage, |mut t| {
        t.total_supply = t.total_supply.checked_add(amount)?;
        Ok(t)
    })?;

    Ok(Response::new().add_attributes([
        attr("action", "stake"),
        attr("sender", info.sender),
        attr("recipient", recipient),
        attr("amount", amount),
    ]))
}

fn unstake(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    ensure!(!amount.is_zero(), ContractError::ZeroAmount {});
    let config = CONFIG.load(deps.storage)?;
    let now = env.block.time.seconds();

    let mut unbondings = UNBONDINGS
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    ensure!(
        unbondings.len() < MAX_UNBONDINGS,
        ContractError::TooManyUnbondings {}
    );

    settle(deps.storage, now, &[&info.sender])?;
    BALANCES.update::<_, StdError>(deps.storage, &info.sender, |b| {
        Ok(b.unwrap_or_default().checked_sub(amount)?)
    })?;
    TOKEN_INFO.update::<_, StdError>(deps.storage, |mut t| {
        t.total_supply = t.total_supply.checked_sub(amount)?;
        Ok(t)
    })?;

    let release_at = now
        .checked_add(config.unbonding_period)
        .ok_or_else(|| StdError::generic_err("Unbonding period overflow"))?;
    unbondings.push(Unbonding { amount, release_at });
    UNBONDINGS.save(deps.storage, &info.sender, &unbondings)?;
    TOTAL_UNBONDING.update::<_, StdError>(deps.storage, |t| Ok(t.checked_add(amount)?))?;

    Ok(Response::new().add_attributes([
        attr("action", "unstake"),
        attr("sender", info.sender),
        attr("amount", amount),
        attr("release_at", release_at.to_string()),
    ]))
}

fn withdraw(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let config = CONFIG.load(deps.storage)?;
    let now = env.block.time.seconds();

    let unbondings = UNBONDINGS
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    let (matured, open): (Vec<_>, Vec<_>) =
        unbondings.into_iter().partition(|u| u.release_at <= now);
    let amount = matured
        .iter()
        .try_fold(Uint128::zero(), |acc, u| acc.checked_add(u.amount))?;
    ensure!(!amount.is_zero(), ContractError::NothingToWithdraw {});

    if open.is_empty() {
        UNBONDINGS.remove(deps.storage, &info.sender);
    } else {
        UNBONDINGS.save(deps.storage, &info.sender, &open)?;
    }
    TOTAL_UNBONDING.update::<_, StdError>(deps.storage, |t| Ok(t.checked_sub(amount)?))?;

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![coin(amount.u128(), config.astro_denom)],
        })
        .add_attributes([
            attr("action", "withdraw"),
            attr("sender", info.sender),
            attr("amount", amount),
        ]))
}

fn deposit_rewards(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    ensure!(
        info.funds.iter().any(|c| !c.amount.is_zero()),
        ContractError::Payment(cw_utils::PaymentError::NoFunds {})
    );
    let now = env.block.time.seconds();
    let accepted = REWARD_DENOMS.load(deps.storage)?;
    let mut epoch_end = None;
    for coin in &info.funds {
        ensure!(
            accepted.contains(&coin.denom),
            ContractError::NotARewardDenom {
                denom: coin.denom.clone()
            }
        );
        if !coin.amount.is_zero() {
            epoch_end = Some(deposit(deps.storage, now, coin)?.epoch_end);
        }
    }

    Ok(Response::new().add_attributes([
        attr("action", "deposit_rewards"),
        attr("sender", info.sender),
        attr("rewards", join(&info.funds)),
        attr("streams_from", epoch_end.unwrap_or_default().to_string()),
        attr("total_staked", TOKEN_INFO.load(deps.storage)?.total_supply),
    ]))
}

fn claim(deps: DepsMut, now: u64, owner: Addr, recipient: Addr) -> Result<Response, ContractError> {
    let rewards = take(deps.storage, now, &owner)?;
    ensure!(!rewards.is_empty(), ContractError::NothingToClaim {});

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: recipient.to_string(),
            amount: rewards.clone(),
        })
        .add_attributes([
            attr("action", "claim"),
            attr("owner", owner),
            attr("recipient", recipient),
            attr("rewards", join(&rewards)),
        ]))
}

fn forward_pool_rewards(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    pool: String,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let config = CONFIG.load(deps.storage)?;
    let incentives = config
        .incentives
        .ok_or(ContractError::IncentivesNotSet {})?;
    let pool = deps.api.addr_validate(&pool)?;
    let lp_token =
        pool_lp_token(deps.as_ref(), &env, &pool).ok_or_else(|| ContractError::NotAPool {
            address: pool.to_string(),
        })?;

    // Incentives gives rewards streamed while nobody is staked to whoever stakes first
    let staked = deps
        .querier
        .query_wasm_smart::<PoolInfoResponse>(
            &incentives,
            &IncentivesQueryMsg::PoolInfo {
                lp_token: lp_token.clone(),
            },
        )
        .map(|p| p.total_lp)
        .unwrap_or_default();
    ensure!(
        !staked.is_zero(),
        ContractError::NoLpStakers {
            lp_token: lp_token.clone()
        }
    );

    let rewards = take(deps.storage, env.block.time.seconds(), &pool)?;
    let mut forward_id = NEXT_FORWARD_ID.load(deps.storage)?;
    let mut forwarded = vec![];
    let mut kept = vec![];
    let mut messages = vec![];
    for reward in rewards {
        let schedule = InputSchedule {
            reward: AssetInfo::native(&reward.denom).with_balance(reward.amount),
            duration_periods: 1,
        };
        // Too small for Incentives' minimum of 1 unit per second: keep it until it adds up
        if IncentivesSchedule::from_input(&env, &schedule).is_err() {
            restore(deps.storage, &pool, &reward)?;
            kept.push(reward);
            continue;
        }
        // If Incentives rejects it anyway, the reply gives it back to the pool
        messages.push(SubMsg::reply_always(
            wasm_execute(
                &incentives,
                &IncentivesExecuteMsg::Incentivize {
                    lp_token: lp_token.clone(),
                    schedule,
                },
                vec![reward.clone()],
            )?,
            forward_id,
        ));
        FORWARDS.save(deps.storage, forward_id, &(pool.clone(), reward.clone()))?;
        forward_id = forward_id.wrapping_add(1);
        forwarded.push(reward);
    }
    ensure!(!forwarded.is_empty(), ContractError::NothingToForward {});
    NEXT_FORWARD_ID.save(deps.storage, &forward_id)?;

    let mut attrs = vec![
        attr("action", "forward_pool_rewards"),
        attr("pool", pool),
        attr("lp_token", lp_token),
        attr("rewards", join(&forwarded)),
    ];
    if !kept.is_empty() {
        attrs.push(attr("kept", join(&kept)));
    }
    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(attrs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    let (pool, reward) = FORWARDS.load(deps.storage, msg.id)?;
    FORWARDS.remove(deps.storage, msg.id);
    match msg.result {
        SubMsgResult::Ok(_) => Ok(Response::new()),
        SubMsgResult::Err(err) => {
            restore(deps.storage, &pool, &reward)?;
            Ok(Response::new().add_attributes([
                attr("action", "forward_failed"),
                attr("pool", pool),
                attr("reward", reward.to_string()),
                attr("error", err),
            ]))
        }
    }
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    add_reward_denoms: Option<Vec<String>>,
    remove_reward_denoms: Option<Vec<String>>,
    unbonding_period: Option<u64>,
    incentives: Option<String>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let mut config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    let mut attrs = vec![attr("action", "update_config")];

    let mut denoms = REWARD_DENOMS.load(deps.storage)?;
    for denom in add_reward_denoms.unwrap_or_default() {
        ensure!(
            denom != config.astro_denom,
            ContractError::StakedDenomAsReward { denom }
        );
        if !denoms.contains(&denom) {
            attrs.push(attr("add_reward_denom", &denom));
            denoms.push(denom);
        }
    }
    for denom in remove_reward_denoms.unwrap_or_default() {
        if let Some(i) = denoms.iter().position(|d| *d == denom) {
            attrs.push(attr("remove_reward_denom", &denom));
            denoms.remove(i);
        }
    }
    // Every denom ever streamed is settled on every balance change, accepted or not
    let mut ever: Vec<String> = STREAMS
        .keys(deps.storage, None, None, cosmwasm_std::Order::Ascending)
        .collect::<StdResult<_>>()?;
    for denom in &denoms {
        if !ever.contains(denom) {
            ever.push(denom.clone());
        }
    }
    ensure!(
        ever.len() <= MAX_REWARD_DENOMS,
        ContractError::TooManyRewardDenoms {
            max: MAX_REWARD_DENOMS
        }
    );
    REWARD_DENOMS.save(deps.storage, &denoms)?;

    if let Some(period) = unbonding_period {
        check_unbonding_period(period)?;
        config.unbonding_period = period;
        attrs.push(attr("unbonding_period", period.to_string()));
    }
    if let Some(incentives) = incentives {
        let addr = deps.api.addr_validate(&incentives)?;
        attrs.push(attr("incentives", &addr));
        config.incentives = Some(addr);
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attrs))
}

fn join(coins: &[Coin]) -> String {
    coins
        .iter()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let now = env.block.time.seconds();
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::PendingRewards { address } => {
            let addr = deps.api.addr_validate(&address)?;
            to_json_binary(&pending(deps.storage, now, &addr)?)
        }
        QueryMsg::Unbondings { address } => {
            let addr = deps.api.addr_validate(&address)?;
            to_json_binary(
                &UNBONDINGS
                    .may_load(deps.storage, &addr)?
                    .unwrap_or_default(),
            )
        }
        QueryMsg::RewardState {} => {
            let accepted = REWARD_DENOMS.load(deps.storage)?;
            let mut states: Vec<RewardState> = streams_at(deps.storage, now)?
                .into_iter()
                .map(|(denom, s)| RewardState {
                    index: s.index,
                    rate: s.rate,
                    epoch_end: s.epoch_end,
                    queued: s.queued,
                    total_deposited: s.total_deposited,
                    accepted: accepted.contains(&denom),
                    denom,
                })
                .collect();
            for denom in accepted {
                if !states.iter().any(|s| s.denom == denom) {
                    states.push(RewardState {
                        denom,
                        index: Default::default(),
                        rate: Default::default(),
                        epoch_end: 0,
                        queued: Default::default(),
                        total_deposited: Uint128::zero(),
                        accepted: true,
                    });
                }
            }
            to_json_binary(&states)
        }
        QueryMsg::StakingState {} => to_json_binary(&StakingState {
            total_staked: TOKEN_INFO.load(deps.storage)?.total_supply,
            total_unbonding: TOTAL_UNBONDING.load(deps.storage)?,
        }),

        QueryMsg::Balance { address } => to_json_binary(&query_balance(deps, address)?),
        QueryMsg::TokenInfo {} => to_json_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_json_binary(&Option::<MinterResponse>::None),
        QueryMsg::Allowance { owner, spender } => {
            to_json_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_json_binary(&query_owner_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllSpenderAllowances {
            spender,
            start_after,
            limit,
        } => to_json_binary(&query_spender_allowances(
            deps,
            spender,
            start_after,
            limit,
        )?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_json_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_json_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_json_binary(&query_download_logo(deps)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // Versions this one can migrate from
    const SUPPORTED: &[&str] = &[];

    let version = get_contract_version(deps.storage)?;
    ensure!(
        version.contract == CONTRACT_NAME && SUPPORTED.contains(&version.version.as_str()),
        ContractError::MigrationError {}
    );
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("previous_contract_version", version.version)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
