use cosmwasm_std::{Addr, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

use astroport::astro_converter::TIMEOUT_LIMITS;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PaymentError(#[from] PaymentError),

    #[error(
        "Invalid endpoint. Consider cw20::send on Terra and converter::convert on Astroport outposts"
    )]
    InvalidEndpoint {},

    #[error("Burn is only allowed on Terra")]
    BurnError {},

    #[error("Transfer to burn is only available on Astroport outposts")]
    IbcTransferError {},

    #[error("Invalid cw20 token: {0}")]
    UnsupportedCw20Token(Addr),

    #[error("Invalid timeout: {0}. Max {}s, min {}s", TIMEOUT_LIMITS.end(), TIMEOUT_LIMITS.start())]
    InvalidTimeout {},
}
