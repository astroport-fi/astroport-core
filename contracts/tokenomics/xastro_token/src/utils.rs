use cosmwasm_std::{Pair, StdError, StdResult, Uint128};

/// Helper function for deserialization
pub(crate) fn deserialize_pair(pair: StdResult<Pair<Uint128>>) -> StdResult<String> {
    let (addr, _) = pair?;
    let addr =
        String::from_utf8(addr).map_err(|_| StdError::generic_err("Deserialization error"))?;

    Ok(addr)
}
