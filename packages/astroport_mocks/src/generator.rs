use std::fmt::Debug;

use astroport::{
    asset::AssetInfo,
    factory::ExecuteMsg as FactoryExecuteMsg,
    generator::{Config, ExecuteMsg, InstantiateMsg, PendingTokenResponse, QueryMsg},
    token::ExecuteMsg as Cw20ExecuteMsg,
    vesting::{
        Cw20HookMsg as VestingCw20HookMsg, VestingAccount, VestingSchedule, VestingSchedulePoint,
    },
};
use cosmwasm_std::{to_binary, Addr, Api, CustomQuery, Storage, Uint128};
use cw_multi_test::{Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::{
    astroport_address,
    factory::{MockFactory, MockFactoryBuilder},
    MockToken, MockTokenBuilder, MockVestingBuilder, WKApp, ASTROPORT,
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
    use astroport_generator as cnt;
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

pub struct MockGeneratorBuilder<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
}

impl<B, A, S, C, X, D, I, G> MockGeneratorBuilder<B, A, S, C, X, D, I, G>
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
        Self { app: app.clone() }
    }
    pub fn instantiate(self) -> MockGenerator<B, A, S, C, X, D, I, G> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let factory = MockFactoryBuilder::new(&self.app).instantiate();
        let astro_token = MockTokenBuilder::new(&self.app, "ASTRO").instantiate();
        let astro_token_info = astro_token.asset_info();
        let vesting = MockVestingBuilder::new(&self.app)
            .with_astro_token(&astro_token_info)
            .instantiate();

        let start_block = self.app.borrow().block_info().height;
        let whitelist_code_id = factory.whitelist_code_id();
        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport.clone(),
                &InstantiateMsg {
                    owner: ASTROPORT.to_owned(),
                    factory: factory.address.to_string(),
                    guardian: None,
                    astro_token: astro_token_info,
                    start_block: start_block.into(),
                    voting_escrow: None,
                    tokens_per_block: Uint128::new(1_000_000),
                    vesting_contract: vesting.address.to_string(),
                    generator_controller: None,
                    voting_escrow_delegation: None,
                    whitelist_code_id,
                },
                &[],
                "Astroport Generator",
                Some(ASTROPORT.to_owned()),
            )
            .unwrap();

        self.app
            .borrow_mut()
            .execute_contract(
                astroport.clone(),
                factory.address,
                &FactoryExecuteMsg::UpdateConfig {
                    fee_address: None,
                    token_code_id: None,
                    generator_address: Some(address.to_string()),
                    whitelist_code_id: None,
                    coin_registry_address: None,
                },
                &[],
            )
            .unwrap();

        astro_token.mint(&astroport, Uint128::new(1_000_000_000_000));

        let time = self.app.borrow().block_info().time.seconds();
        self.app
            .borrow_mut()
            .execute_contract(
                astroport.clone(),
                astro_token.address,
                &Cw20ExecuteMsg::Send {
                    contract: vesting.address.to_string(),
                    amount: Uint128::new(1_000_000_000_000),
                    msg: to_binary(&VestingCw20HookMsg::RegisterVestingAccounts {
                        vesting_accounts: vec![VestingAccount {
                            address: address.to_string(),
                            schedules: vec![VestingSchedule {
                                start_point: VestingSchedulePoint {
                                    time,
                                    amount: Uint128::new(1_000_000_000_000),
                                },
                                end_point: None,
                            }],
                        }],
                    })
                    .unwrap(),
                },
                &[],
            )
            .unwrap();

        self.app
            .borrow_mut()
            .execute_contract(
                astroport,
                address.clone(),
                &ExecuteMsg::UpdateConfig {
                    vesting_contract: Some(vesting.address.to_string()),
                    generator_controller: None,
                    guardian: None,
                    voting_escrow_delegation: None,
                    voting_escrow: None,
                    checkpoint_generator_limit: None,
                },
                &[],
            )
            .unwrap();

        MockGenerator {
            app: self.app,
            address,
        }
    }
}

pub struct MockGenerator<B, A, S, C: Module, X, D, I, G> {
    pub app: WKApp<B, A, S, C, X, D, I, G>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G> MockGenerator<B, A, S, C, X, D, I, G>
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
    pub fn factory(&self) -> MockFactory<B, A, S, C, X, D, I, G> {
        let res: Config = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(&self.address, &QueryMsg::Config {})
            .unwrap();

        MockFactory {
            app: self.app.clone(),
            address: res.factory,
        }
    }
    pub fn astro_token_info(&self) -> AssetInfo {
        let res: Config = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
            .unwrap();

        res.astro_token
    }

    pub fn query_deposit(
        &self,
        lp_token: &MockToken<B, A, S, C, X, D, I, G>,
        user: &Addr,
    ) -> Uint128 {
        self.app
            .borrow()
            .wrap()
            .query_wasm_smart(
                self.address.to_string(),
                &QueryMsg::Deposit {
                    lp_token: lp_token.address.to_string(),
                    user: user.into(),
                },
            )
            .unwrap()
    }

    pub fn setup_pools(&mut self, pools: &[(String, Uint128)]) {
        self.app
            .borrow_mut()
            .execute_contract(
                astroport_address(),
                self.address.clone(),
                &ExecuteMsg::SetupPools {
                    pools: pools.to_vec(),
                },
                &[],
            )
            .unwrap();
    }

    pub fn set_tokens_per_block(&mut self, amount: Uint128) {
        self.app
            .borrow_mut()
            .execute_contract(
                astroport_address(),
                self.address.clone(),
                &ExecuteMsg::SetTokensPerBlock { amount },
                &[],
            )
            .unwrap();
    }

    pub fn pending_token(&self, lp_token: &Addr, user: &Addr) -> PendingTokenResponse {
        let res: PendingTokenResponse = self
            .app
            .borrow()
            .wrap()
            .query_wasm_smart(
                self.address.clone(),
                &QueryMsg::PendingToken {
                    lp_token: lp_token.into(),
                    user: user.into(),
                },
            )
            .unwrap();

        res
    }
}
