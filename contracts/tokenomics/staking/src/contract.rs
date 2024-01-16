use cosmwasm_std::{
    attr, coin, entry_point, to_binary, BankMsg, Binary, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Reply, ReplyOn, Response, StdResult, SubMsg, Uint128,
};
use cw2::set_contract_version;
use cw_utils::must_pay;
use osmosis_std::types::cosmos::bank::v1beta1::{DenomUnit, Metadata};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgMint, MsgSetBeforeSendHook, MsgSetDenomMetadata,
};

use astroport::querier::query_balance;
use astroport::staking::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, StakingResponse,
};
use astroport::tokenfactory_tracker::{track_before_send, SudoMsg};

use crate::error::ContractError;
use crate::state::{Config, CONFIG};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-staking";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// xASTRO information
/// TODO: Once Neutron allows setting metadata, add this as decimals
const TOKEN_NAME: &str = "Staked Astroport Token";
const TOKEN_SYMBOL: &str = "xASTRO";

/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_DENOM_REPLY_ID: u64 = 1;

/// Minimum initial xastro share
pub(crate) const MINIMUM_STAKE_AMOUNT: Uint128 = Uint128::new(1_000);

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // TODO: Validate that deposit_token_denom exists on chain

    // Store config
    CONFIG.save(
        deps.storage,
        &Config {
            astro_denom: msg.deposit_token_denom,
            xastro_denom: "".to_string(),
            tracking_code_id: msg.tracking_code_id,
            tracking_contract_address: "".to_string(),
        },
    )?;

    // Create the xASTRO TokenFactory token
    // TODO: After creating the TokenFactory token, also set the tracking contract
    // we need a Neutron upgrade to enable that

    let sub_msg = SubMsg {
        id: INSTANTIATE_DENOM_REPLY_ID,
        msg: MsgCreateDenom {
            sender: env.contract.address.to_string(),
            subdenom: TOKEN_SYMBOL.to_owned(),
        }
        .into(),
        gas_limit: None,
        reply_on: ReplyOn::Success,
    };

    Ok(Response::new().add_submessage(sub_msg))
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Enter** Stake the provided ASTRO tokens for xASTRO
/// * **ExecuteMsg::Leave** Unstake the provided xASTRO tokens for ASTRO
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Enter {} => execute_enter(deps, env, info),
        ExecuteMsg::Leave {} => execute_leave(deps, env, info),
    }
}

/// Exposes execute functions called by the chain's TokenFactory module
///
/// ## Variants
/// * **SudoMsg::BlockBeforeSend** Called before sending a token, error fails the transaction
/// * **SudoMsg::TrackBeforeSend** Called before sending a token, error is ignored
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // For xASTRO we don't implement any blocking, but is still required
        // to be implemented
        SudoMsg::BlockBeforeSend { .. } => Ok(Response::default()),
        // TrackBeforeSend is called before a send - if an error is returned it will
        // be ignored and the send will continue
        // Minting a token directly to an address is also tracked
        SudoMsg::TrackBeforeSend { from, to, amount } => {
            //let config = CONFIG.load(deps.storage)?;
            // Get the module address
            track_before_send(deps, env, from, to, amount).map_err(Into::into)
        }
    }
}
/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_DENOM_REPLY_ID => {
            let mut config = CONFIG.load(deps.storage)?;

            // TODO: Once Neutron implements the same flow as Osmosis, we'll
            // be able to get the created denom from the reply data
            // For now, we reconstruct the denom from the contract address
            // TODO: Use new TokenFactory abstraction
            let created_denom = format!("factory/{}/{}", env.contract.address, TOKEN_SYMBOL);

            // TODO: Decide correct metadata
            let denom_metadata_msg = MsgSetDenomMetadata {
                sender: env.contract.address.to_string(),
                metadata: Some(Metadata {
                    symbol: TOKEN_SYMBOL.to_string(),
                    name: TOKEN_NAME.to_string(),
                    base: created_denom.clone(),
                    display: TOKEN_SYMBOL.to_string(),
                    denom_units: vec![
                        DenomUnit {
                            denom: created_denom.to_string(),
                            exponent: 12,
                            aliases: vec![],
                        },
                        DenomUnit {
                            denom: TOKEN_SYMBOL.to_string(),
                            exponent: 6,
                            aliases: vec![],
                        },
                    ],
                    description: TOKEN_NAME.to_string(),
                }),
            };

            config.xastro_denom = created_denom;

            // Enable balance tracking for xASTRO
            let set_hook_msg = MsgSetBeforeSendHook {
                sender: env.contract.address.to_string(),
                denom: config.xastro_denom.clone(),
                cosmwasm_address: env.contract.address.to_string(),
            };

            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new()
                .add_message(set_hook_msg)
                .add_message(denom_metadata_msg)
                .add_attribute("xastro_denom", config.xastro_denom))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// Enter stakes TokenFactory ASTRO for xASTRO. xASTRO is minted to the sender
