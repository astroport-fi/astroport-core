use astroport::asset::{
    native_asset, native_asset_info, token_asset, token_asset_info, Asset, AssetInfo, PairInfo,
    ULUNA_DENOM, UUSD_DENOM,
};
use astroport::factory::{PairConfig, PairType, UpdateAddr};
use astroport::maker::{
    AssetWithLimit, BalancesResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use cosmwasm_std::{
    attr, Addr, Coin, Decimal, Uint128, Uint64,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};
use std::str::FromStr;
use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account, Bank};
use classic_test_tube::cosmrs::proto::cosmos::base::v1beta1::Coin as CosmosCoin;
use classic_test_tube::cosmrs::proto::cosmos::bank::v1beta1::MsgSend;

fn instantiate_contracts(
    app: &TerraTestApp,
    owner: &SigningAccount,
    staking: Addr,
    governance_percent: Uint64,
    max_spread: Option<Decimal>,
) -> (Addr, Addr, Addr, Addr) {
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

    let factory_contract = std::fs::read("../../../../artifacts/astroport_factory.wasm").unwrap();
    let factory_code_id = wasm.store_code(&factory_contract, None, owner).unwrap().data.code_id;
    let msg = astroport::factory::InstantiateMsg {
        pair_configs: vec![PairConfig {
            code_id: pair_code_id,
            pair_type: PairType::Xyk {},
            total_fee_bps: 0,
            maker_fee_bps: 0,
            is_disabled: Some(false),
        }],
        token_code_id: 1u64,
        fee_address: None,
        owner: owner.address(),
        generator_address: Some(String::from("generator")),
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

    let escrow_fee_distributor_contract = std::fs::read("../../../../artifacts/astroport_escrow_fee_distributor.wasm").unwrap();
    let escrow_fee_distributor_code_id = wasm.store_code(&escrow_fee_distributor_contract, None, owner).unwrap().data.code_id;

    let init_msg = astroport_governance::escrow_fee_distributor::InstantiateMsg {
        owner: owner.address(),
        astro_token: astro_token_instance.clone().data.address,
        voting_escrow_addr: "voting".to_string(),
        claim_many_limit: None,
        is_claim_disabled: None,
    };

    let governance_instance = wasm.instantiate(
        escrow_fee_distributor_code_id, 
        &init_msg, 
        Some(owner.address().as_str()), 
        Some("Astroport escrow fee distributor"), 
        &[], 
        owner
    ).unwrap();

    let maker_contract = std::fs::read("../../../../artifacts/astroport_maker.wasm").unwrap();
    let market_code_id = wasm.store_code(&maker_contract, None, owner).unwrap().data.code_id;

    let msg = InstantiateMsg {
        owner: String::from("owner"),
        factory_contract: factory_instance.clone().data.address,
        staking_contract: staking.to_string(),
        governance_contract: Option::from(governance_instance.clone().data.address),
        governance_percent: Option::from(governance_percent),
        astro_token_contract: astro_token_instance.clone().data.address,
        max_spread,
    };

    let maker_instance = wasm.instantiate(
        market_code_id,
        &msg,
        Some(owner.address().as_str()),
        Some("MAKER"),
        &[],
        owner
    ).unwrap();

    (
        Addr::unchecked(astro_token_instance.data.address),
        Addr::unchecked(factory_instance.data.address),
        Addr::unchecked(maker_instance.data.address),
        Addr::unchecked(governance_instance.data.address),
    )
}

fn instantiate_token(wasm: &Wasm<TerraTestApp>, owner: &SigningAccount, name: String, symbol: String) -> Addr {
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
    owner:&SigningAccount,
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

    let res: Result<BalanceResponse, _> = wasm.query(token.as_str(), &msg);
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

    funds.sort_by(|l, r| l.denom.cmp(&r.denom));

    let user_funds: Vec<Coin> = funds
        .iter()
        .map(|c| Coin {
            denom: c.denom.clone(),
            amount: c.amount * Uint128::new(2),
        })
        .collect();

    // give money to user
    let minter = app.init_account(&user_funds).unwrap();
    let bank = Bank::new(app);
    let mut cosmos_funds = vec![];
    for coin in user_funds.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: user.address(), amount: cosmos_funds }, &minter).unwrap();

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

