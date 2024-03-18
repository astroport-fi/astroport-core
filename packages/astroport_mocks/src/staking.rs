use std::fmt::Debug;

use astroport::{
    staking::{ConfigResponse, Cw20HookMsg, InstantiateMsg, QueryMsg},
    token::ExecuteMsg,
};
use cosmwasm_std::{to_json_binary, Addr, Api, CustomQuery, Storage, Uint128};
use cw_multi_test::{
    Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking, Stargate,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    astroport_address, token::MockTokenOpt, MockToken, MockTokenBuilder, WKApp, ASTROPORT,
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
    use astroport_staking as cnt;
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

pub struct MockStakingBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub astro_token: MockTokenOpt<B, A, S, C, X, D, I, G, T>,
}

impl<B, A, S, C, X, D, I, G, T> MockStakingBuilder<B, A, S, C, X, D, I, G, T>
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
            astro_token: None,
        }
    }

    pub fn with_astro_token(mut self, astro_token: &MockToken<B, A, S, C, X, D, I, G, T>) -> Self {
        self.astro_token = Some(MockToken {
            app: self.app.clone(),
            address: astro_token.address.clone(),
        });
        self
    }

    pub fn instantiate(self) -> MockStaking<B, A, S, C, X, D, I, G, T> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let astro_token = self
            .astro_token
            .unwrap_or_else(|| MockTokenBuilder::new(&self.app, "ASTRO").instantiate());

        let token_code_id = crate::xastro::store_code(&self.app);

        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport,
                &InstantiateMsg {
                    owner: ASTROPORT.to_owned(),
                    marketing: None,
                    token_code_id,
                    deposit_token_addr: astro_token.address.to_string(),
                },
                &[],
                "Astroport Staking",
                Some(ASTROPORT.to_string()),
            )
            .unwrap();

        MockStaking {
            app: self.app,
            address,
        }
    }
}

pub struct MockStaking<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G, T> MockStaking<B, A, S, C, X, D, I, G, T>
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
    pub fn astro_token(&self) -> MockToken<B, A, S, C, X, D, I, G, T> {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.to_string(), &QueryMsg::Config {})
            .unwrap();

        MockToken {
            app: self.app.clone(),
            address: config.deposit_token_addr,
        }
    }

    pub fn enter(&self, sender: &Addr, amount: Uint128) {
        let astro_token = self.astro_token();
        self.app
            .borrow_mut()
            .execute_contract(
                sender.clone(),
                astro_token.address,
                &ExecuteMsg::Send {
                    amount,
                    msg: to_json_binary(&Cw20HookMsg::Enter {}).unwrap(),
                    contract: self.address.to_string(),
                },
                &[],
            )
            .unwrap();
    }

    pub fn xastro_token(&self) -> MockToken<B, A, S, C, X, D, I, G, T> {
        let config: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.to_string(), &QueryMsg::Config {})
            .unwrap();

        MockToken {
            app: self.app.clone(),
            address: config.share_token_addr,
        }
    }

    pub fn leave(&self, sender: &Addr, amount: Uint128) {
        let xastro_token = self.xastro_token();
        self.app
            .borrow_mut()
            .execute_contract(
                sender.clone(),
                xastro_token.address,
                &ExecuteMsg::Send {
                    amount,
                    msg: to_json_binary(&Cw20HookMsg::Leave {}).unwrap(),
                    contract: self.address.to_string(),
                },
                &[],
            )
            .unwrap();
    }
}
