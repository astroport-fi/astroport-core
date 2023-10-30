// #![allow(dead_code)]
// #![cfg(not(tarpaulin_include))]

// use std::collections::HashMap;
// use std::error::Error;
// use std::fmt::Display;
// use std::str::FromStr;

// use anyhow::Result as AnyResult;
// use cosmwasm_schema::serde::de::DeserializeOwned;
// use cosmwasm_std::{
//     coin, from_slice, to_binary, Addr, Coin, Decimal, Empty, StdError, StdResult, Uint128,
// };
// use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg};
// use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};
// use derivative::Derivative;
// use itertools::Itertools;

// use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
// use astroport::factory::{PairConfig, PairType};
// use astroport::liquidity_manager::{Cw20HookMsg, ExecuteMsg};
// use astroport::liquidity_manager::{InstantiateMsg, QueryMsg};
// use astroport::pair::{Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg};
// use astroport::pair::{
//     ReverseSimulationResponse, SimulationResponse, StablePoolParams, XYKPoolParams,
// };
// use astroport::pair_concentrated::{ConcentratedPoolParams, QueryMsg as PairQueryMsg};
// use astroport::{factory, generator};
// use astroport_liquidity_manager::contract::{execute, instantiate, reply};
// use astroport_liquidity_manager::query::query;

// const NATIVE_TOKEN_PRECISION: u8 = 6;

// const INIT_BALANCE: u128 = 1_000_000_000_000;

// pub enum PoolParams {
//     Constant(XYKPoolParams),
//     Stable(StablePoolParams),
//     Concentrated(ConcentratedPoolParams),
// }

// #[derive(Clone, Debug, PartialEq, Eq, Hash)]
// pub enum TestCoin {
//     Cw20(String),
//     Cw20Precise(String, u8),
//     Native(String),
// }

// impl TestCoin {
//     pub fn denom(&self) -> Option<String> {
//         match self {
//             TestCoin::Native(denom) => Some(denom.clone()),
//             _ => None,
//         }
//     }

//     pub fn cw20_init_data(&self) -> Option<(String, u8)> {
//         match self {
//             TestCoin::Cw20(name) => Some((name.clone(), 6u8)),
//             TestCoin::Cw20Precise(name, precision) => Some((name.clone(), *precision)),
//             _ => None,
//         }
//     }

//     pub fn native(denom: &str) -> Self {
//         Self::Native(denom.to_string())
//     }

//     pub fn cw20(name: &str) -> Self {
//         Self::Cw20(name.to_string())
//     }

//     pub fn cw20precise(name: &str, precision: u8) -> Self {
//         Self::Cw20Precise(name.to_string(), precision)
//     }
// }

// pub fn init_native_coins(test_coins: &[TestCoin]) -> Vec<Coin> {
//     let mut test_coins: Vec<Coin> = test_coins
//         .iter()
//         .filter_map(|test_coin| match test_coin {
//             TestCoin::Native(name) => {
//                 let init_balance = INIT_BALANCE * 10u128.pow(NATIVE_TOKEN_PRECISION as u32);
//                 Some(coin(init_balance, name))
//             }
//             _ => None,
//         })
//         .collect();
//     test_coins.push(coin(INIT_BALANCE, "random-coin"));

//     test_coins
// }

// fn token_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(ContractWrapper::new_with_empty(
//         astroport_token::contract::execute,
//         astroport_token::contract::instantiate,
//         astroport_token::contract::query,
//     ))
// }

// fn xyk_pair_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_pair::contract::execute,
//             astroport_pair::contract::instantiate,
//             astroport_pair::contract::query,
//         )
//         .with_reply_empty(astroport_pair::contract::reply),
//     )
// }

// fn stable_pair_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_pair_stable::contract::execute,
//             astroport_pair_stable::contract::instantiate,
//             astroport_pair_stable::contract::query,
//         )
//         .with_reply_empty(astroport_pair_stable::contract::reply),
//     )
// }

