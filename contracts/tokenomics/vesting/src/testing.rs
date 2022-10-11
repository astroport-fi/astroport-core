use crate::contract::{instantiate, query};
use astroport::vesting::{ConfigResponse, InstantiateMsg, QueryMsg};

use astroport::asset::token_asset_info;
use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{from_binary, Addr};

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        vesting_token: token_asset_info(Addr::unchecked("astro_token")),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            vesting_token: token_asset_info(Addr::unchecked("astro_token")),
        }
    );
}
