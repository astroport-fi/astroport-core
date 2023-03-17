use cosmwasm_std::{
    attr, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Uint128,
};
use cw20::{
    AllAccountsResponse, BalanceResponse, Cw20Coin, Cw20ReceiveMsg, EmbeddedLogo, Logo, LogoInfo,
    MarketingInfoResponse,
};
use cw20_base::allowances::{
    deduct_allowance, execute_decrease_allowance, execute_increase_allowance, query_allowance,
};

use crate::state::{capture_total_supply_history, get_total_supply_at, BALANCES};
use astroport::asset::addr_opt_validate;
use astroport::xastro_token::{InstantiateMsg, MigrateMsg, QueryMsg};
use cw2::{get_contract_version, set_contract_version};
use cw20_base::contract::{
    execute_update_marketing, execute_upload_logo, query_download_logo, query_marketing_info,
    query_minter, query_token_info,
};
use cw20_base::enumerable::query_owner_allowances;
use cw20_base::msg::ExecuteMsg;
use cw20_base::state::{MinterData, TokenInfo, LOGO, MARKETING_INFO, TOKEN_INFO};
use cw20_base::ContractError;
use cw_storage_plus::Bound;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-xastro-token";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// Settings for pagination.
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

const LOGO_SIZE_CAP: usize = 5 * 1024;

/// Checks if data starts with XML preamble
fn verify_xml_preamble(data: &[u8]) -> Result<(), ContractError> {
    // The easiest way to perform this check would be just match on regex, however regex
    // compilation is heavy and probably not worth it.

    let preamble = data
        .split_inclusive(|c| *c == b'>')
        .next()
        .ok_or(ContractError::InvalidXmlPreamble {})?;

    const PREFIX: &[u8] = b"<?xml ";
    const POSTFIX: &[u8] = b"?>";

    if !(preamble.starts_with(PREFIX) && preamble.ends_with(POSTFIX)) {
        Err(ContractError::InvalidXmlPreamble {})
    } else {
        Ok(())
    }

    // Additionally attributes format could be validated as they are well defined, as well as
    // comments presence inside of preable, but it is probably not worth it.
}

/// Validates XML logo
fn verify_xml_logo(logo: &[u8]) -> Result<(), ContractError> {
    verify_xml_preamble(logo)?;

    if logo.len() > LOGO_SIZE_CAP {
        Err(ContractError::LogoTooBig {})
    } else {
        Ok(())
    }
}

/// Validates png logo
fn verify_png_logo(logo: &[u8]) -> Result<(), ContractError> {
    // PNG header format:
    // 0x89 - magic byte, out of ASCII table to fail on 7-bit systems
    // "PNG" ascii representation
    // [0x0d, 0x0a] - dos style line ending
    // 0x1a - dos control character, stop displaying rest of the file
    // 0x0a - unix style line ending
    const HEADER: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if logo.len() > LOGO_SIZE_CAP {
        Err(ContractError::LogoTooBig {})
    } else if !logo.starts_with(&HEADER) {
        Err(ContractError::InvalidPngHeader {})
    } else {
        Ok(())
    }
}

/// Checks if passed logo is correct, and if not, returns an error
fn verify_logo(logo: &Logo) -> Result<(), ContractError> {
    match logo {
        Logo::Embedded(EmbeddedLogo::Svg(logo)) => verify_xml_logo(logo),
        Logo::Embedded(EmbeddedLogo::Png(logo)) => verify_png_logo(logo),
        Logo::Url(_) => Ok(()), // Any reasonable url validation would be regex based, probably not worth it
    }
}

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Check valid token info
    msg.validate()?;

    // Create initial accounts
    let total_supply = create_accounts(&mut deps, &env, &msg.initial_balances)?;

    if !total_supply.is_zero() {
        capture_total_supply_history(deps.storage, &env, total_supply)?;
    }

    // Check supply cap
    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap").into());
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.addr_validate(&m.minter)?,
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

    if let Some(marketing) = msg.marketing {
        let logo = if let Some(logo) = marketing.logo {
            verify_logo(&logo)?;
            LOGO.save(deps.storage, &logo)?;

            match logo {
                Logo::Url(url) => Some(LogoInfo::Url(url)),
                Logo::Embedded(_) => Some(LogoInfo::Embedded),
            }
        } else {
            None
        };

        let data = MarketingInfoResponse {
            project: marketing.project,
            description: marketing.description,
            marketing: marketing
                .marketing
                .map(|addr| deps.api.addr_validate(&addr))
                .transpose()?,
            logo,
        };
        MARKETING_INFO.save(deps.storage, &data)?;
    }

    Ok(Response::default())
}

/// Mints tokens for specific accounts.
///
/// * **accounts** array with accounts for which to mint tokens.
pub fn create_accounts(deps: &mut DepsMut, env: &Env, accounts: &[Cw20Coin]) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();

    for row in accounts {
        let address = deps.api.addr_validate(&row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount, env.block.height)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
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
/// * **ExecuteMsg::UpdateMarketing { project, description, marketing }** Updates marketing info.
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
        _ => Err(StdError::generic_err("Unsupported execute message").into()),
    }
}

/// Executes a token transfer.
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

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "transfer"),
        attr("from", info.sender),
        attr("to", rcpt_addr),
        attr("amount", amount),
    ]))
}

