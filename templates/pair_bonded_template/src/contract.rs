use crate::state::Params;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128, WasmMsg,
};

use astroport::asset::{Asset, AssetInfo};

use astroport::pair::{ReverseSimulationResponse, SimulationResponse};
use astroport::pair_bonded::Config;
use astroport_pair_bonded::base::PairBonded;
use astroport_pair_bonded::error::ContractError;
use astroport_pair_bonded::state::CONFIG;
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
        deps: DepsMut,
        env: Env,
        _info: MessageInfo,
        _sender: Addr,
        offer_asset: Asset,
        _belief_price: Option<Decimal>,
        _max_spread: Option<Decimal>,
        _to: Option<Addr>,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // If the asset balance already increased
        // We should subtract the user deposit from the pool offer asset amount
        let pools = config
            .pair_info
            .query_pools(&deps.querier, &env.contract.address)?
            .into_iter()
            .map(|mut p| {
                if p.info.equal(&offer_asset.info) {
                    p.amount = p.amount.checked_sub(offer_asset.amount)?;
                }
                Ok(p)
            })
            .collect::<StdResult<Vec<_>>>()?;

        let offer_pool: Asset;
        let ask_pool: Asset;

        if offer_asset.info.equal(&pools[0].info) {
            offer_pool = pools[0].clone();
            ask_pool = pools[1].clone();
        } else if offer_asset.info.equal(&pools[1].info) {
            offer_pool = pools[1].clone();
            ask_pool = pools[0].clone();
        } else {
            return Err(ContractError::AssetMismatch {});
        }

        let mut _messages = vec![];

        // Swap assets using 3rd party contract.
        unimplemented!();

        Ok(Response::new().add_messages(messages))
    }

    /// Simulation swap using Astroport Staking contract.
    fn query_simulation(
        &self,
        deps: Deps,
        _env: Env,
        offer_asset: Asset,
    ) -> StdResult<SimulationResponse> {
        let config: Config = CONFIG.load(deps.storage)?;
        let pools: [AssetInfo; 2] = config.pair_info.asset_infos;

        if !offer_asset.info.equal(&pools[0]) && !offer_asset.info.equal(&pools[1]) {
            return Err(StdError::generic_err(
                "Given offer asset doesn't belong to pair",
            ));
        }

        // Simulate swap for the specific pool using 3rd party contract.
        unimplemented!();

        Ok(SimulationResponse {
            return_amount,
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        })
    }

    /// Reverse simulation swap using 3rd party contract.
    fn query_reverse_simulation(
        &self,
        deps: Deps,
        _env: Env,
        ask_asset: Asset,
    ) -> StdResult<ReverseSimulationResponse> {
        let config: Config = CONFIG.load(deps.storage)?;
        let pools: [AssetInfo; 2] = config.pair_info.asset_infos;

        if !ask_asset.info.equal(&pools[0]) && !ask_asset.info.equal(&pools[1]) {
            return Err(StdError::generic_err(
                "Given ask asset doesn't belong to pairs",
            ));
        }

        let _params = self.params.load(deps.storage)?;

        // Simulate reverse swap for the specific pool using 3rd party contract.
        unimplemented!();

        Ok(ReverseSimulationResponse {
            offer_amount,
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        })
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
        // Execute swap using 3rd party contract(Only if the pool has native asset).
        unimplemented!()
    }
}
