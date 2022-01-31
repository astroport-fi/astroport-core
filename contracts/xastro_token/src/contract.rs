use cosmwasm_std::{
    attr, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Uint128,
};
use cw20::{AllAccountsResponse, BalanceResponse, Cw20Coin, Cw20ReceiveMsg};
use cw20_base::allowances::{
    deduct_allowance, execute_decrease_allowance, execute_increase_allowance, query_allowance,
};

use crate::state::{capture_total_supply_history, get_total_supply_at, BALANCES};
use astroport::asset::addr_validate_to_lower;
use cw2::set_contract_version;
use cw20_base::contract::{
    execute_update_marketing, execute_upload_logo, query_download_logo, query_marketing_info,
    query_minter, query_token_info,
};
use cw20_base::enumerable::query_all_allowances;
use cw20_base::msg::ExecuteMsg;
use cw20_base::state::{MinterData, TokenInfo, TOKEN_INFO};
use cw20_base::ContractError;
use cw_storage_plus::Bound;

use astroport::xastro_token::{InstantiateMsg, QueryMsg};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-xastro-token";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Check valid token info
    msg.validate()?;

    // Create initial accounts
    let total_supply = create_accounts(&mut deps, &env, msg.initial_balances.as_slice())?;

    if !total_supply.is_zero() {
        capture_total_supply_history(deps.storage, &env, total_supply)?;
    }

    // Check supply cap
    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap"));
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: addr_validate_to_lower(deps.api, &m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // Store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
    };

    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}

/// # Description
/// Creates accounts.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **accounts** is the array of objects of type [`Cw20Coin`].
pub fn create_accounts(deps: &mut DepsMut, env: &Env, accounts: &[Cw20Coin]) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();

    for row in accounts {
        let address = addr_validate_to_lower(deps.api, &row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount, env.block.height)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
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
/// * **ExecuteMsg::Transfer { recipient, amount }** Transfers tokens to recipient.
///
/// * **ExecuteMsg::Burn { amount }** Burns tokens.
///
/// * **ExecuteMsg::Send { contract, amount, msg }** Sends tokens to contract and executes message.
///
/// * **ExecuteMsg::Mint { recipient, amount }** Mints tokens.
///
/// * **ExecuteMsg::IncreaseAllowance { spender, amount, expires }** Increases allowance.
///
/// * **ExecuteMsg::DecreaseAllowance { spender, amount, expires }** Decreases allowance.
///
/// * **ExecuteMsg::TransferFrom { owner, recipient, amount }** Transfers tokens from.
///
/// * **ExecuteMsg::BurnFrom { owner, amount }** Burns tokens from.
///
/// * **ExecuteMsg::SendFrom { owner, contract, amount, msg }** Sends tokens from.
///
/// * **ExecuteMsg::UpdateMarketing { project, description, marketing }** Updates marketing.
///
/// * **ExecuteMsg::UploadLogo(logo)** Uploads logo.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),
        ExecuteMsg::UploadLogo(logo) => execute_upload_logo(deps, env, info, logo),
    }
}

/// # Description
/// Executes token transfer. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **recipient** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = addr_validate_to_lower(deps.api, &recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

/// # Description
/// Executes token burn. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **amount** is the object of type [`Uint128`].
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // lower balance
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;

    // reduce total_supply
    let token_info = TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    capture_total_supply_history(deps.storage, &env, token_info.total_supply)?;

    let res = Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount);
    Ok(res)
}

/// # Description
/// Executes token minting. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **recipient** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = TOKEN_INFO.load(deps.storage)?;
    if config.mint.is_none() || config.mint.as_ref().unwrap().minter != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }

    TOKEN_INFO.save(deps.storage, &config)?;

    capture_total_supply_history(deps.storage, &env, config.total_supply)?;

    // add amount to recipient balance
    let rcpt_addr = addr_validate_to_lower(deps.api, &recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

/// # Description
/// Executes send tokens. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **contract** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
///
/// * **msg** is the object of type [`Binary`].
pub fn execute_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = addr_validate_to_lower(deps.api, &contract)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "send")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &contract)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(contract)?,
        );
    Ok(res)
}