/// Burns a token.
///
/// * **amount** amount of tokens that the function caller wants to burn from their own account.
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // Lower the sender's balance
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;

    // Reduce total_supply
    let token_info = TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    capture_total_supply_history(deps.storage, &env, token_info.total_supply)?;

    let res = Response::new().add_attributes(vec![
        attr("action", "burn"),
        attr("from", info.sender),
        attr("amount", amount),
    ]);
    Ok(res)
}

/// Mints a token.
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

    if let Some(ref mint_data) = config.mint {
        if mint_data.minter.as_ref() != info.sender {
            return Err(ContractError::Unauthorized {});
        }
    } else {
        return Err(ContractError::Unauthorized {});
    }

    // Update supply and enforce cap
    config.total_supply = config
        .total_supply
        .checked_add(amount)
        .map_err(StdError::from)?;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }

    TOKEN_INFO.save(deps.storage, &config)?;

    capture_total_supply_history(deps.storage, &env, config.total_supply)?;

    // Add amount to recipient balance
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "mint"),
        attr("to", rcpt_addr),
        attr("amount", amount),
    ]))
}

/// Executes a token send.
///
/// * **contract** token contract.
///
/// * **amount** amount of tokens to send.
///
/// * **msg** internal serialized message.
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

    let rcpt_addr = deps.api.addr_validate(&contract)?;

    // Move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attributes(vec![
            attr("action", "send"),
            attr("from", &info.sender),
            attr("to", &rcpt_addr),
            attr("amount", amount),
        ])
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

/// Executes a transfer from.
///
/// * **owner** account from which to transfer tokens.
///
/// * **recipient** transfer recipient.
///
/// * **amount** amount to transfer.
pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // Deduct allowance before doing anything else
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_add(amount)?) },
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

/// Executes a burn from.
///
/// * **owner** account from which to burn tokens.
///
/// * **amount** amount of tokens to burn.
pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let owner_addr = deps.api.addr_validate(&owner)?;

    // Deduct allowance before doing anything else
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // Lower balance
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;

    // Reduce total_supply
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

/// Executes a send from.
///
/// * **owner** account from which to send tokens.
///
/// * **contract** token contract address.
///
/// * **amount** amount of tokens to send.
///
/// * **msg** internal serialized message.
pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&contract)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // Deduct allowance before doing anything else
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // Move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_sub(amount)?) },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance| -> StdResult<_> { Ok(balance.unwrap_or_default().checked_add(amount)?) },
    )?;

    let res = Response::new()
        .add_attributes(vec![
            attr("action", "send_from"),
            attr("from", &owner),
            attr("to", &contract),
            attr("by", &info.sender),
            attr("amount", amount),
        ])
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

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **Balance { address: String }** Returns the current balance of the given address, 0 if unset.
/// Uses a [`BalanceResponse`] object.
///
/// * **BalanceAt { address, block }** Returns the balance of the given address at the given block
/// using a [`BalanceResponse`] object.
///
/// * **TotalSupplyAt { block }** Returns the total supply at the given block.
///
/// * **TokenInfo {}** Returns the token metadata - name, decimals, supply, etc
/// using a [`cw20::TokenInfoResponse`] object.
///
/// * **Minter {}** Returns the address that can mint tokens and the hard cap on the total amount of tokens using
/// a [`cw20::MinterResponse`] object.
///
/// * **QueryMsg::Allowance { owner, spender }** Returns how much the spender can use from the owner account, 0 if unset.
/// Uses a [`cw20::AllowanceResponse`] object.
///
/// * **QueryMsg::AllAllowances { owner, start_after, limit }** Returns all allowances this owner has approved
/// using a [`cw20::AllAllowancesResponse`] object.
///
/// * **QueryMsg::AllAccounts { start_after, limit }** Returns all accounts that have a balance
/// using a [`cw20::AllAccountsResponse`] object.
///
/// * **QueryMsg::MarketingInfo {}** Returns the token metadata
/// using a [`cw20::MarketingInfoResponse`] object.
///
/// * **QueryMsg::DownloadLogo {}** Downloads the embedded logo data (if stored on-chain)
/// and returns the result using a [`cw20::DownloadLogoResponse`] object.
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
        } => to_binary(&query_owner_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
    }
}

/// Returns the specified account's balance.
pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// Returns the balance of the given address at the given block.
pub fn query_balance_at(deps: Deps, address: String, block: u64) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, block)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// Returns the current balances of multiple accounts.
///
/// * **start_after** account from which to start querying for balances.
///
/// * **limit** amount of account balances to return.
pub fn query_all_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllAccountsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = addr_opt_validate(deps.api, &start_after)?;
    let start = start.as_ref().map(Bound::exclusive);

    let accounts = BALANCES
        .keys(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|addr| addr.map(Into::into))
        .collect::<StdResult<_>>()?;

    Ok(AllAccountsResponse { accounts })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-xastro-token" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" => {}
            _ => {
                return Err(StdError::generic_err(
                    "Cannot migrate. Unsupported contract version",
                ))
            }
        },
        _ => {
            return Err(StdError::generic_err(
                "Cannot migrate. Unsupported contract name",
            ))
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
