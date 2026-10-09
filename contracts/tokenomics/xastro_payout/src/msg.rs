use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

use astroport::asset::AssetInfo;

#[cw_serde]
pub struct InstantiateMsg {
    /// Can close the payout
    pub owner: String,
    /// Where `close` sends what is left. Fixed here so the owner can't sweep anywhere else.
    pub sweep_recipient: String,
    /// The asset paid out
    pub asset_info: AssetInfo,
    /// Hex sha256 Merkle root of the (address, amount) list
    pub merkle_root: String,
    /// Sum of every amount in the list. The contract never pays out more than this.
    pub total: Uint128,
}

/// One entry of the list with its Merkle proof
#[cw_serde]
pub struct Payment {
    pub address: String,
    pub amount: Uint128,
    /// Hex sibling hashes from the leaf up to the root
    pub proof: Vec<String>,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Pays every entry to its own address. Anyone can call it. An address that was already paid
    /// is skipped, and an invalid proof fails the whole call.
    Pay { payments: Vec<Payment> },
    /// Ends the payout and sends the whole remaining balance to the sweep recipient. Only the
    /// owner can call it, and no payment can be made afterwards.
    Close {},
    /// Creates a request to change contract ownership
    ProposeNewOwner { owner: String, expires_in: u64 },
    /// Removes the ownership change request
    DropOwnershipProposal {},
    /// Claims contract ownership
    ClaimOwnership {},
}

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub sweep_recipient: Addr,
    pub asset_info: AssetInfo,
    pub merkle_root: String,
    pub total: Uint128,
}

#[cw_serde]
pub struct Status {
    pub paid_total: Uint128,
    pub paid_count: u64,
    pub closed: bool,
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(Status)]
    Status {},
    /// The amount paid to an address, if it was paid
    #[returns(Option<Uint128>)]
    Paid { address: String },
    /// Paid addresses in address order
    #[returns(Vec<(Addr, Uint128)>)]
    PaidList {
        start_after: Option<String>,
        limit: Option<u32>,
    },
}

#[cw_serde]
pub struct MigrateMsg {}
