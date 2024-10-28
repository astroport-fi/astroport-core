#![cfg(not(tarpaulin_include))]
#![allow(dead_code)]

use anyhow::Result as AnyResult;
use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, Coin, DepsMut, Empty, Env, MessageInfo, Response, StdResult,
    Uint128,
};
use derivative::Derivative;
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::pair::{
    ConfigResponse, ExecuteMsg, PoolResponse, QueryMsg, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport::pair_xastro::XastroPairInitParams;
use astroport::staking;
use astroport::staking::InstantiateMsg;
use astroport_pair_xastro::contract::{execute, instantiate};
use astroport_pair_xastro::queries::query;
use astroport_test::coins::TestCoin;
use astroport_test::cw_multi_test::{
    no_init, AppBuilder, AppResponse, BankSudo, Contract, ContractWrapper, Executor,
    TOKEN_FACTORY_MODULE,
};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};

const INIT_BALANCE: u128 = u128::MAX;

pub fn init_native_coins(test_coins: &[TestCoin]) -> Vec<Coin> {
    let test_coins: Vec<Coin> = test_coins
        .iter()
        .unique()
        .filter_map(|test_coin| match test_coin {
            TestCoin::Native(name) => {
                let init_balance = INIT_BALANCE;
                Some(coin(init_balance, name))
            }
            _ => None,
        })
        .collect();

    test_coins
}

fn pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

fn factory_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(astroport_factory::contract::reply),
    )
}

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
            astroport_tokenfactory_tracker::query::query,
        )
        .with_sudo_empty(astroport_tokenfactory_tracker::contract::sudo),
    )
}

pub const ASTRO_DENOM: &str = "factory/assembly/ASTRO";

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: TestApp,
    pub owner: Addr,
    pub factory: Addr,
    pub pair_addr: Addr,
    pub fake_maker: Addr,
    pub xastro_denom: String,
}

