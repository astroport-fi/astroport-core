use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::state::{read_vesting_infos, Config, CONFIG, OWNERSHIP_PROPOSAL, VESTING_INFO};

use crate::error::ContractError;
use astroport::asset::{addr_opt_validate, addr_validate_to_lower};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::vesting::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, OrderBy, QueryMsg,
    VestingAccount, VestingAccountResponse, VestingAccountsResponse, VestingInfo, VestingSchedule,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-vesting";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// Returns a default [`Response`] object if the operation was successful, otherwise returns
/// a [`StdResult`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters for
/// creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: addr_validate_to_lower(deps.api, &msg.owner)?,
            token_addr: addr_validate_to_lower(deps.api, &msg.token_addr)?,
        },
    )?;

    Ok(Response::new())
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
/// * **ExecuteMsg::Claim { recipient, amount }** Claims vested tokens and transfers them to the vesting recipient.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes it
/// depending on the received template.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Claim { recipient, amount } => claim(deps, env, info, recipient, amount),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, info, msg),
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

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 message to process.
fn receive_cw20(
    deps: DepsMut,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Permission check
    if cw20_msg.sender != config.owner || info.sender != config.token_addr {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, vesting_accounts, cw20_msg.amount)
        }
    }
}

/// ## Description
/// Create new vesting schedules. Returns a [`Response`] with the specified attributes if the
/// operation was successful, otherwise returns a [`ContractError`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **vesting_accounts** is an array with items of type [`VestingAccount`].
/// This is the list of accounts and associated vesting schedules to create.
///
/// * **cw20_amount** is an object of type [`Uint128`]. Sets the amount that confirms the total
/// amount of all accounts to register
pub fn register_vesting_accounts(
    deps: DepsMut,
    vesting_accounts: Vec<VestingAccount>,
    cw20_amount: Uint128,
) -> Result<Response, ContractError> {
    let response = Response::new();

    let mut to_deposit = Uint128::zero();

    for mut vesting_account in vesting_accounts {
        let mut released_amount = Uint128::zero();
        let account_address = addr_validate_to_lower(deps.api, &vesting_account.address)?;

        assert_vesting_schedules(&account_address, &vesting_account.schedules)?;

        for sch in &vesting_account.schedules {
            let amount = if let Some(end_point) = &sch.end_point {
                end_point.amount
            } else {
                sch.start_point.amount
            };
            to_deposit = to_deposit.checked_add(amount)?;
        }

        if let Some(mut old_info) = VESTING_INFO.may_load(deps.storage, &account_address)? {
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

    if to_deposit != cw20_amount {
        return Err(ContractError::VestingScheduleAmountError {});
    }

    Ok(response.add_attributes({
        vec![
            attr("action", "register_vesting_accounts"),
            attr("deposited", to_deposit),
        ]
    }))
}

/// ## Description
/// Asserts the validity of a list of vesting schedules. Returns an [`Ok`] if the schedules are valid, otherwise returns a
/// [`ContractError`].
/// ## Params
/// * **addr** is an object of type [`Addr`]. This is the receiver of the vested tokens.
///
/// * **vesting_schedules** is an object of type [`Env`]. These are the vesting schedules to validate.
fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.to_string()));
            }
        }
    }

    Ok(())
}

/// ## Description
/// Claims vested tokens and transfers them to the vesting recipient. Returns a [`Response`] with
/// specified attributes if operation was successful, otherwise returns a [`ContractError`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **recipient** is an [`Option`] field of type [`String`]. This is the vesting recipient for which to claim tokens.
///
/// * **amount** is an [`Option`] field of type [`Uint128`]. This is the amount of vested tokens to claim.
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
        response = response.add_submessage(SubMsg::new(WasmMsg::Execute {
            contract_addr: config.token_addr.to_string(),
            funds: vec![],
            msg: to_binary(&Cw20ExecuteMsg::Transfer {
                recipient: recipient.unwrap_or_else(|| info.sender.to_string()),
                amount: claim_amount,
            })?,
        }));

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

