use crate::error::ContractError;
use crate::state::{Config, BRIDGES};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::maker::ExecuteMsg;
use astroport::pair::Cw20HookMsg;
use astroport::querier::query_pair_info;
use cosmwasm_std::{
    to_binary, Addr, Decimal, Deps, Env, QuerierWrapper, StdResult, SubMsg, Uint128, WasmMsg,
};

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

/// # Description
/// The function checks from<>to pool exists and creates swap message.
///
/// # Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **cfg** is an object of type [`Config`]. This is the contracts' configuration.
///
/// * **from** is an object of type [`AssetInfo`] which represents the asset we want to swap.
///
/// * **to** is an object of type [`AssetInfo`] which represents the asset we want to swap to.
///
/// * **amount_in** is an object of type [`Uint128`]. This is the amount of tokens to swap.
pub fn try_build_swap_msg(
    querier: &QuerierWrapper,
    cfg: &Config,
    from: &AssetInfo,
    to: &AssetInfo,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    let pool = get_pool(querier, &cfg.factory_contract, from, to)?;
    let msg = build_swap_msg(querier, cfg.max_spread, &pool, from, amount_in)?;
    Ok(msg)
}

/// # Description
/// This function creates swap message.
///
/// # Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **max_spread** is a value of type [`Decimal`]. This is max allowed spread.
///
/// * **pool** is an object of type [`PairInfo`]. This is the pool's information.
///
/// * **from** is an object of type [`AssetInfo`] which represents the asset we want to swap.
///
/// * **amount_in** is an object of type [`Uint128`]. This is the amount of tokens to swap.
pub fn build_swap_msg(
    querier: &QuerierWrapper,
    max_spread: Decimal,
    pool: &PairInfo,
    from: &AssetInfo,
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
                    belief_price: None,
                    max_spread: Some(max_spread),
                    to: None,
                })?,
            })?,
            funds: vec![],
        }))
    }
}

/// # Description
/// This function builds distribute messages. It swap all assets through bridges if needed.
///
/// # Params
/// * **env** is an object of type [`Env`].
///
/// * **bridge_assets** is an array of objects of type [`AssetInfo`].
/// This is the assets we want to swap and then to distribute.
///
/// * **depth** is a value of type [`Uint128`]. This is the current depth of the swap.
/// It is intended to prevent dead loops in recursive calls.
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

/// # Description
/// This function checks that there is a direct pool to swap to $ASTRO.
/// Otherwise it looks for an intermediate token to swap to $ASTRO.
///
/// # Params
/// * **deps** is an object of type [`Deps`].
///
/// * **factory_contract** is a value of type [`Addr`]. This is the factory contract address.
///
/// * **from_token** is an object of type [`AssetInfo`] which represents the asset we want to swap.
///
/// * **to_token** is an object of type [`AssetInfo`] which represents the asset we want to swap to.
///
/// * **astro_token** is an object of type [`AssetInfo`] which represents $ASTRO.
///
/// * **depth** is a value of type [`Uint128`]. This is the current recursion depth of the validation.
pub fn validate_bridge(
    deps: Deps,
    factory_contract: &Addr,
    from_token: &AssetInfo,
    bridge_token: &AssetInfo,
    astro_token: &AssetInfo,
    depth: u64,
) -> Result<PairInfo, ContractError> {
    // Check if the bridge pool exists
    let bridge_pool = get_pool(&deps.querier, factory_contract, from_token, bridge_token)?;

    // Check if the bridge token - ASTRO pool exists
    let astro_pool = get_pool(&deps.querier, factory_contract, bridge_token, astro_token);
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
        )?;
    }

    Ok(bridge_pool)
}

/// # Description
/// This function checks that there a pool to swap between `from` and `to`.
///
/// # Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **factory_contract** is an object of type [`Addr`] which is the factory contract.
///
/// * **from** is an object of type [`AssetInfo`] which is the source asset.
///
/// * **to** is an object of type [`AssetInfo`] which is the destination asset.
pub fn get_pool(
    querier: &QuerierWrapper,
    factory_contract: &Addr,
    from: &AssetInfo,
    to: &AssetInfo,
) -> Result<PairInfo, ContractError> {
    query_pair_info(
        querier,
        factory_contract.clone(),
        &[from.clone(), to.clone()],
    )
    .map_err(|_| ContractError::InvalidBridgeNoPool(from.to_string(), to.to_string()))
}
