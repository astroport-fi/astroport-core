use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Empty, Env,
    MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};

use astroport::asset::{native_asset_info, Asset, AssetInfoExt, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::pair::{ExecuteMsg, PoolResponse, QueryMsg};
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport_pair_exit_only::contract::MigrateMsg;
use astroport_pair_exit_only::error::ContractError;
use astroport_test::cw_multi_test::{
    AppBuilder, AppResponse, Contract, ContractWrapper, Executor, TOKEN_FACTORY_MODULE,
};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};

const OWNER: &str = "owner";

fn exit_only_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_exit_only::contract::execute,
            astroport_pair_exit_only::contract::instantiate,
            astroport_pair_exit_only::contract::query,
        )
        .with_migrate_empty(astroport_pair_exit_only::contract::migrate),
    )
}

fn pcl_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_concentrated::contract::execute,
            astroport_pair_concentrated::contract::instantiate,
            astroport_pair_concentrated::queries::query,
        )
        .with_reply_empty(astroport_pair_concentrated::contract::reply)
        .with_migrate_empty(astroport_pair_concentrated::contract::migrate),
    )
}

fn factory_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    )
}

fn coin_registry_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ))
}

fn tracker_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
                unimplemented!()
            },
            astroport_tokenfactory_tracker::contract::instantiate,
            astroport_tokenfactory_tracker::query::query,
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    )
}

fn cw20_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

/// A pair that stores a given config (with `{self}` replaced by its address) and cw2 version,
/// standing in for the CW20-LP pairs
/// (pair_concentrated 1.2.x, transmuter 1.0.0) whose old code can't run here
#[cw_serde]
struct MockPairInit {
    config: String,
    contract: String,
    version: String,
    #[serde(default)]
    orderbook_enabled: Option<bool>,
}

fn mock_pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
            Err(StdError::generic_err("old pair"))
        },
        |deps: DepsMut, env: Env, _: MessageInfo, msg: MockPairInit| -> StdResult<Response> {
            let config = msg.config.replace("{self}", env.contract.address.as_str());
            deps.storage.set(b"config", config.as_bytes());
            if let Some(enabled) = msg.orderbook_enabled {
                // The duality pair's orderbook state, abridged
                let ob = format!(
                    r#"{{"enabled":{enabled},"orders_number":1,"liquidity_percent":"0.1"}}"#
                );
                deps.storage.set(b"orderbook_config", ob.as_bytes());
            }
            cw2::set_contract_version(deps.storage, msg.contract, msg.version)?;
            Ok(Response::new())
        },
        |_: Deps, _: Env, _: Empty| -> StdResult<Binary> { Err(StdError::generic_err("old pair")) },
    ))
}

fn a(name: &str) -> Addr {
    Addr::unchecked(name)
}

fn err(e: anyhow::Error) -> ContractError {
    e.downcast().unwrap()
}

fn bank(app: &TestApp, who: &Addr, denom: &str) -> u128 {
    app.wrap().query_balance(who, denom).unwrap().amount.u128()
}

fn mint(app: &mut TestApp, to: &Addr, funds: &[Coin]) {
    app.send_tokens(a(OWNER), to.clone(), funds).unwrap();
}

struct Pcl {
    app: TestApp,
    pair: Addr,
    lp: String,
    exit_only_code: u64,
}

const X: &str = "uatom";
const Y: &str = "udatom";

