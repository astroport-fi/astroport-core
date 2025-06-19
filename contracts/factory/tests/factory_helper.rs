#![cfg(not(tarpaulin_include))]

use anyhow::Result as AnyResult;
use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{Config, ConfigResponse, PairConfig, PairType, TrackerConfig};
use astroport_test::cw_multi_test::{AppResponse, ContractWrapper, Executor};
use astroport_test::modules::stargate::StargateApp as TestApp;

use cosmwasm_std::{Addr, Binary, StdResult};
use cw20::MinterResponse;

pub struct FactoryHelper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub factory: Addr,
    pub cw20_token_code_id: u64,
}

impl FactoryHelper {
    pub fn init(router: &mut TestApp, owner: &Addr) -> Self {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        ));

        let cw20_token_code_id = router.store_code(astro_token_contract);

        let msg = astroport::token::InstantiateMsg {
            name: String::from("Astro token"),
            symbol: String::from("ASTRO"),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: owner.to_string(),
                cap: None,
            }),
            marketing: None,
        };

        let astro_token = router
            .instantiate_contract(
                cw20_token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                None,
            )
            .unwrap();

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair::contract::execute,
                astroport_pair::contract::instantiate,
                astroport_pair::contract::query,
            )
            .with_reply_empty(astroport_pair::contract::reply),
        );

        let pair_code_id = router.store_code(pair_contract);

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );

        let factory_code_id = router.store_code(factory_contract);

        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![
                PairConfig {
                    code_id: pair_code_id,
                    pair_type: PairType::Xyk {},
                    total_fee_bps: 100,
                    maker_fee_bps: 10,
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                    whitelist: None,
                },
                PairConfig {
                    code_id: pair_code_id,
                    pair_type: PairType::Custom("transmuter".to_string()),
                    total_fee_bps: 0,
                    maker_fee_bps: 0,
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: true,
                    whitelist: None,
                },
            ],
            token_code_id: cw20_token_code_id,
            fee_address: None,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: "coin_registry".to_string(),
            tracker_config: None,
        };

        let factory = router
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                None,
            )
            .unwrap();

        Self {
            owner: owner.clone(),
            astro_token,
            factory,
            cw20_token_code_id,
        }
    }

    pub fn update_config(
        &mut self,
        router: &mut TestApp,
        sender: &Addr,
        token_code_id: Option<u64>,
        fee_address: Option<String>,
        generator_address: Option<String>,
        whitelist_code_id: Option<u64>,
        coin_registry_address: Option<String>,
    ) -> AnyResult<AppResponse> {
        let msg = astroport::factory::ExecuteMsg::UpdateConfig {
            token_code_id,
            fee_address,
            generator_address,
            whitelist_code_id,
            coin_registry_address,
        };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }

    pub fn create_pair(
        &mut self,
        router: &mut TestApp,
        sender: &Addr,
        pair_type: PairType,
        tokens: [&Addr; 2],
        init_params: Option<Binary>,
    ) -> AnyResult<AppResponse> {
        let asset_infos = vec![
            AssetInfo::Token {
                contract_addr: tokens[0].clone(),
            },
            AssetInfo::Token {
                contract_addr: tokens[1].clone(),
            },
        ];

        let msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos,
            init_params,
        };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }

    pub fn update_tracker_config(
        &mut self,
        router: &mut TestApp,
        sender: &Addr,
        tracker_code_id: u64,
        token_factory_addr: Option<String>,
    ) -> AnyResult<AppResponse> {
        let msg = astroport::factory::ExecuteMsg::UpdateTrackerConfig {
            tracker_code_id,
            token_factory_addr,
        };

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])
    }

    pub fn query_tracker_config(&mut self, router: &mut TestApp) -> StdResult<TrackerConfig> {
        let msg = astroport::factory::QueryMsg::TrackerConfig {};
        router
            .wrap()
            .query_wasm_smart::<TrackerConfig>(self.factory.clone(), &msg)
    }

    pub fn query_config(&mut self, router: &mut TestApp) -> StdResult<ConfigResponse> {
        let msg = astroport::factory::QueryMsg::Config {};
        router
            .wrap()
            .query_wasm_smart::<ConfigResponse>(self.factory.clone(), &msg)
    }
}

pub fn instantiate_token(
    app: &mut TestApp,
    token_code_id: u64,
    owner: &Addr,
    token_name: &str,
    decimals: Option<u8>,
) -> Addr {
    let init_msg = astroport::token::InstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: decimals.unwrap_or(6),
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        owner.clone(),
        &init_msg,
        &[],
        token_name,
        None,
    )
    .unwrap()
}
