#![allow(dead_code)]

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Display;
use std::str::FromStr;

use anyhow::Result as AnyResult;
use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::observation::OracleObservation;
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, QueryMsg,
};
use astroport::token;
use astroport::token::Cw20Coin;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coin, coins, from_slice, to_binary, Addr, Coin, Decimal, Decimal256, Empty, GovMsg, IbcMsg,
    IbcQuery, MemoryStorage, StdError, StdResult, Storage, Uint128,
};
use cw_multi_test::{
    AddressGenerator, App, AppResponse, BankKeeper, BasicAppBuilder, Contract, ContractWrapper,
    DistributionKeeper, Executor, FailingModule, StakeKeeper, WasmKeeper,
};
use cw_storage_plus::Item;
use derivative::Derivative;
use itertools::Itertools;

use crate::common::neutron_ext::NeutronStargate;

const NATIVE_TOKEN_PRECISION: u8 = 6;

const FACTORY_ADDRESS: &str = "osmo1nc5tatafv6eyq7llkr2gv50ff9e22mnf70qgjlv737ktmt4eswrqvlx82r";

const INIT_BALANCE: u128 = 1_000_000_000000;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TestCoin {
    Native(String),
}

impl TestCoin {
    pub fn denom(&self) -> Option<String> {
        match self {
            TestCoin::Native(denom) => Some(denom.clone()),
            _ => None,
        }
    }

    pub fn native(denom: &str) -> Self {
        Self::Native(denom.to_string())
    }
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

pub fn init_native_coins(test_coins: &[TestCoin]) -> Vec<Coin> {
    let mut test_coins: Vec<Coin> = test_coins
        .iter()
        .filter_map(|test_coin| match test_coin {
            TestCoin::Native(name) => {
                let init_balance = INIT_BALANCE * 10u128.pow(NATIVE_TOKEN_PRECISION as u32);
                Some(coin(init_balance, name))
            }
            _ => None,
        })
        .collect();
    test_coins.push(coin(INIT_BALANCE, "random-coin"));
    test_coins.push(coin(INIT_BALANCE, "untrn"));

    test_coins
}

#[derive(Default)]
struct HackyAddressGenerator<'a> {
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> HackyAddressGenerator<'a> {
    pub const CONTRACTS_COUNT: Item<'a, u64> = Item::new("wasm_contracts_count");
    pub const FACTORY_MARKER: Item<'a, ()> = Item::new("factory_marker");
}