#[test]
fn update_config() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let unauthorized = &accs[2];
    let governance_percent = Uint64::new(10);

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &app,
            owner.clone(),
            Addr::unchecked(staking.address()),
            governance_percent,
            None,
        );

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(maker_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.owner.to_string(), owner.address());
    assert_eq!(res.astro_token_contract, astro_token_instance);
    assert_eq!(res.factory_contract, factory_instance);
    assert_eq!(res.staking_contract.to_string(), staking.address());
    assert_eq!(res.governance_contract, Some(governance_instance));
    assert_eq!(res.governance_percent, governance_percent);
    assert_eq!(res.max_spread, Decimal::from_str("0.05").unwrap());

    let new_staking = Addr::unchecked("new_staking");
    let new_factory = Addr::unchecked("new_factory");
    let new_governance = Addr::unchecked("new_governance");
    let new_governance_percent = Uint64::new(50);
    let new_max_spread = Decimal::from_str("0.5").unwrap();

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: Some(new_governance_percent),
        governance_contract: Some(UpdateAddr::Set(new_governance.to_string())),
        staking_contract: Some(new_staking.to_string()),
        factory_contract: Some(new_factory.to_string()),
        max_spread: Some(new_max_spread),
    };

    // Assert cannot update with improper owner
    let e = wasm.execute(
        maker_instance.as_str(), 
        &msg, 
        &[], 
        unauthorized
    ).unwrap_err();

    assert_eq!(e.to_string(), "Unauthorized");

    wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(maker_instance.as_str(), &msg)
        .unwrap();

    assert_eq!(res.factory_contract, new_factory);
    assert_eq!(res.staking_contract, new_staking);
    assert_eq!(res.governance_percent, new_governance_percent);
    assert_eq!(res.governance_contract, Some(new_governance.clone()));
    assert_eq!(res.max_spread, new_max_spread);

    let msg = ExecuteMsg::UpdateConfig {
        governance_percent: None,
        governance_contract: Some(UpdateAddr::Remove {}),
        staking_contract: None,
        factory_contract: None,
        max_spread: None,
    };

    wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap();

    let msg = QueryMsg::Config {};
    let res: ConfigResponse = wasm
        .query(maker_instance.as_str(), &msg)
        .unwrap();
    assert_eq!(res.governance_contract, None);
}

fn test_maker_collect(
    app: &TerraTestApp,
    owner:&SigningAccount,
    factory_instance: Addr,
    maker_instance: Addr,
    staking: Addr,
    governance: Addr,
    governance_percent: Uint64,
    pairs: Vec<[Asset; 2]>,
    assets: Vec<AssetWithLimit>,
    bridges: Vec<(AssetInfo, AssetInfo)>,
    mint_balances: Vec<(Addr, u128)>,
    native_balances: Vec<Coin>,
    expected_balances: Vec<Asset>,
    collected_balances: Vec<(Addr, u128)>,
) {
    let wasm = Wasm::new(app);
    let user = app.init_account(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
    ).unwrap();

    // Create pairs
    for t in pairs {
        create_pair(
            &app,
            owner,
            &user,
            &factory_instance,
            t,
        );
    }

    // Setup bridge to withdraw USDC via USDC -> TEST -> UUSD -> ASTRO route
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::UpdateBridges {
            add: Some(bridges),
            remove: None,
        }, 
        &[], 
        owner).unwrap();

    // enable rewards distribution
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 1 }, 
        &[], 
        owner
    ).unwrap();

    // Mint all tokens for maker
    for t in mint_balances {
        let (token, amount) = t;
        mint_some_token(
            &wasm,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &wasm,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    // give money to user
    let minter = app.init_account(&native_balances).unwrap();
    let bank = Bank::new(app);
    let mut cosmos_funds = vec![];
    for coin in native_balances.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: maker_instance.to_string(), amount: cosmos_funds }, &minter).unwrap();

    let balances_resp: BalancesResponse = wasm
        .query(maker_instance.as_str(), &QueryMsg::Balances {
                assets: expected_balances.iter().map(|a| a.info.clone()).collect(),
        })
        .unwrap();

    for b in expected_balances {
        let found = balances_resp
            .balances
            .iter()
            .find(|n| n.info.equal(&b.info))
            .unwrap();

        assert_eq!(found, &b);
    }

    let anyone = app.init_account(&[]).unwrap();
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets }, 
        &[], 
        &anyone
    ).unwrap();

    for t in collected_balances {
        let (token, amount) = t;

        // Check maker balance
        check_balance(
            &wasm,
            maker_instance.clone(),
            token.clone(),
            Uint128::zero(),
        );

        // Check balances
        let amount = Uint128::new(amount);
        let governance_amount =
            amount.multiply_ratio(Uint128::from(governance_percent), Uint128::new(100));
        let staking_amount = amount - governance_amount;

        check_balance(
            &wasm,
            governance.clone(),
            token.clone(),
            governance_amount,
        );

        check_balance(&wasm, staking.clone(), token, staking_amount);
    }
}

