use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{Addr, Uint64};

use crate::contract::instantiate;
use crate::state::{Config, CONFIG};
use astroport::maker::InstantiateMsg;

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let env = mock_env();
    let factory = Addr::unchecked("factory");
    let staking = Addr::unchecked("staking");
    let governance_contract = Addr::unchecked("governance");
    let governance_percent = Uint64::new(50);
    let astro_token_contract = Addr::unchecked("astro-token");

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        staking_contract: staking.to_string(),
        governance_contract: Option::from(governance_contract.to_string()),
        governance_percent: Option::from(governance_percent),
        astro_token_contract: astro_token_contract.to_string(),
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let state = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        Config {
            owner: Addr::unchecked("addr0000"),
            factory_contract: Addr::unchecked("factory"),
            staking_contract: Addr::unchecked("staking"),
            governance_contract: Option::from(governance_contract),
            governance_percent,
            astro_token_contract: Addr::unchecked("astro-token"),
        }
    )
}
