use cosmwasm_std::{
    attr, coin, ensure, entry_point, to_json_binary, wasm_execute, Addr, BankMsg, Binary, Coin,
    Coins, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult,
    Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::MinterResponse;
use cw20_base::allowances::{
    execute_decrease_allowance, execute_increase_allowance, execute_send_from,
    execute_transfer_from, query_allowance,
};
use cw20_base::contract::{
    execute_send, execute_transfer, query_balance, query_marketing_info, query_token_info,
};
use cw20_base::enumerable::{query_all_accounts, query_owner_allowances, query_spender_allowances};
use cw20_base::state::{TokenInfo, BALANCES, TOKEN_INFO};
use cw_utils::{must_pay, nonpayable};

use astroport::asset::{AssetInfo, AssetInfoExt};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::incentives::{ExecuteMsg as IncentivesExecuteMsg, InputSchedule};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, RewardState, StakingState};
use crate::rewards::{distribute, pending, settle, take};
use crate::state::{
    Config, Unbonding, CONFIG, GLOBAL_INDEX, OWNERSHIP_PROPOSAL, POOLS, REWARD_DENOMS,
    TOTAL_UNBONDING, UNBONDINGS, UNDISTRIBUTED,
};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Open unbondings per address, so withdrawing stays cheap
pub const MAX_UNBONDINGS: usize = 30;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    ensure!(
        msg.unbonding_period > 0,
        ContractError::ZeroUnbondingPeriod {}
    );
    let reward_denoms = dedup(msg.reward_denoms);
    for denom in &reward_denoms {
        ensure!(
            *denom != msg.astro_denom,
            ContractError::StakedDenomAsReward {
                denom: denom.clone()
            }
        );
    }

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
    TOKEN_INFO.save(
        deps.storage,
        &TokenInfo {
            name: msg.name,
            symbol: msg.symbol,
            decimals: msg.decimals,
            total_supply: Uint128::zero(),
            mint: None,
        },
    )?;

    Ok(Response::new().add_attribute("action", "instantiate"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Stake { recipient } => stake(deps, info, recipient),
        ExecuteMsg::Unstake { amount } => unstake(deps, env, info, amount),
        ExecuteMsg::Withdraw {} => withdraw(deps, env, info),
        ExecuteMsg::DepositRewards {} => deposit_rewards(deps, info),
        ExecuteMsg::Claim { recipient } => claim(deps, info, recipient),
        ExecuteMsg::ForwardPoolRewards { pool } => forward_pool_rewards(deps, info, pool),

        ExecuteMsg::Transfer { recipient, amount } => {
            nonpayable(&info)?;
            let to = deps.api.addr_validate(&recipient)?;
            settle(deps.storage, &info.sender)?;
            settle(deps.storage, &to)?;
            Ok(execute_transfer(deps, env, info, recipient, amount)?)
        }
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => {
            nonpayable(&info)?;
            let to = deps.api.addr_validate(&contract)?;
            settle(deps.storage, &info.sender)?;
            settle(deps.storage, &to)?;
            Ok(execute_send(deps, env, info, contract, amount, msg)?)
        }
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => {
            nonpayable(&info)?;
            let from = deps.api.addr_validate(&owner)?;
            let to = deps.api.addr_validate(&recipient)?;
            settle(deps.storage, &from)?;
            settle(deps.storage, &to)?;
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
            let to = deps.api.addr_validate(&contract)?;
            settle(deps.storage, &from)?;
            settle(deps.storage, &to)?;
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
        ExecuteMsg::SetPool { pool, lp_token } => set_pool(deps, info, pool, lp_token),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
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
            let config = CONFIG.load(deps.storage)?;
            Ok(drop_ownership_proposal(
                deps,
                info,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )?)
        }
        ExecuteMsg::ClaimOwnership {} => Ok(claim_ownership(
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
        )?),
    }
}

