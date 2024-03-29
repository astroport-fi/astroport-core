use crate::{astroport_address, MockTokenBuilder, WKApp, ASTROPORT};
use astroport::{
    asset::AssetInfo,
    vesting::QueryMsg,
    vesting::{ConfigResponse, InstantiateMsg},
};
use cosmwasm_std::{Addr, Api, CustomMsg, CustomQuery, Storage};
use cw_multi_test::{
    Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking, Stargate,
};
use serde::de::DeserializeOwned;

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
    T: Stargate,
    C::ExecT: CustomMsg + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    use astroport_vesting as cnt;
    let contract = Box::new(ContractWrapper::new_with_empty(
        cnt::contract::execute,
        cnt::contract::instantiate,
        cnt::contract::query,
    ));

    app.borrow_mut().store_code(contract)
}

pub struct MockVestingBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub astro_token: Option<AssetInfo>,
}

impl<B, A, S, C, X, D, I, G, T> MockVestingBuilder<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    T: Stargate,
    C::ExecT: CustomMsg + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G, T>) -> Self {
        Self {
            app: app.clone(),
            astro_token: None,
        }
    }

    pub fn with_astro_token(mut self, astro_token: &AssetInfo) -> Self {
        self.astro_token = Some(astro_token.clone());
        self
    }

    pub fn instantiate(self) -> MockVesting<B, A, S, C, X, D, I, G, T> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let astro_token = self.astro_token.unwrap_or_else(|| {
            MockTokenBuilder::new(&self.app, "ASTRO")
                .instantiate()
                .asset_info()
        });

        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport,
                &InstantiateMsg {
                    owner: ASTROPORT.to_owned(),
                    vesting_token: astro_token,
                },
                &[],
                "Astroport Vesting",
                Some(ASTROPORT.to_owned()),
            )
            .unwrap();

        MockVesting {
            app: self.app,
            address,
        }
    }
}

pub struct MockVesting<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G, T> MockVesting<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    T: Stargate,
    C::ExecT: CustomMsg + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    pub fn vesting_token_info(&self) -> AssetInfo {
        let res: ConfigResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        res.vesting_token
    }
}