#[test]
fn collect_all() {
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
    let staking = &accs[1];

    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &app,
            owner.clone(),
            Addr::unchecked(staking.address()),
            governance_percent,
            Some(max_spread),
        );

    let usdc_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    let uusd_asset = String::from(UUSD_DENOM);
    let uluna_asset = String::from(ULUNA_DENOM);

    // Create pairs
    let pairs = vec![
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ];

    // Specify assets to swap
    let assets = vec![
        AssetWithLimit {
            info: native_asset(uusd_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uluna_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    let bridges = vec![
        (
            token_asset_info(test_token_instance.clone()),
            token_asset_info(bridge2_token_instance.clone()),
        ),
        (
            token_asset_info(usdc_token_instance.clone()),
            token_asset_info(test_token_instance.clone()),
        ),
        (
            native_asset_info(uluna_asset.clone()),
            native_asset_info(uusd_asset.clone()),
        ),
    ];

    let mint_balances = vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token_instance.clone(), 30u128),
    ];

    let native_balances = vec![
        Coin {
            denom: uusd_asset.clone(),
            amount: Uint128::new(100),
        },
        Coin {
            denom: uluna_asset.clone(),
            amount: Uint128::new(110),
        },
    ];

    let expected_balances = vec![
        native_asset(uusd_asset.clone(), Uint128::new(100)),
        native_asset(uluna_asset.clone(), Uint128::new(110)),
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token_instance.clone(), Uint128::new(30)),
    ];

    let collected_balances = vec![
        // 218 ASTRO = 10 ASTRO +
        // 84 ASTRO (100 uusd - 15 tax -> 85 - 1 fee) +
        // 79 ASTRO (110 uluna - 0 tax -> 110 uusd - 1 fee - 16 tax -> 93 - 13 tax - 1 fee) +
        // 17 ASTRO (20 usdc -> 20 test - 1 fee -> 19 bridge - 1 fee -> 18 - 1 fee) +
        // 28 ASTRO (30 test -> 30 bridge - 1 fee -> 29 - 1 fee)
        (astro_token_instance.clone(), 218u128),
        (usdc_token_instance.clone(), 0u128),
        (test_token_instance.clone(), 0u128),
    ];

    test_maker_collect(
        &app,
        owner,
        factory_instance,
        maker_instance,
        Addr::unchecked(staking.address()),
        governance_instance,
        governance_percent,
        pairs,
        assets,
        bridges,
        mint_balances,
        native_balances,
        expected_balances,
        collected_balances,
    );
}

