use cosmwasm_std::{
    attr, coins, ensure, entry_point, from_json, to_json_binary, wasm_execute, Addr, Binary, Deps,
    DepsMut, Env, MessageInfo, Response, StdError, StdResult, SubMsg, Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::Cw20ReceiveMsg;
use cw_utils::must_pay;

use astroport::asset::{addr_opt_validate, token_asset_info, AssetInfo, AssetInfoExt};
use astroport::astro_converter;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::vesting::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, OrderBy, QueryMsg,
    VestingAccount, VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
    VestingSchedulePoint,
};

use crate::error::ContractError;
use crate::state::{read_vesting_infos, Config, CONFIG, OWNERSHIP_PROPOSAL, VESTING_INFO};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-vesting";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Maximum limit of schedules per user
const SCHEDULES_LIMIT: usize = 8;

/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    msg.vesting_token.check(deps.api)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            vesting_token: msg.vesting_token,
        },
    )?;

    Ok(Response::new())
}

/// Exposes execute functions available in the contract.
///
/// * **ExecuteMsg::Claim { recipient, amount }** Claims vested tokens and transfers them to the vesting recipient.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes it
///   depending on the received template.
///
/// * **ExecuteMsg::RegisterVestingAccounts { vesting_accounts }** Registers vesting accounts
///   using the provided vector of [`VestingAccount`] structures.
///
/// * **ExecuteMsg::WithdrawFromActiveSchedule { account, recipient, withdraw_amount }**
///   Withdraws tokens from the only one active vesting schedule of the specified account.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Claim { recipient, amount } => claim(deps, env, info, recipient, amount),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::RegisterVestingAccounts { vesting_accounts } => {
            let config = CONFIG.load(deps.storage)?;

            match &config.vesting_token {
                AssetInfo::NativeToken { denom } if info.sender == config.owner => {
                    let amount = must_pay(&info, denom)?;
                    register_vesting_accounts(deps, env, vesting_accounts, amount)
                }
                _ => Err(ContractError::Unauthorized {}),
            }
        }
        ExecuteMsg::WithdrawFromActiveSchedule {
            account,
            recipient,
            withdraw_amount,
        } => withdraw_from_active_schedule(deps, env, info, account, recipient, withdraw_amount),
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
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(Into::into)
        }
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Permission check
    if cw20_msg.sender != config.owner || token_asset_info(info.sender) != config.vesting_token {
        return Err(ContractError::Unauthorized {});
    }

    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, env, vesting_accounts, cw20_msg.amount)
        }
    }
}

/// Create new vesting schedules.
///
/// * **vesting_accounts** list of accounts and associated vesting schedules to create.
///
/// * **cw20_amount** sets the amount that confirms the total amount of all accounts to register.
pub fn register_vesting_accounts(
    deps: DepsMut,
    env: Env,
    vesting_accounts: Vec<VestingAccount>,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let response = Response::new();

    let mut to_deposit = Uint128::zero();

    for mut vesting_account in vesting_accounts {
        let mut released_amount = Uint128::zero();
        let account_address = deps.api.addr_validate(&vesting_account.address)?;

        assert_vesting_schedules(&env, &account_address, &vesting_account.schedules)?;

        for sch in &vesting_account.schedules {
            let amount = if let Some(end_point) = &sch.end_point {
                end_point.amount
            } else {
                sch.start_point.amount
            };
            to_deposit = to_deposit.checked_add(amount)?;
        }

        if let Some(mut old_info) = VESTING_INFO.may_load(deps.storage, &account_address)? {
            if old_info.schedules.len() + 1 > SCHEDULES_LIMIT {
                return Err(ContractError::ExceedSchedulesMaximumLimit(
                    vesting_account.address,
                ));
            };
            released_amount = old_info.released_amount;
            vesting_account.schedules.append(&mut old_info.schedules);
        }

        VESTING_INFO.save(
            deps.storage,
            &account_address,
            &VestingInfo {
                schedules: vesting_account.schedules,
                released_amount,
            },
        )?;
    }

    if to_deposit != amount {
        return Err(ContractError::VestingScheduleAmountError {});
    }

    Ok(response.add_attributes({
        vec![
            attr("action", "register_vesting_accounts"),
            attr("deposited", to_deposit),
        ]
    }))
}

/// Asserts the validity of a list of vesting schedules.
///
/// * **addr** receiver of the vested tokens.
///
/// * **vesting_schedules** vesting schedules to validate.
fn assert_vesting_schedules(
    env: &Env,
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time
                && end_point.time > env.block.time.seconds()
                && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.to_string()));
            }
        }
    }

    Ok(())
}