impl Pcl {
    fn new() -> Self {
        let mut app = AppBuilder::new_custom()
            .with_stargate(MockStargate::default())
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(
                        storage,
                        &a(OWNER),
                        vec![coin(u128::MAX / 4, X), coin(u128::MAX / 4, Y)],
                    )
                    .unwrap()
            });
        let owner = a(OWNER);

        let token_code_id = app.store_code(cw20_contract());
        let tracker_code_id = app.store_code(tracker_contract());
        let pair_code_id = app.store_code(pcl_contract());
        let factory_code_id = app.store_code(factory_contract());
        let registry_code_id = app.store_code(coin_registry_contract());
        let exit_only_code = app.store_code(exit_only_contract());

        let registry = app
            .instantiate_contract(
                registry_code_id,
                owner.clone(),
                &astroport::native_coin_registry::InstantiateMsg {
                    owner: owner.to_string(),
                },
                &[],
                "registry",
                None,
            )
            .unwrap();
        app.execute_contract(
            owner.clone(),
            registry.clone(),
            &astroport::native_coin_registry::ExecuteMsg::Add {
                native_coins: vec![(X.to_string(), 6), (Y.to_string(), 6)],
            },
            &[],
        )
        .unwrap();

        let pair_type = PairType::Custom("concentrated".to_string());
        let factory = app
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &astroport::factory::InstantiateMsg {
                    fee_address: None,
                    pair_configs: vec![PairConfig {
                        code_id: pair_code_id,
                        maker_fee_bps: 5000,
                        total_fee_bps: 0,
                        pair_type: pair_type.clone(),
                        is_disabled: false,
                        is_generator_disabled: false,
                        permissioned: false,
                        whitelist: None,
                    }],
                    token_code_id,
                    generator_address: None,
                    owner: owner.to_string(),
                    whitelist_code_id: 0,
                    coin_registry_address: registry.to_string(),
                    tracker_config: Some(astroport::factory::TrackerConfig {
                        code_id: tracker_code_id,
                        token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
                    }),
                },
                &[],
                "factory",
                None,
            )
            .unwrap();

        let params = ConcentratedPoolParams {
            amp: Decimal::from_ratio(500u32, 1u32),
            gamma: Decimal::from_ratio(1u32, 100u32),
            mid_fee: Decimal::from_ratio(3u32, 10000u32),
            out_fee: Decimal::from_ratio(45u32, 10000u32),
            fee_gamma: Decimal::from_ratio(3u32, 10u32),
            repeg_profit_threshold: Decimal::from_ratio(1u32, 100_000_000u32),
            min_price_scale_delta: Decimal::from_ratio(55u32, 10_000_000u32),
            price_scale: Decimal::one(),
            ma_half_time: 600,
            track_asset_balances: Some(true),
            fee_share: None,
            allowed_xcp_profit_drop: None,
            xcp_profit_losses_threshold: None,
        };
        let asset_infos = vec![native_asset_info(X.into()), native_asset_info(Y.into())];
        app.execute_contract(
            owner.clone(),
            factory.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type,
                asset_infos: asset_infos.clone(),
                init_params: Some(to_json_binary(&params).unwrap()),
            },
            &[],
        )
        .unwrap();
        let pair_info: PairInfo = app
            .wrap()
            .query_wasm_smart(
                &factory,
                &astroport::factory::QueryMsg::Pair { asset_infos },
            )
            .unwrap();

        Pcl {
            app,
            pair: pair_info.contract_addr,
            lp: pair_info.liquidity_token,
            exit_only_code,
        }
    }

    fn provide(&mut self, who: &Addr, x: u128, y: u128) -> anyhow::Result<AppResponse> {
        mint(&mut self.app, who, &[coin(x, X), coin(y, Y)]);
        self.app.execute_contract(
            who.clone(),
            self.pair.clone(),
            &ExecuteMsg::ProvideLiquidity {
                assets: vec![
                    native_asset_info(X.into()).with_balance(x),
                    native_asset_info(Y.into()).with_balance(y),
                ],
                slippage_tolerance: Some(Decimal::percent(50)),
                auto_stake: None,
                receiver: None,
                min_lp_to_receive: None,
            },
            &[coin(x, X), coin(y, Y)],
        )
    }

    fn withdraw(
        &mut self,
        who: &Addr,
        amount: u128,
        assets: Vec<Asset>,
        min: Option<Vec<Asset>>,
    ) -> anyhow::Result<AppResponse> {
        self.app.execute_contract(
            who.clone(),
            self.pair.clone(),
            &ExecuteMsg::WithdrawLiquidity {
                assets,
                min_assets_to_receive: min,
            },
            &coins(amount, &self.lp),
        )
    }

    fn lp_of(&self, who: &Addr) -> u128 {
        bank(&self.app, who, &self.lp)
    }

    fn pool(&self) -> PoolResponse {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair, &QueryMsg::Pool {})
            .unwrap()
    }

    fn migrate_to_exit_only(&mut self) -> anyhow::Result<AppResponse> {
        self.app.migrate_contract(
            a(OWNER),
            self.pair.clone(),
            &MigrateMsg {},
            self.exit_only_code,
        )
    }

    /// What the 2026-09-22 drainer did: every pool balance out, LP supply untouched
    fn drain(&mut self, to: &Addr) -> (u128, u128) {
        let (x, y) = (
            bank(&self.app, &self.pair, X),
            bank(&self.app, &self.pair, Y),
        );
        self.app
            .send_tokens(self.pair.clone(), to.clone(), &[coin(x, X), coin(y, Y)])
            .unwrap();
        (x, y)
    }
}

