#![cfg(not(tarpaulin_include))]

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coins, to_json_binary, Addr, Binary, Decimal, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20QueryMsg, MinterResponse};

use astroport::asset::{native_asset_info, token_asset_info, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport_maker::error::ContractError;
use astroport_maker::msg::{
    AssetWithLimit, CollectAsset, Config, ExecuteMsg, InstantiateMsg, Leg, LegAction, MigrateMsg,
    QueryMsg, RouteResponse, SeizeConfig, SwapOperation,
};
use astroport_test::cw_multi_test::{
    AppBuilder, AppResponse, BankSudo, Contract, ContractWrapper, Executor,
};
use astroport_test::modules::stargate::{MockStargate, StargateApp as App};

const OWNER: &str = "owner";
const LUNA: &str = "uluna";
const USDC: &str = "usdc_inj";
const ATOM: &str = "uatom";
const ASTRO: &str = "factory/assembly/astro";
/// Deep enough that a collect of a few units moves the price well under 1%
const POOL_DEPTH: u128 = 1_000_000_000_000;

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

fn pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    )
}

fn registry_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ))
}

fn token_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

fn router_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_router::contract::execute,
            astroport_router::contract::instantiate,
            astroport_router::contract::query,
        )
        .with_reply_empty(astroport_router::contract::reply),
    )
}

fn maker_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_maker::contract::execute,
            astroport_maker::contract::instantiate,
            astroport_maker::contract::query,
        )
        .with_migrate_empty(astroport_maker::contract::migrate),
    )
}

fn burner_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_astro_burner::contract::execute,
        astroport_astro_burner::contract::instantiate,
        astroport_astro_burner::contract::query,
    ))
}

/// Stands in for the Terra satellite: accepts `transfer_astro` with ASTRO attached.
mod satellite_mock {
    use super::*;

    #[cw_serde]
    pub enum SatelliteMsg {
        TransferAstro {},
    }

    fn instantiate(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn execute(_: DepsMut, _: Env, _: MessageInfo, _: SatelliteMsg) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn query(_: Deps, _: Env, _: Empty) -> StdResult<Binary> {
        to_json_binary(&Empty {})
    }

    pub fn contract() -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }
}

/// A placeholder that only exists to hold a Maker 1.7.0's storage before migrating it.
mod placeholder {
    use super::*;

