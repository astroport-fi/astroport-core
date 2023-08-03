use astroport_pair::error::ContractError as PairContractError;
use cosmwasm_std::{StdError, Uint128};

use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    XykPairError(#[from] PairContractError),

    #[error("Unsupported Cw20 hook message")]
    UnsupportedCw20HookMsg {},

    #[error("Unsupported execute message")]
    UnsupportedExecuteMsg {},

    #[error("Invalid reply data")]
    InvalidReplyData {},

    #[error("Slippage is more than expected: received {0}, expected {1} LP tokens")]
    ProvideSlippageViolation(Uint128, Uint128),

    #[error("Received {received} {asset_name} but expected {expected}")]
    WithdrawSlippageViolation {
        asset_name: String,
        received: Uint128,
        expected: Uint128,
    },

    #[error("Asset {0} is not in the pair")]
    AssetNotInPair(String),

    #[error("Wrong asset length: expected {expected}, actual {actual}")]
    WrongAssetLength { expected: usize, actual: usize },

    #[error("Liquidity manager supports only pools with 2 assets")]
    WrongPoolLength {},
}