// fn coin_registry_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(ContractWrapper::new_with_empty(
//         astroport_native_coin_registry::contract::execute,
//         astroport_native_coin_registry::contract::instantiate,
//         astroport_native_coin_registry::contract::query,
//     ))
// }

// fn factory_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_factory::contract::execute,
//             astroport_factory::contract::instantiate,
//             astroport_factory::contract::query,
//         )
//         .with_reply_empty(astroport_factory::contract::reply),
//     )
// }

// fn whitelist_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(ContractWrapper::new_with_empty(
//         astroport_whitelist::contract::execute,
//         astroport_whitelist::contract::instantiate,
//         astroport_whitelist::contract::query,
//     ))
// }

// fn generator_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(
//         ContractWrapper::new_with_empty(
//             astroport_generator::contract::execute,
//             astroport_generator::contract::instantiate,
//             astroport_generator::contract::query,
//         )
//         .with_reply_empty(astroport_generator::contract::reply),
//     )
// }

// fn manager_contract() -> Box<dyn Contract<Empty>> {
//     Box::new(ContractWrapper::new_with_empty(execute, instantiate, query).with_reply_empty(reply))
// }

// #[derive(Derivative)]
// #[derivative(Debug)]
// pub struct Helper {
//     #[derivative(Debug = "ignore")]
//     pub app: App,
//     pub owner: Addr,
//     pub assets: HashMap<TestCoin, AssetInfo>,
//     pub factory: Addr,
//     pub pair_addr: Addr,
//     pub lp_token: Addr,
//     pub fake_maker: Addr,
//     pub liquidity_manager: Addr,
//     pub generator: Addr,
// }

// impl Helper {
//     pub fn new(owner: &Addr, test_coins: Vec<TestCoin>, params: PoolParams) -> AnyResult<Self> {
//         let mut app = App::new(|router, _, storage| {
//             router
//                 .bank
//                 .init_balance(storage, owner, init_native_coins(&test_coins))
//                 .unwrap()
//         });

//         let mut asset_infos_vec: Vec<_> = test_coins
//             .clone()
//             .into_iter()
//             .filter_map(|coin| Some((coin.clone(), native_asset_info(coin.denom()?))))
//             .collect();

//         let token_code_id = app.store_code(token_contract());

//         test_coins.into_iter().for_each(|coin| {
//             if let Some((name, decimals)) = coin.cw20_init_data() {
//                 let token_addr = Self::init_token(&mut app, token_code_id, name, decimals, owner);
//                 asset_infos_vec.push((coin, token_asset_info(token_addr)))
//             }
//         });

//         let factory_code_id = app.store_code(factory_contract());

//         let (pair_code_id, pair_type, inner_params);
//         match &params {
//             PoolParams::Constant(inner) => {
//                 pair_code_id = app.store_code(xyk_pair_contract());
//                 pair_type = PairType::Xyk {};
//                 inner_params = to_binary(inner).unwrap();
//             }
//             PoolParams::Stable(inner) => {
//                 pair_code_id = app.store_code(stable_pair_contract());
//                 pair_type = PairType::Stable {};
//                 inner_params = to_binary(inner).unwrap();
//             }
//             PoolParams::Concentrated(_) => {
//                 unimplemented!("Concentrated pool is not supported yet");
//                 // pair_code_id = app.store_code(pcl_pair_contract());
//                 // pair_type = PairType::Custom("concentrated".to_owned());
//                 // inner_params = to_binary(inner).unwrap();
//             }
//         }

//         let fake_maker = Addr::unchecked("fake_maker");

//         let coin_registry_id = app.store_code(coin_registry_contract());

//         let coin_registry_address = app
//             .instantiate_contract(
//                 coin_registry_id,
//                 owner.clone(),
//                 &astroport::native_coin_registry::InstantiateMsg {
//                     owner: owner.to_string(),
//                 },
//                 &[],
//                 "Coin registry",
//                 None,
//             )
//             .unwrap();

