use std::str::FromStr;

use cosmwasm_std::{
    from_slice, to_binary, Decimal, Querier, QuerierResult, QueryRequest, SystemError,
    SystemResult, WasmQuery,
};
use terra_cosmwasm::TerraQueryWrapper;

pub const ENTRY_ORACLE_ADDR: &str = "entry_oracle";
pub const ENTRY_EXCHANGE_RATE: &str = "0.000025"; // 1 UST -> 0.000025 BTC
pub const EXIT_ORACLE_ADDR: &str = "exit_oracle";
pub const EXIT_EXCHANGE_RATE: &str = "40000"; // 1 BTC -> 40000 UST

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
                    ENTRY_ORACLE_ADDR => SystemResult::Ok(
                        to_binary(&Decimal::from_str(ENTRY_EXCHANGE_RATE).unwrap()).into(),
                    ),
                    EXIT_ORACLE_ADDR => SystemResult::Ok(
                        to_binary(&Decimal::from_str(EXIT_EXCHANGE_RATE).unwrap()).into(),
                    ),
                    _ => unimplemented!(),
                }
            }
            _ => unimplemented!(),
        }
    }
}
