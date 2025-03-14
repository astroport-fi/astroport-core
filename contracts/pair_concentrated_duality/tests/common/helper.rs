#![cfg(not(tarpaulin_include))]
#![allow(dead_code)]

use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{anyhow, Result as AnyResult};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coin, coins, from_json, to_json_binary, to_json_vec, Addr, BankMsg, Decimal, Decimal256, Empty,
    GovMsg, IbcMsg, IbcQuery, MemoryStorage, Querier, QueryRequest, StdError, StdResult, Uint128,
};
use derivative::Derivative;
use itertools::Itertools;
use neutron_std::types::neutron::dex::QueryAllLimitOrderTrancheUserByAddressRequest;

use astroport::asset::{native_asset_info, Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::observation::OracleObservation;
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, ExecuteMsgExt, PoolResponse,
    ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair_concentrated::{
    ConcentratedPoolConfig, ConcentratedPoolParams, ConcentratedPoolUpdateParams, QueryMsg,
};
use astroport::pair_concentrated_duality::{
    ConcentratedDualityParams, DualityPairMsg, OrderbookConfig, UpdateDualityOrderbook,
};
use astroport_pair_concentrated_duality::orderbook::custom_types::CustomQueryAllLimitOrderTrancheUserByAddressResponse;
use astroport_pair_concentrated_duality::orderbook::state::OrderbookState;
use astroport_pcl_common::state::Config;
use astroport_test::coins::TestCoin;
use astroport_test::convert::f64_to_dec;
use astroport_test::cw_multi_test::{
    no_init, App, AppResponse, BankKeeper, BankSudo, BasicAppBuilder, Contract, ContractWrapper,
    DistributionKeeper, Executor, FailingModule, StakeKeeper, WasmKeeper,
};
use astroport_test::modules::neutron_stargate::NeutronStargate;

pub type ExecuteMsg = ExecuteMsgExt<DualityPairMsg>;

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

pub fn pcl_duality_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new(
            astroport_pair_concentrated_duality::execute::execute,
            astroport_pair_concentrated_duality::instantiate::instantiate,
            astroport_pair_concentrated_duality::queries::query,
        )
        .with_reply_empty(astroport_pair_concentrated_duality::reply::reply)
        .with_migrate(astroport_pair_concentrated_duality::migrate::migrate),
    )
}

pub fn pcl_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new(
            astroport_pair_concentrated::contract::execute,
            astroport_pair_concentrated::contract::instantiate,
            astroport_pair_concentrated::queries::query,
        )
        .with_reply_empty(astroport_pair_concentrated::contract::reply),
    )
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: NeutronApp,
    pub owner: Addr,
    pub assets: HashMap<TestCoin, AssetInfo>,
    pub factory: Addr,
    pub pair_addr: Addr,
    pub lp_token: String,
    pub fake_maker: Addr,
    pub native_coin_registry: Addr,
}

impl Helper {
    pub fn new(
        owner: &Addr,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
        with_orderbook: bool,
    ) -> AnyResult<Self> {
        let mut app = BasicAppBuilder::new()
            .with_stargate(NeutronStargate::default())
            .build(no_init);

        let asset_infos_vec = test_coins
            .iter()
            .cloned()
            .map(|coin| {
                let asset_info = match &coin {
                    TestCoin::Native(denom) | TestCoin::NativePrecise(denom, ..) => {
                        native_asset_info(denom.clone())
                    }
                    _ => unimplemented!(),
                };
                (coin, asset_info)
            })
            .collect::<Vec<_>>();

        let factory_code_id = app.store_code(factory_contract());

        let pair_type = if with_orderbook {
            PairType::Custom("concentrated_duality_orderbook".to_string())
        } else {
            PairType::Custom("concentrated".to_string())
        };

        let fake_maker = Addr::unchecked("fake_maker");

        let coin_registry_id = app.store_code(coin_registry_contract());

        let native_coin_registry = app
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

        // Register decimals
        test_coins.iter().for_each(|test_coin| {
            if let Some(denom) = test_coin.denom() {
                register_decimals(
                    &mut app,
                    &native_coin_registry,
                    &denom,
                    test_coin.decimals(),
                )
                .unwrap();
            }
        });

        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: Some(fake_maker.to_string()),
            pair_configs: vec![
                PairConfig {
                    code_id: app.store_code(pcl_contract()),
                    maker_fee_bps: 5000,
                    total_fee_bps: 0u16, // Concentrated pair does not use this field,
                    pair_type: PairType::Custom("concentrated".to_string()),
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                },
                PairConfig {
                    code_id: app.store_code(pcl_duality_contract()),
                    maker_fee_bps: 5000,
                    total_fee_bps: 0u16, // Concentrated pair does not use this field,
                    pair_type: PairType::Custom("concentrated_duality_orderbook".to_string()),
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                },
            ],
            token_code_id: 0,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: native_coin_registry.to_string(),
            tracker_config: None,
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

        let init_pair_msg: astroport::factory::ExecuteMsg =
            astroport::factory::ExecuteMsg::CreatePair {
                pair_type,
                asset_infos: asset_infos.clone(),
                init_params: Some(if with_orderbook {
                    to_json_binary(&ConcentratedDualityParams {
                        main_params: params,
                        orderbook_config: OrderbookConfig {
                            liquidity_percent: Decimal::percent(20),
                            orders_number: 5,
                            min_asset_0_order_size: Uint128::from(1000u128),
                            min_asset_1_order_size: Uint128::from(1000u128),
                            executor: Some(owner.to_string()),
                            avg_price_adjustment: Decimal::from_str("0.001").unwrap(),
                        },
                    })
                    .unwrap()
                } else {
                    to_json_binary(&params).unwrap()
                }),
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
            fake_maker,
            native_coin_registry,
        })
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
        let funds = assets
            .iter()
            .map(|asset| asset.as_coin().unwrap())
            .collect_vec();

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
        let funds = assets
            .iter()
            .map(|asset| asset.as_coin().unwrap())
            .collect_vec();

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

