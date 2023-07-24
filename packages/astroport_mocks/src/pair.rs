use std::fmt::Debug;

use astroport::{
    asset::{Asset, AssetInfo, PairInfo},
    pair::{ExecuteMsg, QueryMsg},
};
use cosmwasm_std::{Addr, Api, Coin, CustomQuery, Decimal, Storage};
use cw_multi_test::{Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    factory::{MockFactory, MockFactoryOpt},
    MockFactoryBuilder, MockToken, WKApp,
};

pub fn store_code<B, A, S, C, X, D, I, G>(app: &WKApp<B, A, S, C, X, D, I, G>) -> u64
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
{
    use astroport_pair as cnt;
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
pub struct MockXykPairBuilder<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub asset_infos: Vec<AssetInfo>,
    pub factory: MockFactoryOpt<B, A, S, C, X, D, I, G>,
}

impl<B, A, S, C: Module, X, D, I, G> MockXykPairBuilder<B, A, S, C, X, D, I, G>
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

    pub fn instantiate(self) -> MockXykPair<B, A, S, C, X, D, I, G> {
        let factory = self
            .factory
            .unwrap_or_else(|| MockFactoryBuilder::new(&self.app).instantiate());

        factory.instantiate_xyk_pair(&self.asset_infos)
    }
}

#[derive(Clone)]
pub struct MockXykPair<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G> MockXykPair<B, A, S, C, X, D, I, G>
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
        let coins: Vec<Coin> = assets
            .iter()
            .filter_map(|a| match &a.info {
                AssetInfo::Token { .. } => None,
                AssetInfo::NativeToken { denom } => Some(Coin {
                    denom: denom.clone(),
                    amount: a.amount,
                }),
            })
            .collect();

        self.app
            .borrow_mut()
            .execute_contract(
                sender.clone(),
                self.address.clone(),
                &ExecuteMsg::ProvideLiquidity {
                    assets: assets.into(),
                    slippage_tolerance,
                    auto_stake: Some(auto_stake),
                    receiver: receiver.into(),
                },
                &coins,
            )
            .unwrap();
    }

    pub fn mint_allow_provide_and_stake(&self, sender: &Addr, assets: &[Asset]) {
        for asset in assets {
            if let AssetInfo::Token { contract_addr } = &asset.info {
                let token = MockToken {
                    app: self.app.clone(),
                    address: contract_addr.clone(),
                };
                token.mint(sender, asset.amount);
                token.allow(sender, &self.address, asset.amount);
            };
        }
        self.provide(sender, assets, None, true, None)
    }
}
