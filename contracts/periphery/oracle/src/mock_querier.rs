use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::factory::QueryMsg::Pair;
use astroport::pair::CumulativePricesResponse;
use astroport::pair::QueryMsg::CumulativePrices;
use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_json, to_json_binary, Addr, Coin, Empty, OwnedDeps, Querier, QuerierResult, QueryRequest,
    SystemError, SystemResult, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg, TokenInfoResponse};
use std::collections::HashMap;

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
}

#[derive(Clone, Default)]
pub struct TokenQuerier {
    // this lets us iterate over all pairs that match the first string
    pairs: HashMap<String, CumulativePricesResponse>,
    balances: HashMap<String, HashMap<String, Uint128>>,
}

impl TokenQuerier {
    pub fn set(
        &mut self,
        pair: Addr,
        assets: Vec<Asset>,
        total: Uint128,
        cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    ) {
        self.pairs = HashMap::new();
        self.pairs.insert(
            pair.to_string(),
            CumulativePricesResponse {
                assets,
                total_share: total,
                cumulative_prices,
            },
        );
    }

    pub fn new(balances: &[(&String, &[(&String, &Uint128)])]) -> Self {
        TokenQuerier {
            pairs: Default::default(),
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

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_json(bin_request) {
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
                if contract_addr == "factory" {
                    match from_json(&msg).unwrap() {
                        Pair { asset_infos } => SystemResult::Ok(
                            to_json_binary(&PairInfo {
                                asset_infos,
                                contract_addr: Addr::unchecked("pair"),
                                liquidity_token: "lp_token".to_string(),
                                pair_type: PairType::Xyk {},
                            })
                            .into(),
                        ),
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else if contract_addr == "pair" {
                    match from_json(&msg).unwrap() {
                        CumulativePrices { .. } => {
                            let balance = match self.token_querier.pairs.get(contract_addr) {
                                Some(v) => v,
                                None => {
                                    return SystemResult::Err(SystemError::Unknown {});
                                }
                            };
                            SystemResult::Ok(to_json_binary(&balance).into())
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                } else {
                    match from_json(&msg).unwrap() {
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

                            SystemResult::Ok(
                                to_json_binary(&TokenInfoResponse {
                                    name: "mAPPL".to_string(),
                                    symbol: "mAPPL".to_string(),
                                    decimals: 6,
                                    total_supply: total_supply,
                                })
                                .into(),
                            )
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

                            SystemResult::Ok(
                                to_json_binary(&BalanceResponse { balance: *balance }).into(),
                            )
                        }
                        _ => panic!("DO NOT ENTER HERE"),
                    }
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}
impl WasmMockQuerier {
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            token_querier: TokenQuerier::default(),
        }
    }

    pub fn set_cumulative_price(
        &mut self,
        pair: Addr,
        assert: Vec<Asset>,
        total: Uint128,
        cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    ) {
        self.token_querier
            .set(pair, assert, total, cumulative_prices)
    }

    // Configure the mint whitelist mock querier
    pub fn with_token_balances(&mut self, balances: &[(&String, &[(&String, &Uint128)])]) {
        self.token_querier = TokenQuerier::new(balances);
    }
}