#[test]
fn collect_default_bridges() {
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
    let staking = &accs[1];

    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &app,
            owner.clone(),
            Addr::unchecked(staking.address()),
            governance_percent,
            Some(max_spread),
        );

    let bridge_uusd_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge uusd token".to_string(),
        "BRIDGE-UUSD".to_string(),
    );

    let bridge_uluna_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge uluna token".to_string(),
        "BRIDGE-ULUNA".to_string(),
    );

    let uusd_asset = String::from(UUSD_DENOM);
    let uluna_asset = String::from(ULUNA_DENOM);

    // Create pairs
    let pairs = vec![
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(
                bridge_uusd_token_instance.clone(),
                Uint128::from(100_000_u128),
            ),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(
                bridge_uluna_token_instance.clone(),
                Uint128::from(100_000_u128),
            ),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
    ];

    // Set asset to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(bridge_uusd_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(bridge_uluna_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // No need bridges for this
    let bridges = vec![];

    let mint_balances = vec![
        (bridge_uusd_token_instance.clone(), 100u128),
        (bridge_uluna_token_instance.clone(), 200u128),
    ];

    let native_balances = vec![];

    let expected_balances = vec![
        token_asset(bridge_uusd_token_instance.clone(), Uint128::new(100)),
        token_asset(bridge_uluna_token_instance.clone(), Uint128::new(200)),
    ];

    let collected_balances = vec![
        // 1.
        // 100 uusd-bridge -> 99 uusd (-15 native transfer fee from swap) -> 84 uusd
        // 200 uluna-bridge -1 fee -> 199 uluna

        // 2.
        // 84 uusd (-12 native transfer fee) - 1 fee -> 71 ASTRO
        // 119 uluna -1 fee -> 198 uusd (-28 native transfer fee from swap) -> 170 uusd

        // 3.
        // 170 uusd (-25 native transfer fee) -> 145 uusd -> 144 ASTRO

        // Total: 25
        (astro_token_instance, 215u128),
        // (bridge_uusd_token_instance, 0u128),
        // (bridge_uluna_token_instance, 0u128),
    ];

    test_maker_collect(
        &app,
        owner,
        factory_instance,
        maker_instance,
        Addr::unchecked(staking.address()),
        governance_instance,
        governance_percent,
        pairs,
        assets,
        bridges,
        mint_balances,
        native_balances,
        expected_balances,
        collected_balances,
    );
}

#[test]
fn collect_maxdepth_test() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let user = &accs[2];

    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &app,
        owner.clone(),
        Addr::unchecked(staking.address()),
        governance_percent,
        Some(max_spread),
    );

    let usdc_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    // Create pairs
    let mut pair_addresses = vec![];
    for t in vec![
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        let pair_info = create_pair(
            &app,
            owner,
            user,
            &factory_instance,
            t,
        );

        pair_addresses.push(pair_info.contract_addr);
    }

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    let err = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::UpdateBridges {
            add: Some(vec![
                (
                    token_asset_info(test_token_instance.clone()),
                    token_asset_info(bridge2_token_instance.clone()),
                ),
                (
                    token_asset_info(usdc_token_instance.clone()),
                    token_asset_info(test_token_instance.clone()),
                ),
                (
                    native_asset_info(uluna_asset.clone()),
                    token_asset_info(usdc_token_instance.clone()),
                ),
                (
                    native_asset_info(uusd_asset.clone()),
                    native_asset_info(uluna_asset.clone()),
                ),
            ]),
            remove: None,
        }, 
        &[], 
        owner
    ).unwrap_err();

    assert_eq!(err.to_string(), "Max bridge length of 2 was reached")
}

