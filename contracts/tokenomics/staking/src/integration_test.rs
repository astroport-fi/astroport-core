use cosmwasm_std::{
    attr, from_binary,
    testing::{mock_env, MockApi, MockStorage},
    to_binary, Addr, QueryRequest, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_multi_test::{App, ContractWrapper, SimpleBank};
use terraswap::staking::{InstantiateMsg as xInstatiateMsg, QueryMsg};
use terraswap::token::InstantiateMsg;

const ALICE: &str = "Alice";

fn mock_app() -> App {
    let env = mock_env();
    let api = Box::new(MockApi::default());
    let bank = SimpleBank {};

    App::new(api, env.block, bank, || Box::new(MockStorage::new()))
}

fn instantiate_contracts(router: &mut App, owner: Addr) -> (Addr, Addr, Addr) {
    let astro_token_contract = Box::new(ContractWrapper::new(
        terraswap_token::contract::execute,
        terraswap_token::contract::instantiate,
        terraswap_token::contract::query,
    ));

    let astro_token_code_id = router.store_code(astro_token_contract);

    let msg = InstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        init_hook: None,
    };

    let astro_token_instance = router
        .instantiate_contract(
            astro_token_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ASTRO"),
        )
        .unwrap();

    let staking_contract = Box::new(ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    ));
    let staking_code_id = router.store_code(staking_contract);

    let msg = xInstatiateMsg {
        token_code_id: astro_token_code_id,
        deposit_token_addr: astro_token_instance.clone(),
    };
    let staking_instance = router
        .instantiate_contract(staking_code_id, owner, &msg, &[], String::from("xASTRO"))
        .unwrap();

    let msg = QueryMsg::ShareToken {};
    let x_astro_token_instance = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: staking_instance.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
        .unwrap();

    (
        astro_token_instance,
        staking_instance,
        from_binary(&x_astro_token_instance).unwrap(),
    )
}

fn mint_some_astro(router: &mut App, owner: Addr, astro_token_instance: Addr, to: &str) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: String::from(to),
        amount: Uint128::from(100u128),
    };
    let res = router
        .execute_contract(owner.clone(), astro_token_instance.clone(), &msg, &[])
        .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "mint"),
            attr("to", String::from(to)),
            attr("amount", Uint128::from(100u128)),
        ],
    );
}

#[test]
fn should_not_allow_enter_if_not_enough_approve() {
    let mut router = mock_app();

    let owner = Addr::unchecked("owner");

    let (astro_token_instance, staking_instance, x_astro_token_instance) =
        instantiate_contracts(&mut router, owner.clone());

    // mint 100 ASTRO for Alice
    mint_some_astro(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        ALICE,
    );
    let alice_address = Addr::unchecked(ALICE);

    // check Alice's ASTRO balance
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.to_string(),
    };
    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: astro_token_instance.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
        .unwrap();
    assert_eq!(
        from_binary::<BalanceResponse>(&res).unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );

    // try to enter Alice's 100 ASTRO for 100 xASTRO
    let msg = terraswap::staking::ExecuteMsg::Enter {
        amount: Uint128::from(100u128),
    };
    let res = router
        .execute_contract(alice_address.clone(), staking_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res, "No allowance for this account");

    // increase Alice's allowance to 50 ASTRO for staking contract
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: staking_instance.to_string(),
        amount: Uint128::from(50u128),
        expires: None,
    };
    let res = router
        .execute_contract(
            alice_address.clone(),
            astro_token_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "increase_allowance"),
            attr("owner", alice_address.clone()),
            attr("spender", staking_instance.clone()),
            attr("amount", 50),
        ],
    );

    // try to enter Alice's 100 ASTRO for 100 xASTRO
    let msg = terraswap::staking::ExecuteMsg::Enter {
        amount: Uint128::from(100u128),
    };
    let res = router
        .execute_contract(alice_address.clone(), staking_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res, "Overflow: Cannot Sub with 50 and 100");

    // increase Alice's allowance to 100 ASTRO for staking contract
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: staking_instance.to_string(),
        amount: Uint128::from(50u128),
        expires: None,
    };
    let res = router
        .execute_contract(
            alice_address.clone(),
            astro_token_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "increase_allowance"),
            attr("owner", alice_address.clone()),
            attr("spender", staking_instance.clone()),
            attr("amount", 50),
        ],
    );

    // enter Alice's 100 ASTRO for 100 xASTRO
    let msg = terraswap::staking::ExecuteMsg::Enter {
        amount: Uint128::from(100u128),
    };
    router
        .execute_contract(alice_address.clone(), staking_instance.clone(), &msg, &[])
        .unwrap();

    // check Alice's xASTRO balance
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.to_string(),
    };
    let res = router
        .query(QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: x_astro_token_instance.to_string(),
            msg: to_binary(&msg).unwrap(),
        }))
        .unwrap();
    assert_eq!(
        from_binary::<BalanceResponse>(&res).unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );
}

#[test]
fn should_not_allow_withraw_more_than_what_you_have() {
    let mut router = mock_app();

    let owner = Addr::unchecked("owner");

    let (astro_token_instance, staking_instance, x_astro_token_instance) =
        instantiate_contracts(&mut router, owner.clone());

    // mint 100 ASTRO for Alice
    mint_some_astro(
        &mut router,
        owner.clone(),
        astro_token_instance.clone(),
        ALICE,
    );
    let alice_address = Addr::unchecked(ALICE);

    // increase Alice's allowance to 100 ASTRO for staking contract
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: staking_instance.to_string(),
        amount: Uint128::from(100u128),
        expires: None,
    };
    let res = router
        .execute_contract(
            alice_address.clone(),
            astro_token_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "increase_allowance"),
            attr("owner", alice_address.clone()),
            attr("spender", staking_instance.clone()),
            attr("amount", 100),
        ],
    );

    // enter Alice's 100 ASTRO for 100 xASTRO
    let msg = terraswap::staking::ExecuteMsg::Enter {
        amount: Uint128::from(100u128),
    };
    router
        .execute_contract(alice_address.clone(), staking_instance.clone(), &msg, &[])
        .unwrap();

    // increase Alice's allowance to 100 xASTRO for staking contract
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: staking_instance.to_string(),
        amount: Uint128::from(100u128),
        expires: None,
    };
    let res = router
        .execute_contract(
            alice_address.clone(),
            x_astro_token_instance.clone(),
            &msg,
            &[],
        )
        .unwrap();
    assert_eq!(
        res.attributes,
        vec![
            attr("action", "increase_allowance"),
            attr("owner", alice_address.clone()),
            attr("spender", staking_instance.clone()),
            attr("amount", 100),
        ],
    );

    // try to leave Alice's 200 xASTRO
    let msg = terraswap::staking::ExecuteMsg::Leave {
        share: Uint128::from(200u128),
    };
    let res = router
        .execute_contract(alice_address.clone(), staking_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res, "Overflow: Cannot Sub with 100 and 200");
}

/* in development
#[test]
fn should_work_with_more_than_one_participant() {}
*/
