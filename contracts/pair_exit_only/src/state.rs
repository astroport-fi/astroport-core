use cosmwasm_std::{from_json, StdError, StdResult, Storage};
use serde::Deserialize;

use astroport::asset::PairInfo;

/// The part of a pair's config this contract needs. Every Astroport pair it supports stores its
/// config under `config` with `pair_info` in it; the other fields differ between pair types and
/// are left untouched.
#[derive(Deserialize)]
struct PairConfig {
    pair_info: PairInfo,
}

/// The pair's `PairInfo`, read from the config it stored before migrating here
pub fn load_pair_info(storage: &dyn Storage) -> StdResult<PairInfo> {
    let raw = storage
        .get(b"config")
        .ok_or_else(|| StdError::not_found("pair config"))?;
    Ok(from_json::<PairConfig>(&raw)?.pair_info)
}

/// Whether the pool's LP token is a tokenfactory denom, which it mints and burns itself, rather
/// than a CW20 contract
pub fn is_native_lp(pair_info: &PairInfo) -> bool {
    pair_info.liquidity_token.starts_with("factory/")
}
