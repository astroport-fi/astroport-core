use cosmwasm_std::StdError;
use thiserror::Error;

/// ## Description
/// This enum describes errors for generator_proxy_to_mirror contract!
#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Incorrect CW20 hook message variant!")]
    IncorrectCw20HookMessageVariant {},
}