//         app.execute_contract(
//             owner.clone(),
//             coin_registry_address.clone(),
//             &astroport::native_coin_registry::ExecuteMsg::Add {
//                 native_coins: vec![("uluna".to_owned(), 6), ("uusd".to_owned(), 6)],
//             },
//             &[],
//         )
//         .unwrap();
//         let init_msg = astroport::factory::InstantiateMsg {
//             fee_address: Some(fake_maker.to_string()),
//             pair_configs: vec![PairConfig {
//                 code_id: pair_code_id,
//                 maker_fee_bps: 5000,
//                 total_fee_bps: 30,
//                 pair_type: pair_type.clone(),
//                 is_disabled: false,
//                 is_generator_disabled: false,
//             }],
//             token_code_id,
//             generator_address: None,
//             owner: owner.to_string(),
//             whitelist_code_id: 234u64,
//             coin_registry_address: coin_registry_address.to_string(),
//         };

//         let factory = app.instantiate_contract(
//             factory_code_id,
//             owner.clone(),
//             &init_msg,
//             &[],
//             "FACTORY",
//             None,
//         )?;

//         let whitelist_code_id = app.store_code(whitelist_contract());
//         let generator_code_id = app.store_code(generator_contract());
//         let generator = app
//             .instantiate_contract(
//                 generator_code_id,
//                 owner.clone(),
//                 &generator::InstantiateMsg {
//                     owner: owner.to_string(),
//                     factory: factory.to_string(),
//                     generator_controller: None,
//                     voting_escrow_delegation: None,
//                     voting_escrow: None,
//                     guardian: None,
//                     astro_token: native_asset_info("astro".to_string()),
//                     tokens_per_block: Default::default(),
//                     start_block: Default::default(),
//                     vesting_contract: "vesting".to_string(),
//                     whitelist_code_id,
//                 },
//                 &[],
//                 "Generator",
//                 None,
//             )
//             .unwrap();

//         app.execute_contract(
//             owner.clone(),
//             factory.clone(),
//             &factory::ExecuteMsg::UpdateConfig {
//                 token_code_id: None,
//                 fee_address: None,
//                 generator_address: Some(generator.to_string()),
//                 whitelist_code_id: None,
//                 coin_registry_address: None,
//             },
//             &[],
//         )
//         .unwrap();

//         let manager_code = app.store_code(manager_contract());
//         let liquidity_manager = app
//             .instantiate_contract(
//                 manager_code,
//                 owner.clone(),
//                 &InstantiateMsg {
//                     astroport_factory: factory.to_string(),
//                 },
//                 &[],
//                 "Liquidity manager",
//                 None,
//             )
//             .unwrap();

//         let asset_infos = asset_infos_vec
//             .clone()
//             .into_iter()
//             .map(|(_, asset_info)| asset_info)
//             .collect_vec();
//         let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
//             pair_type,
//             asset_infos: asset_infos.clone(),
//             init_params: Some(inner_params),
//         };

//         app.execute_contract(owner.clone(), factory.clone(), &init_pair_msg, &[])?;

//         let resp: PairInfo = app.wrap().query_wasm_smart(
//             &factory,
//             &astroport::factory::QueryMsg::Pair { asset_infos },
//         )?;

//         Ok(Self {
//             app,
//             owner: owner.clone(),
//             assets: asset_infos_vec.into_iter().collect(),
//             factory,
//             pair_addr: resp.contract_addr,
//             lp_token: resp.liquidity_token,
//             fake_maker,
//             liquidity_manager,
//             generator,
//         })
//     }

//     pub fn simulate_provide(
//         &self,
//         slippage_tolerance: Option<Decimal>,
//         assets: &[Asset],
//     ) -> AnyResult<Uint128> {
//         let pair_msg = PairExecuteMsg::ProvideLiquidity {
//             assets: assets.to_vec(),
//             slippage_tolerance,
//             auto_stake: None,
//             receiver: None,
//         };

