use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo,
    Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
};

use crate::state::{read_vesting_infos, Config, CONFIG, OWNERSHIP_PROPOSAL, VESTING_INFO};

use crate::error::ContractError;
use astroport::asset::addr_validate_to_lower;
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
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default [`Response`] object if the operation was successful, otherwise returns
/// the [`StdResult`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for
/// creating a contract
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
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Claim { recipient, amount }** Claims the amount from Vesting for transfer
/// to the recipient.
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
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
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
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;

    // Check owner
    if cw20_msg.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Check token
    if info.sender != config.token_addr {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::RegisterVestingAccounts { vesting_accounts } => {
            register_vesting_accounts(deps, env, vesting_accounts, cw20_msg.amount)
        }
    }
}

/// ## Description
/// Register vesting accounts. Returns the [`Response`] with the specified attributes if the
/// operation was successful, otherwise returns the [`ContractError`].
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **vesting_accounts** is an array with items the type of [`VestingAccount`]. Sets the list of accounts to register.
///
/// * **cw20_amount** is the object of type [`Uint128`]. Sets the amount that confirms the total
/// amount of all accounts to register
pub fn register_vesting_accounts(
    deps: DepsMut,
    _env: Env,
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

    Ok(response
        .add_attribute("action", "register_vesting_accounts")
        .add_attribute("deposited", to_deposit))
}

/// ## Description
/// Approves vesting schedules. Returns the [`Ok`] if schedules are valid, otherwise returns the
/// [`ContractError`].
/// ## Params
/// * **addr** is the object of type [`Addr`]. Sets the address of the contract for which the error
/// will be returned
///
/// * **vesting_schedules** is the object of type [`Env`]. Sets the schedules to validate.
fn assert_vesting_schedules(
    addr: &Addr,
    vesting_schedules: &[VestingSchedule],
) -> Result<(), ContractError> {
    for sch in vesting_schedules.iter() {
        if let Some(end_point) = &sch.end_point {
            if !(sch.start_point.time < end_point.time && sch.start_point.amount < end_point.amount)
            {
                return Err(ContractError::VestingScheduleError(addr.clone()));
            }
        }
    }

    Ok(())
}

/// ## Description
/// Claims the amount from Vesting for transfer to the recipient. Returns the [`Response`] with
/// specified attributes if operation was successful, otherwise returns the [`ContractError`].
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **recipient** is an [`Option`] field of type [`String`]. Sets the recipient for claim.
///
/// * **amount** is an [`Option`] field of type [`Uint128`]. Sets the amount of claim.
pub fn claim(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: Option<String>,
    amount: Option<Uint128>,
) -> Result<Response, ContractError> {
    let mut response = Response::new();
    let mut attributes = vec![
        attr("action", "claim"),
        attr("address", info.sender.clone()),
    ];

    let config: Config = CONFIG.load(deps.storage)?;

    let mut vesting_info: VestingInfo = VESTING_INFO.load(deps.storage, &info.sender)?;

    let available_amount = compute_available_amount(env.block.time.seconds(), &vesting_info)?;

    let claim_amount = if let Some(a) = amount {
        if a > available_amount {
            return Err(ContractError::AmountIsNotAvailable {});
        };
        a
    } else {
        available_amount
    };

    attributes.append(&mut vec![
        attr("available_amount", available_amount),
        attr("claimed_amount", claim_amount),
    ]);

    if !claim_amount.is_zero() {
        response
            .messages
            .append(&mut vec![SubMsg::new(WasmMsg::Execute {
                contract_addr: config.token_addr.to_string(),
                funds: vec![],
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.unwrap_or_else(|| info.sender.to_string()),
                    amount: claim_amount,
                })?,
            })]);

        vesting_info.released_amount = vesting_info.released_amount.checked_add(claim_amount)?;
        VESTING_INFO.save(deps.storage, &info.sender, &vesting_info)?;
    };

    Ok(response.add_attributes(attributes))
}

/// ## Description
/// Computes the available amount for the specified input parameters. Returns the computed amount
/// if operation was successful.
/// ## Params
/// * **current_time** is the object of type [`Timestamp`]. Schedules with start point time bigger
/// then the current time will be omitted
///
/// * **vesting_info** is the object of type [`VestingInfo`]. Sets the vesting schedules to compute
/// available amount.
fn compute_available_amount(current_time: u64, vesting_info: &VestingInfo) -> StdResult<Uint128> {
    let mut available_amount: Uint128 = Uint128::zero();
    for sch in vesting_info.schedules.iter() {
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
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the base controls configs that contains in the [`Config`].
///
/// * **QueryMsg::VestingAccount { address }** Returns information for this account that contains
/// in the [`VestingInfo`].
///
/// * **QueryMsg::VestingAccounts {
///             start_after,
///             limit,
///             order_by,
///         }** Returns a list of accounts for the given input parameters.
///
/// * **QueryMsg::AvailableAmount { address }** Returns the available amount for specified account.
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
/// Returns information about the vesting configs in the [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        owner: config.owner,
        token_addr: config.token_addr,
    };

    Ok(resp)
}

pub fn query_timestamp(env: Env) -> StdResult<u64> {
    Ok(env.block.time.seconds())
}

/// ## Description
/// Returns information about the vesting account in the [`VestingAccountResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **address** is the object of type [`String`].
pub fn query_vesting_account(deps: Deps, address: String) -> StdResult<VestingAccountResponse> {
    let address = addr_validate_to_lower(deps.api, &address)?;
    let info: VestingInfo = VESTING_INFO.load(deps.storage, &address)?;

    let resp = VestingAccountResponse { address, info };

    Ok(resp)
}

/// ## Description
/// Returns a list of accounts, for the given input parameters, in the [`VestingAccountsResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field of type [`String`].
///
/// * **limit** is an [`Option`] field of type [`u32`].
///
/// * **order_by** is an [`Option`] field of type [`OrderBy`].
pub fn query_vesting_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<VestingAccountsResponse> {
    let start_after = start_after
        .map(|v| addr_validate_to_lower(deps.api, &v))
        .transpose()?;

    let vesting_infos = read_vesting_infos(deps, start_after, limit, order_by)?;

    let vesting_account_responses: Vec<VestingAccountResponse> = vesting_infos
        .into_iter()
        .map(|(address, info)| VestingAccountResponse { address, info })
        .collect();

    Ok(VestingAccountsResponse {
        vesting_accounts: vesting_account_responses,
    })
}

/// ## Description
/// Returns the available amount for specified account.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **address** is the object of type [`String`].
pub fn query_vesting_available_amount(deps: Deps, env: Env, address: String) -> StdResult<Uint128> {
    let address = addr_validate_to_lower(deps.api, &address)?;

    let info: VestingInfo = VESTING_INFO.load(deps.storage, &address)?;
    let available_amount = compute_available_amount(env.block.time.seconds(), &info)?;
    Ok(available_amount)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