/// # Description
/// Executes transfer from. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **owner** is the object of type [`String`].
///
/// * **recipient** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let rcpt_addr = addr_validate_to_lower(deps.api, &recipient)?;
    let owner_addr = addr_validate_to_lower(deps.api, &owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new().add_attributes(vec![
        attr("action", "transfer_from"),
        attr("from", owner),
        attr("to", recipient),
        attr("by", info.sender),
        attr("amount", amount),
    ]);
    Ok(res)
}

/// # Description
/// Executes burn from. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **owner** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let owner_addr = addr_validate_to_lower(deps.api, &owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // lower balance
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;

    // reduce total_supply
    let token_info = TOKEN_INFO.update(deps.storage, |mut meta| -> StdResult<_> {
        meta.total_supply = meta.total_supply.checked_sub(amount)?;
        Ok(meta)
    })?;

    capture_total_supply_history(deps.storage, &env, token_info.total_supply)?;

    let res = Response::new().add_attributes(vec![
        attr("action", "burn_from"),
        attr("from", owner),
        attr("by", info.sender),
        attr("amount", amount),
    ]);
    Ok(res)
}

/// # Description
/// Executes send from. Returns an [`ContractError`] on
/// failure, otherwise returns the [`Response`] with the specified attributes if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **owner** is the object of type [`String`].
///
/// * **contract** is the object of type [`String`].
///
/// * **amount** is the object of type [`Uint128`].
///
/// * **msg** is the object of type [`Binary`].
pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let rcpt_addr = addr_validate_to_lower(deps.api, &contract)?;
    let owner_addr = addr_validate_to_lower(deps.api, &owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let attrs = vec![
        attr("action", "send_from"),
        attr("from", &owner),
        attr("to", &contract),
        attr("by", &info.sender),
        attr("amount", amount),
    ];

    // create a send message
    let msg = Cw20ReceiveMsg {
        sender: info.sender.into(),
        amount,
        msg,
    }
    .into_cosmos_msg(contract)?;

    let res = Response::new().add_message(msg).add_attributes(attrs);
    Ok(res)
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
/// * **Balance { address: String }** Returns the current balance of the given address, 0 if unset.
/// in a [`BalanceResponse`] object.
///
/// * **BalanceAt { address, block }** Returns balance of the given address at the given block
/// in a [`BalanceResponse`] object.
///
/// * **TotalSupplyAt { block }** Returns total supply at the given block.
///
/// * **TokenInfo {}** Returns the metadata on the contract - name, decimals, supply, etc
/// in a [`TokenInfoResponse`] object.
///
/// * **Minter {}** Returns who can mint and the hard cap on maximum tokens after minting
/// in a [`MinterResponse`] object.
///
/// * **QueryMsg::Allowance { owner, spender }** Returns how much spender can use from owner account, 0 if unset
/// in a [`AllowanceResponse`] object.
///
/// * **QueryMsg::AllAllowances { owner, start_after, limit }** Returns all allowances this owner has approved
/// in a [`AllAllowancesResponse`] object.
///
/// * **QueryMsg::AllAccounts { start_after, limit }** Returns all accounts that have balances
/// in a [`AllAccountsResponse`] object.
///
/// * **QueryMsg::MarketingInfo {}** Returns more metadata on the contract
/// in a [`MarketingInfoResponse`] object.
///
/// * **QueryMsg::DownloadLogo {}** Downloads the mbeded logo data (if stored on chain)
/// in a [`DownloadLogoResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::BalanceAt { address, block } => {
            to_binary(&query_balance_at(deps, address, block)?)
        }
        QueryMsg::TotalSupplyAt { block } => to_binary(&get_total_supply_at(deps.storage, block)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
    }
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **address** is the object of type [`String`].
pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = addr_validate_to_lower(deps.api, &address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address at the given block.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **address** is the object of type [`String`].
///
/// * **block** is the object of type [`u64`].
pub fn query_balance_at(deps: Deps, address: String, block: u64) -> StdResult<BalanceResponse> {
    let address = addr_validate_to_lower(deps.api, &address)?;
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, block)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address at the given block.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field object of type [`String`].
///
/// * **limit** is an [`Option`] field object of type [`u32`].
pub fn query_all_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllAccountsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let accounts: Result<Vec<_>, _> = BALANCES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(StdResult::unwrap)
        .map(|(a, _)| String::from_utf8(a))
        .collect();

    Ok(AllAccountsResponse {
        accounts: accounts?,
    })
}
