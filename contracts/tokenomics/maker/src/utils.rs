use crate::error::ContractError;
use crate::state::{Config, BRIDGES};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::maker::ExecuteMsg;
use astroport::pair::{Cw20HookMsg, SimulationResponse};
use cosmwasm_std::{
    to_binary, Addr, Decimal, Deps, Env, QuerierWrapper, StdResult, SubMsg, Uint128, WasmMsg,
};

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;
/// This amount of tokens is used in get_pool swap simulations.
/// TODO: adjust according to token's precision?
pub const SWAP_SIMULATION_AMOUNT: Uint128 = Uint128::new(1_000_000u128);

/// The function checks from<>to pool exists and creates swap message.
///
/// * **from** asset we want to swap.
///
/// * **to** asset we want to swap to.
///
/// * **amount_in** amount of tokens to swap.
pub fn try_build_swap_msg(
    querier: &QuerierWrapper,
    cfg: &Config,
    from: &AssetInfo,
    to: &AssetInfo,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    let (pool, _) = get_pool(querier, &cfg.factory_contract, from, to, Some(amount_in))?;
    let msg = build_swap_msg(querier, cfg.max_spread, &pool, from, Some(to), amount_in)?;
    Ok(msg)
}

/// This function creates swap message.
///
/// * **max_spread** max allowed spread.
///
/// * **pool** pool's information.
///
/// * **from**  asset we want to swap.
///
/// * **to** asset we want to swap to.
///
/// * **amount_in** amount of tokens to swap.
pub fn build_swap_msg(
    querier: &QuerierWrapper,
    max_spread: Decimal,
    pool: &PairInfo,
    from: &AssetInfo,
    to: Option<&AssetInfo>,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    if from.is_native_token() {
        let mut offer_asset = Asset {
            info: from.clone(),
            amount: amount_in,
        };
        // Deduct tax
        let coin = offer_asset.deduct_tax(querier)?;
        offer_asset.amount = coin.amount;

        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: pool.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset,
                ask_asset_info: to.cloned(),
                belief_price: None,
                max_spread: Some(max_spread),
                to: None,
            })?,
            funds: vec![coin],
        }))
    } else {
        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: from.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pool.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    ask_asset_info: to.cloned(),
                    belief_price: None,
                    max_spread: Some(max_spread),
                    to: None,
                })?,
            })?,
            funds: vec![],
        }))
    }
}

/// This function builds distribute messages. It swap all assets through bridges if needed.
///
/// * **bridge_assets** array with assets we want to swap and then to distribute.
///
/// * **depth** current depth of the swap. It is intended to prevent dead loops in recursive calls.
pub fn build_distribute_msg(
    env: Env,
    bridge_assets: Vec<AssetInfo>,
    depth: u64,
) -> StdResult<SubMsg> {
    let msg = if !bridge_assets.is_empty() {
        // Swap bridge assets
        SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SwapBridgeAssets {
                assets: bridge_assets,
                depth,
            })?,
            funds: vec![],
        })
    } else {
        // Update balances and distribute rewards
        SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::DistributeAstro {})?,
            funds: vec![],
        })
    };

    Ok(msg)
}

/// This function checks that there is a direct pool to swap to $ASTRO.
/// Otherwise it looks for an intermediate token to swap to $ASTRO.
///
/// * **from_token** asset we want to swap.
///
/// * **to_token** asset we want to swap to.
///
/// * **astro_token** represents $ASTRO.
///
/// * **depth** current recursion depth of the validation.
///
/// * **amount** is an amount of from_token.
pub fn validate_bridge(
    deps: Deps,
    factory_contract: &Addr,
    from_token: &AssetInfo,
    bridge_token: &AssetInfo,
    astro_token: &AssetInfo,
    depth: u64,
    amount: Option<Uint128>,
) -> Result<PairInfo, ContractError> {
    // Check if the bridge pool exists
    let (bridge_pool, ret_amount) = get_pool(
        &deps.querier,
        factory_contract,
        from_token,
        bridge_token,
        amount,
    )?;

    // Check if the bridge token - ASTRO pool exists
    let astro_pool = get_pool(
        &deps.querier,
        factory_contract,
        bridge_token,
        astro_token,
        ret_amount,
    );
    if astro_pool.is_err() {
        if depth >= BRIDGES_MAX_DEPTH {
            return Err(ContractError::MaxBridgeDepth(depth));
        }

        // Check if next level of bridge exists
        let next_bridge_token = BRIDGES
            .load(deps.storage, bridge_token.to_string())
            .map_err(|_| ContractError::InvalidBridgeDestination(from_token.to_string()))?;

        validate_bridge(
            deps,
            factory_contract,
            bridge_token,
            &next_bridge_token,
            astro_token,
            depth + 1,
            ret_amount,
        )?;
    }

    Ok(bridge_pool)
}

/// This function checks that there a pool to swap between `from` and `to`. In case of success
/// returns [`PairInfo`] of selected pool and simulated return amount.
///
/// * **from** source asset.
///
/// * **to** destination asset.
///
/// * **amount** optional. The value is used in swap simulations to select the best pool.
pub fn get_pool(
    querier: &QuerierWrapper,
    factory_contract: &Addr,
    from: &AssetInfo,
    to: &AssetInfo,
    amount: Option<Uint128>,
) -> Result<(PairInfo, Option<Uint128>), ContractError> {
    // We use raw query to save gas
    let result = astroport::factory::ROUTE.query(
        querier,
        factory_contract.clone(),
        (from.to_string(), to.to_string()),
    )?;
    match result {
        Some(pairs) if !pairs.is_empty() => {
            let (best_pair, sim_res) = pairs
                .into_iter()
                .map(|pair_contract| {
                    let sim_res: SimulationResponse = querier.query_wasm_smart(
                        &pair_contract,
                        &astroport::pair::QueryMsg::Simulation {
                            offer_asset: Asset {
                                info: from.clone(),
                                amount: amount.unwrap_or(SWAP_SIMULATION_AMOUNT),
                            },
                            ask_asset_info: Some(to.clone()),
                        },
                    )?;
                    Ok((pair_contract, sim_res))
                })
                .collect::<StdResult<Vec<_>>>()?
                .into_iter()
                .max_by(|(_, sim_res1), (_, sim_res2)| {
                    sim_res1.return_amount.cmp(&sim_res2.return_amount)
                })
                .unwrap();

            Ok((
                querier.query_wasm_smart(&best_pair, &astroport::pair::QueryMsg::Pair {})?,
                Some(sim_res.return_amount),
            ))
        }
        _ => Err(ContractError::InvalidBridgeNoPool(
            from.to_string(),
            to.to_string(),
        )),
    }
}