    fn instantiate(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn execute(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn query(_: Deps, _: Env, _: Empty) -> StdResult<Binary> {
        to_json_binary(&Empty {})
    }

    pub fn contract() -> Box<dyn Contract<Empty>> {
        Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
    }
}

struct Suite {
    app: App,
    owner: Addr,
    factory: Addr,
    registry: Addr,
    router: Addr,
    token_code: u64,
    maker_code: u64,
}

impl Suite {
    fn new() -> Self {
        let owner = Addr::unchecked(OWNER);
        let mut app = AppBuilder::new_custom()
            .with_stargate(MockStargate::default())
            .build(|_, _, _| {});

        let token_code = app.store_code(token_contract());
        let pair_code = app.store_code(pair_contract());
        let registry_code = app.store_code(registry_contract());
        let factory_code = app.store_code(factory_contract());
        let router_code = app.store_code(router_contract());
        let maker_code = app.store_code(maker_contract());

        let registry = app
            .instantiate_contract(
                registry_code,
                owner.clone(),
                &astroport::native_coin_registry::InstantiateMsg {
                    owner: OWNER.to_string(),
                },
                &[],
                "registry",
                None,
            )
            .unwrap();

        let factory = app
            .instantiate_contract(
                factory_code,
                owner.clone(),
                &astroport::factory::InstantiateMsg {
                    pair_configs: vec![PairConfig {
                        code_id: pair_code,
                        pair_type: PairType::Xyk {},
                        total_fee_bps: 30,
                        maker_fee_bps: 0,
                        is_disabled: false,
                        is_generator_disabled: false,
                        permissioned: false,
                        whitelist: None,
                    }],
                    token_code_id: token_code,
                    fee_address: None,
                    generator_address: None,
                    owner: OWNER.to_string(),
                    whitelist_code_id: 0,
                    coin_registry_address: registry.to_string(),
                    tracker_config: None,
                },
                &[],
                "factory",
                None,
            )
            .unwrap();

        let router = app
            .instantiate_contract(
                router_code,
                owner.clone(),
                &astroport_router::msg::InstantiateMsg {
                    astroport_factory: factory.to_string(),
                },
                &[],
                "router",
                None,
            )
            .unwrap();

        Self {
            app,
            owner,
            factory,
            registry,
            router,
            token_code,
            maker_code,
        }
    }

    fn mint_native(&mut self, to: &Addr, amount: u128, denom: &str) {
        self.app
            .sudo(
                BankSudo::Mint {
                    to_address: to.to_string(),
                    amount: coins(amount, denom),
                }
                .into(),
            )
            .unwrap();
    }

    fn balance(&self, who: &Addr, asset: &AssetInfo) -> u128 {
        match asset {
            AssetInfo::NativeToken { denom } => self
                .app
                .wrap()
                .query_balance(who, denom)
                .unwrap()
                .amount
                .u128(),
            AssetInfo::Token { contract_addr } => {
                let res: BalanceResponse = self
                    .app
                    .wrap()
                    .query_wasm_smart(
                        contract_addr,
                        &Cw20QueryMsg::Balance {
                            address: who.to_string(),
                        },
                    )
                    .unwrap();
                res.balance.u128()
            }
        }
    }

    fn fund(&mut self, to: &Addr, asset: &AssetInfo, amount: u128) {
        match asset {
            AssetInfo::NativeToken { denom } => {
                let denom = denom.clone();
                self.mint_native(to, amount, &denom)
            }
            AssetInfo::Token { contract_addr } => {
                self.app
                    .execute_contract(
                        self.owner.clone(),
                        contract_addr.clone(),
                        &cw20::Cw20ExecuteMsg::Mint {
                            recipient: to.to_string(),
                            amount: amount.into(),
                        },
                        &[],
                    )
                    .unwrap();
            }
        }
    }

    fn token(&mut self, name: &str) -> AssetInfo {
        let addr = self
            .app
            .instantiate_contract(
                self.token_code,
                self.owner.clone(),
                &astroport::token::InstantiateMsg {
                    name: name.to_string(),
                    symbol: name.to_string(),
                    decimals: 6,
                    initial_balances: vec![],
                    mint: Some(MinterResponse {
                        minter: OWNER.to_string(),
                        cap: None,
                    }),
                    marketing: None,
                },
                &[],
                name,
                None,
            )
            .unwrap();
        token_asset_info(addr)
    }

    /// A 1:1 xyk pool with POOL_DEPTH of each side.
    fn pool(&mut self, a: &AssetInfo, b: &AssetInfo) -> Addr {
        for asset in [a, b] {
            if let AssetInfo::NativeToken { denom } = asset {
                self.app
                    .execute_contract(
                        self.owner.clone(),
                        self.registry.clone(),
                        &astroport::native_coin_registry::ExecuteMsg::Add {
                            native_coins: vec![(denom.clone(), 6)],
                        },
                        &[],
                    )
                    .unwrap();
            }
        }

        self.app
            .execute_contract(
                self.owner.clone(),
                self.factory.clone(),
                &astroport::factory::ExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: vec![a.clone(), b.clone()],
                    init_params: None,
                },
                &[],
            )
            .unwrap();
        let pair: PairInfo = self
            .app
            .wrap()
            .query_wasm_smart(
                &self.factory,
                &astroport::factory::QueryMsg::Pair {
                    asset_infos: vec![a.clone(), b.clone()],
                },
            )
            .unwrap();

        self.fund(&pair.contract_addr, a, POOL_DEPTH);
        self.fund(&pair.contract_addr, b, POOL_DEPTH);
        pair.contract_addr
    }

    fn maker(&mut self, msg: InstantiateMsg) -> anyhow::Result<Addr> {
        self.app.instantiate_contract(
            self.maker_code,
            self.owner.clone(),
            &msg,
            &[],
            "maker",
            Some(OWNER.to_string()),
        )
    }

    fn collect(
        &mut self,
        maker: &Addr,
        assets: &[(&AssetInfo, Option<u128>)],
    ) -> anyhow::Result<AppResponse> {
        self.collect_as(&Addr::unchecked("anyone"), maker, assets)
    }

    fn collect_as(
        &mut self,
        sender: &Addr,
        maker: &Addr,
        assets: &[(&AssetInfo, Option<u128>)],
    ) -> anyhow::Result<AppResponse> {
        self.collect_assets(
            sender,
            maker,
            assets
                .iter()
                .map(|(info, limit)| CollectAsset {
                    info: (*info).clone(),
                    limit: limit.map(Uint128::new),
                    min_receive: None,
                })
                .collect(),
        )
    }

    fn collect_assets(
        &mut self,
        sender: &Addr,
        maker: &Addr,
        assets: Vec<CollectAsset>,
    ) -> anyhow::Result<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            maker.clone(),
            &ExecuteMsg::Collect { assets },
            &[],
        )
    }
}

fn hop(a: &AssetInfo, b: &AssetInfo) -> SwapOperation {
    SwapOperation::AstroSwap {
        offer_asset_info: a.clone(),
        ask_asset_info: b.clone(),
    }
}

fn pool_hop(pool: &Addr, a: &AssetInfo, b: &AssetInfo) -> SwapOperation {
    SwapOperation::PoolSwap {
        pool_addr: pool.to_string(),
        offer_asset_info: a.clone(),
        ask_asset_info: b.clone(),
    }
}

fn err<T: std::fmt::Debug>(res: anyhow::Result<T>) -> ContractError {
    res.unwrap_err().downcast().unwrap()
}

