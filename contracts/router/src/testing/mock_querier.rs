use cosmwasm_schema::cw_serde;
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Binary, Coin, ContractResult, Empty, OwnedDeps,
    Querier, QuerierResult, QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use std::collections::HashMap;

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::pair::SimulationResponse;
use cw20::{BalanceResponse, Cw20QueryMsg, TokenInfoResponse};

#[cw_serde]
pub enum QueryMsg {
    Pair { asset_infos: [AssetInfo; 2] },
    Simulation { offer_asset: Asset },
}

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies.
/// This uses the Astroport CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
        custom_query_type: Default::default(),
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    token_querier: TokenQuerier,
    astroport_factory_querier: AstroportFactoryQuerier,
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // This lets us iterate over all pairs that match the first string
    balances: HashMap<String, HashMap<String, Uint128>>,
}

impl TokenQuerier {
    pub fn new(balances: &[(&String, &[(&String, &Uint128)])]) -> Self {
        TokenQuerier {
            balances: balances_to_map(balances),
        }
    }
}

pub(crate) fn balances_to_map(
    balances: &[(&String, &[(&String, &Uint128)])],
) -> HashMap<String, HashMap<String, Uint128>> {
    let mut balances_map: HashMap<String, HashMap<String, Uint128>> = HashMap::new();
    for (contract_addr, balances) in balances.iter() {
        let mut contract_balances_map: HashMap<String, Uint128> = HashMap::new();
        for (addr, balance) in balances.iter() {
            contract_balances_map.insert(addr.to_string(), **balance);
        }

        balances_map.insert(contract_addr.to_string(), contract_balances_map);
    }
    balances_map
}

#[derive(Clone, Default)]
pub struct AstroportFactoryQuerier {
    pairs: HashMap<String, String>,
}

impl AstroportFactoryQuerier {
    pub fn new(pairs: &[(&String, &String)]) -> Self {
        AstroportFactoryQuerier {
            pairs: pairs_to_map(pairs),
        }
    }
}

pub(crate) fn pairs_to_map(pairs: &[(&String, &String)]) -> HashMap<String, String> {
    let mut pairs_map: HashMap<String, String> = HashMap::new();
    for (key, pair) in pairs.iter() {
        pairs_map.insert(key.to_string(), pair.to_string());
    }
    pairs_map
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

#[cw_serde]
pub enum MockQueryMsg {
    Price {},
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        match &request {
            // QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
            //     if route == &TerraRoute::Treasury {
            //         match query_data {
            //             TerraQuery::TaxRate {} => {
            //                 let res = TaxRateResponse {
            //                     rate: self.tax_querier.rate,
            //                 };
            //                 SystemResult::Ok(ContractResult::from(to_binary(&res)))
            //             }
            //             TerraQuery::TaxCap { denom } => {
            //                 let cap = self
            //                     .tax_querier
            //                     .caps
            //                     .get(denom)
            //                     .copied()
            //                     .unwrap_or_default();
            //                 let res = TaxCapResponse { cap };
            //                 SystemResult::Ok(ContractResult::from(to_binary(&res)))
            //             }
            //             _ => panic!("DO NOT ENTER HERE"),
            //         }
            //     } else if route == &TerraRoute::Market {
            //         match query_data {
            //             TerraQuery::Swap {
            //                 offer_coin,
            //                 ask_denom: _,
            //             } => {
            //                 let res = SwapResponse {
            //                     receive: offer_coin.clone(),
            //                 };
            //                 SystemResult::Ok(ContractResult::from(to_binary(&res)))
            //             }
            //             _ => panic!("DO NOT ENTER HERE"),
            //         }
            //     } else {
            //         panic!("DO NOT ENTER HERE")
            //     }
            // }
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                if contract_addr.to_string().starts_with("token")
                    || contract_addr.to_string().starts_with("asset")
                {
                    self.handle_cw20(&contract_addr, &msg)
                } else {
                    self.handle_default(&msg)
                }
            }
            _ => self.base.handle_query(request),
        }
    }

    fn handle_default(&self, msg: &Binary) -> QuerierResult {
        match from_binary(&msg).unwrap() {
            QueryMsg::Pair { asset_infos } => {
                let key = asset_infos[0].to_string() + asset_infos[1].to_string().as_str();
                match self.astroport_factory_querier.pairs.get(&key) {
                    Some(v) => SystemResult::Ok(ContractResult::from(to_binary(&PairInfo {
                        contract_addr: Addr::unchecked(v),
                        liquidity_token: Addr::unchecked("liquidity"),
                        asset_infos: [
                            AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                            AssetInfo::NativeToken {
                                denom: "uusd".to_string(),
                            },
                        ],
                        pair_type: PairType::Xyk {},
                    }))),
                    None => SystemResult::Err(SystemError::InvalidRequest {
                        error: "No pair info exists".to_string(),
                        request: msg.as_slice().into(),
                    }),
                }
            }
            QueryMsg::Simulation { offer_asset } => {
                SystemResult::Ok(ContractResult::from(to_binary(&SimulationResponse {
                    return_amount: offer_asset.amount,
                    commission_amount: Uint128::zero(),
                    spread_amount: Uint128::zero(),
                })))
            }
        }
    }

    fn handle_cw20(&self, contract_addr: &String, msg: &Binary) -> QuerierResult {
        match from_binary(&msg).unwrap() {
            Cw20QueryMsg::TokenInfo {} => {
                let balances: &HashMap<String, Uint128> =
                    match self.token_querier.balances.get(contract_addr) {
                        Some(balances) => balances,
                        None => {
                            return SystemResult::Err(SystemError::Unknown {});
                        }
                    };

                let mut total_supply = Uint128::zero();

                for balance in balances {
                    total_supply += *balance.1;
                }

                SystemResult::Ok(ContractResult::from(to_binary(&TokenInfoResponse {
                    name: "mAPPL".to_string(),
                    symbol: "mAPPL".to_string(),
                    decimals: 6,
                    total_supply: total_supply,
                })))
            }
            Cw20QueryMsg::Balance { address } => {
                let balances: &HashMap<String, Uint128> =
                    match self.token_querier.balances.get(contract_addr) {
                        Some(balances) => balances,
                        None => {
                            return SystemResult::Err(SystemError::Unknown {});
                        }
                    };

                let balance = match balances.get(&address) {
                    Some(v) => v,
                    None => {
                        return SystemResult::Err(SystemError::Unknown {});
                    }
                };

                SystemResult::Ok(ContractResult::from(to_binary(&BalanceResponse {
                    balance: *balance,
                })))
            }
            _ => panic!("DO NOT ENTER HERE"),
        }
    }
}

impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
            astroport_factory_querier: AstroportFactoryQuerier::default(),
        }
    }

    pub fn with_balance(&mut self, balances: &[(&String, &[Coin])]) {
        for (addr, balance) in balances {
            self.base.update_balance(addr.clone(), balance.to_vec());
        }
    }

    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances);
    }

    pub fn with_astroport_pairs(&mut self, pairs: &[(&String, &String)]) {
        self.astroport_factory_querier = AstroportFactoryQuerier::new(pairs);
    }
}
