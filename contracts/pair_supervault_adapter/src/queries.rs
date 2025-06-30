use astroport::asset::Asset;
use astroport::pair::{ConfigResponse, PoolResponse, QueryMsg};
use astroport::querier::query_factory_config;
use cosmwasm_std::{to_json_binary, Binary, Deps, Env, StdError, StdResult, Uint128};

use crate::external::SvQuerier;
use crate::state::{Config, CONFIG};

#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_json_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => to_json_binary(&query_pool(deps)?),
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Share { amount } => simulate_withdraw(deps, amount),
        QueryMsg::SimulateProvide { assets, .. } => {
            let config: Config = CONFIG.load(deps.storage)?;
            let sv_querier = SvQuerier::new(config.vault_addr);

            let sv_config = sv_querier.query_config(deps.querier)?;
            let (denom_0, denom_1) = (
                sv_config.pair_data.token_0.denom.clone(),
                sv_config.pair_data.token_1.denom.clone(),
            );

            let (amount_0, amount_1) = if assets[0].info.to_string() == denom_0
                && assets[1].info.to_string() == denom_1
            {
                (assets[0].amount, assets[1].amount)
            } else if assets[0].info.to_string() == denom_1 && assets[1].info.to_string() == denom_0
            {
                (assets[1].amount, assets[0].amount)
            } else {
                return Err(StdError::generic_err("Invalid assets"));
            };

            let response = sv_querier.simulate_provide_liquidity(
                deps.querier,
                amount_0,
                amount_1,
                env.contract.address,
            )?;

            to_json_binary(&response)
        }
        QueryMsg::SimulateWithdraw { lp_amount } => simulate_withdraw(deps, lp_amount),
        QueryMsg::Simulation { .. } => unimplemented!(),
        QueryMsg::ReverseSimulation { .. } => unimplemented!(),
        _ => Err(StdError::generic_err("Unsupported query type")),
    }
}

pub fn simulate_withdraw(deps: Deps, lp_amount: Uint128) -> StdResult<Binary> {
    let config: Config = CONFIG.load(deps.storage)?;
    let sv_querier = SvQuerier::new(config.vault_addr);

    let sv_config = sv_querier.query_config(deps.querier)?;
    let response = sv_querier.simulate_withdraw_liquidity(deps.querier, lp_amount)?;

    to_json_binary(&[
        Asset::native(
            sv_config.pair_data.token_0.denom,
            response.withdraw_amount_0,
        ),
        Asset::native(
            sv_config.pair_data.token_1.denom,
            response.withdraw_amount_1,
        ),
    ])
}

pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let sv_querier = SvQuerier::new(config.vault_addr);

    let assets = sv_querier
        .query_balance(deps.querier)?
        .into_iter()
        .map(Asset::from)
        .collect();

    Ok(PoolResponse {
        assets,
        total_share: deps
            .querier
            .query_supply(&config.pair_info.liquidity_token)?
            .amount,
    })
}

/// Returns the pair contract configuration in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    Ok(ConfigResponse {
        block_time_last: 0,
        params: None,
        owner: factory_config.owner,
        factory_addr: config.factory_addr,
        tracker_addr: None,
    })
}
