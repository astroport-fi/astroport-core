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
    Addr, BankMsg, Binary, Coin, Decimal, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult,
};
use cw20::MinterResponse;
use derivative::Derivative;
use itertools::Itertools;

use astroport::maker::{
    AssetWithLimit, Config, ExecuteMsg, PoolRoute, QueryMsg, RouteStep, SeizeConfig,
    UpdateDevFundConfig,
};
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

fn cw20_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

pub const MOCK_IBC_ESCROW: &str = "wasm1u8asrdfc9gheq2qh0nvlpflvpem3q4t5maqllcfdds9grfzuttjqxsl0yn";

fn mock_satellite_contract() -> Box<dyn Contract<Empty>> {
    let instantiate = |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
        Ok(Default::default())
    };
    let execute = |_: DepsMut,
                   _: Env,
                   info: MessageInfo,
                   msg: astro_satellite_package::ExecuteMsg|
     -> StdResult<Response> {
        match msg {
            astro_satellite_package::ExecuteMsg::TransferAstro {} => Ok(Response::new()
                .add_message(BankMsg::Send {
                    to_address: MOCK_IBC_ESCROW.to_string(),
                    amount: info.funds,
                })),
            _ => unimplemented!(),
        }
    };
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
    pub astro_denom: String,
    pub owner: Addr,
    pub factory: Addr,
    pub maker: Addr,
    pub satellite: Addr,
    pub cw20_code_id: u64,
}

impl Helper {
    pub fn new(astro_denom: &str) -> AnyResult<Self> {
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
                    astro_denom: astro_denom.to_string(),
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

        let cw20_code_id = app.store_code(cw20_contract());

        Ok(Self {
            app,
            astro_denom: astro_denom.to_string(),
            owner,
            factory,
            maker,
            satellite,
            cw20_code_id,
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

    pub fn seize(&mut self, sender: &Addr, assets: Vec<AssetWithLimit>) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.maker.clone(),
            &ExecuteMsg::Seize { assets },
            &[],
        )
    }

    pub fn query_seize_config(&self) -> StdResult<SeizeConfig> {
        self.app
            .wrap()
            .query_wasm_smart(&self.maker, &QueryMsg::QuerySeizeConfig {})
    }

    pub fn set_dev_fund_config(
        &mut self,
        sender: &Addr,
        dev_fund_config: UpdateDevFundConfig,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.maker.clone(),
            &ExecuteMsg::UpdateConfig {
                astro_denom: None,
                max_spread: None,
                collect_cooldown: None,
                dev_fund_config: Some(Box::new(dev_fund_config)),
                collector: None,
            },
            &[],
        )
    }

    pub fn query_config(&self) -> StdResult<Config> {
        self.app
            .wrap()
            .query_wasm_smart(&self.maker, &QueryMsg::Config {})
    }

    pub fn init_cw20(&mut self, ticker: &str) -> AnyResult<Addr> {
        self.app.instantiate_contract(
            self.cw20_code_id,
            self.owner.clone(),
            &cw20_base::msg::InstantiateMsg {
                name: ticker.to_string(),
                symbol: ticker.to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: self.owner.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            &[],
            "label",
            None,
        )
    }

    pub fn mint_cw20(
        &mut self,
        token_addr: &Addr,
        recipient: &Addr,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            token_addr.clone(),
            &cw20::Cw20ExecuteMsg::Mint {
                recipient: recipient.to_string(),
                amount: amount.into(),
            },
            &[],
        )
    }

    pub fn set_allowance_cw20(
        &mut self,
        token_addr: &Addr,
        sender: &Addr,
        spender: &Addr,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            token_addr.clone(),
            &cw20::Cw20ExecuteMsg::IncreaseAllowance {
                spender: spender.to_string(),
                amount: amount.into(),
                expires: None,
            },
            &[],
        )
    }
}

pub fn f64_to_dec<T>(val: f64) -> T
where
    T: FromStr,
    T::Err: Error,
{
    T::from_str(&val.to_string()).unwrap()
}
