use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

#[cw_serde]
pub struct InstantiateMsg {
    /// The tokenfactory denom to burn. The burner must be made its admin to burn it.
    pub denom: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Burns the burner's whole balance of the denom, including funds sent with this call.
    /// Anyone can call it.
    Burn {},
}

#[cw_serde]
pub struct Config {
    pub denom: String,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    /// Total amount burned by this contract
    #[returns(Uint128)]
    TotalBurned {},
}

#[cw_serde]
pub struct MigrateMsg {}