/// ## Description
/// Computes the amount of vested and yet unclaimed tokens for a specific vesting recipient. Returns the computed amount
/// if the operation is successful.
/// ## Params
/// * **current_time** is an object of type [`Timestamp`]. This is the timestamp from which to start querying for vesting schedules.
/// Schedules that started later than current_time will be omitted.
///
/// * **vesting_info** is an object of type [`VestingInfo`]. These are the vesting schedules for which to compute the amount of tokens
/// that are vested and can be claimed by the recipient.
fn compute_available_amount(current_time: u64, vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in &vesting_info.schedules {
        if sch.start_point.time > current_time {
            continue;
        }

        available_amount = available_amount.checked_add(sch.start_point.amount)?;

        if let Some(end_point) = &sch.end_point {
            let passed_time = current_time.min(end_point.time) - sch.start_point.time;
            let time_period = end_point.time - sch.start_point.time;
            if passed_time != 0 && time_period != 0 {
                let release_amount = Uint128::from(passed_time).multiply_ratio(
                    end_point.amount.checked_sub(sch.start_point.amount)?,
                    time_period,
                );
                available_amount = available_amount.checked_add(release_amount)?;
            }
        }
    }

    available_amount
        .checked_sub(vesting_info.released_amount)
        .map_err(StdError::from)
}

/// ## Description
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
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
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::VestingAccount { address } => {
            Ok(to_binary(&query_vesting_account(deps, address)?)?)
        }
        QueryMsg::VestingAccounts {
            start_after,
            limit,
            order_by,
        } => Ok(to_binary(&query_vesting_accounts(
            deps,
            start_after,
            limit,
            order_by,
        )?)?),
        QueryMsg::AvailableAmount { address } => Ok(to_binary(&query_vesting_available_amount(
            deps, env, address,
        )?)?),
        QueryMsg::Timestamp {} => Ok(to_binary(&query_timestamp(env)?)?),
    }
}

/// ## Description
/// Returns the vesting contract configuration using a [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;

    Ok(ConfigResponse {
        owner: config.owner,
        token_addr: config.token_addr,
    })
}

/// ## Description
/// Return the current block timestamp (in seconds)
///
/// ## Params
/// * **env** is an object of type [`Env`].
pub fn query_timestamp(env: Env) -> StdResult<u64> {
    Ok(env.block.time.seconds())
}

/// ## Description
/// Returns the vesting data for a specific vesting recipient using a [`VestingAccountResponse`] object.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **address** is an object of type [`String`]. This is the vesting recipient for which to return vesting data.
pub fn query_vesting_account(deps: Deps, address: String) -> StdResult<VestingAccountResponse> {
    let address = addr_validate_to_lower(deps.api, &address)?;
    let info = VESTING_INFO.load(deps.storage, &address)?;

    Ok(VestingAccountResponse { address, info })
}

/// ## Description
/// Returns a list of vesting schedules using a [`VestingAccountsResponse`] object.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field of type [`String`]. This is the index from which to start reading vesting schedules.
///
/// * **limit** is an [`Option`] field of type [`u32`]. This is the amount of vesting schedules to return.
///
/// * **order_by** is an [`Option`] field of type [`OrderBy`]. This dictates whether results
/// should be returned in an ascending or descending order.
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

/// ## Description
/// Returns the available amount of vested and yet to be claimed tokens for a specific vesting recipient.
///
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **address** is an object of type [`String`]. This is the vesting recipient for which to return the available amount of tokens to claim.
pub fn query_vesting_available_amount(deps: Deps, env: Env, address: String) -> StdResult<Uint128> {
    let address = addr_validate_to_lower(deps.api, &address)?;

    let info = VESTING_INFO.load(deps.storage, &address)?;
    let available_amount = compute_available_amount(env.block.time.seconds(), &info)?;
    Ok(available_amount)
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
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
