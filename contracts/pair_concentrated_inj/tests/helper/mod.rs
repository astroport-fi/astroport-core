#![allow(dead_code)]

use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display};
use std::str::FromStr;

use anyhow::Result as AnyResult;
use cosmwasm_schema::cw_serde;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_std::{
    coin, from_slice, to_binary, Addr, Coin, CustomMsg, CustomQuery, Decimal, Decimal256, StdError,
    StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_multi_test::{AppResponse, Contract, ContractWrapper, Executor};
use derivative::Derivative;
use injective_cosmwasm::{InjectiveMsgWrapper, InjectiveQueryWrapper};
use itertools::Itertools;

use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::native_coin_registry;
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, PoolResponse,
    ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair_concentrated::{ConcentratedPoolParams, ConcentratedPoolUpdateParams};
use astroport::pair_concentrated_inj::{
    ConcentratedInjObParams, OrderbookConfig, OrderbookStateResponse, QueryMsg,
};
use astroport_pair_concentrated_injective::contract::{execute, instantiate, reply};
use astroport_pair_concentrated_injective::migrate::migrate;
use astroport_pair_concentrated_injective::orderbook::state::OrderbookState;
use astroport_pair_concentrated_injective::orderbook::sudo::sudo;
use astroport_pair_concentrated_injective::orderbook::utils::calc_market_ids;
use astroport_pair_concentrated_injective::queries::query;
use astroport_pair_concentrated_injective::state::Config;

use crate::helper::mocks::{mock_inj_app, InjApp, InjAppExt};

pub mod mocks;

const INIT_BALANCE: u128 = u128::MAX;

#[cw_serde]
pub struct AmpGammaResponse {
    pub amp: Decimal,
    pub gamma: Decimal,
    pub future_time: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TestCoin {
    Cw20(String),
    Cw20Precise(String, u8),
    Native(String),
}

impl TestCoin {
    pub fn denom(&self) -> Option<String> {
        match self {
            TestCoin::Native(denom) => Some(denom.clone()),
            _ => None,
        }
    }

    pub fn cw20_init_data(&self) -> Option<(String, u8)> {
        match self {
            TestCoin::Cw20(name) => Some((name.clone(), 6u8)),
            TestCoin::Cw20Precise(name, precision) => Some((name.clone(), *precision)),
            _ => None,
        }
    }

    pub fn native(denom: &str) -> Self {
        Self::Native(denom.to_string())
    }

    pub fn cw20(name: &str) -> Self {
        Self::Cw20(name.to_string())
    }

    pub fn cw20precise(name: &str, precision: u8) -> Self {
        Self::Cw20Precise(name.to_string(), precision)
    }
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

fn token_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    T: Clone + Debug + PartialEq + JsonSchema + 'static,
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ))
}

pub fn orderbook_pair_contract() -> Box<dyn Contract<InjectiveMsgWrapper, InjectiveQueryWrapper>> {
    Box::new(
        ContractWrapper::new(execute, instantiate, query)
            .with_sudo(sudo)
            .with_migrate(migrate)
            .with_reply(reply),
    )
}

fn concentrated_pair_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
    T: CustomMsg + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_concentrated::contract::execute,
            astroport_pair_concentrated::contract::instantiate,
            astroport_pair_concentrated::queries::query,
        )
        .with_reply_empty(mocks::cl_pair_reply),
    )
}

fn coin_registry_contract<T, C>() -> Box<dyn Contract<T, C>>
where
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
    T: CustomMsg + 'static,
{
    Box::new(ContractWrapper::new_with_empty(
        astroport_native_coin_registry::contract::execute,
        astroport_native_coin_registry::contract::instantiate,
        astroport_native_coin_registry::contract::query,
    ))
}

fn factory_contract<C, T>() -> Box<dyn Contract<T, C>>
where
    C: CustomQuery + for<'de> cosmwasm_schema::serde::Deserialize<'de> + 'static,
    T: CustomMsg + 'static,
{
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_factory::contract::execute,
            astroport_factory::contract::instantiate,
            astroport_factory::contract::query,
        )
        .with_reply_empty(mocks::factory_reply),
    )
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: InjApp,
    pub owner: Addr,
    pub assets: HashMap<TestCoin, AssetInfo>,
    pub factory: Addr,
    pub pair_addr: Addr,
    pub lp_token: Addr,
    pub maker: Addr,
}

