use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr, Decimal, Uint128, Uint64};

use crate::contract::{execute, instantiate, query};
use crate::state::{Config, CONFIG};
use astroport::asset::{native_asset_info, token_asset_info};
use astroport::maker::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use std::str::FromStr;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();
    let info = mock_info("addr0000", &[]);

    let env = mock_env();
    let owner = Addr::unchecked("owner");
    let factory = Addr::unchecked("factory");
    let staking = Addr::unchecked("staking");
    let governance_contract = Addr::unchecked("governance");
    let governance_percent = Uint64::new(50);
    let astro_token_contract = Addr::unchecked("astro-token");

    let instantiate_msg = InstantiateMsg {
        owner: owner.to_string(),
        factory_contract: factory.to_string(),
        staking_contract: Some(staking.to_string()),
        governance_contract: Option::from(governance_contract.to_string()),
        governance_percent: Option::from(governance_percent),
        astro_token: token_asset_info(astro_token_contract.clone()),
        default_bridge: Some(native_asset_info("uluna".to_string())),
        max_spread: None,
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let state = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        Config {
            owner: Addr::unchecked("owner"),
            factory_contract: Addr::unchecked("factory"),
            staking_contract: Some(Addr::unchecked("staking")),
            default_bridge: Some(native_asset_info("uluna".to_string())),
            governance_contract: Option::from(governance_contract),
            governance_percent,
            astro_token: token_asset_info(astro_token_contract),
            max_spread: Decimal::from_str("0.05").unwrap(),
            rewards_enabled: false,
            pre_upgrade_blocks: 0,
            last_distribution_block: 0,
            remainder_reward: Uint128::zero(),
            pre_upgrade_astro_amount: Uint128::zero(),
        }
    )
}

#[test]
fn update_owner() {
    let mut deps = mock_dependencies();
    let info = mock_info("addr0000", &[]);

    let owner = Addr::unchecked("owner");
    let factory = Addr::unchecked("factory");
    let staking = Addr::unchecked("staking");
    let governance_contract = Addr::unchecked("governance");
    let governance_percent = Uint64::new(50);
    let astro_token_contract = Addr::unchecked("astro-token");

    let msg = InstantiateMsg {
        owner: owner.to_string(),
        factory_contract: factory.to_string(),
        staking_contract: Some(staking.to_string()),
        governance_contract: Option::from(governance_contract.to_string()),
        governance_percent: Option::from(governance_percent),
        astro_token: token_asset_info(astro_token_contract),
        default_bridge: Some(native_asset_info("uluna".to_string())),
        max_spread: None,
    };

    let env = mock_env();

    // We can just call .unwrap() to assert this was a success
    instantiate(deps.as_mut(), env, info, msg).unwrap();

    let new_owner = String::from("new_owner");

    // BNew owner
    let env = mock_env();
    let msg = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.clone(),
        expires_in: 100, // seconds
    };

    let info = mock_info(new_owner.as_str(), &[]);

    // Unauthorized check
    let err = execute(deps.as_mut(), env.clone(), info, msg.clone()).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim before a proposal
    let info = mock_info(new_owner.as_str(), &[]);
    execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();

    // Propose new owner
    let info = mock_info(owner.as_str(), &[]);
    let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
    assert_eq!(0, res.messages.len());

    // Unauthorized ownership claim
    let info = mock_info("invalid_addr", &[]);
    let err = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unauthorized");

    // Claim ownership
    let info = mock_info(new_owner.as_str(), &[]);
    let res = execute(
        deps.as_mut(),
        env.clone(),
        info,
        ExecuteMsg::ClaimOwnership {},
    )
    .unwrap();
    assert_eq!(0, res.messages.len());

    // Let's query the state
    let config: ConfigResponse =
        from_binary(&query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap()).unwrap();
    assert_eq!(new_owner, config.owner);
}
