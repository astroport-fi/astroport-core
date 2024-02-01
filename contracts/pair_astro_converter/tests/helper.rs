#![cfg(not(tarpaulin_include))]
#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::Result as AnyResult;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, from_json, to_json_binary, Addr, Coin, Decimal, Empty, StdError, StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};
use derivative::Derivative;
use itertools::Itertools;

use astroport::asset::{native_asset_info, token_asset_info, Asset, AssetInfo, PairInfo};
use astroport::astro_converter;
use astroport::astro_converter::OutpostBurnParams;
use astroport::factory::{PairConfig, PairType};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, PoolResponse, ReverseSimulationResponse,
    SimulationResponse,
};
use astroport::pair_concentrated::QueryMsg;
use astroport_pair_converter::state::Config;

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

pub fn token_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_token::contract::execute,
        astroport_token::contract::instantiate,
        astroport_token::contract::query,
    ))
}

fn pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair::contract::execute,
            astroport_pair::contract::instantiate,
            astroport_pair::contract::query,
        )
        .with_reply_empty(astroport_pair::contract::reply),
    )
}

pub fn converter_pair_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_pair_converter::contract::execute,
            astroport_pair_converter::contract::instantiate,
            astroport_pair_converter::queries::query,
        )
        .with_migrate(astroport_pair_converter::contract::migrate),
    )
}

pub fn converter_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astro_token_converter::contract::execute,
        astro_token_converter::contract::instantiate,
        astro_token_converter::contract::query,
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

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Helper {
    #[derivative(Debug = "ignore")]
    pub app: App,
    pub owner: Addr,
    pub assets: HashMap<TestCoin, AssetInfo>,
    pub factory: Addr,
    pub pair_addr: Addr,
}

impl Helper {
    pub fn new(owner: &Addr, test_coins: Vec<TestCoin>) -> AnyResult<Self> {
        let mut app = App::new(|router, _, storage| {
            router
                .bank
                .init_balance(storage, owner, init_native_coins(&test_coins))
                .unwrap()
        });

        let token_code_id = app.store_code(token_contract());

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
                };
                (coin, asset_info)
            })
            .collect::<Vec<_>>();

        let pair_code_id = app.store_code(pair_contract());
        let factory_code_id = app.store_code(factory_contract());
        let pair_type = PairType::Xyk {};

        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: None,
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                maker_fee_bps: 3333,
                total_fee_bps: 30u16,
                pair_type: pair_type.clone(),
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: false,
            }],
            token_code_id,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: "registry".to_string(),
        };

        let factory = app.instantiate_contract(
            factory_code_id,
            owner.clone(),
            &init_msg,
            &[],
            "factory",
            None,
        )?;

        let asset_infos = asset_infos_vec
            .clone()
            .into_iter()
            .map(|(_, asset_info)| asset_info)
            .collect_vec();
        let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.clone(),
            init_params: None,
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
        })
    }

    pub fn setup_converter(
        &mut self,
        old_astro_asset_info: AssetInfo,
        new_astro_denom: &str,
    ) -> AnyResult<(Addr, u64)> {
        let converter_pair_code_id = self.app.store_code(converter_pair_contract());
        let converter_code_id = self.app.store_code(converter_contract());
        self.app
            .instantiate_contract(
                converter_code_id,
                self.owner.clone(),
                &astro_converter::InstantiateMsg {
                    outpost_burn_params: if matches!(
                        &old_astro_asset_info,
                        AssetInfo::NativeToken { .. }
                    ) {
                        Some(OutpostBurnParams {
                            terra_burn_addr: "terra1xxx".to_string(),
                            old_astro_transfer_channel: "channel-100".to_string(),
                        })
                    } else {
                        None
                    },
                    old_astro_asset_info,
                    new_astro_denom: new_astro_denom.to_string(),
                },
                &[],
                "converter",
                None,
            )
            .map(|addr| (addr, converter_pair_code_id))
    }

    pub fn setup_converter_and_migrate(&mut self, old: &TestCoin, new: &TestCoin) {
        let (converter_addr, converter_pair_code_id) = self
            .setup_converter(self.assets[old].clone(), &self.assets[new].to_string())
            .unwrap();
        // Top up converter with new ASTRO
        self.give_me_money(
            &[Asset::native(
                self.assets[new].to_string(),
                100_000_000_000000u128,
            )],
            &converter_addr,
        );
        self.app
            .migrate_contract(
                self.owner.clone(),
                self.pair_addr.clone(),
                &astroport_pair_converter::migration::MigrateMsg {
                    converter_contract: converter_addr.to_string(),
                },
                converter_pair_code_id,
            )
            .unwrap();
    }

    pub fn provide_liquidity(&mut self, sender: &Addr, assets: &[Asset]) -> AnyResult<AppResponse> {
        let funds =
            assets.mock_coins_sent(&mut self.app, sender, &self.pair_addr, SendType::Allowance);

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance: None,
            auto_stake: None,
            receiver: None,
        };

        self.app
            .execute_contract(sender.clone(), self.pair_addr.clone(), &msg, &funds)
    }

    pub fn swap(&mut self, sender: &Addr, offer_asset: &Asset) -> AnyResult<AppResponse> {
        match &offer_asset.info {
            AssetInfo::Token { contract_addr } => {
                let msg = Cw20ExecuteMsg::Send {
                    contract: self.pair_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_json_binary(&Cw20HookMsg::Swap {
                        ask_asset_info: None,
                        belief_price: None,
                        max_spread: None,
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
                    max_spread: None,
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

    fn init_token(
        app: &mut App,
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
        from_json(&binary)
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
        app: &mut App,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetExt for Asset {
    fn mock_coin_sent(
        &self,
        app: &mut App,
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
        app: &mut App,
        user: &Addr,
        spender: &Addr,
        typ: SendType,
    ) -> Vec<Coin>;
}

impl AssetsExt for &[Asset] {
    fn mock_coins_sent(
        &self,
        app: &mut App,
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
