use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_slice, to_binary, Addr, Binary, Coin, ContractResult, Decimal, OwnedDeps, Querier,
    QuerierResult, QueryRequest, SystemError,
};
use std::collections::HashMap;
use terra_cosmwasm::{
    ExchangeRateItem, ExchangeRatesResponse, TerraQuery, TerraQueryWrapper, TerraRoute,
};

pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, AstroMockQuerier> {
    let contract_addr = Addr::unchecked(MOCK_CONTRACT_ADDR);
    let custom_querier: AstroMockQuerier = AstroMockQuerier::new(MockQuerier::new(&[(
        &contract_addr.to_string(),
        contract_balance,
    )]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct NativeQuerier {
    /// maps denom to exchange rates
    pub exchange_rates: HashMap<String, HashMap<String, Decimal>>,
}

impl Default for NativeQuerier {
    fn default() -> Self {
        NativeQuerier {
            exchange_rates: HashMap::new(),
        }
    }
}

impl NativeQuerier {
    pub fn handle_query(&self, route: &TerraRoute, query_data: &TerraQuery) -> QuerierResult {
        match route {
            TerraRoute::Oracle => {
                if let TerraQuery::ExchangeRates {
                    base_denom,
                    quote_denoms,
                } = query_data
                {
                    return self.query_oracle(base_denom, quote_denoms);
                }
                let err: ContractResult<Binary> = Err(format!(
                    "[mock]: Unsupported query data for QueryRequest::Custom : {:?}",
                    query_data
                ))
                .into();

                Ok(err).into()
            }
            _ => {
                let err: ContractResult<Binary> = Err(format!(
                    "[mock]: Unsupported query data for QueryRequest::Custom : {:?}",
                    query_data
                ))
                .into();

                Ok(err).into()
            }
        }
    }
    fn query_oracle(&self, base_denom: &str, quote_denoms: &[String]) -> QuerierResult {
        let base_exchange_rates = match self.exchange_rates.get(base_denom) {
            Some(res) => res,
            None => {
                let err: ContractResult<Binary> = Err(format!(
                    "no exchange rates available for provided base denom: {}",
                    base_denom
                ))
                .into();
                return Ok(err).into();
            }
        };

        let exchange_rate_items: Result<Vec<ExchangeRateItem>, String> = quote_denoms
            .iter()
            .map(|denom| {
                let exchange_rate = match base_exchange_rates.get(denom) {
                    Some(rate) => rate,
                    None => return Err(format!("no exchange rate available for {}", denom)),
                };

                Ok(ExchangeRateItem {
                    quote_denom: denom.into(),
                    exchange_rate: *exchange_rate,
                })
            })
            .collect();

        let res = ExchangeRatesResponse {
            base_denom: base_denom.into(),
            exchange_rates: exchange_rate_items.unwrap(),
        };
        let cr: ContractResult<Binary> = to_binary(&res).into();
        Ok(cr).into()
    }
}

pub struct AstroMockQuerier {
    base: MockQuerier<TerraQueryWrapper>,
    native_querier: NativeQuerier,
}

impl Querier for AstroMockQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        // MockQuerier doesn't support Custom, so we ignore it completely here
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {:?}", e),
                    request: bin_request.into(),
                })
                .into()
            }
        };
        self.handle_query(&request)
    }
}

impl AstroMockQuerier {
    pub fn new(base: MockQuerier<TerraQueryWrapper>) -> Self {
        AstroMockQuerier {
            base,
            native_querier: NativeQuerier::default(),
        }
    }

    pub fn set_native_exchange_rates(
        &mut self,
        base_denom: String,
        exchange_rates: &[(String, Decimal)],
    ) {
        self.native_querier
            .exchange_rates
            .insert(base_denom, exchange_rates.iter().cloned().collect());
    }

    pub fn handle_query(&self, request: &QueryRequest<TerraQueryWrapper>) -> QuerierResult {
        match &request {
            QueryRequest::Custom(TerraQueryWrapper { route, query_data }) => {
                self.native_querier.handle_query(route, query_data)
            }
            _ => self.base.handle_query(request),
        }
    }
}