/// Terra: fees swap into USDC.inj; half is swapped to ASTRO and deposited into the satellite
/// (transfer_astro), half goes to the Dev DAO.
#[test]
fn terra_collect_swaps_fees_and_splits_across_legs() {
    let mut suite = Suite::new();
    let (luna, usdc, atom, astro) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ATOM.to_string()),
        native_asset_info(ASTRO.to_string()),
    );
    suite.pool(&atom, &luna);
    suite.pool(&luna, &usdc);
    let luna_astro = suite.pool(&luna, &astro);

    let satellite_code = suite.app.store_code(satellite_mock::contract());
    let satellite = suite
        .app
        .instantiate_contract(
            satellite_code,
            suite.owner.clone(),
            &Empty {},
            &[],
            "satellite",
            None,
        )
        .unwrap();
    let dev_dao = Addr::unchecked("dev_dao");

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: Some(Decimal::percent(5)),
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![
                Leg {
                    share: Decimal::percent(50),
                    route: vec![hop(&usdc, &luna), pool_hop(&luna_astro, &luna, &astro)],
                    action: LegAction::Deposit {
                        contract: satellite.to_string(),
                        msg: to_json_binary(&satellite_mock::SatelliteMsg::TransferAstro {})
                            .unwrap(),
                    },
                },
                Leg {
                    share: Decimal::percent(50),
                    route: vec![],
                    action: LegAction::Send {
                        recipient: dev_dao.to_string(),
                    },
                },
            ],
            routes: vec![(atom.clone(), vec![hop(&atom, &luna), hop(&luna, &usdc)])],
        })
        .unwrap();

    suite.fund(&maker, &atom, 10_000000);
    suite.collect(&maker, &[(&atom, None)]).unwrap();

    let dev_usdc = suite.balance(&dev_dao, &usdc);
    let satellite_astro = suite.balance(&satellite, &astro);
    assert!(dev_usdc > 9_000000 / 2, "dev dao got {dev_usdc}");
    assert!(satellite_astro > 0, "satellite got no ASTRO");

    // everything was swapped and paid out
    for asset in [&atom, &luna, &usdc, &astro] {
        assert_eq!(suite.balance(&maker, asset), 0, "maker kept {asset}");
    }
}

/// Neutron: fees swap into ASTRO, which is all deposited into the burner, together with ASTRO
/// that arrived without a swap (the Treasury, the Terra satellite).
#[test]
fn neutron_collect_burns_everything() {
    let mut suite = Suite::new();
    let (luna, astro) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(ASTRO.to_string()),
    );
    suite.pool(&luna, &astro);

    let burner_code = suite.app.store_code(burner_contract());
    let burner = suite
        .app
        .instantiate_contract(
            burner_code,
            suite.owner.clone(),
            &astroport_astro_burner::msg::InstantiateMsg {
                denom: ASTRO.to_string(),
            },
            &[],
            "burner",
            None,
        )
        .unwrap();

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: astro.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Deposit {
                    contract: burner.to_string(),
                    msg: to_json_binary(&astroport_astro_burner::msg::ExecuteMsg::Burn {}).unwrap(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &astro)])],
        })
        .unwrap();

    suite.fund(&maker, &luna, 5_000000);
    suite.fund(&maker, &astro, 3_000000);
    let supply_before = suite.app.wrap().query_supply(ASTRO).unwrap().amount.u128();

    suite.collect(&maker, &[(&luna, None)]).unwrap();

    let burned: Uint128 = suite
        .app
        .wrap()
        .query_wasm_smart(
            &burner,
            &astroport_astro_burner::msg::QueryMsg::TotalBurned {},
        )
        .unwrap();
    let supply_after = suite.app.wrap().query_supply(ASTRO).unwrap().amount.u128();

    assert!(burned.u128() > 3_000000, "burned {burned}");
    assert_eq!(supply_before - supply_after, burned.u128());
    assert_eq!(suite.balance(&maker, &astro), 0);
    assert_eq!(suite.balance(&maker, &luna), 0);
}

/// A swap whose price impact is above max_spread fails the collect; a smaller limit goes
/// through.
#[test]
fn max_spread_caps_price_impact() {
    let mut suite = Suite::new();
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    suite.pool(&luna, &usdc);

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: Some(Decimal::percent(5)),
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: "dev_dao".to_string(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();

    // a fifth of the pool's depth moves the price far more than 5%
    suite.fund(&maker, &luna, POOL_DEPTH / 5);
    let res = suite.collect(&maker, &[(&luna, None)]);
    let err_text = res.unwrap_err().root_cause().to_string();
    assert!(
        err_text.contains("minimum receive"),
        "expected the router's minimum receive to fail: {err_text}"
    );
    assert_eq!(suite.balance(&maker, &luna), POOL_DEPTH / 5);

    // collecting a small part at a time stays under the cap
    suite.collect(&maker, &[(&luna, Some(10_000000))]).unwrap();
    assert_eq!(suite.balance(&maker, &luna), POOL_DEPTH / 5 - 10_000000);
    assert!(suite.balance(&Addr::unchecked("dev_dao"), &usdc) > 9_000000);
}

#[test]
fn collects_cw20_fee_tokens() {
    let mut suite = Suite::new();
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    let token = suite.token("TOKEN");
    suite.pool(&token, &luna);
    suite.pool(&luna, &usdc);

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: "dev_dao".to_string(),
                },
            }],
            routes: vec![(token.clone(), vec![hop(&token, &luna), hop(&luna, &usdc)])],
        })
        .unwrap();

    suite.fund(&maker, &token, 2_000000);
    suite.collect(&maker, &[(&token, None)]).unwrap();

    assert_eq!(suite.balance(&maker, &token), 0);
    assert!(suite.balance(&Addr::unchecked("dev_dao"), &usdc) > 1_900000);
}

