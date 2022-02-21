use crate::error::ContractError;
use crate::state::{Config, BRIDGES};
use astroport::asset::{native_asset_info, Asset, AssetInfo, PairInfo};
use astroport::maker::ExecuteMsg;
use astroport::pair::Cw20HookMsg;
use astroport::querier::query_pair_info;
use cosmwasm_std::{to_binary, Coin, Deps, Env, StdResult, SubMsg, Uint128, WasmMsg};

pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// The depth of bridges to swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Execute depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

/// UST token denom
pub const UUSD_DENOM: &str = "uusd";
/// LUNA token denom
pub const ULUNA_DENOM: &str = "uluna";

pub fn build_swap_msg(
    deps: Deps,
    cfg: &Config,
    pool: PairInfo,
    from: AssetInfo,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    if from.is_native_token() {
        let mut offer_asset = Asset {
            info: from.clone(),
            amount: amount_in,
        };

        // deduct tax first
        let amount_in = amount_in.checked_sub(offer_asset.compute_tax(&deps.querier)?)?;

        offer_asset.amount = amount_in;

        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: pool.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset,
                belief_price: None,
                max_spread: Some(cfg.max_spread),
                to: None,
            })?,
            funds: vec![Coin {
                denom: from.to_string(),
                amount: amount_in,
            }],
        }))
    } else {
        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: from.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pool.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: Some(cfg.max_spread),
                    to: None,
                })?,
            })?,
            funds: vec![],
        }))
    }
}

pub fn build_distribute_msg(
    env: Env,
    bridge_assets: Vec<AssetInfo>,
    depth: u64,
) -> StdResult<SubMsg> {
    let msg: SubMsg;
    if !bridge_assets.is_empty() {
        // Swap bridge assets
        msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SwapBridgeAssets {
                assets: bridge_assets,
                depth,
            })?,
            funds: vec![],
        });
    } else {
        // Update balances and distribute rewards
        msg = SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::DistributeAstro {})?,
            funds: vec![],
        });
    }

    Ok(msg)
}

pub fn validate_bridge(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    bridge_token: AssetInfo,
    astro_token: AssetInfo,
    depth: u64,
) -> Result<PairInfo, ContractError> {
    // check if bridge pool exists
    let bridge_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), bridge_token.clone()],
    )
    .map_err(|_| ContractError::InvalidBridgeNoPool(from_token.clone(), bridge_token.clone()))?;

    // check bridge token - ASTRO pool exists
    let astro_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[bridge_token.clone(), astro_token.clone()],
    );

    if astro_pool.is_err() {
        if depth >= BRIDGES_MAX_DEPTH {
            return Err(ContractError::MaxBridgeDepth(depth));
        }

        // Check if next level of bridge exists
        let next_bridge_token = BRIDGES
            .load(deps.storage, bridge_token.to_string())
            .map_err(|_| ContractError::InvalidBridgeDestination(from_token.clone()))?;

        validate_bridge(
            deps,
            cfg,
            bridge_token,
            next_bridge_token,
            astro_token,
            depth + 1,
        )?;
    }

    Ok(bridge_pool)
}

/// # Description
/// Checks if from_token - UST pool exists
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **cfg** is the object of type [`Config`].
///
/// * **from_token** is the object of type [`AssetInfo`].
pub fn uusd_pool_exists(deps: Deps, cfg: &Config, from_token: AssetInfo) -> Option<PairInfo> {
    let uusd = native_asset_info(UUSD_DENOM.to_string());

    let uusd_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), uusd.clone()],
    );

    if !uusd_pool.is_ok() {
        return None;
    }

    return Some(uusd_pool.unwrap());
}

/// # Description
/// /// Checks if from_token - luna - ust pool exists
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **cfg** is the object of type [`Config`].
///
/// * **from_token** is the object of type [`AssetInfo`].
pub fn uluna_uusd_pools_exist(deps: Deps, cfg: &Config, from_token: AssetInfo) -> Option<PairInfo> {
    let uluna = native_asset_info(ULUNA_DENOM.to_string());

    // check from_token -> uluna pool exists
    let uluna_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), uluna.clone()],
    );

    if !uluna_pool.is_ok() {
        return None;
    }

    if uusd_pool_exists(deps, cfg, uluna).is_none() {
        return None;
    }

    return Some(uluna_pool.unwrap());
}
