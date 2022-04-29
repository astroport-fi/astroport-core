use std::str::FromStr;

use cosmwasm_std::{
    from_slice, to_binary, Decimal, Querier, QuerierResult, QueryRequest, SystemError,
    SystemResult, WasmQuery,
};
use terra_cosmwasm::TerraQueryWrapper;

pub const ORACLE_ADDR1: &str = "oracle1";
pub const EXCHANGE_RATE_1: &str = "39000"; // 1 BTC -> 39000 USD
pub const ORACLE_ADDR2: &str = "oracle2";
pub const EXCHANGE_RATE_2: &str = "41000"; // 1 BTC -> 41000 USD

pub struct CustomQuerier;

impl Querier for CustomQuerier {
    fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
        let request: QueryRequest<TerraQueryWrapper> = match from_slice(bin_request) {
            Ok(v) => v,
            Err(e) => {
                return SystemResult::Err(SystemError::InvalidRequest {
                    error: format!("Parsing query request: {:?}", e),
                    request: bin_request.into(),
                })
            }
        };

        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, .. }) => {
                match contract_addr.as_str() {
                    ORACLE_ADDR1 => SystemResult::Ok(
                        to_binary(&Decimal::from_str(EXCHANGE_RATE_1).unwrap()).into(),
                    ),
                    ORACLE_ADDR2 => SystemResult::Ok(
                        to_binary(&Decimal::from_str(EXCHANGE_RATE_2).unwrap()).into(),
                    ),
                    _ => unimplemented!(),
                }
            }
            _ => unimplemented!(),
        }
    }
}
