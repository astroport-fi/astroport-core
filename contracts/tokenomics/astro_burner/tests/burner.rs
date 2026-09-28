use cosmwasm_std::{coin, coins, Addr, Empty, Uint128};

use astroport_astro_burner::error::ContractError;
use astroport_astro_burner::msg::{Config, ExecuteMsg, InstantiateMsg, QueryMsg};
use astroport_test::cw_multi_test::{AppBuilder, BankSudo, Contract, ContractWrapper, Executor};
use astroport_test::modules::stargate::{MockStargate, StargateApp};

const ASTRO: &str = "factory/assembly/astro";

fn burner_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_astro_burner::contract::execute,
        astroport_astro_burner::contract::instantiate,
        astroport_astro_burner::contract::query,
    ))
}

fn setup() -> (StargateApp, Addr) {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {});
    let code = app.store_code(burner_contract());
    let burner = app
        .instantiate_contract(
            code,
            Addr::unchecked("owner"),
            &InstantiateMsg {
                denom: ASTRO.to_string(),
            },
            &[],
            "burner",
            Some("assembly".to_string()),
        )
        .unwrap();
    (app, burner)
}

fn mint(app: &mut StargateApp, to: &Addr, amount: u128) {
    app.sudo(
        BankSudo::Mint {
            to_address: to.to_string(),
            amount: coins(amount, ASTRO),
        }
        .into(),
    )
    .unwrap();
}

fn supply(app: &StargateApp) -> u128 {
    app.wrap().query_supply(ASTRO).unwrap().amount.u128()
}

#[test]
fn burns_everything_it_holds_and_what_is_sent() {
    let (mut app, burner) = setup();
    let user = Addr::unchecked("user");

    let config: Config = app
        .wrap()
        .query_wasm_smart(&burner, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.denom, ASTRO);

    // a plain transfer (the Treasury, an IBC transfer) sits there until anyone calls burn
    mint(&mut app, &burner, 1_000);
    mint(&mut app, &user, 500);
    assert_eq!(supply(&app), 1_500);

    // funds sent with the call are burned together with the balance
    app.execute_contract(
        user.clone(),
        burner.clone(),
        &ExecuteMsg::Burn {},
        &coins(500, ASTRO),
    )
    .unwrap();

    assert_eq!(supply(&app), 0);
    assert!(app.wrap().query_all_balances(&burner).unwrap().is_empty());
    let total: Uint128 = app
        .wrap()
        .query_wasm_smart(&burner, &QueryMsg::TotalBurned {})
        .unwrap();
    assert_eq!(total.u128(), 1_500);

    // nothing to burn is a no-op, not an error
    app.execute_contract(user, burner.clone(), &ExecuteMsg::Burn {}, &[])
        .unwrap();
    let total: Uint128 = app
        .wrap()
        .query_wasm_smart(&burner, &QueryMsg::TotalBurned {})
        .unwrap();
    assert_eq!(total.u128(), 1_500);
}

#[test]
fn refuses_other_denoms() {
    let (mut app, burner) = setup();
    let user = Addr::unchecked("user");
    app.sudo(
        BankSudo::Mint {
            to_address: user.to_string(),
            amount: vec![coin(10, "uluna")],
        }
        .into(),
    )
    .unwrap();

    let err = app
        .execute_contract(user, burner, &ExecuteMsg::Burn {}, &coins(10, "uluna"))
        .unwrap_err();
    assert_eq!(
        err.downcast::<ContractError>().unwrap(),
        ContractError::UnexpectedFunds {
            denom: ASTRO.to_string()
        }
    );
}