fn execute_enter(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Ensure that the correct token is sent. This will fail if
    // zero tokens are sent.
    let mut amount = must_pay(&info, &config.astro_denom)?;

    // Receiver of the xASTRO tokens
    let recipient = info.sender;

    // Get the current deposits and shares held in the contract
    let total_deposit = query_balance(
        &deps.querier,
        env.contract.address.clone(),
        config.astro_denom.clone(),
    )?;
    let total_shares = deps
        .querier
        .query_supply(config.xastro_denom.clone())?
        .amount;

    let mut messages: Vec<CosmosMsg> = vec![];

    let mint_amount: Uint128 = if total_shares.is_zero() || total_deposit.is_zero() {
        amount = amount
            .checked_sub(MINIMUM_STAKE_AMOUNT)
            .map_err(|_| ContractError::MinimumStakeAmountError {})?;

        // There needs to be a minimum amount initially staked, thus the result
        // cannot be zero if the amount if not enough
        if amount.is_zero() {
            return Err(ContractError::MinimumStakeAmountError {});
        }

        // Mint the xASTRO tokens to ourselves if this is the first stake
        messages.push(
            MsgMint {
                sender: env.contract.address.to_string(),
                amount: Some(coin(MINIMUM_STAKE_AMOUNT.u128(), config.xastro_denom.clone()).into()),
                mint_to_address: env.contract.address.to_string(),
            }
            .into(),
        );

        amount
    } else {
        amount = amount
            .checked_mul(total_shares)?
            .checked_div(total_deposit)?;

        if amount.is_zero() {
            return Err(ContractError::StakeAmountTooSmall {});
        }

        amount
    };

    let minted_coins = coin(mint_amount.u128(), config.xastro_denom);

    // Mint new xASTRO tokens to the sender
    messages.push(
        MsgMint {
            sender: env.contract.address.to_string(),
            amount: Some(minted_coins.clone().into()),
            mint_to_address: env.contract.address.to_string(),
        }
        .into(),
    );

    // TokenFactory minting only allows minting to the sender for now, thus we
    // need to send the minted tokens to the recipient
    messages.push(
        BankMsg::Send {
            to_address: recipient.to_string(),
            amount: vec![minted_coins],
        }
        .into(),
    );

    // Set the data to be returned in set_data to easy integration with
    // other contracts
    let staking_response = to_binary(&StakingResponse {
        astro_amount: amount,
        xastro_amount: mint_amount,
    })?;

    Ok(Response::new()
        .add_messages(messages)
        .set_data(staking_response)
        .add_attributes(vec![
            attr("action", "enter"),
            attr("recipient", recipient),
            attr("astro_amount", amount),
            attr("xastro_amount", mint_amount),
        ]))
}

/// Leave unstakes TokenFactory xASTRO for ASTRO. xASTRO is burned and ASTRO
/// returned to the sender
fn execute_leave(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Ensure that the correct token is sent. This will fail if
    // zero tokens are sent.
    let amount = must_pay(&info, &config.xastro_denom)?;

    // Receiver of the xASTRO tokens
    let recipient = info.sender;

    // Get the current deposits and shares held in the contract
    let total_deposit = query_balance(
        &deps.querier,
        env.contract.address.clone(),
        config.astro_denom.clone(),
    )?;
    let total_shares = deps
        .querier
        .query_supply(config.xastro_denom.clone())?
        .amount;

    // Calculate the amount of ASTRO to return based on the ratios of
    // deposit and shares
    let return_amount = amount
        .checked_mul(total_deposit)?
        .checked_div(total_shares)?;

    // Burn the received xASTRO tokens
    let burn_msg = MsgBurn {
        sender: env.contract.address.to_string(),
        amount: Some(coin(amount.u128(), config.xastro_denom).into()),
        burn_from_address: "".to_string(), // This needs to be "" for now
    };

    // Return the ASTRO tokens to the sender
    let transfer_msg = BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![coin(return_amount.u128(), config.astro_denom)],
    };

    // Set the data to be returned in set_data to easy integration with
    // other contracts
    // TODO: Test if this works now
    let staking_response = to_binary(&StakingResponse {
        astro_amount: return_amount,
        xastro_amount: amount,
    })?;

    Ok(Response::new()
        .add_message(burn_msg)
        .add_message(transfer_msg)
        .set_data(staking_response)
        .add_attributes(vec![
            attr("action", "leave"),
            attr("recipient", recipient),
            attr("xastro_amount", amount),
            attr("astro_amount", return_amount),
        ]))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the staking contract configuration using a [`ConfigResponse`] object.
///
/// * **QueryMsg::TotalShares {}** Returns the total xASTRO supply using a [`Uint128`] object.
///
/// * **QueryMsg::Config {}** Returns the amount of ASTRO that's currently in the staking pool using a [`Uint128`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let config = CONFIG.load(deps.storage)?;
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&ConfigResponse {
            deposit_denom: config.astro_denom,
            share_denom: config.xastro_denom,
            share_tracking_address: config.tracking_contract_address,
        })?),
        QueryMsg::TotalShares {} => {
            to_binary(&deps.querier.query_supply(config.xastro_denom)?.amount)
        }
        QueryMsg::TotalDeposit {} => to_binary(&query_balance(
            &deps.querier,
            env.contract.address,
            config.astro_denom,
        )?),
    }
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
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    // No migration is possible to move from CW20 ASTRO and
    // xASTRO to TokenFactory versions
    Err(ContractError::MigrationError {})
}