#[test]
fn validates_legs_and_routes() {
    let mut suite = Suite::new();
    let (luna, usdc, atom) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ATOM.to_string()),
    );
    let send = |share: u64| Leg {
        share: Decimal::percent(share),
        route: vec![],
        action: LegAction::Send {
            recipient: "dev_dao".to_string(),
        },
    };
    let base_msg = |legs: Vec<Leg>, routes: Vec<(AssetInfo, Vec<SwapOperation>)>| InstantiateMsg {
        owner: OWNER.to_string(),
        router: "router".to_string(),
        base_asset: usdc.clone(),
        max_spread: None,
        collect_cooldown: None,
        collectors: vec![],
        legs,
        routes,
    };

    // shares must add up to exactly 1
    let res = suite.maker(base_msg(vec![send(50), send(40)], vec![]));
    assert!(matches!(err(res), ContractError::InvalidLegShares { .. }));
    let res = suite.maker(base_msg(vec![send(100), send(0)], vec![]));
    assert!(matches!(err(res), ContractError::InvalidLegShares { .. }));
    let res = suite.maker(base_msg(vec![], vec![]));
    assert!(matches!(err(res), ContractError::InvalidLegCount { .. }));

    // a leg's route has to start at the base asset
    let bad_leg = Leg {
        share: Decimal::one(),
        route: vec![hop(&luna, &atom)],
        action: LegAction::Send {
            recipient: "x".to_string(),
        },
    };
    let res = suite.maker(base_msg(vec![bad_leg], vec![]));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));

    // fee routes have to run from their asset to the base asset, hop by hop
    let res = suite.maker(base_msg(
        vec![send(100)],
        vec![(atom.clone(), vec![hop(&atom, &luna)])],
    ));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));
    let res = suite.maker(base_msg(
        vec![send(100)],
        vec![(atom.clone(), vec![hop(&atom, &luna), hop(&atom, &usdc)])],
    ));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));
    let res = suite.maker(base_msg(
        vec![send(100)],
        vec![(usdc.clone(), vec![hop(&usdc, &luna)])],
    ));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));

    // a route can't visit the same asset twice
    let res = suite.maker(base_msg(
        vec![send(100)],
        vec![(
            atom.clone(),
            vec![hop(&atom, &usdc), hop(&usdc, &luna), hop(&luna, &usdc)],
        )],
    ));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));
    let round_trip = Leg {
        share: Decimal::one(),
        route: vec![hop(&usdc, &luna), hop(&luna, &usdc)],
        action: LegAction::Send {
            recipient: "x".to_string(),
        },
    };
    let res = suite.maker(base_msg(vec![round_trip], vec![]));
    assert!(matches!(err(res), ContractError::InvalidRoute { .. }));

    // the same asset can't get two routes
    let res = suite.maker(base_msg(
        vec![send(100)],
        vec![
            (luna.clone(), vec![hop(&luna, &usdc)]),
            (luna.clone(), vec![hop(&luna, &usdc)]),
        ],
    ));
    assert!(matches!(err(res), ContractError::DuplicateAsset { .. }));

    // max spread and cooldown bounds
    let mut msg = base_msg(vec![send(100)], vec![]);
    msg.max_spread = Some(Decimal::zero());
    assert_eq!(err(suite.maker(msg)), ContractError::IncorrectMaxSpread {});
    let mut msg = base_msg(vec![send(100)], vec![]);
    msg.collect_cooldown = Some(5);
    assert!(matches!(
        err(suite.maker(msg)),
        ContractError::IncorrectCooldown { .. }
    ));
}

