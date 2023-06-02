use crate::error::ContractError;
use crate::state::{validate_admins, CONFIG, GRANTS, OWNERSHIP_PROPOSAL};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::fee_granter::{Config, ExecuteMsg, InstantiateMsg};
use cosmos_sdk_proto::cosmos::base::v1beta1::Coin as SdkCoin;
use cosmos_sdk_proto::cosmos::feegrant::v1beta1::{
    BasicAllowance, MsgGrantAllowance, MsgRevokeAllowance,
};
use cosmos_sdk_proto::prost::Message;
use cosmos_sdk_proto::traits::TypeUrl;
use cosmos_sdk_proto::Any;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coins, Addr, BankMsg, CosmosMsg, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128,
};
use cw_utils::must_pay;
use std::collections::HashSet;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, StdError> {
    let admins = validate_admins(deps.api, &msg.admins)?;
    CONFIG.save(
        deps.storage,
        &Config {
            owner: deps.api.addr_validate(&msg.owner)?,
            admins,
            gas_denom: msg.gas_denom,
        },
    )?;

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
        ExecuteMsg::Grant {
            grantee_contract,
            amount,
            bypass_amount_check,
        } => {
            let grantee_contract = deps.api.addr_validate(&grantee_contract)?;
            grant(
                deps,
                env,
                info,
                grantee_contract,
                amount,
                bypass_amount_check,
            )
        }
        ExecuteMsg::Revoke { grantee_contract } => {
            let grantee_contract = deps.api.addr_validate(&grantee_contract)?;
            revoke(deps, env, info, grantee_contract)
        }
        ExecuteMsg::TransferCoins { amount, receiver } => {
            transfer_coins(deps, info, amount, receiver)
        }
        ExecuteMsg::UpdateAdmins { add, remove } => update_admins(deps, info, add, remove),
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
    }
}

fn grant(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    grantee_contract: Addr,
    amount: Uint128,
    bypass_amount_check: bool,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender && !config.admins.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    if !bypass_amount_check {
        let sent_amount = must_pay(&info, &config.gas_denom)?;
        if sent_amount != amount {
            return Err(ContractError::InvalidAmount {
                expected: amount,
                actual: sent_amount,
            });
        }
    }

    GRANTS.update(
        deps.storage,
        &grantee_contract,
        |existing| -> StdResult<_> {
            match existing {
                None => Ok(amount),
                Some(_) => Err(StdError::generic_err(format!(
                    "Grant already exists for {grantee_contract}",
                ))),
            }
        },
    )?;

    let allowance = BasicAllowance {
        spend_limit: vec![SdkCoin {
            denom: config.gas_denom,
            amount: amount.to_string(),
        }],
        expiration: None,
    };
    let grant_msg = MsgGrantAllowance {
        granter: env.contract.address.to_string(),
        grantee: grantee_contract.to_string(),
        allowance: Some(Any {
            type_url: BasicAllowance::TYPE_URL.to_string(),
            value: allowance.encode_to_vec(),
        }),
    };

    let msg = CosmosMsg::Stargate {
        type_url: MsgGrantAllowance::TYPE_URL.to_string(),
        value: grant_msg.encode_to_vec().into(),
    };
    Ok(Response::default().add_message(msg).add_attributes([
        ("action", "grant"),
        ("grantee_contract", grantee_contract.as_str()),
        ("amount", amount.to_string().as_str()),
    ]))
}

fn revoke(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    grantee_contract: Addr,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender && !config.admins.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }

    GRANTS.remove(deps.storage, &grantee_contract);

    let revoke_msg = MsgRevokeAllowance {
        granter: env.contract.address.to_string(),
        grantee: grantee_contract.to_string(),
    };
    let msg = CosmosMsg::Stargate {
        type_url: MsgRevokeAllowance::TYPE_URL.to_string(),
        value: revoke_msg.encode_to_vec().into(),
    };

    Ok(Response::default().add_message(msg).add_attributes([
        ("action", "revoke"),
        ("grantee_contract", grantee_contract.as_str()),
    ]))
}

fn transfer_coins(
    deps: DepsMut,
    info: MessageInfo,
    amount: Uint128,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender && !config.admins.contains(&info.sender) {
        return Err(ContractError::Unauthorized {});
    }
    let send_msg = CosmosMsg::Bank(BankMsg::Send {
        to_address: receiver.unwrap_or(info.sender.to_string()),
        amount: coins(amount.u128(), config.gas_denom),
    });
    Ok(Response::default().add_message(send_msg).add_attributes([
        ("action", "transfer_coins"),
        ("amount", amount.to_string().as_str()),
    ]))
}

fn update_admins(
    deps: DepsMut,
    info: MessageInfo,
    add_admins: Vec<String>,
    remove_admins: Vec<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if config.owner != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let mut admins: HashSet<_> = config.admins.into_iter().collect();
    validate_admins(deps.api, &add_admins)?
        .into_iter()
        .try_for_each(|admin| {
            if !admins.insert(admin.clone()) {
                return Err(StdError::generic_err(format!(
                    "Admin {admin} already exists",
                )));
            };
            Ok(())
        })?;

    let remove_set: HashSet<_> = validate_admins(deps.api, &remove_admins)?
        .into_iter()
        .collect();
    config.admins = admins.difference(&remove_set).cloned().collect();
    CONFIG.save(deps.storage, &config)?;

    let mut attributes = vec![attr("action", "update_admins")];
    if !add_admins.is_empty() {
        attributes.push(attr("add_admins", add_admins.join(",")));
    }
    if !remove_admins.is_empty() {
        attributes.push(attr("remove_admins", remove_admins.join(",")));
    }

    Ok(Response::default().add_attributes(attributes))
}