impl Helper {
    pub fn new_with_app(
        mut app: InjApp,
        owner: &Addr,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
        with_orderbook: bool,
        market_id: Option<String>,
    ) -> AnyResult<Self> {
        app.init_modules(|router, _, storage| {
            router
                .bank
                .init_balance(storage, owner, init_native_coins(&test_coins))
                .unwrap();
        });

        let mut asset_infos_vec: Vec<_> = test_coins
            .clone()
            .into_iter()
            .filter_map(|coin| Some((coin.clone(), native_asset_info(coin.denom()?))))
            .collect();

        let token_code_id = app.store_code(token_contract());

        test_coins.into_iter().for_each(|coin| {
            if let Some((name, decimals)) = coin.cw20_init_data() {
                let token_addr = Self::init_token(&mut app, token_code_id, name, decimals, owner);
                asset_infos_vec.push((coin, token_asset_info(token_addr)))
            }
        });

        let factory_code_id = app.store_code(factory_contract());

        let pair_type = if with_orderbook {
            PairType::Custom("concentrated_inj_orderbook".to_string())
        } else {
            PairType::Custom("concentrated".to_string())
        };

        let fake_maker = Addr::unchecked("fake_maker");

        let coin_registry_id = app.store_code(coin_registry_contract());

        let coin_registry_address = app
            .instantiate_contract(
                coin_registry_id,
                owner.clone(),
                &native_coin_registry::InstantiateMsg {
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
            &native_coin_registry::ExecuteMsg::Add {
                native_coins: vec![
                    ("uluna".to_owned(), 6),
                    ("uusd".to_owned(), 6),
                    ("ASTRO".to_owned(), 6),
                    ("USDC".to_owned(), 6),
                    ("FOO".to_owned(), 5),
                    ("BAR".to_owned(), 6),
                    ("inj".to_owned(), 18),
                    ("astro".to_owned(), 6),
                ],
            },
            &[],
        )
        .unwrap();
        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: Some(fake_maker.to_string()),
            pair_configs: vec![
                PairConfig {
                    code_id: app.store_code(concentrated_pair_contract()),
                    maker_fee_bps: 5000,
                    total_fee_bps: 0u16, // Concentrated pair does not use this field,
                    pair_type: PairType::Custom("concentrated".to_string()),
                    is_disabled: false,
                    is_generator_disabled: false,
                },
                PairConfig {
                    code_id: app.store_code(orderbook_pair_contract()),
                    maker_fee_bps: 5000,
                    total_fee_bps: 0u16, // Concentrated pair does not use this field,
                    pair_type: PairType::Custom("concentrated_inj_orderbook".to_string()),
                    is_disabled: false,
                    is_generator_disabled: false,
                },
            ],
            token_code_id,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 234u64,
            coin_registry_address: coin_registry_address.to_string(),
        };

        let factory = app.instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "FACTORY",
            None,
        )?;

        let asset_infos = asset_infos_vec
            .clone()
            .into_iter()
            .map(|(_, asset_info)| asset_info)
            .collect_vec();

        let params = if with_orderbook {
            let market_id =
                market_id.unwrap_or_else(|| calc_market_ids(&asset_infos).unwrap()[0].clone());
            to_binary(&ConcentratedInjObParams {
                main_params: params,
                orderbook_config: OrderbookConfig {
                    market_id,
                    orders_number: 5,
                    min_trades_to_avg: 1,
                },
            })
            .unwrap()
        } else {
            to_binary(&params).unwrap()
        };

