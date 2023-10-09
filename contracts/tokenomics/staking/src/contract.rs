use cosmwasm_std::{
    attr, coin, entry_point, to_binary, BankMsg, Binary, Deps, DepsMut, Env, MessageInfo, Reply,
    ReplyOn, Response, StdResult, SubMsg, Uint128,
};
use neutron_sdk::bindings::msg::NeutronMsg;
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::query::token_factory::query_full_denom;

use crate::error::ContractError;
use crate::state::{Config, CONFIG};
use astroport::staking::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg, StakingResponse,
};
use cw2::set_contract_version;
use cw_utils::must_pay;

use astroport::querier::query_balance;

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
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response<NeutronMsg>> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // TODO: Validate that deposit_token_denom exists on chain

    // Store config
    CONFIG.save(
        deps.storage,
        &Config {
            astro_denom: msg.deposit_token_denom,
            xastro_denom: "".to_string(),
        },
    )?;

    // Create the xASTRO TokenFactory token
    // TODO: After creating the TokenFactory token, also set the tracking contract
    // we need a Neutron upgrade to enable that

    let sub_msg: SubMsg<NeutronMsg> = SubMsg {
        id: INSTANTIATE_DENOM_REPLY_ID,
        msg: NeutronMsg::CreateDenom {
            subdenom: TOKEN_SYMBOL.to_string(),
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
) -> Result<Response<NeutronMsg>, ContractError> {
    match msg {
        ExecuteMsg::Enter {} => execute_enter(deps, env, info),
        ExecuteMsg::Leave {} => execute_leave(deps, env, info),
    }
}

/// The entry point to the contract for processing replies from submessages.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut<NeutronQuery>, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        INSTANTIATE_DENOM_REPLY_ID => {
            // Query the chain to get the final xASTRO denom
            // Neutron does not respond with the created denom in the reply
            // so msg.result.try_into()? has no value
            let denom_response = query_full_denom(
                deps.as_ref(),
                env.contract.address,
                TOKEN_SYMBOL.to_string(),
            )
            .map_err(|_| ContractError::FailedToCreateDenom {})?;

            let mut config = CONFIG.load(deps.storage)?;
            config.xastro_denom = denom_response.denom;
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new().add_attribute("xastro_denom", config.xastro_denom))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// Enter stakes TokenFactory ASTRO for xASTRO. xASTRO is minted to the sender
fn execute_enter(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<NeutronMsg>, ContractError> {
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

    let mut messages = vec![];

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
        messages.push(NeutronMsg::MintTokens {
            denom: config.xastro_denom.clone(),
            amount: MINIMUM_STAKE_AMOUNT,
            mint_to_address: env.contract.address.to_string(),
        });

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

    // Mint new xASTRO tokens to the sender
    messages.push(NeutronMsg::MintTokens {
        denom: config.xastro_denom,
        amount: mint_amount,
        mint_to_address: recipient.to_string(),
    });

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
fn execute_leave(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
) -> Result<Response<NeutronMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // Ensure that the correct token is sent. This will fail if
    // zero tokens are sent.
    let amount = must_pay(&info, &config.xastro_denom)?;

    // Receiver of the xASTRO tokens
    let recipient = info.sender;

    // Get the current deposits and shares held in the contract
    let total_deposit = query_balance(
        &deps.querier,
        env.contract.address,
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
    let burn_msg = NeutronMsg::BurnTokens {
        denom: config.xastro_denom,
        amount,
        burn_from_address: "".to_string(), // This needs to be "" for now
    };

    // Return the ASTRO tokens to the sender
    let transfer_msg = BankMsg::Send {
        to_address: recipient.to_string(),
        amount: vec![coin(return_amount.u128(), config.astro_denom)],
    };

    // Set the data to be returned in set_data to easy integration with
    // other contracts
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