//         self.app
//             .wrap()
//             .query_wasm_smart(
//                 &self.liquidity_manager,
//                 &QueryMsg::SimulateProvide {
//                     pair_addr: self.pair_addr.to_string(),
//                     pair_msg,
//                 },
//             )
//             .map_err(Into::into)
//     }

//     pub fn simulate_withdraw(&self, lp_tokens_amount: impl Into<Uint128>) -> AnyResult<Vec<Asset>> {
//         self.app
//             .wrap()
//             .query_wasm_smart(
//                 &self.liquidity_manager,
//                 &QueryMsg::SimulateWithdraw {
//                     pair_addr: self.pair_addr.to_string(),
//                     lp_tokens: lp_tokens_amount.into(),
//                 },
//             )
//             .map_err(Into::into)
//     }

//     /// If min_lp_receive is Some provide is done via liquidity manager contract.
//     pub fn provide_liquidity(
//         &mut self,
//         sender: &Addr,
//         assets: &[Asset],
//         min_lp_receive: Option<Uint128>,
//     ) -> AnyResult<AppResponse> {
//         self.provide_liquidity_with_slip_tolerance(
//             sender,
//             assets,
//             Some(f64_to_dec(0.5)),
//             min_lp_receive,
//             false,
//             None,
//         )
//     }

//     /// If min_lp_receive is Some provide is done via liquidity manager contract.
//     pub fn provide_liquidity_with_slip_tolerance(
//         &mut self,
//         sender: &Addr,
//         assets: &[Asset],
//         slippage_tolerance: Option<Decimal>,
//         min_lp_receive: Option<Uint128>,
//         auto_stake: bool,
//         receiver: Option<String>,
//     ) -> AnyResult<AppResponse> {
//         let msg = PairExecuteMsg::ProvideLiquidity {
//             assets: assets.to_vec(),
//             slippage_tolerance,
//             auto_stake: Some(auto_stake),
//             receiver,
//         };

//         if min_lp_receive.is_some() {
//             let funds = assets.mock_coins_sent(
//                 &mut self.app,
//                 sender,
//                 &self.liquidity_manager,
//                 SendType::Allowance,
//             );

//             let manager_msg = ExecuteMsg::ProvideLiquidity {
//                 pair_addr: self.pair_addr.to_string(),
//                 pair_msg: msg,
//                 min_lp_to_receive: min_lp_receive,
//             };
//             self.app.execute_contract(
//                 sender.clone(),
//                 self.liquidity_manager.clone(),
//                 &manager_msg,
//                 &funds,
//             )
//         } else {
//             let funds =
//                 assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);
//             self.app
//                 .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
//         }
//     }

//     pub fn withdraw_liquidity(
//         &mut self,
//         sender: &Addr,
//         amount: u128,
//         min_assets: Option<Vec<Asset>>,
//     ) -> AnyResult<AppResponse> {
//         let pair_msg = PairCw20HookMsg::WithdrawLiquidity { assets: vec![] };
//         let (contract, msg);
//         if let Some(min_assets_to_receive) = min_assets {
//             contract = self.liquidity_manager.to_string();
//             msg = to_binary(&Cw20HookMsg::WithdrawLiquidity {
//                 pair_msg,
//                 min_assets_to_receive,
//             })
//             .unwrap();
//         } else {
//             contract = self.pair_addr.to_string();
//             msg = to_binary(&pair_msg).unwrap();
//         }

//         let msg = Cw20ExecuteMsg::Send {
//             contract,
//             amount: Uint128::from(amount),
//             msg,
//         };

//         self.app
//             .execute_contract(sender.clone(), self.lp_token.clone(), &msg, &[])
//     }

