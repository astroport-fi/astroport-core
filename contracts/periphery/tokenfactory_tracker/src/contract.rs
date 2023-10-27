use astroport::tokenfactory_tracker::{InstantiateMsg, SudoMsg};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};
use osmosis_std::types::cosmos::auth::v1beta1::AuthQuerier;

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG};

const CONTRACT_NAME: &str = "astroport-tokenfactory-tracker";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    CONFIG.save(
        deps.storage,
        &Config {
            // Temporary save the module address until we can fetch on init
            tokenfactory_module_address: msg.tokenfactory_module_address,
            tracked_denom: msg.tracked_denom,
        },
    )?;

    // TODO: We need to get the module account for TokenFactory so we don't try and
    // subtract from it when minting to an account
    let accounts = AuthQuerier::new(&deps.querier).module_accounts()?;
    // cosmos.auth.v1beta1.ModuleAccount

    Ok(Response::default()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", CONTRACT_NAME)
        .add_attribute("accounts", format!("{:?}", accounts.accounts)))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // BlockBeforeSend is called before a send - if an error is returned the send
        // is cancelled
        // TODO: Check if gas is charged for this, I suspect it might not according to the SDK code
        SudoMsg::BlockBeforeSend { .. } => Ok(Response::default()),
        // TrackBeforeSend is called before a send - if an error is returned it will
        // be ignored and the send will continue
        // Minting a token directly to an address is also tracked
        // TODO: Check if gas is charged for this, I think gas is charged for this
        SudoMsg::TrackBeforeSend { from, to, amount } => {
            let config = CONFIG.load(deps.storage)?;

            // Temporary checks
            // If the token is minted directly to an address, we don't need to subtract
            if config.tokenfactory_module_address != from {
                BALANCES.update(
                    deps.storage,
                    &from,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_sub(amount.amount)?)
                    },
                )?;
            }

            // Temporary checks
            // TODO: Check if burn follows this path
            if config.tokenfactory_module_address != to {
                BALANCES.update(
                    deps.storage,
                    &to,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_add(amount.amount)?)
                    },
                )?;
            }

            // TODO: Update total supply

            // Sudo calls don't emit the attributes, so we need to emit them here
            Ok(Response::default())
        }
    }
}
