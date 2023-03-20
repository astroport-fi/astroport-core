use crate::state::Params;
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdResult};

use astroport::asset::Asset;

use astroport::pair::{ReverseSimulationResponse, SimulationResponse};
use astroport_pair_bonded::base::PairBonded;
use astroport_pair_bonded::error::ContractError;
use cw_storage_plus::Item;

/// This structure stores contract params.
pub(crate) struct Contract<'a> {
    pub params: Item<'a, Params>,
}

impl<'a> Contract<'a> {
    pub(crate) fn new(params_key: &'a str) -> Self {
        Contract {
            params: Item::<Params>::new(params_key),
        }
    }
}

/// Implementation of the bonded pair template.
impl<'a> PairBonded<'a> for Contract<'a> {
    const CONTRACT_NAME: &'a str = "astroport-pair-bonded-template";

    fn swap(
        &self,
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _sender: Addr,
        _offer_asset: Asset,
        _belief_price: Option<Decimal>,
        _max_spread: Option<Decimal>,
        _to: Option<Addr>,
    ) -> Result<Response, ContractError> {
        todo!("Implement swap assets using 3rd party contract.")
    }

    /// Simulation swap using Astroport Staking contract.
    fn query_simulation(
        &self,
        _deps: Deps,
        _env: Env,
        _offer_asset: Asset,
    ) -> StdResult<SimulationResponse> {
        todo!("Implement simulate swap for the specific pool using 3rd party contract.")
    }

    /// Reverse simulation swap using 3rd party contract.
    fn query_reverse_simulation(
        &self,
        _deps: Deps,
        _env: Env,
        _ask_asset: Asset,
    ) -> StdResult<ReverseSimulationResponse> {
        todo!("Implement simulate reverse swap for the specific pool using 3rd party contract.")
    }

    /// Execute swap operation using 3rd party contract.
    fn execute_swap(
        &self,
        _deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        _offer_asset: Asset,
        _belief_price: Option<Decimal>,
        _max_spread: Option<Decimal>,
        _to: Option<String>,
    ) -> Result<Response, ContractError> {
        todo!("Execute swap using 3rd party contract(Only if the pool has native asset).")
    }
}
