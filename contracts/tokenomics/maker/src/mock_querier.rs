use std::collections::HashMap;
use std::ops::Add;

use cosmwasm_std::testing::{MockApi, MockQuerier, MockStorage, MOCK_CONTRACT_ADDR};
use cosmwasm_std::{
    from_binary, from_slice, to_binary, Addr, Coin, Empty, OwnedDeps, Querier, QuerierResult,
    QueryRequest, SystemError, SystemResult, Uint128, WasmQuery,
};
use cw20::{BalanceResponse, Cw20QueryMsg};

/// mock_dependencies is a drop-in replacement for cosmwasm_std::testing::mock_dependencies
/// this uses our CustomQuerier.
pub fn mock_dependencies(
    contract_balance: &[Coin],
) -> OwnedDeps<MockStorage, MockApi, WasmMockQuerier> {
    let custom_querier: WasmMockQuerier =
        WasmMockQuerier::new(MockQuerier::new(&[(MOCK_CONTRACT_ADDR, contract_balance)]));

    OwnedDeps {
        storage: MockStorage::default(),
        api: MockApi::default(),
        querier: custom_querier,
    }
}

pub struct WasmMockQuerier {
    base: MockQuerier<Empty>,
    balances: HashMap<Addr, Vec<Coin>>,
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
                });
            }
        };
        self.handle_query(&request)
    }
}

impl WasmMockQuerier {
    pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
        // let bb = self.balances;
        match &request {
            QueryRequest::Wasm(WasmQuery::Smart { contract_addr, msg }) => {
                match from_binary(&msg).unwrap() {
                    Cw20QueryMsg::Balance { address } => SystemResult::Ok(
                        to_binary(&BalanceResponse {
                            //balance:Uint128::from(1_000_000u128),
                            balance: self
                                .balances
                                .get(&Addr::unchecked(address))
                                .and_then(|v| {
                                    v.iter()
                                        .find(|c| &c.denom == contract_addr)
                                        .map(|c| c.amount)
                                })
                                .unwrap_or_default(),
                        })
                            .into(),
                    ),
                    _ => panic!("DO NOT ENTER HERE"),
                }
            }
            _ => self.base.handle_query(request),
        }
    }
    pub fn set_balance(&mut self, user: Addr, token: Addr, amount: Uint128) {
        let mut balances = self.balances.get(&user).unwrap_or(&Vec::new()).to_vec();
        match balances
            .iter_mut()
            .find(|ref c| token.to_string() == c.denom)
        {
            Some(bal) => {
                // If there is one, insert into it and update
                bal.amount = amount;
            }
            // o/w insert a new leaf at the end
            None => {
                balances.push(Coin::new(amount.u128(), token.to_string()));
            }
        }

        self.balances.insert(user, balances);
    }
    pub fn add_balance(&mut self, user: Addr, token: Addr, amount: Uint128) {
        let mut balances = self.balances.get(&user).unwrap_or(&Vec::new()).to_vec();
        match balances
            .iter_mut()
            .find(|ref c| token.to_string() == c.denom)
        {
            Some(bal) => {
                // If there is one, insert into it and update
                bal.amount = bal.amount.add(amount);
            }
            // o/w insert a new leaf at the end
            None => {
                balances.push(Coin::new(amount.u128(), token.to_string()));
            }
        }

        self.balances.insert(user, balances);
    }
    pub fn sub_balance(&mut self, user: Addr, token: Addr, amount: Uint128) {
        let mut balances = self.balances.get(&user).unwrap_or(&Vec::new()).to_vec();
        match balances
            .iter_mut()
            .find(|ref c| token.to_string() == c.denom)
        {
            Some(bal) => {
                // If there is one, insert into it and update
                bal.amount = bal.amount.checked_sub(amount).unwrap_or_default();
            }
            // o/w insert a new leaf at the end
            None => {
                balances.push(Coin::new(0, token.to_string()));
            }
        }
        self.balances.insert(user, balances);
    }
    pub fn new(base: MockQuerier<Empty>) -> Self {
        WasmMockQuerier {
            base,
            balances: HashMap::new(),
        }
    }
}
