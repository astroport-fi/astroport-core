use std::fmt::Debug;

use crate::cw_multi_test::{
    Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking, Stargate,
};
use astroport::{
    asset::AssetInfo,
    token::{BalanceResponse, ExecuteMsg, InstantiateMsg, MinterResponse, QueryMsg},
};
use cosmwasm_std::{Addr, Api, CustomQuery, Storage, Uint128};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{astroport_address, WKApp, ASTROPORT};

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
    use astroport_token as cnt;
    let contract = Box::new(ContractWrapper::new_with_empty(
        cnt::contract::execute,
        cnt::contract::instantiate,
        cnt::contract::query,
    ));

    app.borrow_mut().store_code(contract)
}

pub struct MockTokenBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub symbol: String,
}

impl<B, A, S, C, X, D, I, G, T> MockTokenBuilder<B, A, S, C, X, D, I, G, T>
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
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G, T>, symbol: &str) -> Self {
        Self {
            app: app.clone(),
            symbol: symbol.into(),
        }
    }

    pub fn instantiate(self) -> MockToken<B, A, S, C, X, D, I, G, T> {
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

        MockToken {
            app: self.app,
            address,
        }
    }
}

pub struct MockToken<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

pub type MockTokenOpt<B, A, S, C, X, D, I, G, T> = Option<MockToken<B, A, S, C, X, D, I, G, T>>;

impl<B, A, S, C, X, D, I, G, T> MockToken<B, A, S, C, X, D, I, G, T>
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
    pub fn asset_info(&self) -> AssetInfo {
        AssetInfo::Token {
            contract_addr: self.address.clone(),
        }
    }

    pub fn mint(&self, recipient: &Addr, amount: Uint128) {
        let astroport = astroport_address();
        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                self.address.clone(),
                &ExecuteMsg::Mint {
                    recipient: recipient.into(),
                    amount,
                },
                &[],
            )
            .unwrap();
    }

    pub fn balance(&self, address: &Addr) -> Uint128 {
        let res: BalanceResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                self.address.to_string(),
                &QueryMsg::Balance {
                    address: address.into(),
                },
            )
            .unwrap();

        res.balance
    }

    pub fn burn(&self, sender: &Addr, amount: Uint128) {
        self.app
            .borrow_mut()
            .execute_contract(
                sender.clone(),
                self.address.clone(),
                &ExecuteMsg::Burn { amount },
                &[],
            )
            .unwrap();
    }

    pub fn allow(&self, sender: &Addr, spender: &Addr, amount: Uint128) {
        self.app
            .borrow_mut()
            .execute_contract(
                sender.clone(),
                self.address.clone(),
                &ExecuteMsg::IncreaseAllowance {
                    spender: spender.into(),
                    amount,
                    expires: None,
                },
                &[],
            )
            .unwrap();
    }
}
impl<B, A, S, C, X, D, I, G, T> TryFrom<(&WKApp<B, A, S, C, X, D, I, G, T>, &AssetInfo)>
    for MockToken<B, A, S, C, X, D, I, G, T>
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
    type Error = String;
    fn try_from(
        value: (&WKApp<B, A, S, C, X, D, I, G, T>, &AssetInfo),
    ) -> Result<MockToken<B, A, S, C, X, D, I, G, T>, Self::Error> {
        match value.1 {
            AssetInfo::Token { contract_addr } => Ok(MockToken {
                app: value.0.clone(),
                address: contract_addr.clone(),
            }),
            AssetInfo::NativeToken { denom } => Err(format!("{} is native coin!", denom)),
        }
    }
}