#[test]
fn drained_pool_is_exit_only_then_refilled_pro_rata() {
    let mut s = Pcl::new();
    let (alice, bob, attacker, recovery) = (a("alice"), a("bob"), a("attacker"), a("recovery"));
    s.provide(&alice, 3_000_000_000_000, 3_000_000_000_000)
        .unwrap();
    s.provide(&bob, 1_000_000_000_000, 1_000_000_000_000)
        .unwrap();
    let (alice_lp, bob_lp) = (s.lp_of(&alice), s.lp_of(&bob));
    let supply = s.pool().total_share.u128();
    let (x, y) = s.drain(&recovery);

    s.migrate_to_exit_only().unwrap();
    // The pair's version is left as it was, so it can migrate back to its own contract later
    let version = cw2::query_contract_info(&s.app.wrap(), &s.pair).unwrap();
    assert_eq!(version.contract, "astroport-pair-concentrated");

    // Deposits, swaps and one-sided withdrawals are refused: the dust deposit that would mint
    // nearly all LP tokens before the refill can't be made
    mint(&mut s.app, &attacker, &[coin(10, X), coin(10, Y)]);
    s.app
        .send_tokens(
            attacker.clone(),
            s.pair.clone(),
            &[coin(10, X), coin(10, Y)],
        )
        .unwrap();
    let e = s.provide(&attacker, 1_000_000, 1_000_000).unwrap_err();
    assert_eq!(err(e), ContractError::ExitOnly {});
    let e = s
        .app
        .execute_contract(
            attacker.clone(),
            s.pair.clone(),
            &ExecuteMsg::Swap {
                offer_asset: native_asset_info(X.into()).with_balance(1_000_000u128),
                ask_asset_info: None,
                belief_price: None,
                max_spread: None,
                to: None,
            },
            &coins(1_000_000, X),
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::ExitOnly {});
    let e = s
        .app
        .execute_contract(
            a(OWNER),
            s.pair.clone(),
            &ExecuteMsg::UpdateConfig {
                params: Binary::default(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::ExitOnly {});
    let e = s
        .app
        .wrap()
        .query_wasm_smart::<Binary>(
            &s.pair,
            &QueryMsg::Simulation {
                offer_asset: native_asset_info(X.into()).with_balance(1u128),
                ask_asset_info: None,
            },
        )
        .unwrap_err();
    assert!(e.to_string().contains("exit-only"), "{e}");

    // Before the refill the pool holds only the attacker's dust: a withdrawal that would burn LP
    // for nothing is refused
    let e = s.withdraw(&bob, 1, vec![], None).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToWithdraw {});

    // Refill at different recovery rates per asset
    let (rx, ry) = (x * 7121 / 10000, y);
    s.app
        .send_tokens(
            recovery.clone(),
            s.pair.clone(),
            &[coin(rx, X), coin(ry, Y)],
        )
        .unwrap();
    let pool = s.pool();
    assert_eq!(pool.total_share.u128(), supply);
    assert_eq!(pool.assets[0].amount.u128(), rx + 10);

    // One-sided withdrawals stay refused
    let e = s
        .withdraw(
            &alice,
            alice_lp,
            vec![native_asset_info(Y.into()).with_balance(1u128)],
            None,
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::ImbalancedWithdraw {});

    // Minimums are enforced
    let share: Vec<Asset> = s
        .app
        .wrap()
        .query_wasm_smart(
            &s.pair,
            &QueryMsg::Share {
                amount: Uint128::new(alice_lp),
            },
        )
        .unwrap();
    let e = s
        .withdraw(
            &alice,
            alice_lp,
            vec![],
            Some(vec![
                native_asset_info(X.into()).with_balance(share[0].amount + Uint128::one())
            ]),
        )
        .unwrap_err();
    assert!(matches!(
        err(e),
        ContractError::WithdrawSlippageViolation { .. }
    ));

    // Balanced withdrawals pay each LP its share of what came back, and burn the LP tokens
    s.withdraw(&alice, alice_lp, vec![], Some(share.clone()))
        .unwrap();
    assert_eq!(bank(&s.app, &alice, X), share[0].amount.u128());
    assert_eq!(bank(&s.app, &alice, Y), share[1].amount.u128());
    let expected_x = (rx + 10) * alice_lp / supply;
    assert_eq!(share[0].amount.u128(), expected_x);
    s.withdraw(&bob, bob_lp, vec![], None).unwrap();
    assert_eq!(s.lp_of(&alice), 0);
    assert_eq!(s.lp_of(&bob), 0);
    // Alice holds three times Bob's share
    let (ax, bx) = (bank(&s.app, &alice, X), bank(&s.app, &bob, X));
    assert!(ax.abs_diff(3 * bx) <= 10, "{ax} {bx}");
    // Only the pool's own minimum liquidity is left
    let pool = s.pool();
    assert_eq!(pool.total_share.u128(), supply - alice_lp - bob_lp);
    assert!(pool.assets[0].amount.u128() < 10_000, "{pool:?}");

    // Migrating again is harmless
    s.migrate_to_exit_only().unwrap();
}

#[test]
fn back_to_its_own_contract_reopens_the_pool() {
    let mut s = Pcl::new();
    let alice = a("alice");
    s.provide(&alice, 1_000_000_000_000, 1_000_000_000_000)
        .unwrap();
    s.migrate_to_exit_only().unwrap();
    assert_eq!(
        err(s.provide(&alice, 1_000_000, 1_000_000).unwrap_err()),
        ContractError::ExitOnly {}
    );

    // The pair's state was never touched: a newer build of its own contract takes it back
    let pcl_code = s.app.store_code(pcl_contract());
    s.app
        .migrate_contract(
            a(OWNER),
            s.pair.clone(),
            &astroport_pair_concentrated::contract::MigrateMsg::default(),
            pcl_code,
        )
        .unwrap();
    s.provide(&alice, 1_000_000, 1_000_000).unwrap();
}

#[test]
fn refuses_pairs_it_cant_take_over() {
    let mut s = Pcl::new();
    let owner = a(OWNER);
    let mock = s.app.store_code(mock_pair_contract());
    let config = |addr: &str| {
        format!(
            r#"{{"pair_info":{{"asset_infos":[{{"native_token":{{"denom":"{X}"}}}},{{"native_token":{{"denom":"{Y}"}}}}],"contract_addr":"{addr}","liquidity_token":"factory/{addr}/astroport/share","pair_type":{{"xyk":{{}}}}}},"factory_addr":"factory"}}"#
        )
    };
    let instantiate =
        |s: &mut Pcl, contract: &str, cfg: String, orderbook_enabled: Option<bool>| {
            s.app
                .instantiate_contract(
                    mock,
                    owner.clone(),
                    &MockPairInit {
                        config: cfg,
                        contract: contract.to_string(),
                        version: "1.0.0".to_string(),
                        orderbook_enabled,
                    },
                    &[],
                    "mock",
                    Some(owner.to_string()),
                )
                .unwrap()
        };

    // An XYK pair isn't supported
    let pair = instantiate(&mut s, "astroport-pair", config("{self}"), None);
    let e = s
        .app
        .migrate_contract(owner.clone(), pair, &MigrateMsg {}, s.exit_only_code)
        .unwrap_err();
    assert!(matches!(err(e), ContractError::MigrationError { .. }));

    // A config for another address
    let pair = instantiate(
        &mut s,
        "astroport-pair-transmuter",
        config("someone_else"),
        None,
    );
    let e = s
        .app
        .migrate_contract(owner.clone(), pair, &MigrateMsg {}, s.exit_only_code)
        .unwrap_err();
    assert!(err(e).to_string().contains("another address"));

    // An orderbook pool with its Duality orderbook enabled: part of the liquidity isn't in its
    // balance. Disabled, it migrates.
    let pair = instantiate(
        &mut s,
        "astroport-pair-concentrated-duality",
        config("{self}"),
        Some(true),
    );
    let e = s
        .app
        .migrate_contract(owner.clone(), pair, &MigrateMsg {}, s.exit_only_code)
        .unwrap_err();
    assert!(err(e)
        .to_string()
        .contains("orderbook integration is enabled"));
    let pair = instantiate(
        &mut s,
        "astroport-pair-concentrated-duality",
        config("{self}"),
        Some(false),
    );
    s.app
        .migrate_contract(owner.clone(), pair, &MigrateMsg {}, s.exit_only_code)
        .unwrap();

    // A pool already exit-only with a withheld asset (pair_concentrated 4.3.0)
    let e = {
        let alice = a("alice");
        s.provide(&alice, 1_000_000_000, 1_000_000_000).unwrap();
        let pcl_code = s.app.store_code(pcl_contract());
        s.app
            .migrate_contract(
                owner.clone(),
                s.pair.clone(),
                &astroport_pair_concentrated::contract::MigrateMsg {
                    exit_only: Some(astroport_pair_concentrated::contract::ExitOnly {
                        withheld: native_asset_info(Y.into()),
                    }),
                },
                pcl_code,
            )
            .unwrap();
        s.migrate_to_exit_only().unwrap_err()
    };
    assert!(err(e).to_string().contains("withheld"));
}

/// The CW20-LP pairs: pair_concentrated 1.2.13 and 1.2.15 and transmuter 1.0.0. Their code
/// can't run here, so a stand-in holds the same config and version.
#[test]
fn cw20_lp_pools_withdraw_through_send() {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|router, _, storage| {
            router
                .bank
                .init_balance(
                    storage,
                    &a(OWNER),
                    vec![coin(u128::MAX / 4, X), coin(u128::MAX / 4, Y)],
                )
                .unwrap()
        });
    let owner = a(OWNER);
    let (alice, bob) = (a("alice"), a("bob"));
    let mock = app.store_code(mock_pair_contract());
    let cw20 = app.store_code(cw20_contract());
    let exit_only = app.store_code(exit_only_contract());

    let lp = app
        .instantiate_contract(
            cw20,
            owner.clone(),
            &cw20_base::msg::InstantiateMsg {
                name: "Pool LP".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![
                    Cw20Coin {
                        address: alice.to_string(),
                        amount: Uint128::new(750),
                    },
                    Cw20Coin {
                        address: bob.to_string(),
                        amount: Uint128::new(250),
                    },
                ],
                mint: Some(MinterResponse {
                    minter: owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "lp",
            None,
        )
        .unwrap();
    // A fake LP token with the same holders
    let fake_lp = app
        .instantiate_contract(
            cw20,
            owner.clone(),
            &cw20_base::msg::InstantiateMsg {
                name: "Pool LP".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![Cw20Coin {
                    address: alice.to_string(),
                    amount: Uint128::new(1_000_000),
                }],
                mint: None,
                marketing: None,
            },
            &[],
            "fake",
            None,
        )
        .unwrap();

    let config = format!(
        r#"{{"pair_info":{{"asset_infos":[{{"native_token":{{"denom":"{X}"}}}},{{"native_token":{{"denom":"{Y}"}}}}],"contract_addr":"{{self}}","liquidity_token":"{lp}","pair_type":{{"custom":"concentrated"}}}},"factory_addr":"factory","block_time_last":0,"track_asset_balances":false}}"#
    );
    let pair = app
        .instantiate_contract(
            mock,
            owner.clone(),
            &MockPairInit {
                config,
                contract: "astroport-pair-concentrated".to_string(),
                version: "1.2.13".to_string(),
                orderbook_enabled: None,
            },
            &[],
            "pair",
            Some(owner.to_string()),
        )
        .unwrap();

    app.migrate_contract(owner.clone(), pair.clone(), &MigrateMsg {}, exit_only)
        .unwrap();

    let send = |app: &mut TestApp, token: &Addr, who: &Addr, amount: u128, msg: Binary| {
        app.execute_contract(
            who.clone(),
            token.clone(),
            &Cw20ExecuteMsg::Send {
                contract: pair.to_string(),
                amount: Uint128::new(amount),
                msg,
            },
            &[],
        )
    };
    let withdraw = to_json_binary(&WithdrawHook::WithdrawLiquidity {}).unwrap();

    // Before the refill: nothing to withdraw
    let e = send(&mut app, &lp, &bob, 250, withdraw.clone()).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToWithdraw {});

    app.send_tokens(
        owner.clone(),
        pair.clone(),
        &[coin(586_300, X), coin(1_000_000, Y)],
    )
    .unwrap();

    // Only the pool's own LP token, and only withdrawals
    let e = send(&mut app, &fake_lp, &alice, 1_000_000, withdraw.clone()).unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});
    let swap = to_json_binary(&astroport::pair::Cw20HookMsg::Swap {
        ask_asset_info: None,
        belief_price: None,
        max_spread: None,
        to: None,
    })
    .unwrap();
    let e = send(&mut app, &lp, &alice, 750, swap).unwrap_err();
    assert_eq!(err(e), ContractError::ExitOnly {});
    // Native-LP withdrawals don't apply
    let e = app
        .execute_contract(
            alice.clone(),
            pair.clone(),
            &ExecuteMsg::WithdrawLiquidity {
                assets: vec![],
                min_assets_to_receive: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::ExitOnly {});

    send(&mut app, &lp, &alice, 750, withdraw.clone()).unwrap();
    assert_eq!(bank(&app, &alice, X), 586_300 * 750 / 1000);
    assert_eq!(bank(&app, &alice, Y), 750_000);
    let supply: cw20::TokenInfoResponse = app
        .wrap()
        .query_wasm_smart(&lp, &Cw20QueryMsg::TokenInfo {})
        .unwrap();
    assert_eq!(supply.total_supply.u128(), 250);
    let pool: PoolResponse = app
        .wrap()
        .query_wasm_smart(&pair, &QueryMsg::Pool {})
        .unwrap();
    assert_eq!(pool.total_share.u128(), 250);

    send(&mut app, &lp, &bob, 250, withdraw).unwrap();
    assert_eq!(bank(&app, &bob, Y), 250_000);
    let left: BalanceResponse = app
        .wrap()
        .query_wasm_smart(
            &lp,
            &Cw20QueryMsg::Balance {
                address: pair.to_string(),
            },
        )
        .unwrap();
    assert!(left.balance.is_zero());
}

/// The hook the CW20-LP pairs' frontends send: `{"withdraw_liquidity":{}}`
#[cw_serde]
enum WithdrawHook {
    WithdrawLiquidity {},
}

/// Every pool that will migrate, by its live config (read from Neutron on 2026-10-13)
#[test]
fn reads_the_live_pool_configs() {
    let pools = [
        (
            "datom_usdc",
            "neutron1nfns3ck2ykrs0fknckrzd9728cyf77devuzernhwcwrdxw7ssk2s3tjf8r",
            true,
        ),
        (
            "ntrn_usdc",
            "neutron18c8qejysp4hgcfuxdpj4wf29mevzwllz5yh8uayjxamwtrs0n9fshq9vtv",
            true,
        ),
        (
            "datom_atom",
            "neutron1yem82r0wf837lfkwvcu2zxlyds5qrzwkz8alvmg0apyrjthk64gqeq2e98",
            true,
        ),
        (
            "usdc_dydx",
            "neutron1j4xpv03fw664mvntlhqnzp5hjqk2nfw00vrgx9qlq97rxc9fu3lqvmszl2",
            false,
        ),
        (
            "wsteth_weth",
            "neutron1yw0a7nxa8jdgzmdsme4gwxhj76n44z305qgwrzvlfavgna9epcys3k9m2f",
            false,
        ),
        (
            "transmuter",
            "neutron1ns2tcunrlrk5yk62fpl74ycazanceyfmmq7dlj6sq8n0rnkuvk7szstkyx",
            false,
        ),
    ];
    for (name, addr, native_lp) in pools {
        let raw = std::fs::read(format!(
            "{}/tests/fixtures/{name}.config.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        let mut storage = cosmwasm_std::testing::MockStorage::new();
        cosmwasm_std::Storage::set(&mut storage, b"config", &raw);
        let pair_info = astroport_pair_exit_only::state::load_pair_info(&storage)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(pair_info.contract_addr.as_str(), addr, "{name}");
        assert_eq!(pair_info.asset_infos.len(), 2, "{name}");
        assert_eq!(
            astroport_pair_exit_only::state::is_native_lp(&pair_info),
            native_lp,
            "{name}"
        );
    }
}
