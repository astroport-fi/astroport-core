use cosmwasm_std::{ConversionOverflowError, StdError};
use thiserror::Error;

use astroport_pcl_common::error::PclError;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum OrderbookError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    PclError(#[from] PclError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("Orderbook is already synced")]
    NoNeedToSync {},
}
