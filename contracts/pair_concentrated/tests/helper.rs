#![cfg(not(tarpaulin_include))]
#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::Result as AnyResult;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, from_json, to_json_binary, Addr, Coin, Decimal, Decimal256, DepsMut, Empty, Env,
    MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg};
use derivative::Derivative;
use itertools::Itertools;

use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::observation::OracleObservation;
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, PoolResponse,
    ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair_concentrated::{
    ConcentratedPoolConfig, ConcentratedPoolParams, ConcentratedPoolUpdateParams, QueryMsg,
};
use astroport_pair_concentrated::contract::{execute, instantiate, reply};
use astroport_pair_concentrated::queries::query;
use astroport_pcl_common::state::Config;

use astroport_test::coins::TestCoin;
use astroport_test::convert::f64_to_dec;
use astroport_test::cw_multi_test::{
    AppBuilder, AppResponse, Contract, ContractWrapper, Executor, TOKEN_FACTORY_MODULE,
};
use astroport_test::modules::stargate::{MockStargate, StargateApp as TestApp};

const INIT_BALANCE: u128 = u128::MAX;

pub fn common_pcl_params() -> ConcentratedPoolParams {
    ConcentratedPoolParams {
        amp: f64_to_dec(40f64),
        gamma: f64_to_dec(0.000145),
        mid_fee: f64_to_dec(0.0026),
        out_fee: f64_to_dec(0.0045),
        fee_gamma: f64_to_dec(0.00023),
        repeg_profit_threshold: f64_to_dec(0.000002),
        min_price_scale_delta: f64_to_dec(0.000146),
        price_scale: Decimal::one(),
        ma_half_time: 600,
        track_asset_balances: None,
        fee_share: None,
    }
}

#[cw_serde]
pub struct AmpGammaResponse {
    pub amp: Decimal,
    pub gamma: Decimal,
    pub future_time: u64,
}

pub fn init_native_coins(test_coins: &[TestCoin]) -> Vec<Coin> {
    let mut test_coins: Vec<Coin> = test_coins
        .iter()
        .filter_map(|test_coin| match test_coin {
            TestCoin::Native(name) => {
                let init_balance = INIT_BALANCE;
                Some(coin(init_balance, name))
            }
            _ => None,
        })
        .collect();
    test_coins.push(coin(INIT_BALANCE, "random-coin"));

    test_coins
}

fn token_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        cw20_base::contract::execute,
        cw20_base::contract::instantiate,
        cw20_base::contract::query,
    ))
}

fn pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query).with_reply_empty(reply))
}

fn coin_registry_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ))
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
fn generator() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_incentives::execute::execute,
        astroport_incentives::instantiate::instantiate,
        astroport_incentives::query::query,
    ))
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: TestApp,
    pub owner: Addr,
    pub assets: HashMap<TestCoin, AssetInfo>,
    pub factory: Addr,
    pub pair_addr: Addr,
    pub lp_token: String,
    pub fake_maker: Addr,
    pub generator: Addr,
}

