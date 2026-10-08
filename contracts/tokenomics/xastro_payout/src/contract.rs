use cosmwasm_std::{
    attr, ensure, entry_point, to_json_binary, Addr, Binary, Deps, DepsMut, Env, MessageInfo,
    Order, Response, StdError, StdResult, Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw_storage_plus::Bound;
use cw_utils::nonpayable;

use astroport::asset::AssetInfoExt;
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};

use crate::error::ContractError;
use crate::merkle::{decode_hash, leaf_hash, verify};
use crate::msg::{Config, ExecuteMsg, InstantiateMsg, MigrateMsg, Payment, QueryMsg, Status};
use crate::state::{CONFIG, OWNERSHIP_PROPOSAL, PAID, STATUS};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const DEFAULT_LIMIT: u32 = 50;
const MAX_LIMIT: u32 = 200;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    ensure!(
        decode_hash(&msg.merkle_root).is_some(),
        ContractError::InvalidRoot {}
    );
    msg.asset_info.check(deps.api)?;

    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            asset_info: msg.asset_info,
            merkle_root: msg.merkle_root.to_lowercase(),
            total: msg.total,
        },
    )?;
    STATUS.save(
        deps.storage,
        &Status {
            paid_total: Uint128::zero(),
            paid_count: 0,
            closed: false,
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
        ExecuteMsg::Pay { payments } => pay(deps, info, payments),
        ExecuteMsg::Close { recipient } => close(deps, env, info, recipient),
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
                CONFIG.update::<_, StdError>(deps.storage, |mut c| {
                    c.owner = new_owner;
                    Ok(c)
                })?;
                Ok(())
            })
            .map_err(Into::into)
        }
    }
}

/// Pays each entry to its own address, so the caller doesn't matter. Addresses already paid
/// (earlier, or earlier in this batch) are skipped, so a retried batch is harmless.
fn pay(
    deps: DepsMut,
    info: MessageInfo,
    payments: Vec<Payment>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;

    let config = CONFIG.load(deps.storage)?;
    let mut status = STATUS.load(deps.storage)?;
    ensure!(!status.closed, ContractError::Closed {});

    let root = decode_hash(&config.merkle_root).ok_or(ContractError::InvalidRoot {})?;

    let mut messages = vec![];
    let mut paid_amount = Uint128::zero();
    let mut skipped = 0u64;
    for payment in payments {
        let proof = payment
            .proof
            .iter()
            .map(|p| decode_hash(p))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| ContractError::InvalidProof {
                address: payment.address.clone(),
            })?;
        ensure!(
            verify(
                &root,
                leaf_hash(&payment.address, payment.amount.u128()),
                &proof
            ),
            ContractError::InvalidProof {
                address: payment.address
            }
        );

        let recipient = deps.api.addr_validate(&payment.address)?;
        if PAID.has(deps.storage, &recipient) {
            skipped += 1;
            continue;
        }

        status.paid_total = status.paid_total.checked_add(payment.amount)?;
        ensure!(
            status.paid_total <= config.total,
            ContractError::TotalExceeded {}
        );
        status.paid_count += 1;
        paid_amount += payment.amount;

        PAID.save(deps.storage, &recipient, &payment.amount)?;
        messages.push(
            config
                .asset_info
                .with_balance(payment.amount)
                .into_msg(recipient)?,
        );
    }

    let paid = messages.len();
    STATUS.save(deps.storage, &status)?;

    Ok(Response::new().add_messages(messages).add_attributes([
        attr("action", "pay"),
        attr("paid", paid.to_string()),
        attr("skipped", skipped.to_string()),
        attr("amount", paid_amount),
    ]))
}

/// Ends the payout and sends the whole remaining balance to `recipient`.
fn close(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});
    let recipient = deps.api.addr_validate(&recipient)?;

    let mut status = STATUS.load(deps.storage)?;
    status.closed = true;
    STATUS.save(deps.storage, &status)?;

    let balance = config
        .asset_info
        .query_pool(&deps.querier, &env.contract.address)?;
    let mut response = Response::new();
    if !balance.is_zero() {
        response = response.add_message(
            config
                .asset_info
                .with_balance(balance)
                .into_msg(&recipient)?,
        );
    }

    Ok(response.add_attributes([
        attr("action", "close"),
        attr("recipient", recipient),
        attr("swept", balance),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Status {} => to_json_binary(&STATUS.load(deps.storage)?),
        QueryMsg::Paid { address } => {
            let address = deps.api.addr_validate(&address)?;
            to_json_binary(&PAID.may_load(deps.storage, &address)?)
        }
        QueryMsg::PaidList { start_after, limit } => {
            let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
            let start_after = start_after
                .map(|a| deps.api.addr_validate(&a))
                .transpose()?;
            let list = PAID
                .range(
                    deps.storage,
                    start_after.as_ref().map(Bound::exclusive),
                    None,
                    Order::Ascending,
                )
                .take(limit)
                .collect::<StdResult<Vec<(Addr, Uint128)>>>()?;
            to_json_binary(&list)
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    if version.contract != CONTRACT_NAME {
        return Err(ContractError::MigrationError {});
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_version", version.version)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
