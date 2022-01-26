use astroport::xastro_token::{InstantiateMsg, QueryMsg};
use cosmwasm_std::{
    testing::{mock_env, MockApi, MockStorage},
    Addr, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, MinterResponse};
use cw20_base::msg::ExecuteMsg;
use terra_multi_test::{
    next_block, AppBuilder, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

const OWNER: &str = "owner";
const USER1: &str = "user1";
const USER2: &str = "user2";

fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

fn store_token_code(app: &mut TerraApp) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
        astroport_xastro_token::contract::execute,
        astroport_xastro_token::contract::instantiate,
        astroport_xastro_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn instantiate_token(
    app: &mut TerraApp,
    token_code_id: u64,
    initial_balances: Vec<Cw20Coin>,
    cap: Option<u128>,
) -> Addr {
    let name = String::from("xASTRO");

    let msg = InstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances,
        mint: Some(MinterResponse {
            minter: String::from(OWNER),
            cap: cap.map(|v| Uint128::from(v)),
        }),
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

#[test]
fn check_token_total_supply_at_after_mint() {
    let mut router = mock_app();

    let token_contract_code_id = store_token_code(&mut router);

    let token_contract_addr = instantiate_token(&mut router, token_contract_code_id, vec![], None);

    let total_supply: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_345 },
        )
        .unwrap();

    assert_eq!(Uint128::zero(), total_supply);

    router.update_block(next_block);

    let mint_msg = ExecuteMsg::Mint {
        recipient: String::from(USER1),
        amount: Uint128::from(500u32),
    };

    router
        .execute_contract(
            Addr::unchecked(OWNER),
            token_contract_addr.clone(),
            &mint_msg,
            &[],
        )
        .unwrap();

    let total_supply_at_prev_block: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_345 },
        )
        .unwrap();

    let total_supply_at_new_block: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_346 },
        )
        .unwrap();

    assert_eq!(Uint128::zero(), total_supply_at_prev_block);
    assert_eq!(Uint128::from(500u32), total_supply_at_new_block);
}

#[test]
fn check_token_total_supply_at_after_burn() {
    let mut router = mock_app();

    let token_contract_code_id = store_token_code(&mut router);

    let initial_balances = vec![
        Cw20Coin {
            address: String::from(USER1),
            amount: Uint128::from(700u32),
        },
        Cw20Coin {
            address: String::from(USER2),
            amount: Uint128::from(300u32),
        },
    ];

    let token_contract_addr =
        instantiate_token(&mut router, token_contract_code_id, initial_balances, None);

    let total_supply: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_345 },
        )
        .unwrap();

    assert_eq!(Uint128::from(1000u32), total_supply);

    router.update_block(next_block);

    let burn_msg = ExecuteMsg::Burn {
        amount: Uint128::from(200u32),
    };

    router
        .execute_contract(
            Addr::unchecked(USER2),
            token_contract_addr.clone(),
            &burn_msg,
            &[],
        )
        .unwrap();

    let total_supply_at_prev_block: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_345 },
        )
        .unwrap();

    let total_supply_at_new_block: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_346 },
        )
        .unwrap();

    assert_eq!(Uint128::from(1000u32), total_supply_at_prev_block);
    assert_eq!(Uint128::from(800u32), total_supply_at_new_block);
}

#[test]
fn check_token_balance_at_after_transfer() {
    let mut router = mock_app();

    let token_contract_code_id = store_token_code(&mut router);

    let initial_balances = vec![
        Cw20Coin {
            address: String::from(USER1),
            amount: Uint128::from(300u32),
        },
        Cw20Coin {
            address: String::from(USER2),
            amount: Uint128::from(400u32),
        },
    ];

    let token_contract_addr =
        instantiate_token(&mut router, token_contract_code_id, initial_balances, None);

    let total_supply: Uint128 = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::TotalSupplyAt { block: 12_345 },
        )
        .unwrap();

    assert_eq!(Uint128::from(700u32), total_supply);

    router.update_block(next_block);

    let transfer_msg = ExecuteMsg::Transfer {
        recipient: String::from(USER2),
        amount: Uint128::from(150u32),
    };

    router
        .execute_contract(
            Addr::unchecked(USER1),
            token_contract_addr.clone(),
            &transfer_msg,
            &[],
        )
        .unwrap();

    let user1_balance_at_prev_block: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::BalanceAt {
                address: String::from(USER1),
                block: 12_346,
            },
        )
        .unwrap();

    let user2_balance_at_prev_block: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::BalanceAt {
                address: String::from(USER2),
                block: 12_346,
            },
        )
        .unwrap();

    assert_eq!(Uint128::from(300u32), user1_balance_at_prev_block.balance);
    assert_eq!(Uint128::from(400u32), user2_balance_at_prev_block.balance);

    router.update_block(next_block);

    let user1_balance_at_next_block: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::BalanceAt {
                address: String::from(USER1),
                block: 12_347,
            },
        )
        .unwrap();

    let user2_balance_at_next_block: BalanceResponse = router
        .wrap()
        .query_wasm_smart(
            token_contract_addr.clone(),
            &QueryMsg::BalanceAt {
                address: String::from(USER2),
                block: 12_347,
            },
        )
        .unwrap();

    assert_eq!(Uint128::from(150u32), user1_balance_at_next_block.balance);
    assert_eq!(Uint128::from(550u32), user2_balance_at_next_block.balance);
}
