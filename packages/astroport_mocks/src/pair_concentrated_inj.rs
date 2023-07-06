use std::fmt::Debug;

use astroport::{
    asset::{Asset, AssetInfo, PairInfo},
    factory::{ExecuteMsg as FactoryExecuteMsg, PairConfig, PairType, QueryMsg as FactoryQueryMsg},
    pair::QueryMsg,
    pair_concentrated::ConcentratedPoolParams,
    pair_concentrated_inj::{ConcentratedInjObParams, OrderbookConfig},
};
use astroport_pair_concentrated_injective::orderbook::utils::calc_market_ids;
use cosmwasm_std::{to_binary, Addr, Api, CustomQuery, Decimal, Storage};
use cw_multi_test::{Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking};
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    astroport_address,
    factory::{MockFactory, MockFactoryOpt},
    MockFactoryBuilder, MockToken, MockXykPair, WKApp,
};

pub fn store_code<B, A, S, C, X, D, I, G>(app: &WKApp<B, A, S, C, X, D, I, G>) -> u64
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module<ExecT = InjectiveMsgWrapper, QueryT = InjectiveQueryWrapper>,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    use astroport_pair_concentrated_injective as cnt;
    let contract = Box::new(
        ContractWrapper::new(
            cnt::contract::execute,
            cnt::contract::instantiate,
            cnt::queries::query,
        )
        .with_reply(cnt::contract::reply),
    );

    app.borrow_mut().store_code(contract)
}

pub struct MockConcentratedPairInjBuilder<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub asset_infos: Vec<AssetInfo>,
    pub factory: MockFactoryOpt<B, A, S, C, X, D, I, G>,
}

impl<B, A, S, C, X, D, I, G> MockConcentratedPairInjBuilder<B, A, S, C, X, D, I, G>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module<ExecT = InjectiveMsgWrapper, QueryT = InjectiveQueryWrapper>,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G>) -> Self {
        Self {
            app: app.clone(),
            asset_infos: Default::default(),
            factory: None,
        }
    }

    pub fn with_factory(mut self, factory: &MockFactory<B, A, S, C, X, D, I, G>) -> Self {
        self.factory = Some(MockFactory {
            app: self.app.clone(),
            address: factory.address.clone(),
        });
        self
    }

    pub fn with_asset(mut self, asset_info: &AssetInfo) -> Self {
        self.asset_infos.push(asset_info.clone());
        self
    }

    /// Set init_params to None to use the defaults
    pub fn instantiate(
        self,
        init_params: Option<&ConcentratedInjObParams>,
    ) -> MockConcentratedPairInj<B, A, S, C, X, D, I, G> {
        let factory = self
            .factory
            .unwrap_or_else(|| MockFactoryBuilder::new(&self.app).instantiate());

        let config = factory.config();

        if config
            .pair_configs
            .iter()
            .all(|pc| pc.pair_type != PairType::Custom("concentrated_inj_orderbook".to_owned()))
        {
            let code_id = store_code(&self.app);
            self.app
                .borrow_mut()
                .execute_contract(
                    astroport_address(),
                    factory.address.clone(),
                    &astroport::factory::ExecuteMsg::UpdatePairConfig {
                        config: PairConfig {
                            pair_type: PairType::Custom("concentrated_inj_orderbook".to_owned()),
                            code_id,
                            is_disabled: false,
                            total_fee_bps: 30,
                            maker_fee_bps: 3333,
                            is_generator_disabled: false,
                        },
                    },
                    &[],
                )
                .unwrap();
        };

        let astroport = astroport_address();

        let market_id = calc_market_ids(&self.asset_infos).unwrap()[0].clone();

        let default_params = ConcentratedInjObParams {
            main_params: ConcentratedPoolParams {
                amp: Decimal::from_ratio(40u128, 1u128),
                gamma: Decimal::from_ratio(145u128, 1000000u128),
                mid_fee: Decimal::from_ratio(26u128, 10000u128),
                out_fee: Decimal::from_ratio(45u128, 10000u128),
                fee_gamma: Decimal::from_ratio(23u128, 100000u128),
                repeg_profit_threshold: Decimal::from_ratio(2u128, 1000000u128),
                min_price_scale_delta: Decimal::from_ratio(146u128, 1000000u128),
                price_scale: Decimal::one(),
                ma_half_time: 600,
                track_asset_balances: None,
            },
            orderbook_config: OrderbookConfig {
                market_id,
                orders_number: 5,
                min_trades_to_avg: 1,
            },
        };

        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                factory.address.clone(),
                &FactoryExecuteMsg::CreatePair {
                    pair_type: PairType::Custom("concentrated_inj_orderbook".to_owned()),
                    asset_infos: self.asset_infos.to_vec(),
                    init_params: Some(to_binary(init_params.unwrap_or(&default_params)).unwrap()),
                },
                &[],
            )
            .unwrap();

        let res: PairInfo = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                &factory.address,
                &FactoryQueryMsg::Pair {
                    asset_infos: self.asset_infos.to_vec(),
                },
            )
            .unwrap();

        MockConcentratedPairInj {
            app: self.app,
            address: res.contract_addr,
        }
    }
}

pub struct MockConcentratedPairInj<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G> MockConcentratedPairInj<B, A, S, C, X, D, I, G>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module<ExecT = InjectiveMsgWrapper, QueryT = InjectiveQueryWrapper>,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    pub fn lp_token(&self) -> MockToken<B, A, S, C, X, D, I, G> {
        let res: PairInfo = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.to_string(), &QueryMsg::Pair {})
            .unwrap();
        MockToken {
            app: self.app.clone(),
            address: res.liquidity_token,
        }
    }

    pub fn provide(
        &self,
        sender: &Addr,
        assets: &[Asset],
        slippage_tolerance: Option<Decimal>,
        auto_stake: bool,
        receiver: impl Into<Option<String>>,
    ) {
        let xyk = MockXykPair {
            app: self.app.clone(),
            address: self.address.clone(),
        };
        xyk.provide(sender, assets, slippage_tolerance, auto_stake, receiver);
    }

    pub fn mint_allow_provide_and_stake(&self, sender: &Addr, assets: &[Asset]) {
        let xyk = MockXykPair {
            app: self.app.clone(),
            address: self.address.clone(),
        };
        xyk.mint_allow_provide_and_stake(sender, assets);
    }
}
