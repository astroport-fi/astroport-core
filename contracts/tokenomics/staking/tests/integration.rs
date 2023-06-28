use astroport::staking::{ConfigResponse, Cw20HookMsg, InstantiateMsg as xInstatiateMsg, QueryMsg};
use astroport::token::InstantiateMsg;
use cosmwasm_std::{
    attr,
    to_binary, Addr, Uint128, Coin,
};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account};

fn instantiate_contracts(app: &TerraTestApp, owner: &SigningAccount) -> (Addr, Addr, Addr) {
    let wasm = Wasm::new(app);

    let astro_token_contract = std::fs::read("../../../../artifacts/astroport_token.wasm").unwrap();
    let astro_token_code_id = wasm.store_code(&astro_token_contract, None, owner).unwrap().data.code_id;

    let msg = InstantiateMsg {
        name: String::from("Astro token"),
        symbol: String::from("ASTRO"),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.address(),
            cap: None,
        }),
        marketing: None,
    };

    let astro_token_instance = wasm.instantiate(
        astro_token_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some("ASTRO"), 
        &[], 
        owner
    ).unwrap();

    let staking_contract = std::fs::read("../../../../artifacts/astroport_staking.wasm").unwrap();
    let staking_code_id = wasm.store_code(&staking_contract, None, owner).unwrap().data.code_id;

    let msg = xInstatiateMsg {
        owner: owner.address(),
        token_code_id: astro_token_code_id,
        deposit_token_addr: astro_token_instance.clone().data.address,
        marketing: None,
    };
    let staking_instance = wasm.instantiate(
        staking_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some("xASTRO"), 
        &[], 
        owner
    ).unwrap();

    let msg = QueryMsg::Config {};
    let res : ConfigResponse = wasm.query(&staking_instance.data.address, &msg).unwrap();

    // in multitest, contract names are named in the order in which contracts are created.
    assert_eq!("contract #0", astro_token_instance.clone().data.address);
    assert_eq!("contract #1", staking_instance.data.address);
    assert_eq!("contract #2", res.share_token_addr);

    let x_astro_token_instance = res.share_token_addr;

    (
        Addr::unchecked(astro_token_instance.data.address),
        Addr::unchecked(staking_instance.data.address),
        x_astro_token_instance,
    )
}

fn mint_some_astro(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount, astro_token_instance: Addr, to: &str) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: String::from(to),
        amount: Uint128::from(100u128),
    };
    let res = wasm.execute(astro_token_instance.as_str(), &msg, &[], owner).unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
    assert_eq!(
        res.events[1].attributes[3],
        attr("amount", Uint128::from(100u128))
    );
}

#[test]
fn cw20receive_enter_and_leave() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        2
    ).unwrap();
    let owner = &accs[0];
    let alice_address = &accs[1];

    let (astro_token_instance, staking_instance, x_astro_token_instance) =
        instantiate_contracts(&app, owner.clone());

    // mint 100 ASTRO for Alice
    mint_some_astro(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        &alice_address.address(),
    );

    // check if Alice's ASTRO balance is 100
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );

    // we can leave tokens only from xAstro token.
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Leave {}).unwrap(),
        amount: Uint128::from(10u128),
    };

    let resp = wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap_err();
    assert_eq!(resp.to_string(), "Unauthorized");

    // try to enter Alice's 100 ASTRO for 100 xASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(100u128),
    };

    wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap();
    // check if Alice's xASTRO balance is 100
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );

    // check if Alice's ASTRO balance is 0
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(0u128)
        }
    );

    // check if staking contract's ASTRO balance is 100
    let msg = Cw20QueryMsg::Balance {
        address: staking_instance.to_string(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );

    // we can enter tokens only from Astro token.
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(10u128),
    };

    let resp = wasm.execute(
        x_astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap_err();
    assert_eq!(resp.to_string(), "Unauthorized");

    // try to leave Alice's 10 xASTRO for 10 ASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Leave {}).unwrap(),
        amount: Uint128::from(10u128),
    };

    wasm.execute(
        x_astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap();

    // check if Alice's xASTRO balance is 90
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(90u128)
        }
    );

    // check if Alice's ASTRO balance is 10
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(10u128)
        }
    );

    // check if staking contract's ASTRO balance is 90
    let msg = Cw20QueryMsg::Balance {
        address: staking_instance.to_string(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(90u128)
        }
    );

    // check if staking contract's xASTRO balance is 0
    let msg = Cw20QueryMsg::Balance {
        address: staking_instance.to_string(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(0u128)
        }
    );
}

