use cosmwasm_std::StdError;
use thiserror::Error;

pub type BufferResult<R> = Result<R, BufferError>;

/// This enum describes pair contract errors
#[derive(Error, Debug, PartialEq)]
pub enum BufferError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Can't reduce capacity because buffer contains key(s) greater than new head")]
    ReduceCapacityError {},

    #[error("Can't save value at index {0} which greater than capacity")]
    SaveValueError(u32),

    #[error("Can't read value at index {0} which greater than capacity")]
    ReadAheadError(u32),

    #[error("Index {0} not found")]
    IndexNotFound(u32),

    #[error("Buffer not initialized")]
    BufferNotInitialized {},

    #[error("Buffer already initialized")]
    BufferAlreadyInitialized {},
}

impl From<BufferError> for StdError {
    fn from(value: BufferError) -> Self {
        match value {
            BufferError::Std(err) => err,
            _ => StdError::generic_err(value.to_string()),
        }
    }
}

#[cfg(test)]
mod testing {
    use super::*;

    #[test]
    fn test_buffer_error() {
        let err = BufferError::Std(StdError::generic_err("test"));
        let std_err: StdError = err.into();
        assert_eq!(std_err, StdError::generic_err("test"));

        let custom_err = BufferError::ReduceCapacityError {};
        let std_err: StdError = custom_err.into();
        assert_eq!(
            std_err,
            StdError::generic_err(
                "Can't reduce capacity because buffer contains key(s) greater than new head"
            )
        );
    }
}