fn stake(
    deps: DepsMut,
    info: MessageInfo,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let amount = must_pay(&info, &config.astro_denom)?;
    let recipient = recipient
        .map(|r| deps.api.addr_validate(&r))
        .transpose()?
        .unwrap_or(info.sender);

    settle(deps.storage, &recipient)?;
    BALANCES.update::<_, StdError>(deps.storage, &recipient, |b| {
        Ok(b.unwrap_or_default().checked_add(amount)?)
    })?;
    TOKEN_INFO.update::<_, StdError>(deps.storage, |mut t| {
        t.total_supply = t.total_supply.checked_add(amount)?;
        Ok(t)
    })?;

    Ok(Response::new().add_attributes([
        attr("action", "stake"),
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

    let mut unbondings = UNBONDINGS
        .may_load(deps.storage, &info.sender)?
        .unwrap_or_default();
    ensure!(
        unbondings.len() < MAX_UNBONDINGS,
        ContractError::TooManyUnbondings {}
    );

    settle(deps.storage, &info.sender)?;
    BALANCES.update::<_, StdError>(deps.storage, &info.sender, |b| {
        Ok(b.unwrap_or_default().checked_sub(amount)?)
    })?;
    TOKEN_INFO.update::<_, StdError>(deps.storage, |mut t| {
        t.total_supply = t.total_supply.checked_sub(amount)?;
        Ok(t)
    })?;

    let release_at = env.block.time.seconds() + config.unbonding_period;
    unbondings.push(Unbonding { amount, release_at });
    UNBONDINGS.save(deps.storage, &info.sender, &unbondings)?;
    TOTAL_UNBONDING.update::<_, StdError>(deps.storage, |t| Ok(t.checked_add(amount)?))?;

    Ok(Response::new().add_attributes([
        attr("action", "unstake"),
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
        .add_attributes([attr("action", "withdraw"), attr("amount", amount)]))
}

fn deposit_rewards(deps: DepsMut, info: MessageInfo) -> Result<Response, ContractError> {
    ensure!(
        info.funds.iter().any(|c| !c.amount.is_zero()),
        ContractError::Payment(cw_utils::PaymentError::NoFunds {})
    );
    let accepted = REWARD_DENOMS.load(deps.storage)?;
    for coin in &info.funds {
        ensure!(
            accepted.contains(&coin.denom),
            ContractError::NotARewardDenom {
                denom: coin.denom.clone()
            }
        );
        if !coin.amount.is_zero() {
            distribute(deps.storage, coin)?;
        }
    }

    Ok(Response::new().add_attributes([
        attr("action", "deposit_rewards"),
        attr(
            "rewards",
            info.funds
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    ]))
}

fn claim(
    deps: DepsMut,
    info: MessageInfo,
    recipient: Option<String>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    let recipient = recipient
        .map(|r| deps.api.addr_validate(&r))
        .transpose()?
        .unwrap_or_else(|| info.sender.clone());

    let rewards = take(deps.storage, &info.sender)?;
    ensure!(!rewards.is_empty(), ContractError::NothingToClaim {});

    Ok(Response::new()
        .add_message(BankMsg::Send {
            to_address: recipient.to_string(),
            amount: rewards.clone(),
        })
        .add_attributes([
            attr("action", "claim"),
            attr("recipient", recipient),
            attr(
                "rewards",
                rewards
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        ]))
}

fn forward_pool_rewards(
    deps: DepsMut,
    info: MessageInfo,
    pool: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let incentives = config
        .incentives
        .ok_or(ContractError::IncentivesNotSet {})?;
    let pool = deps.api.addr_validate(&pool)?;
    let lp_token =
        POOLS
            .may_load(deps.storage, &pool)?
            .ok_or_else(|| ContractError::PoolNotRegistered {
                pool: pool.to_string(),
            })?;

    let rewards = take(deps.storage, &pool)?;
    ensure!(!rewards.is_empty(), ContractError::NothingToClaim {});

    // the caller's funds (Incentives' fee for a new reward token) ride with the first schedule
    let mut fee: Option<Vec<Coin>> = Some(info.funds);
    let mut messages: Vec<CosmosMsg> = vec![];
    for reward in &rewards {
        let mut funds = Coins::try_from(fee.take().unwrap_or_default())
            .map_err(|e| StdError::generic_err(e.to_string()))?;
        funds.add(reward.clone())?;
        messages.push(
            wasm_execute(
                &incentives,
                &IncentivesExecuteMsg::Incentivize {
                    lp_token: lp_token.clone(),
                    schedule: InputSchedule {
                        reward: AssetInfo::native(&reward.denom).with_balance(reward.amount),
                        duration_periods: 1,
                    },
                },
                funds.into_vec(),
            )?
            .into(),
        );
    }

    Ok(Response::new().add_messages(messages).add_attributes([
        attr("action", "forward_pool_rewards"),
        attr("pool", pool),
        attr("lp_token", lp_token),
        attr(
            "rewards",
            rewards
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    ]))
}

fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    add_reward_denoms: Option<Vec<String>>,
    remove_reward_denoms: Option<Vec<String>>,
    unbonding_period: Option<u64>,
    incentives: Option<String>,
) -> Result<Response, ContractError> {
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
    REWARD_DENOMS.save(deps.storage, &denoms)?;

    if let Some(period) = unbonding_period {
        ensure!(period > 0, ContractError::ZeroUnbondingPeriod {});
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

fn set_pool(
    deps: DepsMut,
    info: MessageInfo,
    pool: String,
    lp_token: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    let pool = deps.api.addr_validate(&pool)?;
    match &lp_token {
        Some(lp) => POOLS.save(deps.storage, &pool, lp)?,
        None => POOLS.remove(deps.storage, &pool),
    }
    Ok(Response::new().add_attributes([
        attr("action", "set_pool"),
        attr("pool", pool),
        attr("lp_token", lp_token.unwrap_or_default()),
    ]))
}

fn dedup(denoms: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for d in denoms {
        if !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::PendingRewards { address } => {
            let addr = deps.api.addr_validate(&address)?;
            to_json_binary(&pending(deps.storage, &addr)?)
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
            let mut denoms: Vec<String> = GLOBAL_INDEX
                .keys(deps.storage, None, None, Order::Ascending)
                .chain(UNDISTRIBUTED.keys(deps.storage, None, None, Order::Ascending))
                .collect::<StdResult<_>>()?;
            denoms.extend(accepted.iter().cloned());
            denoms.sort();
            denoms.dedup();
            let states = denoms
                .into_iter()
                .map(|denom| {
                    Ok(RewardState {
                        index: GLOBAL_INDEX
                            .may_load(deps.storage, &denom)?
                            .unwrap_or_default(),
                        undistributed: UNDISTRIBUTED
                            .may_load(deps.storage, &denom)?
                            .unwrap_or_default(),
                        accepted: accepted.contains(&denom),
                        denom,
                    })
                })
                .collect::<StdResult<Vec<_>>>()?;
            to_json_binary(&states)
        }
        QueryMsg::StakingState {} => to_json_binary(&StakingState {
            total_staked: TOKEN_INFO.load(deps.storage)?.total_supply,
            total_unbonding: TOTAL_UNBONDING.load(deps.storage)?,
        }),
        QueryMsg::Pools {} => {
            let pools: Vec<(Addr, String)> = POOLS
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<_>>()?;
            to_json_binary(&pools)
        }

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
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    ensure!(
        version.contract == CONTRACT_NAME,
        ContractError::MigrationError {}
    );
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::new()
        .add_attribute("previous_contract_version", version.version)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
