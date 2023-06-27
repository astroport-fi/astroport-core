use cosmwasm_std::{
    attr, to_binary, Addr, BlockInfo, Coin, Decimal, QueryRequest, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account, Bank};
use classic_test_tube::cosmrs::proto::cosmos::bank::v1beta1::MsgSend;
use classic_test_tube::cosmrs::proto::cosmos::base::v1beta1::Coin as CosmosCoin;

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use astroport::factory::{PairConfig, PairType};

use astroport::oracle::QueryMsg::Consult;
use astroport::oracle::{ExecuteMsg, InstantiateMsg};
use astroport::pair::StablePoolParams;

fn instantiate_contracts(app: &TerraTestApp, owner: &SigningAccount) -> (Addr, Addr, u64) {
    let wasm = Wasm::new(app);
    let astro_token_contract = std::fs::read("../../../../artifacts/astroport_token.wasm").unwrap();
    let astro_token_code_id = wasm.store_code(&astro_token_contract, None, owner).unwrap().data.code_id;

    let msg = TokenInstantiateMsg {
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

    let pair_contract = std::fs::read("../../../../artifacts/astroport_pair.wasm").unwrap();
    let pair_code_id = wasm.store_code(&pair_contract, None, owner).unwrap().data.code_id;

    let pair_stable_contract = std::fs::read("../../../../artifacts/astroport_pair_stable.wasm").unwrap();
    let pair_stable_code_id = wasm.store_code(&pair_stable_contract, None, owner).unwrap().data.code_id;

    let factory_contract = std::fs::read("../../../../artifacts/astroport_factory.wasm").unwrap();
    let factory_code_id = wasm.store_code(&factory_contract, None, owner).unwrap().data.code_id;
    
    let msg = astroport::factory::InstantiateMsg {
        pair_configs: vec![
            PairConfig {
                code_id: pair_code_id,
                pair_type: PairType::Xyk {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: None,
            },
            PairConfig {
                code_id: pair_stable_code_id,
                pair_type: PairType::Stable {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: None,
            },
        ],
        token_code_id: 1u64,
        fee_address: None,
        generator_address: Some(String::from("generator")),
        owner: owner.address(),
        whitelist_code_id: 234u64,
    };

    let factory_instance = wasm.instantiate(
        factory_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some("FACTORY"), 
        &[], 
        owner
    ).unwrap();

    let oracle_contract = std::fs::read("../../../../artifacts/astroport_oracle.wasm").unwrap();
    let oracle_code_id = wasm.store_code(&oracle_contract, None, owner).unwrap().data.code_id;

    (Addr::unchecked(astro_token_instance.data.address), Addr::unchecked(factory_instance.data.address), oracle_code_id)
}

fn instantiate_token(app: &TerraTestApp, owner: &SigningAccount, name: String, symbol: String) -> Addr {
    let wasm = Wasm::new(app);

    let token_contract = std::fs::read("../../../../artifacts/astroport_token.wasm").unwrap();
    let token_code_id = wasm.store_code(&token_contract, None, owner).unwrap().data.code_id;

    let msg = TokenInstantiateMsg {
        name,
        symbol: symbol.clone(),
        decimals: 6,
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.address(),
            cap: None,
        }),
        marketing: None,
    };

    let token_instance = wasm.instantiate(
        token_code_id, 
        &msg, 
        Some(owner.address().as_str()), 
        Some(&symbol), 
        &[], 
        owner)
        .unwrap();

    Addr::unchecked(token_instance.data.address)
}

fn mint_some_token(
    wasm: &Wasm<TerraTestApp>,
    owner: &SigningAccount,
    token_instance: Addr,
    to: Addr,
    amount: Uint128,
) {
    let msg = cw20::Cw20ExecuteMsg::Mint {
        recipient: to.to_string(),
        amount,
    };

    let res = wasm.execute(token_instance.as_str(), &msg, &[], owner).unwrap();
    assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
    assert_eq!(res.events[1].attributes[2], attr("to", to.to_string()));
    assert_eq!(res.events[1].attributes[3], attr("amount", amount));
}

fn allowance_token(
    wasm: &Wasm<TerraTestApp>,
    owner: &SigningAccount,
    spender: Addr,
    token: Addr,
    amount: Uint128,
) {
    let msg = cw20::Cw20ExecuteMsg::IncreaseAllowance {
        spender: spender.to_string(),
        amount,
        expires: None,
    };
    let res = wasm.execute(token.as_str(), &msg, &[], owner).unwrap();

    assert_eq!(
        res.events[1].attributes[1],
        attr("action", "increase_allowance")
    );
    assert_eq!(
        res.events[1].attributes[2],
        attr("owner", owner.address())
    );
    assert_eq!(
        res.events[1].attributes[3],
        attr("spender", spender.to_string())
    );
    assert_eq!(res.events[1].attributes[4], attr("amount", amount));
}

fn check_balance(wasm: &Wasm<TerraTestApp>, user: Addr, token: Addr, expected_amount: Uint128) {
    let msg = Cw20QueryMsg::Balance {
        address: user.to_string(),
    };

    let res: Result<BalanceResponse, _> =
        wasm.query(token.as_str(), &msg);

    let balance = res.unwrap();

    assert_eq!(balance.balance, expected_amount);
}

fn create_pair(
    app: &TerraTestApp,
    owner: &SigningAccount,
    user: &SigningAccount,
    factory_instance: &Addr,
    assets: [Asset; 2],
) -> PairInfo {
    let wasm = Wasm::new(app);

    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                mint_some_token(
                    &wasm,
                    owner,
                    contract_addr.clone(),
                    Addr::unchecked(user.address()),
                    a.amount,
                );
            }

            _ => {}
        }
    }

    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];

    // Create pair in factory
    let res = wasm.execute(
        factory_instance.as_str(), 
        &astroport::factory::ExecuteMsg::CreatePair {
            pair_type: PairType::Xyk {},
            asset_infos: asset_infos.clone(),
            init_params: None,
        }, 
        &[],
        owner
    ).unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr(
            "pair",
            format!(
                "{}-{}",
                asset_infos[0].to_string(),
                asset_infos[1].to_string()
            ),
        )
    );

    // Get pair
    let pair_info: PairInfo = wasm
        .query(factory_instance.as_str(), &astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
        })
        .unwrap();

    let mut funds = vec![];

    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                allowance_token(
                    &wasm,
                    user.clone(),
                    pair_info.contract_addr.clone(),
                    contract_addr.clone(),
                    a.amount.clone(),
                );
            }
            AssetInfo::NativeToken { denom } => {
                funds.push(Coin {
                    denom,
                    amount: a.amount,
                });
            }
        }
    }

    // give money to user
    let minter = app.init_account(&funds).unwrap();
    let bank = Bank::new(app);
    let cosmos_funds = vec![];
    for coin in funds.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: user.address(), amount: cosmos_funds }, owner).unwrap();

    wasm.execute(
        pair_info.contract_addr.as_str(),
        &astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
        }, 
        &funds, 
        user
    ).unwrap();

    pair_info
}

