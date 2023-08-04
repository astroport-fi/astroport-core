use cosmwasm_schema::cw_serde;

/// Messages handled via CW20 transfers
#[cw_serde]
pub enum Cw20HookMsg {
    /// Executes instructions received via an IBC transfer memo in the
    /// CW20-ICS20 contract
    OutpostMemo {
        /// The channel the memo was received on
        channel: String,
        /// The original sender of the packet on the outpost
        sender: String,
        /// The original receiver of the packet on the Hub
        receiver: String,
        /// The memo containing the instruction to execute
        memo: String,
    },
    /// Handle failed CW20 IBC transfers
    TransferFailure {
        // The original receiver of the funds
        receiver: String,
    },
}