#[test]
fn owner_updates_config_and_routes() {
    let mut suite = Suite::new();
    let (luna, usdc, astro) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ASTRO.to_string()),
    );
    suite.pool(&luna, &usdc);
    suite.pool(&luna, &astro);

    let leg_to = |recipient: &str| Leg {
        share: Decimal::one(),
        route: vec![],
        action: LegAction::Send {
            recipient: recipient.to_string(),
        },
    };
    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![leg_to("dev_dao")],
            routes: vec![],
        })
        .unwrap();

    // a leg can't send to or deposit into the Maker itself
    let self_leg = |action: LegAction| ExecuteMsg::UpdateConfig {
        router: None,
        base_asset: None,
        max_spread: None,
        collect_cooldown: None,
        collectors: None,
        legs: Some(vec![Leg {
            share: Decimal::one(),
            route: vec![],
            action,
        }]),
    };
    for action in [
        LegAction::Send {
            recipient: maker.to_string(),
        },
        LegAction::Deposit {
            contract: maker.to_string(),
            msg: to_json_binary(&ExecuteMsg::Distribute { limit: None }).unwrap(),
        },
    ] {
        let res =
            suite
                .app
                .execute_contract(suite.owner.clone(), maker.clone(), &self_leg(action), &[]);
        assert!(matches!(err(res), ContractError::InvalidLeg { .. }));
    }

    // a cooldown can be set, and removed again with 0
    for (set, expected) in [(Some(60), Some(60)), (Some(0), None)] {
        suite
            .app
            .execute_contract(
                suite.owner.clone(),
                maker.clone(),
                &ExecuteMsg::UpdateConfig {
                    router: None,
                    base_asset: None,
                    max_spread: None,
                    collect_cooldown: set,
                    collectors: None,
                    legs: None,
                },
                &[],
            )
            .unwrap();
        let config: Config = suite
            .app
            .wrap()
            .query_wasm_smart(&maker, &QueryMsg::Config {})
            .unwrap();
        assert_eq!(config.collect_cooldown, expected);
    }

    // only the owner can change anything
    let res = suite.app.execute_contract(
        Addr::unchecked("stranger"),
        maker.clone(),
        &ExecuteMsg::UpdateRoutes {
            add: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
            remove: vec![],
        },
        &[],
    );
    assert_eq!(err(res), ContractError::Unauthorized {});

    // a fee token without a route can't be collected, even with no balance
    assert!(matches!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::NoRoute { .. }
    ));
    suite.fund(&maker, &luna, 1_000000);
    assert!(matches!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::NoRoute { .. }
    ));

    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateRoutes {
                add: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
                remove: vec![],
            },
            &[],
        )
        .unwrap();
    let routes: Vec<RouteResponse> = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::Routes {})
        .unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[0].asset, luna);

    // a new base asset needs legs that start from it; the old routes then no longer fit and
    // are refused at collect until they're replaced
    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateConfig {
                router: None,
                base_asset: Some(astro.clone()),
                max_spread: None,
                collect_cooldown: None,
                collectors: None,
                legs: Some(vec![leg_to("burner")]),
            },
            &[],
        )
        .unwrap();
    assert!(matches!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::InvalidRoute { .. }
    ));

    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateRoutes {
                add: vec![(luna.clone(), vec![hop(&luna, &astro)])],
                remove: vec![],
            },
            &[],
        )
        .unwrap();
    suite.collect(&maker, &[(&luna, None)]).unwrap();
    assert!(suite.balance(&Addr::unchecked("burner"), &astro) > 0);

    let config: Config = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.base_asset, astro);

    // routes can be removed
    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateRoutes {
                add: vec![],
                remove: vec![luna.clone()],
            },
            &[],
        )
        .unwrap();
    let routes: Vec<RouteResponse> = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::Routes {})
        .unwrap();
    assert!(routes.is_empty());
}

#[test]
fn internal_messages_are_self_only() {
    let mut suite = Suite::new();
    let usdc = native_asset_info(USDC.to_string());
    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: "dev_dao".to_string(),
                },
            }],
            routes: vec![],
        })
        .unwrap();

    for msg in [
        ExecuteMsg::Distribute { limit: None },
        ExecuteMsg::SnapshotLeg { leg: 0 },
        ExecuteMsg::DepositLeg { leg: 0 },
    ] {
        let res = suite
            .app
            .execute_contract(Addr::unchecked("anyone"), maker.clone(), &msg, &[]);
        assert_eq!(err(res), ContractError::Unauthorized {});
    }
}

#[test]
fn seize_and_cooldown() {
    let mut suite = Suite::new();
    let (luna, usdc, atom) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ATOM.to_string()),
    );
    suite.pool(&luna, &usdc);

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: Some(60),
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: "dev_dao".to_string(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();

    // cooldown: a second collect inside the period fails
    suite.fund(&maker, &luna, 1_000000);
    suite.app.update_block(|b| b.time = b.time.plus_seconds(61));
    suite.collect(&maker, &[(&luna, None)]).unwrap();
    assert!(matches!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::Cooldown { .. }
    ));

    // seize: only listed assets, to the configured receiver
    suite.fund(&maker, &atom, 500);
    let res = suite.app.execute_contract(
        Addr::unchecked("anyone"),
        maker.clone(),
        &ExecuteMsg::Seize {
            assets: vec![AssetWithLimit {
                info: atom.clone(),
                limit: None,
            }],
        },
        &[],
    );
    assert!(res.is_err());

    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateSeizeConfig {
                receiver: Some("seize_receiver".to_string()),
                seizable_assets: Some(vec![atom.clone()]),
            },
            &[],
        )
        .unwrap();
    suite
        .app
        .execute_contract(
            Addr::unchecked("anyone"),
            maker.clone(),
            &ExecuteMsg::Seize {
                assets: vec![AssetWithLimit {
                    info: atom.clone(),
                    limit: None,
                }],
            },
            &[],
        )
        .unwrap();
    assert_eq!(
        suite.balance(&Addr::unchecked("seize_receiver"), &atom),
        500
    );

    let seize: SeizeConfig = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::QuerySeizeConfig {})
        .unwrap();
    assert_eq!(seize.seizable_assets, vec![atom.clone()]);

    // a receiver-only update keeps the list; an empty list clears it
    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &serde_json::json!({"update_seize_config": {"receiver": "seize_receiver2"}}),
            &[],
        )
        .unwrap();
    let seize: SeizeConfig = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::QuerySeizeConfig {})
        .unwrap();
    assert_eq!(seize.receiver, Addr::unchecked("seize_receiver2"));
    assert_eq!(seize.seizable_assets, vec![atom]);
    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &ExecuteMsg::UpdateSeizeConfig {
                receiver: None,
                seizable_assets: Some(vec![]),
            },
            &[],
        )
        .unwrap();
    let seize: SeizeConfig = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::QuerySeizeConfig {})
        .unwrap();
    assert!(seize.seizable_assets.is_empty());
}

