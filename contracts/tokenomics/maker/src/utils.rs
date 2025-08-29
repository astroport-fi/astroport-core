use std::collections::HashMap;

use astroport::asset::AssetInfo;
use astroport::maker::{COOLDOWN_LIMITS, MAX_SWAPS_DEPTH};
use cosmwasm_std::{ensure_eq, StdError, StdResult, Storage};
use itertools::Itertools;

use crate::error::ContractError;
use crate::state::{RouteStep, ROUTES};

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

pub struct BuiltRoutes {
    pub routes: Vec<RouteStep>,
    pub route_taken: String,
}

impl RoutesBuilder {
    pub fn build_routes(
        &mut self,
        storage: &dyn Storage,
        asset_in: &AssetInfo,
        astro_denom: &str,
    ) -> Result<BuiltRoutes, ContractError> {
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
                route_taken,
            }
        );

        let route_display = routes.iter().map(|r| r.asset_out.to_string()).collect_vec();
        let route_taken = [vec![asset_in.to_string()], route_display]
            .concat()
            .join(" -> ");

        Ok(BuiltRoutes {
            routes,
            route_taken,
        })
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
