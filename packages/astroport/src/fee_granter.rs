use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: String,
    #[serde(default)]
    pub admins: Vec<String>,
    pub gas_denom: String,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Create grant with fixed amount for a contract.
    /// Executor: owner or admin.
    Grant {
        grantee_contract: String,
        amount: Uint128,
        /// Bypassing can be enabled in case when grant was revoked, but some coins are left in grant.
        /// When creating a new grant with bypass enabled be very careful not to clash with other grants.
        #[serde(default)]
        bypass_amount_check: bool,
    },
    /// Revoke grant for a contract. Some coins may be left in fee_granter account.
    /// Executor: owner or admin.
    Revoke { grantee_contract: String },
    /// Transfer coins from fee_granter account.
    /// It doesn't have any checks because wasm module doesn't allow Stargate queries.
    /// Executor: owner or admin.
    TransferCoins {
        amount: Uint128,
        receiver: Option<String>,
    },
    /// Executor: owner.
    UpdateAdmins {
        #[serde(default)]
        add: Vec<String>,
        #[serde(default)]
        remove: Vec<String>,
    },
    /// ProposeNewOwner creates a proposal to change contract ownership.
    /// The validity period for the proposal is set in the `expires_in` variable.
    ProposeNewOwner {
        /// Newly proposed contract owner
        owner: String,
        /// The date after which this proposal expires
        expires_in: u64,
    },
    /// DropOwnershipProposal removes the existing offer to change contract ownership.
    DropOwnershipProposal {},
    /// Used to claim contract ownership.
    ClaimOwnership {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
    #[returns(Vec<GrantResponse>)]
    GrantsList {
        start_after: Option<String>,
        limit: Option<u32>,
    },
    #[returns(GrantResponse)]
    GrantFor { grantee_contract: String },
}

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub admins: Vec<Addr>,
    pub gas_denom: String,
}

#[cw_serde]
pub struct GrantResponse {
    pub grantee_contract: String,
    pub amount: Uint128,
}