        let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.clone(),
            init_params: Some(params),
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
            pair_addr: resp.contract_addr,
            lp_token: resp.liquidity_token,
            maker: fake_maker,
        })
    }

    pub fn new(
        owner: &Addr,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
        with_orderbook: bool,
    ) -> AnyResult<Self> {
        let mut app = mock_inj_app(|_, _, _| {});
        app.create_market(
            &test_coins[0].denom().unwrap(),
            &test_coins[1].denom().unwrap(),
        )
        .unwrap();
        Self::new_with_app(app, owner, test_coins, params, with_orderbook, None)
    }

    pub fn next_block(&mut self, gas_free: bool) -> AnyResult<()> {
        self.app.update_block(|block| {
            block.height += 1;
            block.time = block.time.plus_seconds(1);
        });
        self.app.begin_blocker(&self.app.block_info(), gas_free)
    }

    pub fn provide_liquidity(&mut self, sender: &Addr, assets: &[Asset]) -> AnyResult<AppResponse> {
        self.provide_liquidity_with_slip_tolerance(
            sender,
            assets,
            Some(f64_to_dec(0.5)), // 50% slip tolerance for testing purposes
        )
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
        let msg = Cw20ExecuteMsg::Send {
            contract: self.pair_addr.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::WithdrawLiquidity { assets }).unwrap(),
        };

        self.app
            .execute_contract(sender.clone(), self.lp_token.clone(), &msg, &[])
    }

    pub fn swap(
        &mut self,
        sender: &Addr,
        offer_asset: &Asset,
        max_spread: Option<Decimal>,
    ) -> AnyResult<AppResponse> {
        match &offer_asset.info {
            AssetInfo::Token { contract_addr } => {
                let msg = Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&Cw20HookMsg::Swap {
                        ask_asset_info: None,
                        belief_price: None,
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
                    belief_price: None,
                    max_spread,
                    to: None,
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

    pub fn query_prices(&self) -> StdResult<CumulativePricesResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::CumulativePrices {})
    }

    pub fn query_ob_config(&self) -> StdResult<OrderbookState> {
        let bin = self
            .app
            .wrap()
            .query_wasm_raw(&self.pair_addr, b"orderbook_config")?
            .unwrap();

        from_slice(&bin)
    }

    pub fn query_ob_config_smart(&self) -> StdResult<OrderbookStateResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::OrderbookState {})
    }

    fn init_token(
        app: &mut InjApp,
        token_code: u64,
        name: String,
        decimals: u8,
        owner: &Addr,
    ) -> Addr {
        let init_balance = INIT_BALANCE * 10u128.pow(decimals as u32);
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

    pub fn coin_balance(&self, coin: &TestCoin, user: &Addr) -> u128 {
        match &self.assets[coin] {
            AssetInfo::Token { contract_addr } => self.token_balance(contract_addr, user),
            AssetInfo::NativeToken { denom } => self
                .app
                .wrap()
                .query_balance(user, denom)
                .unwrap()
                .amount
                .u128(),
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
        from_slice(&binary)
    }

    pub fn query_lp_price(&self) -> StdResult<f64> {
        let res: Decimal256 = self
            .app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::LpPrice {})?;
        Ok(dec_to_f64(res))
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
                params: to_binary(action).unwrap(),
            },
            &[],
        )
    }

    pub fn query_amp_gamma(&self) -> StdResult<AmpGammaResponse> {
        let config_resp: ConfigResponse = self
            .app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Config {})?;
        let params: ConcentratedPoolParams = from_slice(
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

    pub fn query_pool(&self) -> StdResult<PoolResponse> {
        self.app
            .wrap()
            .query_wasm_smart(&self.pair_addr, &QueryMsg::Pool {})
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
        app: &mut InjApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetExt for Asset {
    fn mock_coin_sent(
        &self,
        app: &mut InjApp,
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
        app: &mut InjApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetsExt for &[Asset] {
    fn mock_coins_sent(
        &self,
        app: &mut InjApp,
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

impl AppExtension for InjApp {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}

pub fn f64_to_dec<T>(val: f64) -> T
where
    T: FromStr,
    T::Err: Error,
{
    T::from_str(&val.to_string()).unwrap()
}

pub fn dec_to_f64(val: impl Display) -> f64 {
    f64::from_str(&val.to_string()).unwrap()
}
