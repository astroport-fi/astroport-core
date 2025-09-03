#![allow(dead_code)]

use std::error::Error;
use std::str::FromStr;

use anyhow::Result as AnyResult;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::pair_concentrated::ConcentratedPoolParams;
use astroport::{factory, maker, pair};
use astroport_test::cw_multi_test::{
    no_init, App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, Contract, ContractWrapper,
    DistributionKeeper, Executor, FailingModule, GovFailingModule, IbcFailingModule,
    MockAddressGenerator, MockApiBech32, StakeKeeper, WasmKeeper,
};
use cosmwasm_std::testing::MockStorage;
use cosmwasm_std::{
    Addr, Binary, Coin, Decimal, Deps, DepsMut, Empty, Env, MessageInfo, Response, StdResult,
};
use derivative::Derivative;
use itertools::Itertools;

use astroport::maker::{AssetWithLimit, PoolRoute, RouteStep};
use astroport_test::modules::stargate::MockStargate;

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

fn maker_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_maker::execute::execute,
            astroport_maker::instantiate::instantiate,
            astroport_maker::query::query,
        )
        .with_reply_empty(astroport_maker::reply::reply),
    )
}

fn mock_satellite_contract() -> Box<dyn Contract<Empty>> {
    let instantiate = |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
        Ok(Default::default())
    };
    let execute = |_: DepsMut,
                   _: Env,
                   _: MessageInfo,
                   _: astro_satellite_package::ExecuteMsg|
     -> StdResult<Response> { Ok(Default::default()) };
    let empty_query = |_: Deps, _: Env, _: Empty| -> StdResult<Binary> { unimplemented!() };

    Box::new(ContractWrapper::new_with_empty(
        execute,
        instantiate,
        empty_query,
    ))
}

fn common_pcl_params(price_scale: Decimal) -> ConcentratedPoolParams {
    ConcentratedPoolParams {
        amp: f64_to_dec(10f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale,
        ma_half_time: 600,
        track_asset_balances: None,
        fee_share: None,
        allowed_xcp_profit_drop: None,
        xcp_profit_losses_threshold: None,
    }
}

pub const ASTRO_DENOM: &str = "astro";

pub type CustomApp<ExecC = Empty, QueryC = Empty> = App<
    BankKeeper,
    MockApiBech32,
    MockStorage,
    FailingModule<ExecC, QueryC, Empty>,
    WasmKeeper<ExecC, QueryC>,
    StakeKeeper,
    DistributionKeeper,
    IbcFailingModule,
    GovFailingModule,
    MockStargate,
>;

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: CustomApp,
    pub owner: Addr,
    pub factory: Addr,
    pub maker: Addr,
    pub satellite: Addr,
}

