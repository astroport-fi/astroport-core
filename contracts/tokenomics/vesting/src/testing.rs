use std::marker::PhantomData;

use crate::contract::{instantiate, query};
use astroport::vesting::{ConfigResponse, InstantiateMsg, QueryMsg};

use classic_bindings::TerraQuery;
use cosmwasm_std::testing::{mock_env, mock_info, MockQuerier, MockApi, MockStorage};
use cosmwasm_std::{from_binary, Addr, OwnedDeps};

pub fn mock_dependencies() -> OwnedDeps<MockStorage, MockApi, MockQuerier, TerraQuery> {
    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: MockQuerier::default(),
        custom_query_type: PhantomData,
    }
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies();

    let msg = InstantiateMsg {
        owner: "owner".to_string(),
        token_addr: "astro_token".to_string(),
    };

    let env = mock_env();
    let info = mock_info("addr0000", &vec![]);
    let _res = instantiate(deps.as_mut(), env.clone(), info, msg).unwrap();

    assert_eq!(
        from_binary::<ConfigResponse>(&query(deps.as_ref(), env, QueryMsg::Config {}).unwrap())
            .unwrap(),
        ConfigResponse {
            owner: Addr::unchecked("owner"),
            token_addr: Addr::unchecked("astro_token"),
        }
    );
}
