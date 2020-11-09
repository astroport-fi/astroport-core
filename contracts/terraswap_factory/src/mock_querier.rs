use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Api, Coin, Empty, Extern, HumanAddr, Querier, QuerierResult,
    QueryRequest, SystemError, WasmQuery,
};
use cosmwasm_storage::to_length_prefixed;
use std::collections::HashMap;

use terraswap::{AssetInfoRaw, PairInfo, PairInfoRaw};

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    canonical_length: usize,
    contract_balance: &[Coin],
) -> Extern<MockStorage, MockApi, WasmMockQuerier> {
    let contract_addr = HumanAddr::from(MOCK_CONTRACT_ADDR);
    let custom_querier: WasmMockQuerier = WasmMockQuerier::new(
        MockQuerier::new(&[(&contract_addr, contract_balance)]),
        canonical_length,
        MockApi::new(canonical_length),
    );

    Extern {
        storage: MockStorage::default(),
        api: MockApi::new(canonical_length),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    terraswap_pair_querier: TerraswapPairQuerier,
    canonical_length: usize,
}

#[derive(Clone, Default)]
pub struct TerraswapPairQuerier {
    pairs: HashMap<HumanAddr, PairInfo>,
}

impl TerraswapPairQuerier {
    pub fn new(pairs: &[(&HumanAddr, &PairInfo)]) -> Self {
        TerraswapPairQuerier {
            pairs: pairs_to_map(pairs),
        }
    }
}

pub(crate) fn pairs_to_map(pairs: &[(&HumanAddr, &PairInfo)]) -> HashMap<HumanAddr, PairInfo> {
    let mut pairs_map: HashMap<HumanAddr, PairInfo> = HashMap::new();
    for (key, pair) in pairs.iter() {
        pairs_map.insert(HumanAddr::from(key), (*pair).clone());
    }
    pairs_map
}

impl Querier for WasmMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<Empty> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return Err(SystemError::InvalidRequest {
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
            QueryRequest::Wasm(WasmQuery::Raw { contract_addr, key }) => {
                let key: &[u8] = key.as_slice();
                let prefix_pair_info = to_length_prefixed(b"pair_info").to_vec();

                if key.to_vec() == prefix_pair_info {
                    let pair_info: PairInfo =
                        match self.terraswap_pair_querier.pairs.get(&contract_addr) {
                            Some(v) => v.clone(),
                            None => {
                                return Err(SystemError::InvalidRequest {
                                    error: format!("PairInfo is not found for {}", contract_addr),
                                    request: key.into(),
                                })
                            }
                        };

                    let api: MockApi = MockApi::new(self.canonical_length);
                    Ok(to_binary(
                        &to_binary(&PairInfoRaw {
                            contract_addr: api.canonical_address(&pair_info.contract_addr).unwrap(),
                            liquidity_token: api
                                .canonical_address(&pair_info.liquidity_token)
                                .unwrap(),
                            asset_infos: [
                                AssetInfoRaw::NativeToken {
                                    denom: "uusd".to_string(),
                                },
                                AssetInfoRaw::NativeToken {
                                    denom: "uusd".to_string(),
                                },
                            ],
                        })
                        .unwrap(),
                    ))
                } else {
                    panic!("DO NOT ENTER HERE")
                }
            }
            _ => self.base.handle_query(request),
        }
    }
}

impl WasmMockQuerier {
    pub fn new<A: Api>(base: MockQuerier<Empty>, canonical_length: usize, _api: A) -> Self {
        WasmMockQuerier {
            base,
            terraswap_pair_querier: TerraswapPairQuerier::default(),
            canonical_length,
        }
    }

    // configure the terraswap pair
    pub fn with_terraswap_pairs(&mut self, pairs: &[(&HumanAddr, &PairInfo)]) {
        self.terraswap_pair_querier = TerraswapPairQuerier::new(pairs);
    }

    // pub fn with_balance(&mut self, balances: &[(&HumanAddr, &[Coin])]) {
    //     for (addr, balance) in balances {
    //         self.base.update_balance(addr, balance.to_vec());
    //     }
    // }
}
