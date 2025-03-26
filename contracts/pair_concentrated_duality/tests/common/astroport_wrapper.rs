#![cfg(not(tarpaulin_include))]
#![cfg(feature = "test-tube")]
#![allow(dead_code)]

use std::collections::HashMap;

use anyhow::Result as AnyResult;
use cosmwasm_std::{coin, from_json, to_json_binary, Addr, Coin, Decimal, Uint128};
use itertools::Itertools;
use neutron_test_tube::cosmrs::proto::cosmwasm::wasm::v1::{
    MsgExecuteContractResponse, QueryRawContractStateRequest, QueryRawContractStateResponse,
};
use neutron_test_tube::{Account, ExecuteResponse, Runner, SigningAccount};

use astroport::pair::{PoolResponse, SimulationResponse};
use astroport::pair_concentrated_duality::{DualityPairMsg, UpdateDualityOrderbook};
use astroport::{
    asset::{native_asset_info, Asset, AssetInfo, PairInfo},
    factory::{PairConfig, PairType},
    pair,
    pair_concentrated::ConcentratedPoolParams,
    pair_concentrated_duality::{ConcentratedDualityParams, OrderbookConfig},
};
use astroport_pair_concentrated_duality::orderbook::state::OrderbookState;
use astroport_pcl_common::state::Config;
use astroport_test::coins::TestCoin;
use astroport_test::convert::f64_to_dec;

use super::neutron_wrapper::TestAppWrapper;

type ExecuteMsg = pair::ExecuteMsgExt<DualityPairMsg>;

const INIT_BALANCE: u128 = u128::MAX;

pub fn init_native_coins(test_coins: &[TestCoin]) -> Vec<Coin> {
    let mut has_ntrn = false;
    let mut test_coins: Vec<Coin> = test_coins
        .iter()
        .filter_map(|test_coin| match test_coin {
            TestCoin::Native(name) => {
                if name == "untrn" {
                    has_ntrn = true;
                };
                Some(coin(INIT_BALANCE, name))
            }
            _ => None,
        })
        .collect();
    if !has_ntrn {
        test_coins.push(coin(INIT_BALANCE, "untrn"));
    }

    test_coins
}

pub struct AstroportHelper<'a> {
    pub helper: &'a TestAppWrapper<'a>,
    pub owner: SigningAccount,
    pub assets: HashMap<TestCoin, AssetInfo>,
    pub factory: Addr,
    pub maker: Addr,
    pub pair_addr: Addr,
    pub lp_token: String,
    pub token_a: String,
    pub token_b: String,
}