/// Loads a deployed Maker 1.7.0's real storage (fetched from chain) into a contract, migrates
/// it to 2.0 and checks what carried over, then runs a collect on it.
fn migrate_from_fixture(chain: &str) {
    let mut suite = Suite::new();
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    suite.pool(&luna, &usdc);

    let placeholder_code = suite.app.store_code(placeholder::contract());
    let maker = suite
        .app
        .instantiate_contract(
            placeholder_code,
            suite.owner.clone(),
            &Empty {},
            &[],
            "maker v1.7.0",
            Some(OWNER.to_string()),
        )
        .unwrap();

    let fixture = |key: &str| -> Vec<u8> {
        std::fs::read(format!(
            "{}/tests/fixtures/{chain}_maker_v170_{key}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    };
    let old_config: serde_json::Value = serde_json::from_slice(&fixture("config")).unwrap();
    {
        let mut storage = suite.app.contract_storage_mut(&maker);
        for key in ["config", "seize_config", "contract_info", "last_collect_ts"] {
            storage.set(key.as_bytes(), &fixture(key));
        }
        // a couple of 1.x bridges, which the migration clears
        let bridges: cw_storage_plus::Map<String, AssetInfo> = cw_storage_plus::Map::new("bridges");
        bridges
            .save(storage.as_mut(), "uatom".to_string(), &luna)
            .unwrap();
        bridges
            .save(storage.as_mut(), LUNA.to_string(), &usdc)
            .unwrap();
    }

    suite
        .app
        .migrate_contract(
            suite.owner.clone(),
            maker.clone(),
            &MigrateMsg {
                router: suite.router.to_string(),
                base_asset: usdc.clone(),
                max_spread: None,
                collect_cooldown: None,
                collectors: vec!["keeper".to_string()],
                legs: vec![Leg {
                    share: Decimal::one(),
                    route: vec![],
                    action: LegAction::Send {
                        recipient: "dev_dao".to_string(),
                    },
                }],
                routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
            },
            suite.maker_code,
        )
        .unwrap();

    let config: Config = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner.as_str(), old_config["owner"].as_str().unwrap());
    assert_eq!(
        config.max_spread.to_string(),
        old_config["max_spread"].as_str().unwrap()
    );
    assert_eq!(config.base_asset, usdc);
    assert_eq!(config.collectors, vec![Addr::unchecked("keeper")]);

    // the seize config carries over byte for byte
    let seize: SeizeConfig = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::QuerySeizeConfig {})
        .unwrap();
    let old_seize: SeizeConfig = serde_json::from_slice(&fixture("seize_config")).unwrap();
    assert_eq!(seize, old_seize);

    // the old bridges are gone
    {
        let storage = suite.app.contract_storage_mut(&maker);
        let bridges: cw_storage_plus::Map<String, AssetInfo> = cw_storage_plus::Map::new("bridges");
        assert!(bridges
            .range(storage.as_ref(), None, None, cosmwasm_std::Order::Ascending)
            .next()
            .is_none());
    }

    let version = cw2::query_contract_info(&suite.app.wrap(), maker.to_string()).unwrap();
    assert_eq!(version.version, env!("CARGO_PKG_VERSION"));

    // the migrated Maker collects normally, for the collector it was given
    suite.fund(&maker, &luna, 1_000000);
    assert_eq!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::Unauthorized {}
    );
    suite
        .collect_as(&Addr::unchecked("keeper"), &maker, &[(&luna, None)])
        .unwrap();
    assert!(suite.balance(&Addr::unchecked("dev_dao"), &usdc) > 900000);

    // migrating again is refused
    let res = suite.app.migrate_contract(
        suite.owner.clone(),
        maker,
        &MigrateMsg {
            router: suite.router.to_string(),
            base_asset: usdc,
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![],
            routes: vec![],
        },
        suite.maker_code,
    );
    assert_eq!(err(res), ContractError::MigrationError {});
}

#[test]
fn migrates_terra_maker_v170() {
    migrate_from_fixture("terra");
}

#[test]
fn migrates_neutron_maker_v170() {
    migrate_from_fixture("neutron");
}

/// A leg with a route and a plain recipient gets the swap output straight from the router, and
/// fee amounts too small to price are left for a later collect instead of failing it.
#[test]
fn swap_and_send_leg_and_dust() {
    let mut suite = Suite::new();
    let (luna, usdc, astro) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ASTRO.to_string()),
    );
    suite.pool(&luna, &usdc);
    suite.pool(&usdc, &astro);

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![
                Leg {
                    share: Decimal::percent(25),
                    route: vec![hop(&usdc, &astro)],
                    action: LegAction::Send {
                        recipient: "astro_receiver".to_string(),
                    },
                },
                Leg {
                    share: Decimal::percent(75),
                    route: vec![],
                    action: LegAction::Send {
                        recipient: "dev_dao".to_string(),
                    },
                },
            ],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();

    // one unit prices at zero after the pool fee, so it's skipped
    suite.fund(&maker, &luna, 1);
    let res = suite.collect(&maker, &[(&luna, None)]).unwrap();
    assert!(res
        .events
        .iter()
        .any(|e| e.attributes.iter().any(|a| a.key == "skipped_dust")));
    assert_eq!(suite.balance(&maker, &luna), 1);

    suite.fund(&maker, &usdc, 4_000000);
    suite.collect(&maker, &[]).unwrap();
    assert_eq!(suite.balance(&Addr::unchecked("dev_dao"), &usdc), 3_000000);
    let astro_out = suite.balance(&Addr::unchecked("astro_receiver"), &astro);
    assert!(
        astro_out > 990000 && astro_out < 1_000000,
        "got {astro_out}"
    );
    assert_eq!(suite.balance(&maker, &usdc), 0);
}

