use std::fmt::Debug;

use astroport::{
    asset::{Asset, AssetInfo, PairInfo},
    pair::{QueryMsg, StablePoolParams},
};
use cosmwasm_std::{Addr, Api, CustomQuery, Decimal, Storage};
use cw_multi_test::{Bank, ContractWrapper, Distribution, Gov, Ibc, Module, Staking, Stargate};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    factory::MockFactoryOpt, MockFactory, MockFactoryBuilder, MockToken, MockXykPair, WKApp,
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
    use astroport_pair_stable as cnt;
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

pub struct MockStablePairBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub asset_infos: Vec<AssetInfo>,
    pub factory: MockFactoryOpt<B, A, S, C, X, D, I, G, T>,
}

impl<B, A, S, C, X, D, I, G, T> MockStablePairBuilder<B, A, S, C, X, D, I, G, T>
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
        Self {
            app: app.clone(),
            asset_infos: Default::default(),
            factory: None,
        }
    }

    pub fn with_factory(mut self, factory: &MockFactory<B, A, S, C, X, D, I, G, T>) -> Self {
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
        params: Option<&StablePoolParams>,
    ) -> MockStablePair<B, A, S, C, X, D, I, G, T> {
        let factory = self
            .factory
            .unwrap_or_else(|| MockFactoryBuilder::new(&self.app).instantiate());

        factory.instantiate_stable_pair(&self.asset_infos, params)
    }
}

pub struct MockStablePair<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G, T> MockStablePair<B, A, S, C, X, D, I, G, T>
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
    pub fn lp_token(&self) -> MockToken<B, A, S, C, X, D, I, G, T> {
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
