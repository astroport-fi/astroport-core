use cosmwasm_std::{StdError, Uint128};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("This pool is exit-only: only balanced withdrawals are allowed")]
    ExitOnly {},

    #[error("Only balanced withdrawals are allowed: leave `assets` empty")]
    ImbalancedWithdraw {},

    #[error("Nothing to withdraw: the pool holds no assets for this share yet")]
    NothingToWithdraw {},

    #[error(
        "This pool's LP token is a CW20: send it to the pool with {{\"withdraw_liquidity\":{{}}}}"
    )]
    Cw20LpWithdraw {},

    #[error("Unsupported hook: send the LP token with {{\"withdraw_liquidity\":{{}}}}")]
    UnsupportedHook {},

    #[error("Withdrawals are closed: {0}")]
    Closed(String),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Received {received} {asset_name} but expected at least {expected}")]
    WithdrawSlippageViolation {
        asset_name: String,
        received: Uint128,
        expected: Uint128,
    },

    #[error("The asset {0} does not belong to the pair")]
    InvalidAsset(String),

    #[error("Can't migrate {contract} {version} to exit-only: {reason}")]
    MigrationError {
        contract: String,
        version: String,
        reason: String,
    },

    #[error("This code can only be migrated to")]
    NotInstantiable {},
}