impl<'a> AstroportHelper<'a> {
    pub fn new(
        helper: &'a TestAppWrapper<'a>,
        test_coins: Vec<TestCoin>,
        params: ConcentratedPoolParams,
        orderbook_config: OrderbookConfig,
    ) -> AnyResult<Self> {
        let signer = helper.app.init_account(&init_native_coins(&test_coins))?;
        let owner = Addr::unchecked(signer.address());

        let asset_infos_vec: Vec<_> = test_coins
            .clone()
            .into_iter()
            .filter_map(|coin| Some((coin.clone(), native_asset_info(coin.denom()?))))
            .collect();

        // We don't support cw20
        // test_coins.iter().for_each(|coin| {
        //     if let Some((name, decimals)) = coin.cw20_init_data() {
        //         let token_addr = Self::init_token(&helper, name, decimals, &owner);
        //         asset_infos_vec.push((coin.clone(), token_asset_info(token_addr)))
        //     }
        // });

        let maker = helper.app.init_account(&[])?;
        let maker_addr = Addr::unchecked(maker.address());

        let coin_registry_address = helper
            .init_contract(
                helper.code_ids["coin-registry"],
                &astroport::native_coin_registry::InstantiateMsg {
                    owner: owner.to_string(),
                },
                &[],
            )
            .unwrap();

        asset_infos_vec
            .iter()
            .try_for_each(|(test_coin, _)| match &test_coin {
                TestCoin::NativePrecise(denom, decimals) => helper
                    .execute_contract(
                        &signer,
                        coin_registry_address.as_str(),
                        &astroport::native_coin_registry::ExecuteMsg::Add {
                            native_coins: vec![(denom.to_owned(), *decimals)],
                        },
                        &[],
                    )
                    .map(|_| ()),
                TestCoin::Native(denom) => helper
                    .execute_contract(
                        &signer,
                        coin_registry_address.as_str(),
                        &astroport::native_coin_registry::ExecuteMsg::Add {
                            native_coins: vec![(denom.to_owned(), 6)],
                        },
                        &[],
                    )
                    .map(|_| ()),
                _ => Ok(()),
            })
            .unwrap();

        let pair_type = PairType::Custom("concentrated_duality_orderbook".to_string());

        let init_msg = astroport::factory::InstantiateMsg {
            fee_address: Some(maker_addr.to_string()),
            pair_configs: vec![PairConfig {
                code_id: helper.code_ids["pair-concentrated-duality"],
                maker_fee_bps: 5000,
                total_fee_bps: 0u16, // Concentrated pair does not use this field,
                pair_type: pair_type.clone(),
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: true,
            }],
            token_code_id: 0,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: coin_registry_address.to_string(),
            tracker_config: None,
        };

        let factory = helper
            .init_contract(helper.code_ids["factory"], &init_msg, &[])
            .unwrap();

        let asset_infos = asset_infos_vec
            .clone()
            .into_iter()
            .map(|(_, asset_info)| asset_info)
            .collect_vec();

        let pcl_duality_params = ConcentratedDualityParams {
            main_params: params,
            orderbook_config,
        };

        let init_pair_msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.clone(),
            init_params: Some(to_json_binary(&pcl_duality_params).unwrap()),
        };

        helper
            .execute_contract(&signer, factory.as_str(), &init_pair_msg, &[])
            .unwrap();

        let PairInfo {
            liquidity_token,
            contract_addr,
            ..
        } = helper.smart_query(
            &factory,
            &astroport::factory::QueryMsg::Pair {
                asset_infos: asset_infos.clone(),
            },
        )?;