#[test]
fn should_not_allow_withdraw_more_than_what_you_have() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        2
    ).unwrap();
    let owner = &accs[0];
    let alice_address = &accs[1];

    let (astro_token_instance, staking_instance, x_astro_token_instance) =
        instantiate_contracts(&app, owner.clone());

    // mint 100 ASTRO for Alice
    mint_some_astro(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        &alice_address.address(),
    );

    // enter Alice's 100 ASTRO for 100 xASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(100u128),
    };

    wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap();

    // check if Alice's xASTRO balance is 100
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(100u128)
        }
    );

    // try to leave Alice's 200 xASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Leave {}).unwrap(),
        amount: Uint128::from(200u128),
    };

    let res = wasm.execute(
        x_astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap_err();

    assert_eq!(res.to_string(), "Overflow: Cannot Sub with 100 and 200");
}

#[test]
fn should_work_with_more_than_one_participant() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        4
    ).unwrap();
    let owner = &accs[0];
    let alice_address = &accs[1];
    let bob_address = &accs[2];
    let carol_address = &accs[3];

    let (astro_token_instance, staking_instance, x_astro_token_instance) =
        instantiate_contracts(&app, owner.clone());

    // mint 100 ASTRO for Alice
    mint_some_astro(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        &alice_address.address(),
    );

    // mint 100 ASTRO for Bob
    mint_some_astro(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        &bob_address.address(),
    );

    // mint 100 ASTRO for Carol
    mint_some_astro(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        &carol_address.address(),
    );

    // enter Alice's 20 ASTRO for 20 xASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(20u128),
    };

    wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap();

    // enter Bob's 10 ASTRO for 10 xASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(10u128),
    };

    wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        bob_address
    ).unwrap();

    // check if Alice's xASTRO balance is 20
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(20u128)
        }
    );

    // check if Bob's xASTRO balance is 10
    let msg = Cw20QueryMsg::Balance {
        address: bob_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(10u128)
        }
    );

    // check if staking contract's ASTRO balance is 30
    let msg = Cw20QueryMsg::Balance {
        address: staking_instance.to_string(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(30u128)
        }
    );

    // staking contract gets 20 more ASTRO from external source
    let msg = Cw20ExecuteMsg::Transfer {
        recipient: staking_instance.to_string(),
        amount: Uint128::from(20u128),
    };
    let res = wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        carol_address
    ).unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "transfer"));
    assert_eq!(res.events[1].attributes[2], attr("from", carol_address.address()));
    assert_eq!(
        res.events[1].attributes[3],
        attr("to", staking_instance.clone())
    );
    assert_eq!(
        res.events[1].attributes[4],
        attr("amount", Uint128::from(20u128))
    );

    // enter Alice's 10 ASTRO for 6 xASTRO: 10*30/50 = 6
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Enter {}).unwrap(),
        amount: Uint128::from(10u128),
    };

    wasm.execute(
        astro_token_instance.as_str(), 
        &msg, 
        &[], 
        alice_address
    ).unwrap();

    // check if Alice's xASTRO balance is 26
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(26u128)
        }
    );

    // check if Bob's xASTRO balance is 10
    let msg = Cw20QueryMsg::Balance {
        address: bob_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(10u128)
        }
    );

    // leave Bob's 5 xASTRO: gets 5*60/36 = 8 ASTRO
    let msg = Cw20ExecuteMsg::Send {
        contract: staking_instance.to_string(),
        msg: to_binary(&Cw20HookMsg::Leave {}).unwrap(),
        amount: Uint128::from(5u128),
    };

    wasm.execute(
        x_astro_token_instance.as_str(), 
        &msg, 
        &[], 
        bob_address
    ).unwrap();

    // check if Alice's xASTRO balance is 26
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(26u128)
        }
    );

    // check if Bob's xASTRO balance is 5
    let msg = Cw20QueryMsg::Balance {
        address: bob_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(x_astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(5u128)
        }
    );

    // check if staking contract's ASTRO balance is 52 (60 - 8 (Bob left 5 xASTRO))
    let msg = Cw20QueryMsg::Balance {
        address: staking_instance.to_string(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(52u128)
        }
    );

    // check if Alice's ASTRO balance is 70 (100 minted - 20 entered - 10 entered)
    let msg = Cw20QueryMsg::Balance {
        address: alice_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(70u128)
        }
    );

    // check if Bob's ASTRO balance is 98 (100 minted - 10 entered + 8 by leaving)
    let msg = Cw20QueryMsg::Balance {
        address: bob_address.address(),
    };
    let res: Result<BalanceResponse, _> = wasm.query(astro_token_instance.as_str(), &msg);
    assert_eq!(
        res.unwrap(),
        BalanceResponse {
            balance: Uint128::from(98u128)
        }
    );
}