impl Helper {
    pub fn new(
        owner: &Addr,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
    ) -> AnyResult<Self> {
        let mut app = AppBuilder::new_custom()
            .with_stargate(MockStargate::default())
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, owner, init_native_coins(&test_coins))
                    .unwrap()
            });

        let token_code_id = app.store_code(token_contract());
        let tracker_code_id = app.store_code(tracker_contract());

        let asset_infos_vec = test_coins
            .iter()
            .cloned()
            .map(|coin| {
                let asset_info = match &coin {
                    TestCoin::Native(denom) => native_asset_info(denom.clone()),
                    TestCoin::Cw20(..) | TestCoin::Cw20Precise(..) => {
                        let (name, precision) = coin.cw20_init_data().unwrap();
                        token_asset_info(Self::init_token(
                            &mut app,
                            token_code_id,
                            name,
                            precision,
                            owner,
                        ))
                    }
                    TestCoin::NativePrecise(..) => unimplemented!(),
                };
                (coin, asset_info)
            })
            .collect::<Vec<_>>();

        let pair_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let pair_type = PairType::Custom("concentrated".to_string());
        let fake_maker = Addr::unchecked("fake_maker");

        let coin_registry_id = app.store_code(coin_registry_contract());

        let coin_registry_address = app
            .instantiate_contract(
                coin_registry_id,
                owner.clone(),
                &astroport::native_coin_registry::InstantiateMsg {
                    owner: owner.to_string(),
                },
                &[],
                "Coin registry",
                None,
            )
            .unwrap();

        app.execute_contract(
            owner.clone(),
            coin_registry_address.clone(),
            &astroport::native_coin_registry::ExecuteMsg::Add {
                native_coins: vec![
                    ("uluna".to_owned(), 6),
                    ("uusd".to_owned(), 6),
                    ("wsteth".to_owned(), 18),
                    ("eth".to_owned(), 18),
                    ("uusdc".to_owned(), 6),
                ],
            },
            &[],
        )
        .unwrap();
        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: Some(fake_maker.to_string()),
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                maker_fee_bps: 5000,
                total_fee_bps: 0u16, // Concentrated pair does not use this field,
                pair_type: pair_type.clone(),
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: false,
            }],
            token_code_id,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 234u64,
            coin_registry_address: coin_registry_address.to_string(),
            tracker_config: Some(astroport::factory::TrackerConfig {
                code_id: tracker_code_id,
                token_factory_addr: TOKEN_FACTORY_MODULE.to_string(),
            }),
        };

        let factory = app.instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )?;

        let generator = app.store_code(generator());

        let generator_address = app
            .instantiate_contract(
                generator,
                owner.clone(),
                &astroport::incentives::InstantiateMsg {
                    astro_token: native_asset_info("astro".to_string()),
                    factory: factory.to_string(),
                    owner: owner.to_string(),
                    guardian: None,
                    incentivization_fee_info: None,
                    vesting_contract: "vesting".to_string(),
                },
                &[],
                "generator",
                None,
            )
            .unwrap();

        app.execute_contract(
            owner.clone(),
            factory.clone(),
            &astroport::factory::ExecuteMsg::UpdateConfig {
                token_code_id: None,
                fee_address: None,
                generator_address: Some(generator_address.to_string()),
                whitelist_code_id: None,
                coin_registry_address: None,
            },
            &[],
        )
        .unwrap();

        let asset_infos = asset_infos_vec
            .clone()
            .into_iter()
            .map(|(_, asset_info)| asset_info)
            .collect_vec();
        let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.clone(),
            init_params: Some(to_json_binary(&params).unwrap()),
        };

        app.execute_contract(owner.clone(), factory.clone(), &init_pair_msg, &[])?;

        let resp: PairInfo = app.wrap().query_wasm_smart(
            &factory,
            &astroport::factory::QueryMsg::Pair { asset_infos },
        )?;

        Ok(Self {
            app,
            owner: owner.clone(),
            assets: asset_infos_vec.into_iter().collect(),
            factory,
            generator: generator_address,
            pair_addr: resp.contract_addr,
            lp_token: resp.liquidity_token,
            fake_maker,
        })
    }

    pub fn provide_liquidity(&mut self, sender: &Addr, assets: &[Asset]) -> AnyResult<AppResponse> {
        self.provide_liquidity_with_slip_tolerance(
            sender,
            assets,
            Some(f64_to_dec(0.5)), // 50% slip tolerance for testing purposes
        )
    }

    pub fn provide_liquidity_with_auto_staking(
        &mut self,
        sender: &Addr,
        assets: &[Asset],
        slippage_tolerance: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        let funds =
            assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance: Some(slippage_tolerance.unwrap_or(f64_to_dec(0.5))),
            auto_stake: Some(true),
            receiver: None,
            min_lp_to_receive: None,
        };

        self.app
            .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
    }

    pub fn provide_liquidity_with_slip_tolerance(
        &mut self,
        sender: &Addr,
        assets: &[Asset],
        slippage_tolerance: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        let funds =
            assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
            min_lp_to_receive: None,
        };

        self.app
            .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
    }

    pub fn provide_liquidity_full(
        &mut self,
        sender: &Addr,
        assets: &[Asset],
        slippage_tolerance: Option<Decimal>,
        auto_stake: Option<bool>,
        receiver: Option<String>,
        min_lp_to_receive: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        let funds =
            assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance,
            auto_stake,
            receiver,
            min_lp_to_receive,
        };

        self.app
            .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
    }

    pub fn withdraw_liquidity(
        &mut self,
        sender: &Addr,
        amount: u128,
        assets: Vec<Asset>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.pair_addr.clone(),
            &ExecuteMsg::WithdrawLiquidity {
                assets,
                min_assets_to_receive: None,
            },
            &[coin(amount, self.lp_token.to_string())],
        )
    }

    pub fn swap(
        &mut self,
        sender: &Addr,
        offer_asset: &Asset,
        max_spread: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        self.swap_full_params(sender, offer_asset, max_spread, None)
    }

    pub fn swap_full_params(
        &mut self,
        sender: &Addr,
        offer_asset: &Asset,
        max_spread: Option<Decimal>,
        belief_price: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        match &offer_asset.info {
            AssetInfo::Token { contract_addr } => {
                let msg = Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_json_binary(&Cw20HookMsg::Swap {
                        ask_asset_info: None,
                        belief_price,
                        max_spread,
                        to: None,
                    })
                    .unwrap(),
                };

                self.app
                    .execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
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
                    ask_asset_info: None,
                    belief_price,
                    max_spread,
                    to: None,
                };

                self.app
                    .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
            }
        }
    }

    pub fn query_incentives_deposit(&self, denom: impl Into<String>, user: &Addr) -> Uint128 {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.generator,
                &astroport::incentives::QueryMsg::Deposit {
                    lp_token: denom.into(),
                    user: user.to_string(),
                },
            )
            .unwrap()
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

    pub fn query_prices(&self) -> StdResult<CumulativePricesResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::CumulativePrices {})
    }

    fn init_token(
        app: &mut TestApp,
        token_code: u64,
        name: String,
        decimals: u8,
        owner: &Addr,
    ) -> Addr {
        let init_balance = INIT_BALANCE;
        app.instantiate_contract(
            token_code,
            owner.clone(),
            &astroport::token::InstantiateMsg {
                symbol: name.to_string(),
                name,
                decimals,
                initial_balances: vec![Cw20Coin {
                    address: owner.to_string(),
                    amount: Uint128::from(init_balance),
                }],
                mint: None,
                marketing: None,
            },
            &[],
            "{name}_token",
            None,
        )
        .unwrap()
    }

    pub fn token_balance(&self, token_addr: &Addr, user: &Addr) -> u128 {
        let resp: BalanceResponse = self
            .app
            .wrap()
            .query_wasm_smart(
                token_addr,
                &Cw20QueryMsg::Balance {
                    address: user.to_string(),
                },
            )
            .unwrap();

        resp.balance.u128()
    }

    pub fn native_balance(&self, denom: impl Into<String>, user: &Addr) -> u128 {
        self.app
            .wrap()
            .query_balance(user, denom)
            .unwrap()
            .amount
            .u128()
    }

    pub fn coin_balance(&self, coin: &TestCoin, user: &Addr) -> u128 {
        match &self.assets[coin] {
            AssetInfo::Token { contract_addr } => self.token_balance(contract_addr, user),
            AssetInfo::NativeToken { denom } => self.native_balance(denom, user),
        }
    }

    pub fn give_me_money(&mut self, assets: &[Asset], recipient: &Addr) {
        let funds =
            assets.mock_coins_sent(&mut self.app, &self.owner, recipient, SendType::Transfer);

        if !funds.is_empty() {
            self.app
                .send_tokens(self.owner.clone(), recipient.clone(), &funds)
                .unwrap();
        }
    }

    pub fn query_config(&self) -> StdResult<Config> {
        let binary = self
            .app
            .wrap()
            .query_wasm_raw(&self.pair_addr, b"config")?
            .ok_or_else(|| StdError::generic_err("Failed to find config in storage"))?;
        from_json(&binary)
    }

    pub fn query_pool(&self) -> StdResult<PoolResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Pool {})
    }

    pub fn query_lp_price(&self) -> StdResult<Decimal256> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::LpPrice {})
    }

    pub fn query_asset_balance_at(
        &self,
        asset_info: &AssetInfo,
        block_height: u64,
    ) -> StdResult<Option<Uint128>> {
        self.app.wrap().query_wasm_smart(
            &self.pair_addr.clone(),
            &QueryMsg::AssetBalanceAt {
                asset_info: asset_info.clone(),
                block_height: block_height.into(),
            },
        )
    }

    pub fn update_config(
        &mut self,
        user: &Addr,
        action: &ConcentratedPoolUpdateParams,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            user.clone(),
            self.pair_addr.clone(),
            &ExecuteMsg::UpdateConfig {
                params: to_json_binary(action).unwrap(),
            },
            &[],
        )
    }

    pub fn query_amp_gamma(&self) -> StdResult<AmpGammaResponse> {
        let config_resp: ConfigResponse = self
            .app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Config {})?;
        let params: ConcentratedPoolConfig = from_json(
            &config_resp
                .params
                .ok_or_else(|| StdError::generic_err("Params not found in config response!"))?,
        )?;
        Ok(AmpGammaResponse {
            amp: params.amp,
            gamma: params.gamma,
            future_time: self.query_config()?.pool_state.future_time,
        })
    }

    pub fn query_d(&self) -> StdResult<Decimal256> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::ComputeD {})
    }

    pub fn query_share(&self, amount: impl Into<Uint128>) -> StdResult<Vec<Asset>> {
        self.app.wrap().query_wasm_smart::<Vec<Asset>>(
            &self.pair_addr,
            &QueryMsg::Share {
                amount: amount.into(),
            },
        )
    }

    pub fn observe_price(&self, seconds_ago: u64) -> StdResult<Decimal> {
        self.app
            .wrap()
            .query_wasm_smart::<OracleObservation>(
                &self.pair_addr,
                &QueryMsg::Observe { seconds_ago },
            )
            .map(|val| val.price)
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
        app: &mut TestApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin> {
        let mut funds = vec![];
        match &self.info {
            AssetInfo::Token { contract_addr } if !self.amount.is_zero() => {
                let msg = match typ {
                    SendType::Allowance => Cw20ExecuteMsg::IncreaseAllowance {
                        spender: spender.to_string(),
                        amount: self.amount,
                        expires: None,
                    },
                    SendType::Transfer => Cw20ExecuteMsg::Transfer {
                        recipient: spender.to_string(),
                        amount: self.amount,
                    },
                    _ => unimplemented!(),
                };
                app.execute_contract(user.clone(), contract_addr.clone(), &msg, &[])
                    .unwrap();
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

pub trait AppExtension {
    fn next_block(&mut self, time: u64);
}

impl AppExtension for TestApp {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}