    pub fn withdraw_liquidity_full(
        &mut self,
        sender: &Addr,
        amount: u128,
        assets: Vec<Asset>,
        min_assets_to_receive: Option<Vec<Asset>>,
    ) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            sender.clone(),
            self.pair_addr.clone(),
            &ExecuteMsg::WithdrawLiquidity {
                assets,
                min_assets_to_receive,
            },
            &[coin(amount, self.lp_token.to_string())],
        )
    }

    pub fn withdraw_liquidity(
        &mut self,
        sender: &Addr,
        amount: u128,
        assets: Vec<Asset>,
    ) -> AnyResult<AppResponse> {
        self.withdraw_liquidity_full(sender, amount, assets, None)
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
            AssetInfo::Token { .. } => unimplemented!(),
            AssetInfo::NativeToken { .. } => {
                let msg = ExecuteMsg::Swap {
                    offer_asset: offer_asset.clone(),
                    ask_asset_info: None,
                    belief_price,
                    max_spread,
                    to: None,
                };

                self.app.execute_contract(
                    sender.clone(),
                    self.pair_addr.clone(),
                    &msg,
                    &[offer_asset.as_coin().unwrap()],
                )
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

    pub fn native_balance(&self, denom: &str, user: &Addr) -> u128 {
        self.app
            .wrap()
            .query_balance(user, denom)
            .unwrap()
            .amount
            .u128()
    }

    pub fn coin_balance(&self, coin: &TestCoin, user: &Addr) -> u128 {
        match &self.assets[coin] {
            AssetInfo::Token { .. } => unimplemented!(),
            AssetInfo::NativeToken { denom } => self.native_balance(denom, user),
        }
    }

    pub fn give_me_money(&mut self, assets: &[Asset], recipient: &Addr) {
        assets.iter().for_each(|asset| {
            if !asset.amount.is_zero() {
                self.app
                    .sudo(
                        BankSudo::Mint {
                            to_address: recipient.to_string(),
                            amount: vec![asset.as_coin().unwrap()],
                        }
                        .into(),
                    )
                    .unwrap();
            }
        });
    }

    pub fn query_config(&self) -> StdResult<Config> {
        let binary = self
            .app
            .wrap()
            .query_wasm_raw(&self.pair_addr, b"config")?
            .ok_or_else(|| StdError::generic_err("Failed to find config in storage"))?;
        from_json(&binary)
    }

    pub fn query_ob_config(&self) -> StdResult<OrderbookState> {
        let binary = self
            .app
            .wrap()
            .query_wasm_raw(&self.pair_addr, b"orderbook_config")?
            .ok_or_else(|| StdError::generic_err("Failed to find orderbook_config in storage"))?;
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

    pub fn query_orders(
        &self,
        addr: impl Into<String>,
    ) -> AnyResult<CustomQueryAllLimitOrderTrancheUserByAddressResponse> {
        let query_msg = to_json_vec(&QueryRequest::<Empty>::Stargate {
            path: "/neutron.dex.Query/LimitOrderTrancheUserAllByAddress".to_string(),
            data: QueryAllLimitOrderTrancheUserByAddressRequest {
                address: addr.into(),
                pagination: None,
            }
            .into(),
        })?;

        let response_raw = self
            .app
            .raw_query(&query_msg)
            .into_result()
            .map_err(|err| anyhow!(err))?
            .into_result()
            .map_err(|err| anyhow!(err))?;

        from_json(&response_raw).map_err(Into::into)
    }

    pub fn enable_orderbook(&mut self, enable: bool) -> AnyResult<AppResponse> {
        self.app.execute_contract(
            self.owner.clone(),
            self.pair_addr.clone(),
            &ExecuteMsg::Custom(DualityPairMsg::UpdateOrderbookConfig(
                UpdateDualityOrderbook {
                    enable: Some(enable),
                    executor: None,
                    remove_executor: false,
                    orders_number: None,
                    min_asset_0_order_size: None,
                    min_asset_1_order_size: None,
                    liquidity_percent: None,
                    avg_price_adjustment: None,
                },
            )),
            &[],
        )
    }

    pub fn next_block(&mut self, time: u64) {
        self.app.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }
}

pub fn register_decimals(
    app: &mut NeutronApp,
    registry_contract: &Addr,
    denom: &str,
    decimals: u8,
) -> AnyResult<AppResponse> {
    let registrator = app.api().addr_make("registrator");
    let funds = coins(1, denom);

    app.sudo(
        BankSudo::Mint {
            to_address: registrator.to_string(),
            amount: funds.clone(),
        }
        .into(),
    )
    .unwrap();

    app.execute_contract(
        registrator.clone(),
        registry_contract.clone(),
        &astroport::native_coin_registry::ExecuteMsg::Register {
            native_coins: vec![(denom.to_string(), decimals)],
        },
        &funds,
    )
    .unwrap();

    app.execute(registrator, BankMsg::Burn { amount: funds }.into())
}
