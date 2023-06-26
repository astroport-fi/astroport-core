use cosmwasm_std::{Coin, QuerierWrapper, QueryRequest, StdResult, ContractInfoResponse};

use crate::query::{
    SwapResponse, TaxRateResponse, TaxCapResponse, ExchangeRatesResponse, TerraQuery
};

/// This is a helper wrapper to easily use our custom queries
pub struct TerraQuerier<'a> {
    querier: &'a QuerierWrapper<'a, TerraQuery>,
}

impl<'a> TerraQuerier<'a> {
    pub fn new(querier: &'a QuerierWrapper<TerraQuery>) -> Self {
        TerraQuerier { querier }
    }

    pub fn query_swap<T: Into<String>>(
        &self,
        offer_coin: Coin,
        ask_denom: T,
    ) -> StdResult<SwapResponse> {
        let request = TerraQuery::Swap {
            offer_coin,
            ask_denom: ask_denom.into(),
        };

        let request: QueryRequest<TerraQuery> = TerraQuery::into(request);
        self.querier.query(&request)
    }

    pub fn query_tax_cap<T: Into<String>>(&self, denom: T) -> StdResult<TaxCapResponse> {
        let request = TerraQuery::TaxCap {
            denom: denom.into(),
        };

        let request: QueryRequest<TerraQuery> = TerraQuery::into(request);
        self.querier.query(&request)
    }

    pub fn query_tax_rate(&self) -> StdResult<TaxRateResponse> {
        let request = TerraQuery::TaxRate {};

        let request: QueryRequest<TerraQuery> = TerraQuery::into(request);
        self.querier.query(&request)
    }

    pub fn query_exchange_rates<T: Into<String>>(
        &self,
        base_denom: T,
        quote_denoms: Vec<T>,
    ) -> StdResult<ExchangeRatesResponse> {
        let request = TerraQuery::ExchangeRates {
            base_denom: base_denom.into(),
            quote_denoms: quote_denoms.into_iter().map(|x| x.into()).collect(),
        };

        let request: QueryRequest<TerraQuery> = TerraQuery::into(request);
        self.querier.query(&request)
    }

    pub fn query_contract_info<T: Into<String>>(
        &self,
        contract_address: T,
    ) -> StdResult<ContractInfoResponse> {
        self.querier.query_wasm_contract_info(contract_address.into())
    }

}