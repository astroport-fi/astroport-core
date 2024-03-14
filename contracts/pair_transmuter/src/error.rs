use cosmwasm_std::{CheckedFromRatioError, StdError, Uint128};
use cw_utils::{ParseReplyError, PaymentError};
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error("{0}")]
    CheckedFromRatioError(#[from] CheckedFromRatioError),

    #[error("{0}")]
    ParseReplyError(#[from] ParseReplyError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Endpoint is not supported")]
    NotSupported {},

    #[error("Invalid reply message")]
    FailedToParseReply {},

    #[error("Doubling assets in asset infos")]
    DoublingAssets {},

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Event of zero transfer")]
    InvalidZeroAmount {},

    #[error("Insufficient LP tokens. Required: {required}, available: {available}")]
    InsufficientLpTokens {
        required: Uint128,
        available: Uint128,
    },

    #[error("CW20 tokens are not supported")]
    Cw20TokenNotSupported {},

    #[error("Pool supports from 2 to 5 assets")]
    InvalidAssetLength {},

    #[error("Insufficient pool {asset} balance. Want: {want}, available: {available}")]
    InsufficientPoolBalance {
        asset: String,
        want: Uint128,
        available: Uint128,
    },

    #[error("ask_asset_info must be set for pools with >2 assets")]
    AskAssetMustBeSet {},
}
