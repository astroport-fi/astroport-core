use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},
    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
    #[error("Timelock {fun_name} {msg}.")]
    TimelockError { fun_name: String, msg: String },

    #[error("Governor {msg}.")]
    ProposalError { msg: String },

    #[error("balances error: {msg}.")]
    BalanceError { msg: String },
}

impl ContractError {
    pub fn proposal_err<S: Into<String>>(msg: S) -> Self {
        ContractError::ProposalError { msg: msg.into() }
    }
    pub fn balance_err<S: Into<String>>(msg: S) -> Self {
        ContractError::BalanceError { msg: msg.into() }
    }
}