//     pub fn swap(
//         &mut self,
//         sender: &Addr,
//         offer_asset: &Asset,
//         max_spread: Option<Decimal>,
//     ) -> AnyResult<AppResponse> {
//         match &offer_asset.info {
//             AssetInfo::Token { contract_addr } => {
//                 let msg = Cw20ExecuteMsg::Send {
//                     contract: self.pair_addr.to_string(),
//                     amount: offer_asset.amount,
//                     msg: to_binary(&PairCw20HookMsg::Swap {
//                         ask_asset_info: None,
//                         belief_price: None,
//                         max_spread,
//                         to: None,
//                     })
//                     .unwrap(),
//                 };

//                 self.app
//                     .execute_contract(sender.clone(), contract_addr.clone(), &msg, &[])
//             }
//             AssetInfo::NativeToken { .. } => {
//                 let funds = offer_asset.mock_coin_sent(
//                     &mut self.app,
//                     sender,
//                     &self.pair_addr,
//                     SendType::None,
//                 );

//                 let msg = PairExecuteMsg::Swap {
//                     offer_asset: offer_asset.clone(),
//                     ask_asset_info: None,
//                     belief_price: None,
//                     max_spread,
//                     to: None,
//                 };

//                 self.app
//                     .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
//             }
//         }
//     }

//     pub fn simulate_swap(
//         &self,
//         offer_asset: &Asset,
//         ask_asset_info: Option<AssetInfo>,
//     ) -> StdResult<SimulationResponse> {
//         self.app.wrap().query_wasm_smart(
//             &self.pair_addr,
//             &PairQueryMsg::Simulation {
//                 offer_asset: offer_asset.clone(),
//                 ask_asset_info,
//             },
//         )
//     }

//     pub fn simulate_reverse_swap(
//         &self,
//         ask_asset: &Asset,
//         offer_asset_info: Option<AssetInfo>,
//     ) -> StdResult<ReverseSimulationResponse> {
//         self.app.wrap().query_wasm_smart(
//             &self.pair_addr,
//             &PairQueryMsg::ReverseSimulation {
//                 ask_asset: ask_asset.clone(),
//                 offer_asset_info,
//             },
//         )
//     }

//     fn init_token(
//         app: &mut App,
//         token_code: u64,
//         name: String,
//         decimals: u8,
//         owner: &Addr,
//     ) -> Addr {
//         let init_balance = INIT_BALANCE * 10u128.pow(decimals as u32);
//         app.instantiate_contract(
//             token_code,
//             owner.clone(),
//             &astroport::token::InstantiateMsg {
//                 symbol: name.to_string(),
//                 name,
//                 decimals,
//                 initial_balances: vec![Cw20Coin {
//                     address: owner.to_string(),
//                     amount: Uint128::from(init_balance),
//                 }],
//                 mint: None,
//                 marketing: None,
//             },
//             &[],
//             "{name}_token",
//             None,
//         )
//         .unwrap()
//     }

//     pub fn token_balance(&self, token_addr: &Addr, user: &Addr) -> u128 {
//         let resp: BalanceResponse = self
//             .app
//             .wrap()
//             .query_wasm_smart(
//                 token_addr,
//                 &Cw20QueryMsg::Balance {
//                     address: user.to_string(),
//                 },
//             )
//             .unwrap();

//         resp.balance.u128()
//     }

//     pub fn coin_balance(&self, coin: &TestCoin, user: &Addr) -> u128 {
//         match &self.assets[coin] {
//             AssetInfo::Token { contract_addr } => self.token_balance(contract_addr, user),
//             AssetInfo::NativeToken { denom } => self
//                 .app
//                 .wrap()
//                 .query_balance(user, denom)
//                 .unwrap()
//                 .amount
//                 .u128(),
//         }
//     }

//     pub fn give_me_money(&mut self, assets: &[Asset], recipient: &Addr) {
//         let funds =
//             assets.mock_coins_sent(&mut self.app, &self.owner, recipient, SendType::Transfer);

//         if !funds.is_empty() {
//             self.app
//                 .send_tokens(self.owner.clone(), recipient.clone(), &funds)
//                 .unwrap();
//         }
//     }

