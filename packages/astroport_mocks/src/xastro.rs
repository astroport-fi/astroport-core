use std::fmt::Debug;

use astroport::{
    asset::AssetInfo,
    token::{InstantiateMsg, MinterResponse},
};
use cosmwasm_std::{Addr, Api, CustomQuery, Storage};
use cw_multi_test::{Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{astroport_address, MockToken, WKApp, ASTROPORT};

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
    use astroport_xastro_token as cnt;
    let contract = Box::new(ContractWrapper::new_with_empty(
        cnt::contract::execute,
        cnt::contract::instantiate,
        cnt::contract::query,
    ));

    app.borrow_mut().store_code(contract)
}

pub struct MockXastroBuilder<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub symbol: String,
}

impl<B, A, S, C, X, D, I, G> MockXastroBuilder<B, A, S, C, X, D, I, G>
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
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G>, symbol: &str) -> Self {
        Self {
            app: app.clone(),
            symbol: symbol.into(),
        }
    }

    pub fn instantiate(self) -> MockXastro<B, A, S, C, X, D, I, G> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport,
                &InstantiateMsg {
                    name: self.symbol.clone(),
                    mint: Some(MinterResponse {
                        minter: ASTROPORT.to_owned(),
                        cap: None,
                    }),
                    symbol: self.symbol.clone(),
                    decimals: 6,
                    marketing: None,
                    initial_balances: vec![],
                },
                &[],
                self.symbol,
                Some(ASTROPORT.to_owned()),
            )
            .unwrap();

        MockXastro {
            app: self.app.clone(),
            address: address.clone(),
            token: MockToken {
                app: self.app,
                address,
            },
        }
    }
}

pub struct MockXastro<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub address: Addr,
    pub token: MockToken<B, A, S, C, X, D, I, G>,
}

impl<B, A, S, C, X, D, I, G> TryFrom<(WKApp<B, A, S, C, X, D, I, G>, &AssetInfo)>
    for MockXastro<B, A, S, C, X, D, I, G>
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
    type Error = String;
    fn try_from(
        value: (WKApp<B, A, S, C, X, D, I, G>, &AssetInfo),
    ) -> Result<MockXastro<B, A, S, C, X, D, I, G>, Self::Error> {
        match value.1 {
            AssetInfo::Token { contract_addr } => Ok(MockXastro {
                app: value.0.clone(),
                address: contract_addr.clone(),
                token: MockToken {
                    app: value.0,
                    address: contract_addr.clone(),
                },
            }),
            AssetInfo::NativeToken { denom } => Err(format!("{} is native coin!", denom)),
        }
    }
}
