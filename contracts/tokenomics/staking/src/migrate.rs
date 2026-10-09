use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{coins, BankMsg, DepsMut, Env, Response, Uint128};
use cw2::{get_contract_version, set_contract_version};
use osmosis_std::types::osmosis::tokenfactory::v1beta1::MsgSetBeforeSendHook;

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use crate::state::{StakingMode, CONFIG, MODE, RETIRED};

#[cw_serde]
#[derive(Default)]
pub struct MigrateMsg {
    /// Sets what users can do: `"open"`, `"leave_only"` or `"paused"`. Leaves it as it is if not
    /// set.
    #[serde(default)]
    pub mode: Option<StakingMode>,
    /// Points xASTRO's tokenfactory before-send hook at this contract (the staking contract is
    /// the denom admin). Leaves the hook as it is if not set. The chain only calls a hook whose
    /// code id is whitelisted for xASTRO, so this is meant for instances of the whitelisted
    /// tracker code: the balance tracker itself, or one tracking another denom, which refuses
    /// every xASTRO send, mint and burn and so freezes xASTRO until the hook is pointed back.
    #[serde(default)]
    pub before_send_hook: Option<String>,
    /// Sends `amount` of the staked ASTRO to `recipient`. Only allowed while staking is paused
    /// (after this migration's `mode` is applied), and never more than the contract holds. Meant
    /// for moving ASTRO deposited at a broken rate out so it can be returned to its owners. After
    /// a withdrawal staking stays paused for good: later migrations can't set another mode.
    #[serde(default)]
    pub withdraw_astro: Option<WithdrawAstro>,
}

#[cw_serde]
pub struct WithdrawAstro {
    pub recipient: String,
    pub amount: Uint128,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-staking" => match contract_version.version.as_ref() {
            "2.0.0" | "2.1.0" | "2.2.0" | "2.3.0" | "2.3.1" | "2.3.2" | "2.3.3" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    if let Some(mode) = msg.mode {
        if RETIRED.exists(deps.storage) && mode != StakingMode::Paused {
            return Err(ContractError::Retired {});
        }
        MODE.save(deps.storage, &mode)?;
    }

    let mut response = Response::new();
    if let Some(hook) = msg.before_send_hook {
        let hook = deps.api.addr_validate(&hook)?;
        let config = CONFIG.load(deps.storage)?;
        response = response
            .add_message(MsgSetBeforeSendHook {
                sender: env.contract.address.to_string(),
                denom: config.xastro_denom,
                cosmwasm_address: hook.to_string(),
            })
            .add_attribute("before_send_hook", hook);
    }

    if let Some(WithdrawAstro { recipient, amount }) = msg.withdraw_astro {
        if MODE.may_load(deps.storage)?.unwrap_or_default() != StakingMode::Paused {
            return Err(ContractError::WithdrawWhileNotPaused {});
        }
        let recipient = deps.api.addr_validate(&recipient)?;
        let config = CONFIG.load(deps.storage)?;
        let balance = deps
            .querier
            .query_balance(&env.contract.address, &config.astro_denom)?
            .amount;
        if amount.is_zero() {
            return Err(ContractError::WithdrawZero {});
        }
        if amount > balance {
            return Err(ContractError::WithdrawExceedsBalance { amount, balance });
        }
        RETIRED.save(deps.storage, &())?;
        response = response
            .add_message(BankMsg::Send {
                to_address: recipient.to_string(),
                amount: coins(amount.u128(), config.astro_denom),
            })
            .add_attribute("withdraw_astro_recipient", recipient)
            .add_attribute("withdraw_astro_amount", amount);
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(response
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION)
        .add_attribute(
            "mode",
            format!("{:?}", MODE.may_load(deps.storage)?.unwrap_or_default()),
        ))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env};

    use super::*;

    #[test]
    fn migrates_from_deployed_versions() {
        for (version, ok) in [
            ("2.0.0", true),
            ("2.1.0", true),
            ("2.3.0", true),
            ("2.3.1", true),
            ("2.2.0", true),
            ("2.3.2", true),
            ("2.3.3", true),
            ("1.0.0", false),
        ] {
            let mut deps = mock_dependencies();
            set_contract_version(&mut deps.storage, CONTRACT_NAME, version).unwrap();

            let res = migrate(
                deps.as_mut(),
                mock_env(),
                MigrateMsg {
                    mode: Some(StakingMode::Paused),
                    before_send_hook: None,
                    withdraw_astro: None,
                },
            );
            if !ok {
                assert_eq!(res.unwrap_err(), ContractError::MigrationError {});
                continue;
            }
            res.unwrap();
            assert_eq!(MODE.load(&deps.storage).unwrap(), StakingMode::Paused);
            assert_eq!(
                get_contract_version(&deps.storage).unwrap().version,
                CONTRACT_VERSION
            );
        }
    }

    #[test]
    fn rejects_other_contracts() {
        let mut deps = mock_dependencies();
        set_contract_version(&mut deps.storage, "astroport-maker", "2.3.0").unwrap();
        let err = migrate(deps.as_mut(), mock_env(), MigrateMsg::default()).unwrap_err();
        assert_eq!(err, ContractError::MigrationError {});
    }

    #[test]
    fn empty_message_is_accepted() {
        // the message deployed contracts were migrated with so far
        let msg: MigrateMsg = cosmwasm_std::from_json(b"{}").unwrap();
        assert_eq!(msg, MigrateMsg::default());
    }

    #[test]
    fn mode_json() {
        for (json, mode) in [
            (r#"{"mode":"open"}"#, StakingMode::Open),
            (r#"{"mode":"leave_only"}"#, StakingMode::LeaveOnly),
            (r#"{"mode":"paused"}"#, StakingMode::Paused),
        ] {
            let msg: MigrateMsg = cosmwasm_std::from_json(json.as_bytes()).unwrap();
            assert_eq!(msg.mode, Some(mode));
        }

        // a misspelled mode or field is rejected, never read as "keep the current mode"
        for json in [
            r#"{"mode":"pause"}"#,
            r#"{"mode":"Paused"}"#,
            r#"{"mode":"leaveonly"}"#,
            r#"{"paused":true}"#,
        ] {
            assert!(cosmwasm_std::from_json::<MigrateMsg>(json.as_bytes()).is_err());
        }
    }
}