#[test]
fn collect_err_no_swap_pair() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);
    
    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let user = &accs[2];

    let governance_percent = Uint64::new(50);

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &app,
        owner.clone(),
        Addr::unchecked(staking.address()),
        governance_percent,
        None,
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");
    let ukrt_asset = String::from("ukrt");
    let uabc_asset = String::from("uabc");

    // Mint all tokens for Maker
    for t in vec![
        [
            native_asset(ukrt_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(ukrt_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uabc_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &app,
            owner,
            user,
            &factory_instance,
            t,
        );
    }

    // Set the assets to swap
    let assets = vec![
        AssetWithLimit {
            info: native_asset(ukrt_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uabc_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Mint all tokens for the Maker
    for t in vec![(astro_token_instance.clone(), 10u128)] {
        let (token, amount) = t;
        mint_some_token(
            &wasm,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &wasm,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    let maker_funds = vec![
        Coin::new(20, ukrt_asset),
        Coin::new(30, uabc_asset),
    ];
    // give money to user
    let minter = app.init_account(&maker_funds).unwrap();
    let bank = Bank::new(&app);
    let mut cosmos_funds = vec![];
    for coin in maker_funds.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: maker_instance.to_string(), amount: cosmos_funds }, &minter).unwrap();

    let msg = ExecuteMsg::Collect { assets };

    let e = wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap_err();

    assert_eq!(e.to_string(), "Cannot swap uabc. No swap destinations",);
}

#[test]
fn update_bridges() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let user = &accs[2];

    let governance_percent = Uint64::new(10);
    let uusd_asset = String::from("uusd");

    let (astro_token_instance, factory_instance, maker_instance, _) = instantiate_contracts(
        &app,
        owner.clone(),
        Addr::unchecked(staking.address()),
        governance_percent,
        None,
    );

    let msg = ExecuteMsg::UpdateBridges {
        add: Some(vec![
            (
                native_asset_info(String::from("uluna")),
                native_asset_info(String::from("uusd")),
            ),
            (
                native_asset_info(String::from("ukrt")),
                native_asset_info(String::from("uusd")),
            ),
        ]),
        remove: None,
    };

    // Unauthorized check
    let e = wasm.execute(maker_instance.as_str(), &msg, &[], user).unwrap_err();
    assert_eq!(e.to_string(), "Unauthorized");

    // Add bridges
    let err = wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid bridge. Pool uluna to uusd not found"
    );

    // Create pair so that add bridge check does not fail
    for pair in vec![
        [
            native_asset(String::from("uluna"), Uint128::from(100_000_u128)),
            native_asset(String::from("uusd"), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(String::from("ukrt"), Uint128::from(100_000_u128)),
            native_asset(String::from("uusd"), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &app,
            owner,
            user,
            &factory_instance,
            pair,
        );
    }

    // Add bridges
    let err = wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Invalid bridge destination. uluna cannot be swapped to ASTRO"
    );

    // Create pair so that add bridge check does not fail
    create_pair(
        &app,
        owner,
        user,
        &factory_instance,
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    );

    // Add bridges
    wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap();

    let resp: Vec<(String, String)> = wasm.query(maker_instance.as_str(), &QueryMsg::Bridges {}).unwrap();

    assert_eq!(
        resp,
        vec![
            (String::from("ukrt"), String::from("uusd")),
            (String::from("uluna"), String::from("uusd")),
        ]
    );

    let msg = ExecuteMsg::UpdateBridges {
        remove: Some(vec![native_asset_info(String::from("UKRT"))]),
        add: None,
    };

    // Try to remove bridges
    let err = wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Address UKRT should be lowercase"
    );

    let msg = ExecuteMsg::UpdateBridges {
        remove: Some(vec![native_asset_info(String::from("ukrt"))]),
        add: None,
    };

    // Remove bridges
    wasm.execute(maker_instance.as_str(), &msg, &[], owner).unwrap();

    let resp: Vec<(String, String)> = wasm.query(maker_instance.as_str(), &QueryMsg::Bridges {}).unwrap();

    assert_eq!(resp, vec![(String::from("uluna"), String::from("uusd")),]);
}

#[test]
fn collect_with_asset_limit() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);
    
    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let user = &accs[2];

    let governance_percent = Uint64::new(10);
    let max_spread = Decimal::from_str("0.5").unwrap();

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &app,
            owner.clone(),
            Addr::unchecked(staking.address()),
            governance_percent,
            Some(max_spread),
        );

    let usdc_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    // Create pairs
    for t in vec![
        [
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset, Uint128::from(100_000_u128)),
            native_asset(uusd_asset, Uint128::from(100_000_u128)),
        ],
        [
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &app,
            owner,
            user,
            &factory_instance,
            t,
        );
    }

    // Make a list with duplicate assets
    let assets_with_duplicate = vec![
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: Option::from(Uint128::new(5)),
        },
    ];

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::UpdateBridges {
            add: Some(vec![
                (
                    token_asset_info(test_token_instance.clone()),
                    token_asset_info(bridge2_token_instance.clone()),
                ),
                (
                    token_asset_info(usdc_token_instance.clone()),
                    token_asset_info(test_token_instance.clone()),
                ),
            ]),
            remove: None,
        }, 
        &[], 
        owner
    ).unwrap();

    // Enable rewards distribution
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 1 }, 
        &[], 
        owner
    ).unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance.clone(), 20u128),
        (test_token_instance.clone(), 30u128),
    ] {
        let (token, amount) = t;
        mint_some_token(
            &wasm,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &wasm,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    let expected_balances = vec![
        token_asset(astro_token_instance.clone(), Uint128::new(10)),
        token_asset(usdc_token_instance.clone(), Uint128::new(20)),
        token_asset(test_token_instance.clone(), Uint128::new(30)),
    ];

    let balances_resp: BalancesResponse = wasm
        .query(maker_instance.as_str(), &QueryMsg::Balances {
                assets: expected_balances.iter().map(|a| a.info.clone()).collect(),
        })
        .unwrap();

    for b in expected_balances {
        let found = balances_resp
            .balances
            .iter()
            .find(|n| n.info.equal(&b.info))
            .unwrap();

        assert_eq!(found, &b);
    }

    let anyone = app.init_account(&[]).unwrap();
    let resp = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets_with_duplicate.clone() }, 
        &[], 
        &anyone
    ).unwrap_err();
    assert_eq!(resp.to_string(), "Cannot collect. Remove duplicate asset",);

    let anyone = app.init_account(&[]).unwrap();
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    // Check Maker's balance of ASTRO tokens
    check_balance(
        &wasm,
        maker_instance.clone(),
        astro_token_instance.clone(),
        Uint128::zero(),
    );

    // Check Maker's balance of USDC tokens
    check_balance(
        &wasm,
        maker_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::new(15u128),
    );

    // Check Maker's balance of test tokens
    check_balance(
        &wasm,
        maker_instance.clone(),
        test_token_instance.clone(),
        Uint128::new(0u128),
    );

    // Check balances
    // We are losing 1 ASTRO in fees per swap
    // 40 ASTRO = 10 astro +
    // 2 usdc (5 - fee for 3 swaps)
    // 28 test (30 - fee for 2 swaps)
    let amount = Uint128::new(40u128);
    let governance_amount =
        amount.multiply_ratio(Uint128::from(governance_percent), Uint128::new(100));
    let staking_amount = amount - governance_amount;

    // Check the governance contract's balance for the ASTRO token
    check_balance(
        &wasm,
        governance_instance.clone(),
        astro_token_instance.clone(),
        governance_amount,
    );

    // Check the governance contract's balance for the USDC token
    check_balance(
        &wasm,
        governance_instance.clone(),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the governance contract's balance for the test token
    check_balance(
        &wasm,
        governance_instance.clone(),
        test_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the ASTRO token
    check_balance(
        &wasm,
        Addr::unchecked(staking.address()),
        astro_token_instance.clone(),
        staking_amount,
    );

    // Check the staking contract's balance for the USDC token
    check_balance(
        &wasm,
        Addr::unchecked(staking.address()),
        usdc_token_instance.clone(),
        Uint128::zero(),
    );

    // Check the staking contract's balance for the test token
    check_balance(
        &wasm,
        Addr::unchecked(staking.address()),
        test_token_instance.clone(),
        Uint128::zero(),
    );
}

struct CheckDistributedAstro {
    maker_amount: Uint128,
    governance_amount: Uint128,
    staking_amount: Uint128,
    governance_percent: Uint64,
    maker: Addr,
    astro_token: Addr,
    governance: Addr,
    staking: Addr,
}

impl CheckDistributedAstro {
    fn check(&mut self, wasm: &Wasm<TerraTestApp>, distributed_amount: u32) {
        let distributed_amount = Uint128::from(distributed_amount as u128);
        let cur_governance_amount = distributed_amount
            .multiply_ratio(Uint128::from(self.governance_percent), Uint128::new(100));
        self.governance_amount += cur_governance_amount;
        self.staking_amount += distributed_amount - cur_governance_amount;
        self.maker_amount -= distributed_amount;

        check_balance(
            &wasm,
            self.maker.clone(),
            self.astro_token.clone(),
            self.maker_amount,
        );

        check_balance(
            &wasm,
            self.governance.clone(),
            self.astro_token.clone(),
            self.governance_amount,
        );

        check_balance(
            &wasm,
            self.staking.clone(),
            self.astro_token.clone(),
            self.staking_amount,
        );
    }
}

#[test]
fn distribute_initially_accrued_fees() {
    let app = TerraTestApp::new();
    let wasm = Wasm::new(&app);

    // Set balances
    let accs = app.init_accounts(
        &[
            Coin::new(200u128, "uusd"),
            Coin::new(200u128, "uluna"),
        ],
        3
    ).unwrap();
    let owner = &accs[0];
    let staking = &accs[1];
    let user = &accs[2];

    let governance_percent = Uint64::new(10);

    let (astro_token_instance, factory_instance, maker_instance, governance_instance) =
        instantiate_contracts(
            &app,
            owner.clone(),
            Addr::unchecked(staking.address()),
            governance_percent,
            None,
        );

    let usdc_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Usdc token".to_string(),
        "USDC".to_string(),
    );

    let test_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Test token".to_string(),
        "TEST".to_string(),
    );

    let bridge2_token_instance = instantiate_token(
        &wasm,
        owner.clone(),
        "Bridge 2 depth token".to_string(),
        "BRIDGE".to_string(),
    );

    let uusd_asset = String::from("uusd");
    let uluna_asset = String::from("uluna");

    // Create pairs
    for t in vec![
        [
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            native_asset(uluna_asset.clone(), Uint128::from(100_000_u128)),
            native_asset(uusd_asset.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(usdc_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(test_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
        [
            token_asset(bridge2_token_instance.clone(), Uint128::from(100_000_u128)),
            token_asset(astro_token_instance.clone(), Uint128::from(100_000_u128)),
        ],
    ] {
        create_pair(
            &app,
            owner,
            user,
            &factory_instance,
            t,
        );
    }

    // Set assets to swap
    let assets = vec![
        AssetWithLimit {
            info: native_asset(uusd_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(astro_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: native_asset(uluna_asset.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(usdc_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(test_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
        AssetWithLimit {
            info: token_asset(bridge2_token_instance.clone(), Uint128::zero()).info,
            limit: None,
        },
    ];

    // Setup bridge to withdraw USDC via the USDC -> TEST -> UUSD -> ASTRO route
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::UpdateBridges {
            add: Some(vec![
                (
                    token_asset_info(test_token_instance.clone()),
                    token_asset_info(bridge2_token_instance.clone()),
                ),
                (
                    token_asset_info(usdc_token_instance.clone()),
                    token_asset_info(test_token_instance.clone()),
                ),
                (
                    native_asset_info(uluna_asset.clone()),
                    native_asset_info(uusd_asset.clone()),
                ),
            ]),
            remove: None,
        }, 
        &[], 
        owner
    ).unwrap();

    // Mint all tokens for Maker
    for t in vec![
        (astro_token_instance.clone(), 10u128),
        (usdc_token_instance, 20u128),
        (test_token_instance, 30u128),
    ] {
        let (token, amount) = t;
        mint_some_token(
            &wasm,
            owner.clone(),
            token.clone(),
            maker_instance.clone(),
            Uint128::from(amount),
        );

        // Check initial balance
        check_balance(
            &wasm,
            maker_instance.clone(),
            token,
            Uint128::from(amount),
        );
    }

    // fund accounts
    let maker_funds = vec![
        Coin::new(100, uusd_asset),
        Coin::new(110, uluna_asset)
    ];
    let minter = app.init_account(&maker_funds).unwrap();
    let bank = Bank::new(&app);
    let mut cosmos_funds = vec![];
    for coin in maker_funds.clone() {
        cosmos_funds.push(CosmosCoin {
            denom: coin.denom,
            amount: coin.amount.u128().to_string(),
        });
    }

    bank.send(MsgSend { from_address: minter.address(), to_address: maker_instance.to_string(), amount: cosmos_funds }, &minter).unwrap();

    // Unauthorized check
    let err = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 1 }, 
        &[], 
        user
    ).unwrap_err();
    assert_eq!(err.to_string(), "Unauthorized");

    // Check pre_update_blocks = 0
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 0 }, 
        &[], 
        owner
    ).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Generic error: Number of blocks should be > 0"
    );

    // Check that collect does not distribute ASTRO until rewards are enabled
    let anyone = app.init_account(&[]).unwrap();
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets }, 
        &[], 
        &anyone
    ).unwrap();

    // Balances checker
    let mut checker = CheckDistributedAstro {
        maker_amount: Uint128::new(218_u128),
        governance_amount: Uint128::zero(),
        staking_amount: Uint128::zero(),
        maker: maker_instance.clone(),
        astro_token: astro_token_instance.clone(),
        governance_percent,
        governance: governance_instance,
        staking: Addr::unchecked(staking.address()),
    };
    checker.check(&wasm, 0);

    // Enable rewards distribution
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 10 }, 
        &[], 
        owner
    ).unwrap();

    // Try to enable again
    let err = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::EnableRewards { blocks: 1 }, 
        &[], 
        owner
    ).unwrap_err();
    assert_eq!(err.to_string(), "Rewards collecting is already enabled");

    let astro_asset = AssetWithLimit {
        info: token_asset_info(astro_token_instance.clone()),
        limit: None,
    };
    let assets = vec![astro_asset];

    let anyone = app.init_account(&[]).unwrap();
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    // Since the block number is the same, nothing happened
    checker.check(&wasm, 0);

    app.increase_time(10);

    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    checker.check(&wasm, 21);

    // Let's try to collect again within the same block
    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    // But no ASTRO were distributed
    checker.check(&wasm, 0);

    app.increase_time(10);

    // Imagine that we received new fees the while pre-ugrade ASTRO is being distributed
    mint_some_token(
        &wasm,
        owner.clone(),
        astro_token_instance.clone(),
        maker_instance.clone(),
        Uint128::from(30_u128),
    );

    let resp = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    checker.maker_amount += Uint128::from(30_u128);
    // 51 = 30 minted astro + 21 distributed astro
    checker.check(&wasm, 51);

    // Checking that attributes are set properly
    for (attr, value) in [
        ("astro_distribution", 30_u128),
        ("preupgrade_astro_distribution", 21_u128),
    ] {
        let a = resp.events[1]
            .attributes
            .iter()
            .find(|a| a.key == attr)
            .unwrap();
        assert_eq!(a.value, value.to_string());
    }

    // Increment 8 blocks
    for _ in 0..8 {
        app.increase_time(10);
    }

    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    // 168 = 21 * 8
    checker.check(&wasm, 168);

    // Check remainder reward
    let res: ConfigResponse = wasm
        .query(maker_instance.as_str(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.remainder_reward.u128(), 8_u128);

    // Check remainder reward distribution
    app.increase_time(10);

    wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets: assets.clone() }, 
        &[], 
        &anyone
    ).unwrap();

    checker.check(&wasm, 8);

    // Check that the pre-upgrade ASTRO was fully distributed
    let res: ConfigResponse = wasm
        .query(maker_instance.as_str(), &QueryMsg::Config {})
        .unwrap();

    assert_eq!(res.remainder_reward.u128(), 0_u128);
    assert_eq!(res.pre_upgrade_astro_amount.u128(), 218_u128);

    // Check usual collecting works
    mint_some_token(
        &wasm,
        owner,
        astro_token_instance,
        maker_instance.clone(),
        Uint128::from(115_u128),
    );

    let resp = wasm.execute(
        maker_instance.as_str(), 
        &ExecuteMsg::Collect { assets }, 
        &[], 
        &anyone
    ).unwrap();

    checker.maker_amount += Uint128::from(115_u128);
    checker.check(&wasm, 115);

    // Check that attributes are set properly
    let a = resp.events[1]
        .attributes
        .iter()
        .find(|a| a.key == "astro_distribution")
        .unwrap();
    assert_eq!(a.value, 115_u128.to_string());
    assert!(!resp.events[1]
        .attributes
        .iter()
        .any(|a| a.key == "preupgrade_astro_distribution"));
}
