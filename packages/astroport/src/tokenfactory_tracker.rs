use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Uint128, Uint64};

#[cw_serde]
pub struct InstantiateMsg {
    pub tracked_denom: String,
    pub tokenfactory_module_address: String,
}

#[cw_serde]
pub enum ExecuteMsg {}

#[cw_serde]
pub enum SudoMsg {
    BlockBeforeSend {
        from: String,
        to: String,
        amount: Coin,
    },
    TrackBeforeSend {
        from: String,
        to: String,
        amount: Coin,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Uint128)]
    BalanceAt { address: String, timestamp: Uint64 },
    #[returns(Uint128)]
    TotalSupplyAt { timestamp: Uint64 },
}
