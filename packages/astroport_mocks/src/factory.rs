use anyhow::Result as AnyResult;
use std::fmt::Debug;

use astroport::{
    asset::{AssetInfo, PairInfo},
    factory::{ConfigResponse, ExecuteMsg, InstantiateMsg, PairConfig, PairType, QueryMsg},
    pair::StablePoolParams,
    pair_concentrated::ConcentratedPoolParams,
};
use cosmwasm_std::{to_json_binary, Addr, Api, CustomQuery, Decimal, Storage};
use cw_multi_test::{
    AppResponse, Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking, Stargate,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    astroport_address, MockCoinRegistry, MockCoinRegistryBuilder, MockConcentratedPair,
    MockStablePair, MockXykPair, WKApp, ASTROPORT,
};

pub fn store_code<B, A, S, C, X, D, I, G, T>(app: &WKApp<B, A, S, C, X, D, I, G, T>) -> u64
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    use astroport_factory as cnt;
    let contract = Box::new(
        ContractWrapper::new_with_empty(
            cnt::contract::execute,
            cnt::contract::instantiate,
            cnt::contract::query,
        )
        .with_reply_empty(cnt::contract::reply),
    );

    app.borrow_mut().store_code(contract)
}

pub struct MockFactoryBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
}

impl<B, A, S, C, X, D, I, G, T> MockFactoryBuilder<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G, T>) -> Self {
        Self { app: app.clone() }
    }

    pub fn instantiate(self) -> MockFactory<B, A, S, C, X, D, I, G, T> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let xyk_code_id = crate::pair::store_code(&self.app);
        let stable_code_id = crate::pair_stable::store_code(&self.app);
        let concentrated_code_id = crate::pair_concentrated::store_code(&self.app);

        let pair_configs = vec![
            PairConfig {
                code_id: xyk_code_id,
                pair_type: PairType::Xyk {},
                is_disabled: false,
                is_generator_disabled: false,
                total_fee_bps: 30,
                maker_fee_bps: 3333,
                permissioned: false,
            },
            PairConfig {
                code_id: stable_code_id,
                pair_type: PairType::Stable {},
                is_disabled: false,
                is_generator_disabled: false,
                total_fee_bps: 5,
                maker_fee_bps: 5000,
                permissioned: false,
            },
            PairConfig {
                code_id: concentrated_code_id,
                pair_type: PairType::Custom("concentrated".to_owned()),
                is_disabled: false,
                is_generator_disabled: false,
                total_fee_bps: 30,
                maker_fee_bps: 3333,
                permissioned: false,
            },
        ];

        let token_code_id = crate::token::store_code(&self.app);
        let whitelist_code_id = crate::whitelist::store_code(&self.app);

        let coin_registry = MockCoinRegistryBuilder::new(&self.app).instantiate();

        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport,
                &InstantiateMsg {
                    owner: ASTROPORT.to_owned(),
                    fee_address: None,
                    pair_configs,
                    token_code_id,
                    generator_address: None,
                    whitelist_code_id,
                    coin_registry_address: coin_registry.address.to_string(),
                },
                &[],
                "Astroport Factory",
                Some(ASTROPORT.to_owned()),
            )
            .unwrap();

        MockFactory {
            app: self.app,
            address,
        }
    }
}

pub struct MockFactory<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

pub type MockFactoryOpt<B, A, S, C, X, D, I, G, T> = Option<MockFactory<B, A, S, C, X, D, I, G, T>>;

impl<B, A, S, C, X, D, I, G, T> MockFactory<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    pub fn whitelist_code_id(&self) -> u64 {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        config.whitelist_code_id
    }

    pub fn token_code_id(&self) -> u64 {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        config.token_code_id
    }

    pub fn instantiate_xyk_pair(
        &self,
        asset_infos: &[AssetInfo],
    ) -> MockXykPair<B, A, S, C, X, D, I, G, T> {
        let astroport = astroport_address();

        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                self.address.clone(),
                &ExecuteMsg::CreatePair {
                    pair_type: PairType::Xyk {},
                    asset_infos: asset_infos.to_vec(),
                    init_params: None,
                },
                &[],
            )
            .unwrap();

        let res: PairInfo = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                &self.address,
                &QueryMsg::Pair {
                    asset_infos: asset_infos.to_vec(),
                },
            )
            .unwrap();

        MockXykPair {
            app: self.app.clone(),
            address: res.contract_addr,
        }
    }

    /// Set init_params to None to use the defaults
    pub fn instantiate_stable_pair(
        &self,
        asset_infos: &[AssetInfo],
        init_params: Option<&StablePoolParams>,
    ) -> MockStablePair<B, A, S, C, X, D, I, G, T> {
        let astroport = astroport_address();

        let default_params = StablePoolParams {
            amp: 100,
            owner: Some(astroport.to_string()),
        };

        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                self.address.clone(),
                &ExecuteMsg::CreatePair {
                    pair_type: PairType::Stable {},
                    asset_infos: asset_infos.to_vec(),
                    init_params: Some(
                        to_json_binary(init_params.unwrap_or(&default_params)).unwrap(),
                    ),
                },
                &[],
            )
            .unwrap();

        let res: PairInfo = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                &self.address,
                &QueryMsg::Pair {
                    asset_infos: asset_infos.to_vec(),
                },
            )
            .unwrap();

        MockStablePair {
            app: self.app.clone(),
            address: res.contract_addr,
        }
    }

    pub fn coin_registry(&self) -> MockCoinRegistry<B, A, S, C, X, D, I, G, T> {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        MockCoinRegistry {
            app: self.app.clone(),
            address: config.coin_registry_address,
        }
    }

    /// Set init_params to None to use the defaults
    pub fn instantiate_concentrated_pair(
        &self,
        asset_infos: &[AssetInfo],
        init_params: Option<&ConcentratedPoolParams>,
    ) -> MockConcentratedPair<B, A, S, C, X, D, I, G, T> {
        let astroport = astroport_address();

        let default_params = ConcentratedPoolParams {
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
            fee_share: None,
        };

        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                self.address.clone(),
                &ExecuteMsg::CreatePair {
                    pair_type: PairType::Custom("concentrated".to_owned()),
                    asset_infos: asset_infos.to_vec(),
                    init_params: Some(
                        to_json_binary(init_params.unwrap_or(&default_params)).unwrap(),
                    ),
                },
                &[],
            )
            .unwrap();

        let res: PairInfo = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                &self.address,
                &QueryMsg::Pair {
                    asset_infos: asset_infos.to_vec(),
                },
            )
            .unwrap();

        MockConcentratedPair {
            app: self.app.clone(),
            address: res.contract_addr,
        }
    }

    pub fn deregister_pair(&self, asset_infos: &[AssetInfo]) -> AnyResult<AppResponse> {
        let astroport = astroport_address();

        self.app.borrow_mut().execute_contract(
            astroport,
            self.address.clone(),
            &ExecuteMsg::Deregister {
                asset_infos: asset_infos.to_vec(),
            },
            &[],
        )
    }

    pub fn config(&self) -> ConfigResponse {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        config
    }
}