/// With `collectors` set only they can collect; an empty list opens it up again.
#[test]
fn collectors_restrict_collect() {
    let mut suite = Suite::new();
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    suite.pool(&luna, &usdc);
    let keeper = Addr::unchecked("keeper");

    let update_collectors = |collectors: Vec<&str>| ExecuteMsg::UpdateConfig {
        router: None,
        base_asset: None,
        max_spread: None,
        collect_cooldown: None,
        collectors: Some(collectors.into_iter().map(String::from).collect()),
        legs: None,
    };

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: None,
            collect_cooldown: None,
            collectors: vec![keeper.to_string()],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: "dev_dao".to_string(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();
    suite.fund(&maker, &luna, 1_000000);

    // only a listed address can collect
    assert_eq!(
        err(suite.collect(&maker, &[(&luna, None)])),
        ContractError::Unauthorized {}
    );
    assert_eq!(suite.balance(&maker, &luna), 1_000000);
    suite.collect_as(&keeper, &maker, &[(&luna, None)]).unwrap();
    assert_eq!(suite.balance(&maker, &luna), 0);

    // the list is validated
    let res = suite.app.execute_contract(
        suite.owner.clone(),
        maker.clone(),
        &update_collectors(vec!["keeper", "keeper"]),
        &[],
    );
    assert!(matches!(err(res), ContractError::DuplicateCollector { .. }));

    // an empty list lets anyone collect again
    suite
        .app
        .execute_contract(
            suite.owner.clone(),
            maker.clone(),
            &update_collectors(vec![]),
            &[],
        )
        .unwrap();
    let config: Config = suite
        .app
        .wrap()
        .query_wasm_smart(&maker, &QueryMsg::Config {})
        .unwrap();
    assert!(config.collectors.is_empty());
    suite.fund(&maker, &luna, 1_000000);
    suite.collect(&maker, &[(&luna, None)]).unwrap();
    assert_eq!(suite.balance(&maker, &luna), 0);
}

/// `min_receive` given with a fee token is a floor on top of the Maker's own check. The Maker
/// measures a swap against the pool as it is, so it can't see that the pool was pushed off
/// market just before; a floor from a quote taken elsewhere can.
#[test]
fn min_receive_floor_catches_a_skewed_pool() {
    let mut suite = Suite::new();
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    suite.pool(&luna, &usdc);
    let keeper = Addr::unchecked("keeper");
    let dev_dao = Addr::unchecked("dev_dao");

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: Some(Decimal::percent(5)),
            collect_cooldown: None,
            collectors: vec![keeper.to_string()],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![],
                action: LegAction::Send {
                    recipient: dev_dao.to_string(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();

    // the keeper quotes the route first and allows 1% under it
    let fees = POOL_DEPTH / 100;
    let quote: astroport_router::msg::SimulateSwapOperationsResponse = suite
        .app
        .wrap()
        .query_wasm_smart(
            &suite.router,
            &astroport_router::msg::QueryMsg::SimulateSwapOperations {
                offer_amount: Uint128::new(fees),
                operations: vec![hop(&luna, &usdc)],
            },
        )
        .unwrap();
    let floor = quote.amount.u128() * 99 / 100;
    let with_floor = vec![CollectAsset {
        info: luna.clone(),
        limit: None,
        min_receive: Some(Uint128::new(floor)),
    }];

    // an honest collect clears the floor
    suite.fund(&maker, &luna, fees);
    suite
        .collect_assets(&keeper, &maker, with_floor.clone())
        .unwrap();
    assert!(suite.balance(&dev_dao, &usdc) >= floor);

    // someone pushes LUNA's price in the pool far down just before the next collect
    suite.fund(&maker, &luna, fees);
    let attacker = Addr::unchecked("attacker");
    suite.fund(&attacker, &luna, POOL_DEPTH / 2);
    suite
        .app
        .execute_contract(
            attacker,
            suite.router.clone(),
            &astroport_router::msg::ExecuteMsg::ExecuteSwapOperations {
                operations: vec![hop(&luna, &usdc)],
                minimum_receive: Uint128::one(),
                to: None,
            },
            &coins(POOL_DEPTH / 2, LUNA),
        )
        .unwrap();

    // the floor fails the swap and nothing moves
    let paid_before = suite.balance(&dev_dao, &usdc);
    let e = suite
        .collect_assets(&keeper, &maker, with_floor)
        .unwrap_err()
        .root_cause()
        .to_string();
    assert!(e.contains("minimum receive"), "{e}");
    assert_eq!(suite.balance(&maker, &luna), fees);

    // without it the Maker's own check lets the same swap through at the skewed price
    suite.collect_as(&keeper, &maker, &[(&luna, None)]).unwrap();
    let paid = suite.balance(&dev_dao, &usdc) - paid_before;
    assert!(paid < floor / 2, "paid {paid} against a floor of {floor}");
}

/// The base asset's `limit` caps how much of it is split across the legs, so a balance too big
/// for a leg's route is split in parts instead of blocking every collect.
#[test]
fn base_asset_limit_caps_the_split() {
    let mut suite = Suite::new();
    let (luna, usdc, astro) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
        native_asset_info(ASTRO.to_string()),
    );
    suite.pool(&luna, &usdc);
    suite.pool(&usdc, &astro);
    let burner = Addr::unchecked("burner");

    let maker = suite
        .maker(InstantiateMsg {
            owner: OWNER.to_string(),
            router: suite.router.to_string(),
            base_asset: usdc.clone(),
            max_spread: Some(Decimal::percent(5)),
            collect_cooldown: None,
            collectors: vec![],
            legs: vec![Leg {
                share: Decimal::one(),
                route: vec![hop(&usdc, &astro)],
                action: LegAction::Send {
                    recipient: burner.to_string(),
                },
            }],
            routes: vec![(luna.clone(), vec![hop(&luna, &usdc)])],
        })
        .unwrap();

    // a fifth of the leg pool's depth in USDC, plus some fees
    let stuck = POOL_DEPTH / 5;
    suite.fund(&maker, &usdc, stuck);
    suite.fund(&maker, &luna, 1_000000);

    // splitting it all moves the leg's pool more than 5%: every uncapped collect fails
    for assets in [vec![], vec![(&luna, None)]] {
        let e = suite
            .collect(&maker, &assets)
            .unwrap_err()
            .root_cause()
            .to_string();
        assert!(e.contains("minimum receive"), "{e}");
    }

    // capped, the fees are swapped and a slice of the base asset is split
    suite
        .collect(&maker, &[(&luna, None), (&usdc, Some(10_000000))])
        .unwrap();
    assert_eq!(suite.balance(&maker, &luna), 0);
    let left = suite.balance(&maker, &usdc);
    assert!(
        left > stuck - 10_000000 && left < stuck - 9_000000,
        "left {left}"
    );
    let out = suite.balance(&burner, &astro);
    assert!(out > 9_900000 && out <= 10_000000, "burner got {out}");
}

/// Small amounts are priced with a larger probe: the integer rounding of a tiny probe output
/// would otherwise put the minimum far below the route's rate. What even the whole amount can't
/// price is dust.
#[test]
fn small_amounts_use_a_larger_probe() {
    use cosmwasm_std::testing::MockQuerier;
    use cosmwasm_std::{from_json, ContractResult, QuerierWrapper, SystemResult, WasmQuery};

    // a route paying 0.9995 per unit, floored like a pool does, through a pool that refuses
    // offers under 100 units outright instead of returning zero
    let rate = Decimal::from_ratio(9995u128, 10000u128);
    let mut querier = MockQuerier::default();
    querier.update_wasm(move |req| match req {
        WasmQuery::Smart { msg, .. } => {
            let astroport_router::msg::QueryMsg::SimulateSwapOperations { offer_amount, .. } =
                from_json(msg).unwrap()
            else {
                panic!("unexpected query");
            };
            if offer_amount.u128() < 100 {
                return SystemResult::Ok(ContractResult::Err("offer too small".to_string()));
            }
            SystemResult::Ok(ContractResult::Ok(
                to_json_binary(&astroport_router::msg::SimulateSwapOperationsResponse {
                    amount: offer_amount * rate,
                })
                .unwrap(),
            ))
        }
        _ => panic!("unexpected query"),
    });
    let querier = QuerierWrapper::new(&querier);
    let (luna, usdc) = (
        native_asset_info(LUNA.to_string()),
        native_asset_info(USDC.to_string()),
    );
    let route = vec![hop(&luna, &usdc)];
    let minimum = |amount: u128| {
        astroport_maker::utils::minimum_receive(
            &querier,
            "router",
            &route,
            Uint128::new(amount),
            Decimal::percent(5),
        )
        .unwrap()
        .u128()
    };
    // 5% under the fair value, give or take the probe's own rounding
    let within_spread = |minimum: u128, amount: u128| {
        let fair = amount * 9995 / 10000;
        assert!(
            minimum >= fair * 949 / 1000 && minimum <= fair * 95 / 100,
            "{minimum} for {amount} (fair {fair})"
        );
    };

    // 3_000: a thousandth prices as 2 (2.9985 floored), which would put the minimum at 1_900,
    // 37% under the fair 2_998; the whole amount is used instead
    assert_eq!(minimum(3_000), 2_998 * 95 / 100);
    // 300_000: a thousandth prices as 299, a hundredth as 2_998 and is used
    within_spread(minimum(300_000), 300_000);
    // large amounts use a thousandth
    within_spread(minimum(3_000_000), 3_000_000);
    // under 1_000 out even the whole amount can't be priced: dust
    assert_eq!(minimum(999), 0);

    // a route that can't be simulated at all fails the collect instead of being skipped
    let mut broken = MockQuerier::default();
    broken.update_wasm(|_| SystemResult::Ok(ContractResult::Err("pool is paused".to_string())));
    let broken = QuerierWrapper::new(&broken);
    assert!(astroport_maker::utils::minimum_receive(
        &broken,
        "router",
        &route,
        Uint128::new(3_000),
        Decimal::percent(5),
    )
    .is_err());
}