fn create_pair_stable(
    app: &TerraTestApp,
    owner: &SigningAccount,
    user: &SigningAccount,
    factory_instance: &Addr,
    assets: [Asset; 2],
) -> PairInfo {
    let wasm = Wasm::new(app);

    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                mint_some_token(
                    &wasm,
                    owner,
                    contract_addr.clone(),
                    Addr::unchecked(user.address()),
                    a.amount,
                );
            }

            _ => {}
        }
    }

    let asset_infos = [assets[0].info.clone(), assets[1].info.clone()];

    // Create pair in factory
    let res = wasm.execute(
        factory_instance.as_str(),
        &astroport::factory::ExecuteMsg::CreatePair {
            pair_type: PairType::Stable {},
            asset_infos: asset_infos.clone(),
            init_params: Some(to_binary(&StablePoolParams { amp: 100 }).unwrap()),
        },
        &[], 
        owner
    ).unwrap();

    assert_eq!(res.events[1].attributes[1], attr("action", "create_pair"));
    assert_eq!(
        res.events[1].attributes[2],
        attr(
            "pair",
            format!(
                "{}-{}",
                asset_infos[0].to_string(),
                asset_infos[1].to_string()
            ),
        )
    );

    // Get pair
    let pair_info: PairInfo = wasm
        .query(factory_instance.as_str(), &astroport::factory::QueryMsg::Pair {
            asset_infos: asset_infos.clone(),
        })
        .unwrap();

    let mut funds = vec![];

    for a in assets.clone() {
        match a.info {
            AssetInfo::Token { contract_addr } => {
                allowance_token(
                    &wasm,
                    user.clone(),
                    pair_info.contract_addr.clone(),
                    contract_addr.clone(),
                    a.amount.clone(),
                );
            }
            AssetInfo::NativeToken { denom } => {
                funds.push(Coin {
                    denom,
                    amount: a.amount,
                });
            }
        }
    }

    let minter = app.init_account(&funds).unwrap();
    let bank = Bank::new(app);
    let cosmos_funds = vec![];
    for coin in funds.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: user.address(), amount: cosmos_funds }, owner).unwrap();

    wasm.execute(
        pair_info.contract_addr.as_str(),
        &astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
        }, 
        &funds, 
        user
    ).unwrap();

    pair_info
}