impl Helper {
    pub fn new() -> AnyResult<Self> {
        let mut app = BasicAppBuilder::new()
            .with_api(MockApiBech32::new("wasm"))
            .with_stargate(MockStargate::default())
            .with_wasm(WasmKeeper::new().with_address_generator(MockAddressGenerator))
            .build(no_init);

        let owner = app.api().addr_make("owner");

        let pair_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let satellite_code_id = app.store_code(mock_satellite_contract());

        let init_msg = factory::InstantiateMsg {
            fee_address: None,
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                maker_fee_bps: 3333,
                total_fee_bps: 33u16,
                pair_type: PairType::Xyk {},
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: false,
                whitelist: None,
            }],
            token_code_id: 0,
            generator_address: None,
            owner: owner.to_string(),
            coin_registry_address: app.api().addr_make("registry").to_string(),
        };

        let factory = app
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &init_msg,
                &[],
                "Factory",
                None,
            )
            .unwrap();

        let satellite = app
            .instantiate_contract(
                satellite_code_id,
                owner.clone(),
                &Empty {},
                &[],
                "Satellite",
                None,
            )
            .unwrap();

        let maker_code_id = app.store_code(maker_contract());
        let maker = app
            .instantiate_contract(
                maker_code_id,
                owner.clone(),
                &maker::InstantiateMsg {
                    owner: owner.to_string(),
                    factory_contract: factory.to_string(),
                    astro_denom: ASTRO_DENOM.to_string(),
                    collector: satellite.to_string(),
                    max_spread: Decimal::percent(10),
                    collect_cooldown: None,
                },
                &[],
                "Maker",
                None,
            )
            .unwrap();

        app.execute_contract(
            owner.clone(),
            factory.clone(),
            &factory::ExecuteMsg::UpdateConfig {
                token_code_id: None,
                fee_address: Some(maker.to_string()),
                generator_address: None,
                coin_registry_address: None,
            },
            &[],
        )
        .unwrap();

        Ok(Self {
            app,
            owner,
            factory,
            maker,
            satellite,
        })
    }

    pub fn create_and_seed_pair(&mut self, initial_liquidity: [Coin; 2]) -> AnyResult<PairInfo> {
        let asset_infos = initial_liquidity
            .iter()
            .map(|coin| AssetInfo::native(&coin.denom))
            .collect_vec();

        let owner = self.owner.clone();

        let pair_info = self
            .app
            .execute_contract(
                owner.clone(),
                self.factory.clone(),
                &factory::ExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: asset_infos.clone(),
                    init_params: None,
                },
                &[],
            )
            .map(|_| self.query_pair_info(&asset_infos))?;

        let provide_assets = [
            Asset::native(&initial_liquidity[0].denom, initial_liquidity[0].amount),
            Asset::native(&initial_liquidity[1].denom, initial_liquidity[1].amount),
        ];

        self.give_me_money(&provide_assets, &owner);
        self.provide(&pair_info.contract_addr, &owner, &provide_assets)
            .unwrap();

        Ok(pair_info)
    }

    pub fn set_pool_routes(&mut self, pool_routes: Vec<PoolRoute>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.maker.clone(),
            &maker::ExecuteMsg::SetPoolRoutes(pool_routes),
            &[],
        )
    }

    pub fn collect(&mut self, assets: Vec<AssetWithLimit>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.maker.clone(),
            &maker::ExecuteMsg::Collect { assets },
            &[],
        )
    }

    pub fn query_route(&self, denom_in: &str) -> StdResult<Vec<RouteStep>> {
        self.app.wrap().query_wasm_smart(
            &self.maker,
            &maker::QueryMsg::Route {
                asset_in: denom_in.to_string(),
            },
        )
    }

    pub fn query_pair_info(&self, asset_infos: &[AssetInfo]) -> PairInfo {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.factory,
                &factory::QueryMsg::Pair {
                    asset_infos: asset_infos.to_vec(),
                },
            )
            .unwrap()
    }

    pub fn provide(
        &mut self,
        pair: &Addr,
        sender: &Addr,
        assets: &[Asset],
    ) -> AnyResult<AppResponse> {
        let funds = assets
            .iter()
            .map(|x| x.as_coin().unwrap())
            .collect::<Vec<_>>();

        let msg = pair::ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance: Some(f64_to_dec(0.5)),
            auto_stake: None,
            receiver: None,
            min_lp_to_receive: None,
        };

        self.app
            .execute_contract(sender.clone(), pair.clone(), &msg, &funds)
    }

    pub fn swap(
        &mut self,
        pair: &Addr,
        sender: &Addr,
        offer_asset: &Asset,
        max_spread: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        match &offer_asset.info {
            AssetInfo::NativeToken { .. } => self.app.execute_contract(
                sender.clone(),
                pair.clone(),
                &pair::ExecuteMsg::Swap {
                    offer_asset: offer_asset.clone(),
                    ask_asset_info: None,
                    belief_price: None,
                    max_spread,
                    to: None,
                },
                &[offer_asset.as_coin().unwrap()],
            ),
            AssetInfo::Token { .. } => unimplemented!("cw20 not implemented"),
        }
    }

    pub fn native_balance(&self, denom: &str, user: &Addr) -> u128 {
        self.app
            .wrap()
            .query_balance(user, denom)
            .unwrap()
            .amount
            .u128()
    }

    pub fn give_me_money(&mut self, assets: &[Asset], recipient: &Addr) {
        let funds = assets
            .iter()
            .map(|x| x.as_coin().unwrap())
            .collect::<Vec<_>>();

        self.app
            .sudo(
                BankSudo::Mint {
                    to_address: recipient.to_string(),
                    amount: funds,
                }
                .into(),
            )
            .unwrap();
    }
}

pub fn f64_to_dec<T>(val: f64) -> T
where
    T: FromStr,
    T::Err: Error,
{
    T::from_str(&val.to_string()).unwrap()
}