//     pub fn query_config<T>(&self) -> StdResult<T>
//     where
//         T: DeserializeOwned,
//     {
//         let binary = self
//             .app
//             .wrap()
//             .query_wasm_raw(&self.pair_addr, b"config")?
//             .ok_or_else(|| StdError::generic_err("Failed to find config in storage"))?;
//         from_slice(&binary)
//     }

//     pub fn query_asset_balance_at(
//         &self,
//         asset_info: &AssetInfo,
//         block_height: u64,
//     ) -> StdResult<Option<Uint128>> {
//         self.app.wrap().query_wasm_smart(
//             &self.pair_addr,
//             &PairQueryMsg::AssetBalanceAt {
//                 asset_info: asset_info.clone(),
//                 block_height: block_height.into(),
//             },
//         )
//     }

//     pub fn query_staked_lp(&self, user: &Addr) -> StdResult<Uint128> {
//         self.app.wrap().query_wasm_smart(
//             &self.generator,
//             &generator::QueryMsg::Deposit {
//                 lp_token: self.lp_token.to_string(),
//                 user: user.to_string(),
//             },
//         )
//     }
// }

// #[derive(Clone, Copy)]
// pub enum SendType {
//     Allowance,
//     Transfer,
//     None,
// }

// pub trait AssetExt {
//     fn mock_coin_sent(
//         &self,
//         app: &mut App,
//         user: &Addr,
//         spender: &Addr,
//         typ: SendType,
//     ) -> Vec<Coin>;
// }

// impl AssetExt for Asset {
//     fn mock_coin_sent(
//         &self,
//         app: &mut App,
//         user: &Addr,
//         spender: &Addr,
//         typ: SendType,
//     ) -> Vec<Coin> {
//         let mut funds = vec![];
//         match &self.info {
//             AssetInfo::Token { contract_addr } if !self.amount.is_zero() => {
//                 let msg = match typ {
//                     SendType::Allowance => Cw20ExecuteMsg::IncreaseAllowance {
//                         spender: spender.to_string(),
//                         amount: self.amount,
//                         expires: None,
//                     },
//                     SendType::Transfer => Cw20ExecuteMsg::Transfer {
//                         recipient: spender.to_string(),
//                         amount: self.amount,
//                     },
//                     _ => unimplemented!(),
//                 };
//                 app.execute_contract(user.clone(), contract_addr.clone(), &msg, &[])
//                     .unwrap();
//             }
//             AssetInfo::NativeToken { denom } if !self.amount.is_zero() => {
//                 funds = vec![coin(self.amount.u128(), denom)];
//             }
//             _ => {}
//         }

//         funds
//     }
// }

// pub trait AssetsExt {
//     fn mock_coins_sent(
//         &self,
//         app: &mut App,
//         user: &Addr,
//         spender: &Addr,
//         typ: SendType,
//     ) -> Vec<Coin>;
// }

// impl AssetsExt for &[Asset] {
//     fn mock_coins_sent(
//         &self,
//         app: &mut App,
//         user: &Addr,
//         spender: &Addr,
//         typ: SendType,
//     ) -> Vec<Coin> {
//         let mut funds = vec![];
//         for asset in self.iter() {
//             funds.extend(asset.mock_coin_sent(app, user, spender, typ));
//         }
//         funds
//     }
// }

// pub trait AppExtension {
//     fn next_block(&mut self, time: u64);
// }

// impl AppExtension for App {
//     fn next_block(&mut self, time: u64) {
//         self.update_block(|block| {
//             block.time = block.time.plus_seconds(time);
//             block.height += 1
//         });
//     }
// }

// pub fn f64_to_dec<T>(val: f64) -> T
// where
//     T: FromStr,
//     T::Err: Error,
// {
//     T::from_str(&val.to_string()).unwrap()
// }

// pub fn dec_to_f64(val: impl Display) -> f64 {
//     f64::from_str(&val.to_string()).unwrap()
// }
