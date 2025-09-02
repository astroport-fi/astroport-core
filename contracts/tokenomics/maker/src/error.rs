use astroport::maker::{PoolRoute, MAX_ALLOWED_SPREAD, MAX_SWAPS_DEPTH};
use cosmwasm_std::{CheckedMultiplyRatioError, OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    CheckedMultiplyRatioError(#[from] CheckedMultiplyRatioError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Max spread too high. Max allowed: {MAX_ALLOWED_SPREAD}")]
    MaxSpreadTooHigh {},

    #[error("Incorrect cooldown. Min: {min}, Max: {max}")]
    IncorrectCooldown { min: u64, max: u64 },

    #[error("Empty routes")]
    EmptyRoutes {},

    #[error("Pool {pool_addr} doesn't have asset {asset}")]
    InvalidPoolAsset { pool_addr: String, asset: String },

    #[error("Message contains duplicated routes")]
    DuplicatedRoutes {},

    #[error("Route cannot start with ASTRO. Error in route: {route:?}")]
    AstroInRoute { route: PoolRoute },

    #[error("No registered route for {asset}")]
    RouteNotFound { asset: String },

    #[error("Collect cooldown has not elapsed. Next collect is possible at {next_collect_ts}")]
    Cooldown { next_collect_ts: u64 },

    #[error("Failed to build route for {asset} with the max multi-hop depth {MAX_SWAPS_DEPTH}")]
    FailedToBuildRoute { asset: String },

    #[error("Invalid reply id")]
    InvalidReplyId {},

    #[error("Empty collectable assets vector")]
    EmptyAssets {},

    #[error("Nothing to collect")]
    NothingToCollect {},

    #[error("Contract can't be migrated!")]
    MigrationError {},
}
