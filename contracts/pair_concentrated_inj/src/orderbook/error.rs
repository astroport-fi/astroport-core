use crate::error::ContractError;
use astroport::injective_ext::InjMathError;
use astroport_circular_buffer::error::BufferError;
use cosmwasm_std::{ConversionOverflowError, Decimal256RangeExceeded, OverflowError, StdError};
use thiserror::Error;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum OrderbookError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("{0}")]
    ConversionOverflowError(#[from] ConversionOverflowError),

    #[error("{0}")]
    Decimal256RangeExceeded(#[from] Decimal256RangeExceeded),

    #[error("{0}")]
    OverflowError(#[from] OverflowError),

    #[error("{0}")]
    ContractError(#[from] ContractError),

    #[error("{0}")]
    CircularBuffer(#[from] BufferError),

    #[error("{0}")]
    InjMathError(#[from] InjMathError),

    #[error("Market {0} was not found")]
    MarketNotFound(String),

    #[error("No observation found for market")]
    NoObservationFound {},
}

impl From<OrderbookError> for StdError {
    fn from(value: OrderbookError) -> Self {
        match value {
            OrderbookError::Std(err) => err,
            _ => StdError::generic_err(value.to_string()),
        }
    }
}
