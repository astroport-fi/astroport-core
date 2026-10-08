use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, Response};
use cw2::{get_contract_version, set_contract_version};

use crate::contract::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::error::ContractError;
use crate::state::PAUSED;

#[cw_serde]
#[derive(Default)]
pub struct MigrateMsg {
    /// Pauses (`true`) or resumes (`false`) enter and leave. Leaves it as it is if not set.
    #[serde(default)]
    pub paused: Option<bool>,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-staking" => match contract_version.version.as_ref() {
            "2.0.0" | "2.1.0" | "2.3.0" | "2.3.1" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    if let Some(paused) = msg.paused {
        PAUSED.save(deps.storage, &paused)?;
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION)
        .add_attribute(
            "paused",
            PAUSED
                .may_load(deps.storage)?
                .unwrap_or_default()
                .to_string(),
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
            ("2.2.0", false),
            ("1.0.0", false),
        ] {
            let mut deps = mock_dependencies();
            set_contract_version(&mut deps.storage, CONTRACT_NAME, version).unwrap();

            let res = migrate(deps.as_mut(), mock_env(), MigrateMsg { paused: Some(true) });
            if !ok {
                assert_eq!(res.unwrap_err(), ContractError::MigrationError {});
                continue;
            }
            res.unwrap();
            assert!(PAUSED.load(&deps.storage).unwrap());
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
        assert_eq!(msg, MigrateMsg { paused: None });
    }
}
