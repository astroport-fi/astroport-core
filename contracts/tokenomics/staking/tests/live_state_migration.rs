//! Migrates the full storage of the live Neutron staking contract (2.3.0) to this build, the way
//! the DAO will, and checks that the modes apply and the stored data still reads back.

use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{coins, from_json, Storage};

use astroport::staking::{Config, ExecuteMsg, QueryMsg, TrackerData};
use astroport_staking::contract::{execute, query, CONTRACT_NAME, CONTRACT_VERSION};
use astroport_staking::error::ContractError;
use astroport_staking::migrate::{migrate, MigrateMsg};
use astroport_staking::state::StakingMode;

const ASTRO: &str =
    "factory/neutron1ffus553eet978k024lmssw0czsxwr97mggyv85lpcsdkft8v9ufsz3sa07/astro";
const XASTRO: &str =
    "factory/neutron1zlf3hutsa4qnmue53lz2tfxrutp8y2e3rj4nkghg3rupgl4mqy8s5jgxsn/xASTRO";

fn fixture(key: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/neutron_staking_v230_{key}.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn migrate_live_state() {
    let mut deps = mock_dependencies();
    // the live contract stores exactly these three keys
    for key in ["config", "contract_info", "tracker_data"] {
        deps.storage.set(key.as_bytes(), &fixture(key));
    }

    let mode_of = |res: &cosmwasm_std::Response| {
        res.attributes
            .iter()
            .find(|a| a.key == "mode")
            .unwrap()
            .value
            .clone()
    };

    let res = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            mode: Some(StakingMode::Paused),
            before_send_hook: None,
        },
    )
    .unwrap();
    assert_eq!(mode_of(&res), "Paused");
    let version = cw2::get_contract_version(&deps.storage).unwrap();
    assert_eq!(
        (version.contract.as_str(), version.version.as_str()),
        (CONTRACT_NAME, CONTRACT_VERSION)
    );

    // the stored config and tracker data still read back unchanged
    let config: Config =
        from_json(query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(config.astro_denom, ASTRO);
    assert_eq!(config.xastro_denom, XASTRO);
    let tracker: TrackerData =
        from_json(query(deps.as_ref(), mock_env(), QueryMsg::TrackerConfig {}).unwrap()).unwrap();
    assert_eq!(
        tracker.tracker_addr,
        "neutron17q5p2rsz28q55dqud0yzeagw4uhhv9k3cuq4mhmnlqj9kv9zg28srnzrqh"
    );

    // nothing can enter or leave
    let enter = |deps: &mut cosmwasm_std::OwnedDeps<_, _, _>| {
        execute(
            deps.as_mut(),
            mock_env(),
            mock_info("user", &coins(1_000_000, ASTRO)),
            ExecuteMsg::Enter { receiver: None },
        )
        .unwrap_err()
    };
    assert_eq!(enter(&mut deps), ContractError::Paused {});
    let err = execute(
        deps.as_mut(),
        mock_env(),
        mock_info("user", &coins(1_000_000, ASTRO)),
        ExecuteMsg::EnterWithHook {
            contract_address: "hook".to_string(),
            msg: Default::default(),
        },
    )
    .unwrap_err();
    assert_eq!(err, ContractError::Paused {});
    let err = execute(
        deps.as_mut(),
        mock_env(),
        mock_info("user", &coins(1_000_000, XASTRO)),
        ExecuteMsg::Leave { receiver: None },
    )
    .unwrap_err();
    assert_eq!(err, ContractError::Paused {});

    // an empty migration message keeps it paused
    let res = migrate(deps.as_mut(), mock_env(), MigrateMsg::default()).unwrap();
    assert_eq!(mode_of(&res), "Paused");
    assert_eq!(enter(&mut deps), ContractError::Paused {});

    // the later switch to leave-only on the same code: entering stays refused
    let res = migrate(
        deps.as_mut(),
        mock_env(),
        MigrateMsg {
            mode: Some(StakingMode::LeaveOnly),
            before_send_hook: None,
        },
    )
    .unwrap();
    assert_eq!(mode_of(&res), "LeaveOnly");
    assert_eq!(enter(&mut deps), ContractError::LeaveOnly {});
}

#[test]
fn live_state_set_hook() {
    use cosmwasm_std::{Addr, CosmosMsg};

    let mut deps = mock_dependencies();
    for key in ["config", "contract_info", "tracker_data"] {
        deps.storage.set(key.as_bytes(), &fixture(key));
    }
    let mut env = mock_env();
    env.contract.address =
        Addr::unchecked("neutron1zlf3hutsa4qnmue53lz2tfxrutp8y2e3rj4nkghg3rupgl4mqy8s5jgxsn");

    let res = migrate(
        deps.as_mut(),
        env,
        MigrateMsg {
            mode: Some(StakingMode::Paused),
            before_send_hook: Some("freezer".to_string()),
        },
    )
    .unwrap();

    assert_eq!(res.messages.len(), 1);
    let CosmosMsg::Stargate { type_url, value } = &res.messages[0].msg else {
        panic!("expected a stargate message, got {:?}", res.messages[0].msg);
    };
    assert_eq!(
        type_url,
        "/osmosis.tokenfactory.v1beta1.MsgSetBeforeSendHook"
    );
    let msg: osmosis_std::types::osmosis::tokenfactory::v1beta1::MsgSetBeforeSendHook =
        value.clone().try_into().unwrap();
    assert_eq!(
        msg.sender,
        "neutron1zlf3hutsa4qnmue53lz2tfxrutp8y2e3rj4nkghg3rupgl4mqy8s5jgxsn"
    );
    assert_eq!(msg.denom, XASTRO);
    assert_eq!(msg.cosmwasm_address, "freezer");

    // the mode is applied in the same migration
    let err = execute(
        deps.as_mut(),
        mock_env(),
        mock_info("user", &coins(1_000_000, ASTRO)),
        ExecuteMsg::Enter { receiver: None },
    )
    .unwrap_err();
    assert_eq!(err, ContractError::Paused {});
}
