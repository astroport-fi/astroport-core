use crate::state::Params;
use cosmwasm_std::{
    to_binary, Addr, CosmosMsg, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdError,
    StdResult, Uint128, WasmMsg,
};

use astroport::asset::{Asset, AssetInfo};

use astroport::pair::{ReverseSimulationResponse, SimulationResponse};
use astroport::pair_bonded::{Config, ExecuteMsg};
use astroport::querier::{query_supply, query_token_balance};
use astroport::staking::Cw20HookMsg as StakingCw20HookMsg;
use astroport_pair_bonded::base::PairBonded;
use astroport_pair_bonded::error::ContractError;
use astroport_pair_bonded::state::CONFIG;
use cw20::Cw20ExecuteMsg;
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

/// Implementation of the bonded pair template. Performs ASTRO-xASTRO swap operations.
impl<'a> PairBonded<'a> for Contract<'a> {
    const CONTRACT_NAME: &'a str = "astroport-pair-astro-xastro";

    fn swap(
        &self,
        deps: DepsMut,
        env: Env,
        _info: MessageInfo,
        sender: Addr,
        offer_asset: Asset,
        _belief_price: Option<Decimal>,
        _max_spread: Option<Decimal>,
        to: Option<Addr>,
    ) -> Result<Response, ContractError> {
        let config = CONFIG.load(deps.storage)?;

        // If the asset balance already increased
        // We should subtract the user deposit from the pool offer asset amount
        let pools = config
            .pair_info
            .query_pools(&deps.querier, env.contract.address.clone())?
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

        let mut messages = vec![];

        let params = self.params.load(deps.storage)?;

        if offer_asset.info.equal(&AssetInfo::Token {
            contract_addr: params.astro_addr.clone(),
        }) {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: params.astro_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: params.staking_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&StakingCw20HookMsg::Enter {})?,
                })?,
                funds: vec![],
            }))
        } else {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: params.xastro_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Send {
                    contract: params.staking_addr.to_string(),
                    amount: offer_asset.amount,
                    msg: to_binary(&StakingCw20HookMsg::Leave {})?,
                })?,
                funds: vec![],
            }))
        }

        let receiver = to.unwrap_or_else(|| sender.clone());

        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::AssertAndSend {
                offer_asset: Asset {
                    amount: offer_asset.amount,
                    info: offer_pool.info,
                },
                ask_asset_info: ask_pool.info,
                sender,
                receiver,
            })?,
        }));

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
        let pools = config.pair_info.asset_infos;

        if !offer_asset.info.equal(&pools[0]) && !offer_asset.info.equal(&pools[1]) {
            return Err(StdError::generic_err(
                "Given offer asset doesn't belong to pair",
            ));
        }

        let params = self.params.load(deps.storage)?;

        let total_deposit = query_token_balance(
            &deps.querier,
            params.astro_addr.clone(),
            params.staking_addr,
        )?;
        let total_shares = query_supply(&deps.querier, params.xastro_addr)?;

        let return_amount = if offer_asset.info.equal(&AssetInfo::Token {
            contract_addr: params.astro_addr,
        }) {
            if total_shares.is_zero() || total_deposit.is_zero() {
                offer_asset.amount
            } else {
                offer_asset
                    .amount
                    .checked_mul(total_shares)?
                    .checked_div(total_deposit)?
            }
        } else {
            offer_asset
                .amount
                .checked_mul(total_deposit)?
                .checked_div(total_shares)?
        };

        Ok(SimulationResponse {
            return_amount,
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        })
    }

    /// Reverse simulation swap using Astroport Staking contract.
    fn query_reverse_simulation(
        &self,
        deps: Deps,
        _env: Env,
        ask_asset: Asset,
    ) -> StdResult<ReverseSimulationResponse> {
        let config: Config = CONFIG.load(deps.storage)?;
        let pools = config.pair_info.asset_infos;

        if !ask_asset.info.equal(&pools[0]) && !ask_asset.info.equal(&pools[1]) {
            return Err(StdError::generic_err(
                "Given ask asset doesn't belong to pairs",
            ));
        }

        let params = self.params.load(deps.storage)?;

        let total_deposit = query_token_balance(
            &deps.querier,
            params.astro_addr.clone(),
            params.staking_addr,
        )?;
        let total_shares = query_supply(&deps.querier, params.xastro_addr)?;

        let offer_amount = if ask_asset.info.equal(&AssetInfo::Token {
            contract_addr: params.astro_addr,
        }) {
            ask_asset
                .amount
                .checked_mul(total_shares)?
                .checked_div(total_deposit)?
        } else if total_shares.is_zero() || total_deposit.is_zero() {
            ask_asset.amount
        } else {
            ask_asset
                .amount
                .checked_mul(total_deposit)?
                .checked_div(total_shares)?
        };

        Ok(ReverseSimulationResponse {
            offer_amount,
            spread_amount: Uint128::zero(),
            commission_amount: Uint128::zero(),
        })
    }

    /// Not supported due to absence of native token in the pair.
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
        Err(ContractError::NotSupported {})
    }
}
