use astroport::{
    token::InstantiateMsg as TokenInstantiateMsg,
    vesting::{ExecuteMsg, InstantiateMsg, VestingAccount, VestingSchedule, VestingSchedulePoint},
};
use cosmwasm_std::{testing::{mock_env, MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR}, to_binary, Addr, StdResult, Uint128, Uint64, Timestamp, from_binary, attr, WasmMsg, SubMsg, ReplyOn, BlockInfo};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{next_block, App, BankKeeper, ContractWrapper, Executor, TerraMockQuerier};
use astroport::vesting::{QueryMsg, VestingAccountResponse, VestingInfo};
use astroport_vesting::state::Config;

const OWNER: &str = "Owner";
const USER1: &str = "User1";
const USER2: &str = "User2";

#[test]
fn register_vesting_accounts() {
    let user1 = Addr::unchecked(USER1);
    let user2 = Addr::unchecked(USER2);
    let owner = Addr::unchecked(OWNER);

    let mut app = mock_app();

    let token_code_id = store_token_code(&mut app);

    let astro_token_instance =
        instantiate_token(&mut app, token_code_id, "ASTRO", Some(1_000_000_000_000000));

    let vesting_instance = instantiate_vesting(&mut app, &astro_token_instance);

    let msg = ExecuteMsg::UpdateConfig {
        owner: Some(USER1.to_string()),
    };

    let res = app
        .execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[])
        .unwrap_err();
    assert_eq!(res.to_string(), "Unauthorized");

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100),
                    amount: Uint128::zero(),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(150),
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let res = app.execute_contract(user1.clone(), vesting_instance.clone(), &msg.clone(), &[]).unwrap_err();
    assert_eq!(res.to_string(), "Unauthorized");

    let _res = app.execute_contract(owner.clone(), vesting_instance.clone(), &msg, &[]).unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.clone(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(vesting_res, Uint128::new(100u128));

    let msg = ExecuteMsg::Claim {
        recipient: None,
        amount: None,
    };
    let res = app.execute_contract(user1.clone(), vesting_instance.clone(), &msg, &[]).unwrap();

    let msg = QueryMsg::VestingAccount {
        address: user1.clone(),
    };

    let vesting_res: VestingAccountResponse = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();
    assert_eq!(vesting_res.info.released_amount, Uint128::from(100u128));

    // Timestamp::from_seconds(101);

    app.update_block(|bi| next_block(bi));
    let res = app.execute_contract(owner.clone(), vesting_instance.clone(), &msg, &[]).unwrap();

    let msg = ExecuteMsg::RegisterVestingAccounts {
        vesting_accounts: vec![VestingAccount {
            address: user1.to_string(),
            schedules: vec![VestingSchedule {
                start_point: VestingSchedulePoint {
                    time: Timestamp::from_seconds(100),
                    amount: Uint128::new(10),
                },
                end_point: Some(VestingSchedulePoint {
                    time: Timestamp::from_seconds(200),
                    amount: Uint128::new(100),
                }),
            }],
        }],
    };

    let _res = app.execute_contract(owner.clone(), vesting_instance.clone(), &msg, &[]).unwrap();

    let msg = QueryMsg::AvailableAmount {
        address: user1.clone(),
    };

    let vesting_res: Uint128 = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &msg)
        .unwrap();

    assert_eq!(vesting_res, Uint128::new(200u128));
}

fn mock_app() -> App {
    let api = MockApi::default();
    let env = mock_env();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let terra_mock_querier = TerraMockQuerier::new(MockQuerier::new(&[]));

    App::new(api, env.block, bank, storage, terra_mock_querier)
}

fn store_token_code(app: &mut App) -> u64 {
    let astro_token_contract = Box::new(ContractWrapper::new(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ));

    app.store_code(astro_token_contract)
}

fn instantiate_token(app: &mut App, token_code_id: u64, name: &str, cap: Option<u128>) -> Addr {
    let name = String::from(name);

    let msg = TokenInstantiateMsg {
        name: name.clone(),
        symbol: name.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: String::from(OWNER),
            cap: cap.map(|v| Uint128::from(v)),
        }),
    };

    app.instantiate_contract(token_code_id, Addr::unchecked(OWNER), &msg, &[], name, None)
        .unwrap()
}

fn instantiate_vesting(mut app: &mut App, astro_token_instance: &Addr) -> Addr {
    let vesting_contract = Box::new(ContractWrapper::new(
        astroport_vesting::contract::execute,
        astroport_vesting::contract::instantiate,
        astroport_vesting::contract::query,
    ));
    let owner = Addr::unchecked(OWNER);
    let vesting_code_id = app.store_code(vesting_contract);

    let init_msg = InstantiateMsg {
        owner: owner.to_string(),
        token_addr: astro_token_instance.to_string(),
    };

    let vesting_instance = app
        .instantiate_contract(
            vesting_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "Vesting",
            None,
        )
        .unwrap();

    let res: Config = app
        .wrap()
        .query_wasm_smart(vesting_instance.clone(), &QueryMsg::Config {})
        .unwrap();
    assert_eq!(OWNER.clone(), res.owner);
    assert_eq!(astro_token_instance.to_string(), res.token_addr.to_string());

    mint_tokens(
        &mut app,
        &astro_token_instance,
        &owner,
        1_000_000_000_000000,
    );

    allow_tokens(&mut app, &astro_token_instance, OWNER, &vesting_instance.clone(), 1_000_000_000_000000);

    vesting_instance
}

fn mint_tokens(app: &mut App, token: &Addr, recipient: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::Mint {
        recipient: recipient.to_string(),
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(OWNER), token.to_owned(), &msg, &[])
        .unwrap();
}


fn allow_tokens(app: &mut App, token: &Addr, owner: &str, spender: &Addr, amount: u128) {
    let msg = Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        expires: None,
        amount: Uint128::from(amount),
    };

    app.execute_contract(Addr::unchecked(owner), token.to_owned(), &msg, &[])
        .unwrap();
}

fn check_token_balance(app: &mut App, token: &Addr, address: &Addr, expected: u128) {
    let msg = Cw20QueryMsg::Balance {
        address: address.to_string(),
    };
    let res: StdResult<BalanceResponse> = app.wrap().query_wasm_smart(token, &msg);
    assert_eq!(res.unwrap().balance, Uint128::from(expected));
}