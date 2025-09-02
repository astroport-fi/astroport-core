use std::collections::HashMap;

use crate::error::ContractError;
use crate::state::{RouteStep, ROUTES};
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::common::LP_SUBDENOM;
use astroport::factory::QueryMsg;
use astroport::maker::{COOLDOWN_LIMITS, MAX_SWAPS_DEPTH};
use astroport::pair;
use astroport::pair::Cw20HookMsg;
use cosmwasm_std::{
    coins, ensure, ensure_eq, to_json_binary, wasm_execute, Addr, Decimal, QuerierWrapper,
    StdError, StdResult, Storage, WasmMsg,
};

/// Validate cooldown value is within the allowed range
pub fn validate_cooldown(maybe_cooldown: Option<u64>) -> Result<(), ContractError> {
    if let Some(collect_cooldown) = maybe_cooldown {
        if !COOLDOWN_LIMITS.contains(&collect_cooldown) {
            return Err(ContractError::IncorrectCooldown {
                min: *COOLDOWN_LIMITS.start(),
                max: *COOLDOWN_LIMITS.end(),
            });
        }
    }

    Ok(())
}

#[derive(Default)]
pub struct RoutesBuilder {
    pub routes_cache: HashMap<AssetInfo, RouteStep>,
}

impl RoutesBuilder {
    pub fn build_routes(
        &mut self,
        storage: &dyn Storage,
        asset_in: &AssetInfo,
        astro_denom: &str,
    ) -> Result<Vec<RouteStep>, ContractError> {
        let mut prev_asset = asset_in.clone();
        let mut routes = vec![];
        let astro = AssetInfo::native(astro_denom);

        for _ in 0..MAX_SWAPS_DEPTH {
            if prev_asset == astro {
                break;
            }

            let step = if let Some(found) = self.routes_cache.get(&prev_asset).cloned() {
                found
            } else {
                let step = ROUTES
                    .may_load(storage, &asset_info_key(&prev_asset))?
                    .ok_or(ContractError::RouteNotFound {
                        asset: prev_asset.to_string(),
                    })?;
                self.routes_cache.insert(prev_asset, step.clone());

                step
            };

            prev_asset = step.asset_out.clone();

            routes.push(step);
        }

        ensure_eq!(
            prev_asset,
            astro,
            ContractError::FailedToBuildRoute {
                asset: asset_in.to_string(),
            }
        );

        Ok(routes)
    }
}

pub fn asset_info_key(asset_info: &AssetInfo) -> Vec<u8> {
    let mut bytes = vec![];
    match asset_info {
        AssetInfo::NativeToken { denom } => {
            bytes.push(0);
            bytes.extend_from_slice(denom.as_bytes());
        }
        AssetInfo::Token { contract_addr } => {
            bytes.push(1);
            bytes.extend_from_slice(contract_addr.as_bytes());
        }
    }

    bytes
}

pub fn from_key_to_asset_info(bytes: Vec<u8>) -> StdResult<AssetInfo> {
    match bytes[0] {
        0 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::native),
        1 => String::from_utf8(bytes[1..].to_vec())
            .map_err(StdError::invalid_utf8)
            .map(AssetInfo::cw20_unchecked),
        _ => Err(StdError::generic_err(
            "Failed to deserialize asset info key",
        )),
    }
}

pub fn build_swap_msg(
    asset_in: &Asset,
    to: &AssetInfo,
    max_spread: Decimal,
    pool_addr: &Addr,
) -> StdResult<WasmMsg> {
    match &asset_in.info {
        AssetInfo::Token { contract_addr } => wasm_execute(
            contract_addr.to_string(),
            &cw20::Cw20ExecuteMsg::Send {
                contract: pool_addr.to_string(),
                amount: asset_in.amount,
                msg: to_json_binary(&Cw20HookMsg::Swap {
                    ask_asset_info: Some(to.clone()),
                    belief_price: None,
                    max_spread: Some(max_spread),
                    to: None,
                })?,
            },
            vec![],
        ),
        AssetInfo::NativeToken { denom } => wasm_execute(
            pool_addr,
            &pair::ExecuteMsg::Swap {
                offer_asset: asset_in.clone(),
                ask_asset_info: Some(to.clone()),
                belief_price: None,
                max_spread: Some(max_spread),
                to: None,
            },
            coins(asset_in.amount.u128(), denom),
        ),
    }
}

/// Validates that pair was registered using official Astroport factory.
/// Ensures expected asset infos are in the pair.
/// Returns PairInfo in case it needs further analysis.
pub fn check_pair(
    querier: QuerierWrapper,
    factory: impl Into<String>,
    pool_addr: impl Into<String>,
    asset_infos: &[AssetInfo],
) -> Result<PairInfo, ContractError> {
    let pool_addr = pool_addr.into();
    let pair_info = querier.query_wasm_smart::<PairInfo>(
        factory,
        &QueryMsg::PairByLpToken {
            lp_token: format!("factory/{pool_addr}/{LP_SUBDENOM}"),
        },
    )?;

    for asset_info in asset_infos {
        ensure!(
            pair_info.asset_infos.contains(asset_info),
            ContractError::InvalidPoolAsset {
                pool_addr: pool_addr.into(),
                asset: asset_info.to_string()
            }
        );
    }

    Ok(pair_info)
}
