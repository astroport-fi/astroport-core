// SPDX-License-Identifier: GPL-3.0-only
// Copyright Astroport
// Copyright Anchor Protocol
// Copyright Lido

use basset::hub::{CurrentBatchResponse, Parameters, StateResponse};
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Coin, ContractResult, Decimal, Empty, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use cw20::TokenInfoResponse;
use std::str::FromStr;

pub const MOCK_HUB_CONTRACT_ADDR: &str = "hub";
pub const MOCK_BLUNA_TOKEN_CONTRACT_ADDR: &str = "token";
pub const MOCK_STLUNA_TOKEN_CONTRACT_ADDR: &str = "stluna_token";

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = String::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(&contract_addr, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {}", e),
                    request: bin_request.into(),
                })
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if *contract_addr == MOCK_STLUNA_TOKEN_CONTRACT_ADDR {
                    let token_inf: TokenInfoResponse = TokenInfoResponse {
                        name: "stluna".to_string(),
                        symbol: "stLUNA".to_string(),
                        decimals: 6,
                        total_supply: Uint128::new(10000u128),
                    };
                    SystemResult::Ok(ContractResult::Ok(to_binary(&token_inf).unwrap()))
                } else if *contract_addr == MOCK_BLUNA_TOKEN_CONTRACT_ADDR {
                    let token_inf: TokenInfoResponse = TokenInfoResponse {
                        name: "bluna".to_string(),
                        symbol: "bLUNA".to_string(),
                        decimals: 6,
                        total_supply: Uint128::new(10000u128),
                    };
                    SystemResult::Ok(ContractResult::Ok(to_binary(&token_inf).unwrap()))
                } else if *contract_addr == MOCK_HUB_CONTRACT_ADDR {
                    match from_binary(msg).unwrap() {
                        basset::hub::QueryMsg::CurrentBatch {} => {
                            let batch = CurrentBatchResponse {
                                id: 1,
                                requested_bluna_with_fee: Default::default(),
                                requested_stluna: Default::default(),
                                requested_with_fee: Default::default(),
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&batch)))
                        }
                        basset::hub::QueryMsg::State {} => {
                            let state = StateResponse {
                                bluna_exchange_rate: Decimal::from_str("0.95").unwrap(),
                                stluna_exchange_rate: Decimal::from_str("1.5").unwrap(),
                                total_bond_bluna_amount: Uint128::new(9500u128),
                                total_bond_stluna_amount: Uint128::new(15000u128),
                                last_index_modification: 0,
                                prev_hub_balance: Default::default(),
                                last_unbonded_time: 0,
                                last_processed_batch: 0,
                                total_bond_amount: Default::default(),
                                exchange_rate: Decimal::from_str("0.95").unwrap(),
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&state)))
                        }
                        basset::hub::QueryMsg::Parameters {} => {
                            let params = Parameters {
                                epoch_period: 0,
                                underlying_coin_denom: "".to_string(),
                                unbonding_period: 0,
                                peg_recovery_fee: Decimal::from_str("0.05").unwrap(),
                                er_threshold: Decimal::from_str("1.0").unwrap(),
                                reward_denom: "".to_string(),
                                paused: None,
                            };
                            SystemResult::Ok(ContractResult::from(to_binary(&params)))
                        }
                        _ => unimplemented!(),
                    }
                } else {
                    unimplemented!()
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier { base }
    }
}