fn change_provide_liquidity(
    wasm: &Wasm<TerraTestApp>,
    owner: &SigningAccount,
    user: &SigningAccount,
    pair_contract: Addr,
    token1: Addr,
    token2: Addr,
    amount1: Uint128,
    amount2: Uint128,
) {
    mint_some_token(
        &wasm,
        owner,
        token1.clone(),
        Addr::unchecked(user.address()),
        amount1,
    );
    mint_some_token(
        &wasm,
        owner,
        token2.clone(),
        Addr::unchecked(user.address()),
        amount2,
    );
    check_balance(&wasm, Addr::unchecked(user.address()), token1.clone(), amount1);
    check_balance(&wasm, Addr::unchecked(user.address()), token2.clone(), amount2);
    allowance_token(
        &wasm,
        user,
        pair_contract.clone(),
        token1.clone(),
        amount1,
    );
    allowance_token(
        &wasm,
        user,
        pair_contract.clone(),
        token2.clone(),
        amount2,
    );

    wasm.execute(
        pair_contract.as_str(), 
        &astroport::pair::ExecuteMsg::ProvideLiquidity {
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token1.clone(),
                    },
                    amount: amount1,
                },
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: token2.clone(),
                    },
                    amount: amount2,
                },
            ],
            slippage_tolerance: Some(Decimal::percent(50)),
            auto_stake: None,
            receiver: None,
        },
        &[], 
        user
    ).unwrap();
}

pub fn next_day(app: &TerraTestApp) {
    app.increase_time(86400);
}

#[test]
fn consult() {
    let mut wasm = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut wasm, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut wasm,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_u128),
            },
        ],
    );
    wasm.update_block(next_day);
    let pair_info: PairInfo = wasm
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    change_provide_liquidity(
        &mut wasm,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(50_000_u128),
        Uint128::from(50_000_u128),
    );
    wasm.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = wasm
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.to_string(), "Period not elapsed",);
    wasm.update_block(next_day);

    // Change pair liquidity
    change_provide_liquidity(
        &mut wasm,
        owner.clone(),
        user,
        pair_info.contract_addr,
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(10_000_u128),
        Uint128::from(10_000_u128),
    );
    wasm.update_block(next_day);
    wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount) in [
        (astro_token_instance.clone(), Uint128::from(1000u128)),
        (usdc_token_instance.clone(), Uint128::from(100u128)),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = wasm
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount);
    }
}

#[test]
fn consult_pair_stable() {
    let mut wasm = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut wasm, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair_stable(
        &mut wasm,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_000000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000000u128),
            },
        ],
    );
    wasm.update_block(next_day);
    let pair_info: PairInfo = wasm
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    change_provide_liquidity(
        &mut wasm,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(500_000_000000u128),
        Uint128::from(500_000_000000u128),
    );
    wasm.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = wasm
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.to_string(), "Period not elapsed",);
    wasm.update_block(next_day);

    // Change pair liquidity
    change_provide_liquidity(
        &mut wasm,
        owner.clone(),
        user,
        pair_info.contract_addr,
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(100_000_000000u128),
        Uint128::from(100_000_000000u128),
    );
    wasm.update_block(next_day);
    wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount) in [
        (astro_token_instance.clone(), Uint128::from(1000u128)),
        (usdc_token_instance.clone(), Uint128::from(100u128)),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = wasm
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount);
    }
}