impl Helper {
    pub fn new(owner: &Addr) -> AnyResult<Self> {
        let mut app = AppBuilder::new_custom()
            .with_stargate(MockStargate::default())
            .build(no_init);

        app.sudo(
            BankSudo::Mint {
                to_address: owner.to_string(),
                amount: coins(1_100_000_000_000000, ASTRO_DENOM),
            }
            .into(),
        )
        .unwrap();

        let pair_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let pair_type = PairType::Custom("pair_xastro".to_string());

        let fake_maker = Addr::unchecked("fake_maker");

        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: Some(fake_maker.to_string()),
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                maker_fee_bps: 0,
                total_fee_bps: 0,
                pair_type: pair_type.clone(),
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: true,
            }],
            token_code_id: 0,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: "coin_registry".to_string(),
            tracker_config: None,
        };

        let factory = app.instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "factory label",
            None,
        )?;

        let tracker_code_id = app.store_code(tracker_contract());
        let staking_code_id = app.store_code(staking_contract());
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

        let staking::Config { xastro_denom, .. } = app
            .wrap()
            .query_wasm_smart(&staking, &staking::QueryMsg::Config {})
            .unwrap();

        let asset_infos = vec![
            AssetInfo::native(ASTRO_DENOM),
            AssetInfo::native(&xastro_denom),
        ];
        let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.clone(),
            init_params: Some(
                to_json_binary(&XastroPairInitParams {
                    staking: staking.to_string(),
                })
                .unwrap(),
            ),
        };

        app.execute_contract(owner.clone(), factory.clone(), &init_pair_msg, &[])?;

        let resp: PairInfo = app.wrap().query_wasm_smart(
            &factory,
            &astroport::factory::QueryMsg::Pair { asset_infos },
        )?;

        Ok(Self {
            app,
            owner: owner.clone(),
            factory,
            pair_addr: resp.contract_addr,
            fake_maker,
            xastro_denom,
        })
    }

    pub fn provide_liquidity(&mut self, sender: &Addr, assets: &[Asset]) -> AnyResult<AppResponse> {
        let funds =
            assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
            min_lp_to_receive: None,
        };

        self.app
            .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
    }

    pub fn swap(
        &mut self,
        sender: &Addr,
        offer_asset: &Asset,
        ask_asset_info: Option<AssetInfo>,
        to: Option<String>,
    ) -> AnyResult<AppResponse> {
        match &offer_asset.info {
            AssetInfo::Token { .. } => {
                unimplemented!()
            }
            AssetInfo::NativeToken { .. } => {
                let funds = offer_asset.mock_coin_sent(
                    &mut self.app,
                    sender,
                    &self.pair_addr,
                    SendType::None,
                );

                let msg = ExecuteMsg::Swap {
                    offer_asset: offer_asset.clone(),
                    ask_asset_info,
                    belief_price: None,
                    max_spread: None,
                    to,
                };

                self.app
                    .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
            }
        }
    }

    pub fn simulate_swap(
        &self,
        offer_asset: &Asset,
        ask_asset_info: Option<AssetInfo>,
    ) -> StdResult<SimulationResponse> {
        self.app.wrap().query_wasm_smart(
            &self.pair_addr,
            &QueryMsg::Simulation {
                offer_asset: offer_asset.clone(),
                ask_asset_info,
            },
        )
    }

    pub fn simulate_reverse_swap(
        &self,
        ask_asset: &Asset,
        offer_asset_info: Option<AssetInfo>,
    ) -> StdResult<ReverseSimulationResponse> {
        self.app.wrap().query_wasm_smart(
            &self.pair_addr,
            &QueryMsg::ReverseSimulation {
                ask_asset: ask_asset.clone(),
                offer_asset_info,
            },
        )
    }

    pub fn native_balance(&self, denom: impl Into<String>, user: &Addr) -> u128 {
        self.app
            .wrap()
            .query_balance(user, denom)
            .unwrap()
            .amount
            .u128()
    }

    pub fn mint_tokens(&mut self, user: &Addr, coins: &[Coin]) -> AnyResult<AppResponse> {
        self.app.sudo(
            BankSudo::Mint {
                to_address: user.to_string(),
                amount: coins.to_vec(),
            }
            .into(),
        )
    }

    pub fn query_config(&self) -> StdResult<ConfigResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Config {})
    }

    pub fn query_pool(&self) -> StdResult<PoolResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Pool {})
    }

    pub fn query_share(&self, amount: impl Into<Uint128>) -> StdResult<Vec<Asset>> {
        self.app.wrap().query_wasm_smart::<Vec<Asset>>(
            &self.pair_addr,
            &QueryMsg::Share {
                amount: amount.into(),
            },
        )
    }
}

#[derive(Clone, Copy)]
pub enum SendType {
    Allowance,
    Transfer,
    None,
}

pub trait AssetExt {
    fn mock_coin_sent(
        &self,
        app: &mut TestApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetExt for Asset {
    fn mock_coin_sent(
        &self,
        _app: &mut TestApp,
        _user: &Addr,
        _spender: &Addr,
        _typ: SendType,
    ) -> Vec<Coin> {
        let mut funds = vec![];
        match &self.info {
            AssetInfo::Token { .. } if !self.amount.is_zero() => {
                unimplemented!()
            }
            AssetInfo::NativeToken { denom } if !self.amount.is_zero() => {
                funds = vec![coin(self.amount.u128(), denom)];
            }
            _ => {}
        }

        funds
    }
}

pub trait AssetsExt {
    fn mock_coins_sent(
        &self,
        app: &mut TestApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetsExt for &[Asset] {
    fn mock_coins_sent(
        &self,
        app: &mut TestApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin> {
        let mut funds = vec![];
        for asset in self.iter() {
            funds.extend(asset.mock_coin_sent(app, user, spender, typ));
        }
        funds
    }
}
