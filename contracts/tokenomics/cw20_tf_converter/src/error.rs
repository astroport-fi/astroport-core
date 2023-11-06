use cosmwasm_std::{OverflowError, StdError};
use serde_json_wasm::de::Error as SerdeError;
use thiserror::Error;

/// This enum describes Hub's contract errors
#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unable to parse: {0}")]
    ParseError(#[from] std::num::ParseIntError),

    #[error("Contract can't be migrated!")]
    MigrationError {},

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You can not send 0 tokens")]
    ZeroAmount {},

    #[error("Insufficient funds held for the action")]
    InsufficientFunds {},

    #[error("The provided address does not have any funds")]
    NoFunds {},

    #[error("Voting power exceeds channel balance")]
    InvalidVotingPower {},

    #[error("The action {} is not allowed via an IBC memo", action)]
    NotMemoAction { action: String },

    #[error(
        "The action {} is not allowed via IBC and must be actioned via a tranfer memo",
        action
    )]
    NotIBCAction { action: String },

    #[error("Memo does not conform to the expected format: {}", reason)]
    InvalidMemo { reason: SerdeError },

    #[error("Memo was not intended for the hook contract")]
    InvalidDestination {},

    #[error("Got a submessage reply with unknown id: {id}")]
    UnknownReplyId { id: u64 },

    #[error("Invalid submessage {0}", reason)]
    InvalidSubmessage { reason: String },

    #[error("Outpost already added, remove it first: {0}", address)]
    OutpostAlreadyAdded { address: String },

    #[error("No Outpost found that matches the message channels")]
    UnknownOutpost {},

    #[error("Invalid IBC timeout: {timeout}, must be between {min} and {max} seconds")]
    InvalidIBCTimeout { timeout: u64, min: u64, max: u64 },

    #[error("Channel already established: {channel_id}")]
    ChannelAlreadyEstablished { channel_id: String },
}

impl From<OverflowError> for ContractError {
    fn from(o: OverflowError) -> Self {
        StdError::from(o).into()
    }
}