#[test]
fn consult2() {
    let mut wasm = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");
    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut wasm, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut wasm,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(1000_u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(1000_u128),
            },
        ],
    );
    wasm.update_block(next_day);
    let pair_info: PairInfo = wasm
        .wrap()
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: factory_instance.clone().to_string(),
            msg: to_binary(&astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            })
            .unwrap(),
        }))
        .unwrap();

    change_provide_liquidity(
        &mut wasm,
        owner.clone(),
        user.clone(),
        pair_info.contract_addr.clone(),
        astro_token_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::from(1000_u128),
        Uint128::from(1000_u128),
    );
    wasm.update_block(next_day);

    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = wasm
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.to_string(), "Period not elapsed",);
    wasm.update_block(next_day);
    wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    // Change pair liquidity
    for (amount1, amount2) in [
        (Uint128::from(1000_u128), Uint128::from(500_u128)),
        (Uint128::from(1000_u128), Uint128::from(500_u128)),
    ] {
        change_provide_liquidity(
            &mut wasm,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            astro_token_instance.clone(),
            usdc_token_instance.clone(),
            amount1,
            amount2,
        );
        wasm.update_block(next_day);
        wasm
            .execute_contract(
                owner.clone(),
                oracle_instance.clone(),
                &ExecuteMsg::Update {},
                &[],
            )
            .unwrap();
    }
    for (addr, amount, amount_exp) in [
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(750u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1333u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = wasm
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_exp);
    }

    // Change pair liquidity
    for (amount1, amount2) in [
        (Uint128::from(250_u128), Uint128::from(350_u128)),
        (Uint128::from(250_u128), Uint128::from(350_u128)),
    ] {
        change_provide_liquidity(
            &mut wasm,
            owner.clone(),
            user.clone(),
            pair_info.contract_addr.clone(),
            astro_token_instance.clone(),
            usdc_token_instance.clone(),
            amount1,
            amount2,
        );
        wasm.update_block(next_day);
        wasm
            .execute_contract(
                owner.clone(),
                oracle_instance.clone(),
                &ExecuteMsg::Update {},
                &[],
            )
            .unwrap();
    }
    for (addr, amount, amount_exp) in [
        (
            astro_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(822u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(1000u128),
            Uint128::from(1216u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount,
        };
        let res: Uint128 = wasm
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_exp);
    }
}

#[test]
fn consult_zero_price() {
    let mut wasm = mock_app();
    let owner = Addr::unchecked("owner");
    let user = Addr::unchecked("user0000");

    let (astro_token_instance, factory_instance, oracle_code_id) =
        instantiate_contracts(&mut wasm, owner.clone());

    let usdc_token_instance = instantiate_token(
        &mut wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let asset_infos = [
        AssetInfo::Token {
            contract_addr: usdc_token_instance.clone(),
        },
        AssetInfo::Token {
            contract_addr: astro_token_instance.clone(),
        },
    ];
    create_pair(
        &mut wasm,
        owner.clone(),
        user.clone(),
        &factory_instance,
        [
            Asset {
                info: asset_infos[0].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
            Asset {
                info: asset_infos[1].clone(),
                amount: Uint128::from(100_000_000_000u128),
            },
        ],
    );
    wasm.update_block(next_day);
    let msg = InstantiateMsg {
        factory_contract: factory_instance.to_string(),
        asset_infos: asset_infos.clone(),
    };
    let oracle_instance = wasm
        .instantiate_contract(
            oracle_code_id,
            owner.clone(),
            &msg,
            &[],
            String::from("ORACLE"),
            None,
        )
        .unwrap();

    let e = wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap_err();
    assert_eq!(e.to_string(), "Period not elapsed",);
    wasm.update_block(next_day);
    wasm
        .execute_contract(
            owner.clone(),
            oracle_instance.clone(),
            &ExecuteMsg::Update {},
            &[],
        )
        .unwrap();

    for (addr, amount_in, amount_out) in [
        (
            astro_token_instance.clone(),
            Uint128::from(100u128),
            Uint128::from(100u128),
        ),
        (
            usdc_token_instance.clone(),
            Uint128::from(100u128),
            Uint128::from(100u128),
        ),
    ] {
        let msg = Consult {
            token: AssetInfo::Token {
                contract_addr: addr,
            },
            amount: amount_in,
        };
        let res: Uint128 = wasm
            .wrap()
            .query(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: oracle_instance.to_string(),
                msg: to_binary(&msg).unwrap(),
            }))
            .unwrap();
        assert_eq!(res, amount_out);
    }
}