impl<'a> AddressGenerator for HackyAddressGenerator<'a> {
    fn next_address(&self, storage: &mut dyn Storage) -> Addr {
        if Self::FACTORY_MARKER.may_load(storage).unwrap().is_some() {
            Self::FACTORY_MARKER.remove(storage);

            Addr::unchecked(FACTORY_ADDRESS)
        } else {
            let count = if let Some(count) = Self::CONTRACTS_COUNT.may_load(storage).unwrap() {
                Self::CONTRACTS_COUNT.save(storage, &(count + 1)).unwrap();
                count + 1
            } else {
                Self::CONTRACTS_COUNT.save(storage, &1u64).unwrap();
                1
            };

            Addr::unchecked(format!("contract{count}"))
        }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: NeutronApp,
    pub owner: Addr,
    pub assets: HashMap<TestCoin, AssetInfo>,
    // pub factory: Addr,
    // pub pair_addr: Addr,
    // pub lp_token: String,
    // pub fake_maker: Addr,
}

impl Helper {
    pub fn new(
        owner: &Addr,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
    ) -> AnyResult<Self> {
        let mut app = BasicAppBuilder::new()
            .with_stargate(NeutronStargate::default())
            .with_wasm::<FailingModule<Empty, Empty, Empty>, WasmKeeper<Empty, Empty>>(
                WasmKeeper::new_with_custom_address_generator(HackyAddressGenerator::default()),
            )
            .build(|router, _, storage| {
                router
                    .bank
                    .init_balance(storage, owner, init_native_coins(&test_coins))
                    .unwrap()
            });

        // let token_code_id = app.store_code(token_contract());

        let asset_infos_vec = test_coins
            .iter()
            .cloned()
            .map(|coin| {
                let asset_info = match &coin {
                    TestCoin::Native(denom) => native_asset_info(denom.clone()),
                };
                (coin, asset_info)
            })
            .collect::<Vec<_>>();

        // let pair_code_id = app.store_code(pair_contract());
        // let factory_code_id = app.store_code(factory_contract());
        // let pair_type = PairType::Custom("concentrated".to_string());

        // let fake_maker = Addr::unchecked("fake_maker");

        // let coin_registry_id = app.store_code(coin_registry_contract());

        // let coin_registry_address = app
        //     .instantiate_contract(
        //         coin_registry_id,
        //         owner.clone(),
        //         &astroport::native_coin_registry::InstantiateMsg {
        //             owner: owner.to_string(),
        //         },
        //         &[],
        //         "Coin registry",
        //         None,
        //     )
        //     .unwrap();

        // app.execute_contract(
        //     owner.clone(),
        //     coin_registry_address.clone(),
        //     &astroport::native_coin_registry::ExecuteMsg::Add {
        //         native_coins: vec![
        //             ("uosmo".to_owned(), 6),
        //             ("uusd".to_owned(), 6),
        //             ("rc".to_owned(), 6),
        //             ("foo".to_owned(), 5),
        //         ],
        //     },
        //     &[],
        // )
        // .unwrap();
        // let init_msg = astroport::factory::InstantiateMsg {
        //     fee_address: Some(fake_maker.to_string()),
        //     pair_configs: vec![PairConfig {
        //         code_id: pair_code_id,
        //         maker_fee_bps: 5000,
        //         total_fee_bps: 0u16, // Concentrated pair does not use this field,
        //         pair_type: pair_type.clone(),
        //         is_disabled: false,
        //         is_generator_disabled: false,
        //     }],
        //     token_code_id,
        //     generator_address: None,
        //     owner: owner.to_string(),
        //     whitelist_code_id: 234u64,
        //     coin_registry_address: coin_registry_address.to_string(),
        // };

        // Set marker in storage that the next contract is factory. We need this to have exact FACTORY_ADDRESS constant
        // which is hardcoded in the PCL code.
        app.init_modules(|_, _, storage| HackyAddressGenerator::FACTORY_MARKER.save(storage, &()))
            .unwrap();
        // let factory = app.instantiate_contract(
        //     factory_code_id,
        //     owner.clone(),
        //     &init_msg,
        //     &[],
        //     "Factory",
        //     None,
        // )?;

        // let asset_infos = asset_infos_vec
        //     .clone()
        //     .into_iter()
        //     .map(|(_, asset_info)| asset_info)
        //     .collect_vec();
        // let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
        //     pair_type,
        //     asset_infos: asset_infos.clone(),
        //     init_params: Some(to_binary(&params).unwrap()),
        // };

        // app.execute_contract(
        //     owner.clone(),
        //     factory.clone(),
        //     &init_pair_msg,
        //     &osmo_create_pair_fee(),
        // )?;

        // let resp: PairInfo = app.wrap().query_wasm_smart(
        //     &factory,
        //     &astroport::factory::QueryMsg::Pair { asset_infos },
        // )?;

        Ok(Self {
            app,
            owner: owner.clone(),
            assets: asset_infos_vec.into_iter().collect(),
            // factory,
            // pair_addr: resp.contract_addr,
            // lp_token: resp.liquidity_token.to_string(),
            // fake_maker,
        })
    }

    pub fn native_balance(&self, denom: &str, user: &Addr) -> u128 {
        self.app
            .wrap()
            .query_balance(user, denom)
            .unwrap()
            .amount
            .u128()
    }

    pub fn token_balance(&self, token_addr: &Addr, user: &Addr) -> u128 {
        let resp: token::BalanceResponse = self
            .app
            .wrap()
            .query_wasm_smart(
                token_addr,
                &token::QueryMsg::Balance {
                    address: user.to_string(),
                },
            )
            .unwrap();

        resp.balance.u128()
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
        app: &mut NeutronApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetExt for Asset {
    fn mock_coin_sent(
        &self,
        app: &mut NeutronApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin> {
        let mut funds = vec![];
        match &self.info {
            AssetInfo::Token { contract_addr } if !self.amount.is_zero() => {
                let msg = match typ {
                    SendType::Allowance => token::ExecuteMsg::IncreaseAllowance {
                        spender: spender.to_string(),
                        amount: self.amount,
                        expires: None,
                    },
                    SendType::Transfer => token::ExecuteMsg::Transfer {
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
        app: &mut NeutronApp,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetsExt for &[Asset] {
    fn mock_coins_sent(
        &self,
        app: &mut NeutronApp,
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

impl AppExtension for NeutronApp {
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
