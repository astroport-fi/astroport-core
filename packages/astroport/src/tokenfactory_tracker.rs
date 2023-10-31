use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Coin, Uint128, Uint64};

#[cw_serde]
pub struct InstantiateMsg {
    // The denom to track
    pub tracked_denom: String,
    // The module address of the TokenFactory module
    pub tokenfactory_module_address: String,
}

#[cw_serde]
pub enum SudoMsg {
    // Sudo endpoint called by chain before sending tokens
    // Errors returned by this endpoint will prevent the transaction from being sent
    BlockBeforeSend {
        // The address being sent from
        from: String,
        // The address being sent to
        to: String,
        // The amount and denom being sent
        amount: Coin,
    },
    // Sudo endpoint called by chain before sending tokens
    // Errors returned by this endpoint will NOT prevent the transaction from being sent
    TrackBeforeSend {
        // The address being sent from
        from: String,
        // The address being sent to
        to: String,
        // The amount and denom being sent
        amount: Coin,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    // Returns the balance of the given address at the given timestamp
    // in seconds. If unset, returns the balance at the current time
    #[returns(Uint128)]
    BalanceAt {
        address: String,
        timestamp: Option<Uint64>,
    },
    // Returns the total token supply at the given timestamp in seconds.
    // If unset, returns the balance at the current time
    #[returns(Uint128)]
    TotalSupplyAt { timestamp: Option<Uint64> },
}
