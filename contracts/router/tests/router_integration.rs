mod factory_helper;

use crate::factory_helper::{instantiate_token, mint, FactoryHelper};
use astroport::asset::token_asset_info;
use astroport::factory::PairType;
use astroport::router::{ExecuteMsg, InstantiateMsg, SwapOperation};
use cosmwasm_std::{to_binary, Addr, Empty};
use cw20::Cw20ExecuteMsg;
use cw_multi_test::{App, Contract, ContractWrapper, Executor};

fn router_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_router::contract::execute,
        astroport_router::contract::instantiate,
        astroport_router::contract::query,
    ))
}

#[test]
fn router_does_not_enforce_spread_assertion() {
    let mut app = App::default();

    let owner = Addr::unchecked("owner");
    let mut helper = FactoryHelper::init(&mut app, &owner);

    let token_x = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOX", None);
    let token_y = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOY", None);
    let token_z = instantiate_token(&mut app, helper.cw20_token_code_id, &owner, "TOZ", None);

    for (a, b, typ, liq) in [
        (&token_x, &token_y, PairType::Xyk {}, 100_000_000000),
        (&token_y, &token_z, PairType::Stable {}, 1_000_000_000000),
    ] {
        let pair = helper
            .create_pair_with_addr(&mut app, &owner, typ, [a, b], None)
            .unwrap();
        mint(&mut app, &owner, a, liq, &pair).unwrap();
        mint(&mut app, &owner, b, liq, &pair).unwrap();
    }

    let router_code = app.store_code(router_contract());
    let router = app
        .instantiate_contract(
            router_code,
            owner.clone(),
            &InstantiateMsg {
                astroport_factory: helper.factory.to_string(),
            },
            &[],
            "router",
            None,
        )
        .unwrap();

    // Triggering swap with a huge spread fees
    mint(&mut app, &owner, &token_x, 50_000_000000, &owner).unwrap();
    app.execute_contract(
        owner.clone(),
        token_x.clone(),
        &Cw20ExecuteMsg::Send {
            contract: router.to_string(),
            amount: 50_000_000000u128.into(),
            msg: to_binary(&ExecuteMsg::ExecuteSwapOperations {
                operations: vec![
                    SwapOperation::AstroSwap {
                        offer_asset_info: token_asset_info(token_x.clone()),
                        ask_asset_info: token_asset_info(token_y.clone()),
                    },
                    SwapOperation::AstroSwap {
                        offer_asset_info: token_asset_info(token_y.clone()),
                        ask_asset_info: token_asset_info(token_z.clone()),
                    },
                ],
                minimum_receive: None,
                to: None,
                max_spread: None,
            })
            .unwrap(),
        },
        &[],
    )
    .unwrap();

    // However, single hop will still enforce spread assertion
    mint(&mut app, &owner, &token_x, 50_000_000000, &owner).unwrap();
    let err = app
        .execute_contract(
            owner.clone(),
            token_x.clone(),
            &Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: 50_000_000000u128.into(),
                msg: to_binary(&ExecuteMsg::ExecuteSwapOperations {
                    operations: vec![SwapOperation::AstroSwap {
                        offer_asset_info: token_asset_info(token_x.clone()),
                        ask_asset_info: token_asset_info(token_y.clone()),
                    }],
                    minimum_receive: None,
                    to: None,
                    max_spread: None,
                })
                .unwrap(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        astroport_pair::error::ContractError::MaxSpreadAssertion {},
        err.downcast().unwrap()
    )
}