/// Claims vested tokens and transfers them to the vesting recipient.
///
/// * **recipient** vesting recipient for which to claim tokens.
///
/// * **amount** amount of vested tokens to claim.
pub fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut vesting_info = VESTING_INFO.load(deps.storage, &info.sender)?;

    let available_amount = compute_available_amount(env.block.time.seconds(), &vesting_info)?;

    let claim_amount = if let Some(a) = amount {
        if a > available_amount {
            return Err(ContractError::AmountIsNotAvailable {});
        };
        a
    } else {
        available_amount
    };

    let mut response = Response::new();

    if !claim_amount.is_zero() {
        let transfer_msg = config.vesting_token.with_balance(claim_amount).into_msg(
            addr_opt_validate(deps.api, &recipient)?.unwrap_or_else(|| info.sender.clone()),
        )?;
        response = response.add_submessage(SubMsg::new(transfer_msg));

        vesting_info.released_amount = vesting_info.released_amount.checked_add(claim_amount)?;
        VESTING_INFO.save(deps.storage, &info.sender, &vesting_info)?;
    };

    Ok(response.add_attributes(vec![
        attr("action", "claim"),
        attr("address", &info.sender),
        attr("available_amount", available_amount),
        attr("claimed_amount", claim_amount),
    ]))
}

/// Computes the amount of vested and yet unclaimed tokens for a specific vesting recipient.
/// Returns the computed amount if the operation is successful.
///
/// * **current_time** timestamp from which to start querying for vesting schedules.
///   Schedules that started later than current_time will be omitted.
///
/// * **vesting_info** vesting schedules for which to compute the amount of tokens
///   that are vested and can be claimed by the recipient.
fn compute_available_amount(current_time: u64, vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if sch.start_point.time > current_time {
            continue;
        }

        let unlocked_amount = calc_schedule_unlocked_amount(sch, current_time)?;
        available_amount = available_amount.checked_add(unlocked_amount)?;
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

/// Calculate unlocked amount for particular [`VestingSchedule`].
/// This function does not consider released amount.
fn calc_schedule_unlocked_amount(
    schedule: &VestingSchedule,
    current_time: u64,
) -> StdResult<Uint128> {
    let mut available_amount = schedule.start_point.amount;

    if let Some(end_point) = &schedule.end_point {
        let passed_time = current_time.min(end_point.time) - schedule.start_point.time;
        let time_period = end_point.time - schedule.start_point.time;
        if passed_time != 0 {
            let release_amount = Uint128::from(passed_time).multiply_ratio(
                end_point.amount.checked_sub(schedule.start_point.amount)?,
                time_period,
            );
            available_amount = available_amount.checked_add(release_amount)?;
        }
    }

    Ok(available_amount)
}

/// Withdraw tokens from active vesting schedule.
///
/// Withdraw is possible if there is only one active vesting schedule.
/// Only schedules with end_point are considered as active.
/// Active schedule's remaining amount must be greater than withdraw amount.
/// This function changes the current active schedule
/// setting current block time and already unlocked amount for start point
/// and reducing end point amount by the withdrawn amount.
///
/// * **account** whose schedule to withdraw from.
///
/// * **receiver** who will receive the withdrawn amount.
/// * **info.sender** is used if it is not specified.
///
/// * **amount** amount to withdraw from the only one active schedule.
///
fn withdraw_from_active_schedule(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    account: String,
    receiver: Option<String>,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount.is_zero() {
        return Err(ContractError::ZeroAmountWithdrawal {});
    }

    let config = CONFIG.load(deps.storage)?;
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let acc = deps.api.addr_validate(&account)?;
    let mut vesting_info = VESTING_INFO.load(deps.storage, &acc)?;
    let block_time = env.block.time.seconds();

    let mut active_schedules = vesting_info.schedules.iter_mut().filter(|schedule| {
        if let Some(end_point) = schedule.end_point {
            block_time >= schedule.start_point.time && block_time < end_point.time
        } else {
            false
        }
    });

    if let Some(schedule) = active_schedules.next() {
        // Withdraw is not allowed if there are multiple active schedules
        if active_schedules.next().is_some() {
            return Err(ContractError::MultipleActiveSchedules(account));
        }

        // It's safe to unwrap here because we checked that there is an end_point
        let mut end_point = schedule.end_point.unwrap();

        let sch_unlocked_amount = calc_schedule_unlocked_amount(schedule, block_time)?;

        let amount_left = end_point.amount.checked_sub(sch_unlocked_amount)?;
        if amount >= amount_left {
            return Err(ContractError::NotEnoughTokens(amount_left));
        }

        schedule.start_point = VestingSchedulePoint {
            time: block_time,
            amount: sch_unlocked_amount,
        };

        end_point.amount -= amount;
        schedule.end_point = Some(end_point);
    } else {
        return Err(ContractError::NoActiveVestingSchedule(account));
    };

    VESTING_INFO.save(deps.storage, &acc, &vesting_info)?;

    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or(info.sender);
    let transfer_msg = config
        .vesting_token
        .with_balance(amount)
        .into_msg(receiver.clone())?;

    Ok(Response::new().add_message(transfer_msg).add_attributes([
        attr("action", "withdraw_from_active_schedule"),
        attr("account", account),
        attr("amount", amount),
        attr("receiver", receiver),
    ]))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the contract configuration in an object of type [`Config`].
///
/// * **QueryMsg::VestingAccount { address }** Returns information about the vesting schedules that have a specific vesting recipient.
///
/// * **QueryMsg::VestingAccounts {
///             start_after,
///             limit,
///             order_by,
///         }** Returns a list of vesting schedules together with their vesting recipients.
///
/// * **QueryMsg::AvailableAmount { address }** Returns the available amount of tokens that can be claimed by a specific vesting recipient.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => Ok(to_json_binary(&query_config(deps)?)?),
        QueryMsg::VestingAccount { address } => {
            Ok(to_json_binary(&query_vesting_account(deps, address)?)?)
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => Ok(to_json_binary(&query_vesting_accounts(
            deps,
            start_after,
            limit,
            order_by,
        )?)?),
        QueryMsg::AvailableAmount { address } => Ok(to_json_binary(
            &query_vesting_available_amount(deps, env, address)?,
        )?),
        QueryMsg::Timestamp {} => Ok(to_json_binary(&query_timestamp(env)?)?),
    }
}

/// Returns the vesting contract configuration using a [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        owner: config.owner,
        vesting_token: config.vesting_token,
    })
}