        Ok(Self {
            helper,
            owner: signer,
            assets: asset_infos_vec
                .into_iter()
                .map(|(test_coin, asset_info)| (test_coin, asset_info))
                .collect(),
            maker: maker_addr,
            factory: Addr::unchecked(factory),
            pair_addr: contract_addr,
            lp_token: liquidity_token,
            token_a: asset_infos[0].to_string(),
            token_b: asset_infos[1].to_string(),
        })
    }

    pub fn provide_liquidity(
        &self,
        sender: &SigningAccount,
        assets: &[Asset],
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        self.provide_liquidity_with_slip_tolerance(
            sender,
            assets,
            Some(f64_to_dec(0.5)), // 50% slip tolerance for testing purposes
        )
    }

    pub fn provide_liquidity_with_slip_tolerance(
        &self,
        sender: &SigningAccount,
        assets: &[Asset],
        slippage_tolerance: Option<Decimal>,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        let funds = assets
            .iter()
            .map(|a| a.as_coin().unwrap())
            .sorted_by(|a, b| a.denom.cmp(&b.denom))
            .collect_vec();

        let msg = ExecuteMsg::ProvideLiquidity {
            assets: assets.to_vec(),
            slippage_tolerance,
            auto_stake: None,
            receiver: None,
            min_lp_to_receive: None,
        };

        self.helper
            .execute_contract(sender, self.pair_addr.as_str(), &msg, &funds)
    }

    pub fn withdraw_liquidity(
        &self,
        sender: &SigningAccount,
        lp_tokens: Coin,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        let msg = ExecuteMsg::WithdrawLiquidity {
            assets: vec![],
            min_assets_to_receive: None,
        };

        self.helper
            .execute_contract(sender, self.pair_addr.as_str(), &msg, &vec![lp_tokens])
    }

    pub fn swap_max_spread(
        &self,
        sender: &SigningAccount,
        offer_asset: &Asset,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        self.swap(sender, offer_asset, Some(f64_to_dec(0.5)))
    }

    pub fn swap(
        &self,
        sender: &SigningAccount,
        offer_asset: &Asset,
        max_spread: Option<Decimal>,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        match &offer_asset.info {
            AssetInfo::Token { .. } => unimplemented!(),
            AssetInfo::NativeToken { .. } => {
                let funds = [offer_asset.as_coin().unwrap()];

                let msg = ExecuteMsg::Swap {
                    offer_asset: offer_asset.clone(),
                    ask_asset_info: None,
                    belief_price: None,
                    max_spread,
                    to: None,
                };

                self.helper
                    .execute_contract(sender, self.pair_addr.as_str(), &msg, &funds)
            }
        }
    }

    pub fn pool_balances(&self) -> AnyResult<PoolResponse> {
        self.helper
            .wasm
            .query(self.pair_addr.as_str(), &pair::QueryMsg::Pool {})
            .map(|res: PoolResponse| PoolResponse {
                assets: res
                    .assets
                    .into_iter()
                    .sorted_by(|a, b| a.info.to_string().cmp(&b.info.to_string()))
                    .collect_vec(),
                ..res
            })
            .map_err(Into::into)
    }

    pub fn sync_orders(
        &self,
        sender: &SigningAccount,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        self.helper.execute_contract(
            sender,
            self.pair_addr.as_str(),
            &ExecuteMsg::Custom(DualityPairMsg::SyncOrderbook {}),
            &[],
        )
    }

    pub fn enable_orderbook(
        &self,
        sender: &SigningAccount,
        enable: bool,
    ) -> AnyResult<ExecuteResponse<MsgExecuteContractResponse>> {
        self.helper.execute_contract(
            sender,
            self.pair_addr.as_str(),
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

    pub fn query_ob_config(&self) -> AnyResult<OrderbookState> {
        let res = self
            .helper
            .app
            .query::<QueryRawContractStateRequest, QueryRawContractStateResponse>(
                "/cosmwasm.wasm.v1.Query/RawContractState",
                &QueryRawContractStateRequest {
                    address: self.pair_addr.to_string(),
                    query_data: b"orderbook_config".to_vec(),
                },
            )?;

        Ok(from_json(&res.data)?)
    }

    pub fn query_config(&self) -> AnyResult<Config> {
        let res = self
            .helper
            .app
            .query::<QueryRawContractStateRequest, QueryRawContractStateResponse>(
                "/cosmwasm.wasm.v1.Query/RawContractState",
                &QueryRawContractStateRequest {
                    address: self.pair_addr.to_string(),
                    query_data: b"config".to_vec(),
                },
            )?;

        Ok(from_json(&res.data)?)
    }

    pub fn simulate_swap(&self, offer_asset: Asset) -> AnyResult<SimulationResponse> {
        self.helper
            .wasm
            .query(
                self.pair_addr.as_str(),
                &pair::QueryMsg::Simulation {
                    offer_asset,
                    ask_asset_info: None,
                },
            )
            .map_err(Into::into)
    }

    pub fn simulate_provide_liquidity(&self, assets: Vec<Asset>) -> AnyResult<Uint128> {
        self.helper
            .wasm
            .query(
                self.pair_addr.as_str(),
                &pair::QueryMsg::SimulateProvide {
                    assets,
                    slippage_tolerance: None,
                },
            )
            .map_err(Into::into)
    }

    pub fn simulate_withdraw_liquidity(&self, lp_amount: Uint128) -> AnyResult<Vec<Asset>> {
        self.helper
            .wasm
            .query(
                self.pair_addr.as_str(),
                &pair::QueryMsg::SimulateWithdraw { lp_amount },
            )
            .map_err(Into::into)
    }
}
