use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_json, StdError, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use serde::Deserialize;

use astroport::asset::{Asset, PairInfo};

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

/// When withdrawals open, set by the migration
#[cw_serde]
pub struct Gate {
    /// The pool's LP supply when it was drained. Withdrawals stay closed while the supply is
    /// above it, since LP minted after the drain would take a share of the refill.
    pub max_total_share: Uint128,
    /// The refill each asset must reach, for `max_total_share` LP tokens. Withdrawals open once
    /// every asset backs each LP token at least this much, so none happen against a partial
    /// refill or a donation.
    pub min_balances: Vec<Asset>,
}

/// Its own key, apart from `pair_concentrated`'s `exit_only`
pub const GATE: Item<Gate> = Item::new("exit_only_gate");