/// Return the current block timestamp (in seconds)
/// * **env** is an object of type [`Env`].
pub fn query_timestamp(env: Env) -> StdResult<u64> {
    Ok(env.block.time.seconds())
}

/// Returns the vesting data for a specific vesting recipient using a [`VestingAccountResponse`] object.
///
/// * **address** vesting recipient for which to return vesting data.
pub fn query_vesting_account(deps: Deps, address: String) -> StdResult<VestingAccountResponse> {
    let address = deps.api.addr_validate(&address)?;
    let info = VESTING_INFO.load(deps.storage, &address)?;

    Ok(VestingAccountResponse { address, info })
}

/// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
///
/// * **start_after** index from which to start reading vesting schedules.
///
/// * **limit** amount of vesting schedules to return.
///
/// * **order_by** whether results should be returned in an ascending or descending order.
pub fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let start_after = addr_opt_validate(deps.api, &start_after)?;

    let vesting_infos = read_vesting_infos(deps, start_after, limit, order_by)?;

    let vesting_accounts: Vec<_> = vesting_infos
        .into_iter()
        .map(|(address, info)| VestingAccountResponse { address, info })
        .collect();

    Ok(VestingAccountsResponse { vesting_accounts })
}

/// Returns the available amount of vested and yet to be claimed tokens for a specific vesting recipient.
///
/// * **address** vesting recipient for which to return the available amount of tokens to claim.
pub fn query_vesting_available_amount(deps: Deps, env: Env, address: String) -> StdResult<Uint128> {
    let address = deps.api.addr_validate(&address)?;

    let info = VESTING_INFO.load(deps.storage, &address)?;
    let available_amount = compute_available_amount(env.block.time.seconds(), &info)?;
    Ok(available_amount)
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    let mut resp = Response::default();

    match contract_version.contract.as_ref() {
        "astroport-vesting" => match contract_version.version.as_ref() {
            // injective-888 1.1.0
            // pacific-1, injective-1, pisco-1, atlantic-2 1.2.0
            // phoenix-1 1.3.0
            // neutron-1, pion-1 1.3.1
            "1.1.0" | "1.2.0" | "1.3.0" | "1.3.1" => {
                let mut config = CONFIG.load(deps.storage)?;

                let converter_config: astro_converter::Config = deps.querier.query_wasm_smart(
                    &msg.converter_contract,
                    &astro_converter::QueryMsg::Config {},
                )?;

                ensure!(
                    converter_config.old_astro_asset_info == config.vesting_token,
                    StdError::generic_err(format!(
                        "Old astro asset info mismatch between vesting {} and converter {}",
                        config.vesting_token, converter_config.old_astro_asset_info
                    ))
                );

                let total_amount = config
                    .vesting_token
                    .query_pool(&deps.querier, env.contract.address)?;

                let convert_msg = match &config.vesting_token {
                    AssetInfo::Token { contract_addr } => wasm_execute(
                        contract_addr,
                        &cw20::Cw20ExecuteMsg::Send {
                            contract: msg.converter_contract,
                            amount: total_amount,
                            msg: to_json_binary(&astro_converter::Cw20HookMsg { receiver: None })?,
                        },
                        vec![],
                    )?,
                    AssetInfo::NativeToken { denom } => wasm_execute(
                        &msg.converter_contract,
                        &astro_converter::ExecuteMsg::Convert { receiver: None },
                        coins(total_amount.u128(), denom.to_string()),
                    )?,
                };
                resp.messages.push(SubMsg::new(convert_msg));

                config.vesting_token = AssetInfo::native(&converter_config.new_astro_denom);
                CONFIG.save(deps.storage, &config)?;
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(resp
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
