#![allow(dead_code)]

use anyhow::Result as AnyResult;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coins, Addr, Binary, Deps, DepsMut, Empty, Env, GovMsg, IbcMsg, IbcQuery, MemoryStorage,
    MessageInfo, Response, StdResult,
};
use cw_multi_test::{
    App, BankKeeper, BasicAppBuilder, Contract, ContractWrapper, DistributionKeeper, Executor,
    FailingModule, StakeKeeper, WasmKeeper,
};

use astroport::staking::{InstantiateMsg, QueryMsg, TrackerData};

use crate::common::neutron_ext::{NeutronStargate, TOKEN_FACTORY_MODULE};

fn staking_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_staking::contract::execute,
            astroport_staking::contract::instantiate,
            astroport_staking::contract::query,
        )
        .with_reply_empty(astroport_staking::contract::reply),
    )
}

fn tracker_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            |_: DepsMut, _: Env, _: MessageInfo, _: Empty| -> StdResult<Response> {
                unimplemented!()
            },
            astroport_tokenfactory_tracker::contract::instantiate,
            |_: Deps, _: Env, _: Empty| -> StdResult<Binary> { unimplemented!() },
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    )
}

pub type NeutronApp = App<
    BankKeeper,
    MockApi,
    MemoryStorage,
    FailingModule<Empty, Empty, Empty>,
    WasmKeeper<Empty, Empty>,
    StakeKeeper,
    DistributionKeeper,
    FailingModule<IbcMsg, IbcQuery, Empty>,
    FailingModule<GovMsg, Empty, Empty>,
    NeutronStargate,
>;

pub struct Helper {
    pub app: NeutronApp,
    pub owner: Addr,
    pub staking: Addr,
    pub tracker_addr: String,
}

pub const ASTRO_DENOM: &str = "factory/assembly/ASTRO";

impl Helper {
    pub fn new(owner: &Addr) -> AnyResult<Self> {
        let mut app = BasicAppBuilder::new()
            .with_stargate(NeutronStargate::new())
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, owner, coins(u128::MAX, ASTRO_DENOM))
                    .unwrap()
            });

        let staking_code_id = app.store_code(staking_contract());
        let tracker_code_id = app.store_code(tracker_contract());

        let msg = InstantiateMsg {
            deposit_token_denom: ASTRO_DENOM.to_string(),
            tracking_admin: owner.to_string(),
            tracking_code_id: tracker_code_id,
            token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
        };
        let staking = app
            .instantiate_contract(
                staking_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("Astroport Staking"),
                None,
            )
            .unwrap();

        let TrackerData { tracker_addr, .. } = app
            .wrap()
            .query_wasm_smart(&staking, &QueryMsg::TrackerConfig {})
            .unwrap();

        Ok(Self {
            app,
            owner: owner.clone(),
            staking,
            tracker_addr,
        })
    }

    pub fn give_astro(&mut self, amount: u128, recipient: &Addr) {
        self.app
            .send_tokens(
                self.owner.clone(),
                recipient.clone(),
                &coins(amount, ASTRO_DENOM),
            )
            .unwrap();
    }
}

pub trait AppExtension {
    fn next_block(&mut self, time: u64);
}

impl AppExtension for NeutronApp {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}
