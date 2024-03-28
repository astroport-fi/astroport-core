use cosmwasm_schema::{cw_serde, QueryResponses};
use cw20::Cw20ReceiveMsg;
use std::ops::RangeInclusive;

use crate::asset::AssetInfo;

/// Default timeout for IBC transfer (5 minutes)
pub const DEFAULT_TIMEOUT: u64 = 300;
/// Timeout limits for IBC transfer (from 2 to 10 minutes)
pub const TIMEOUT_LIMITS: RangeInclusive<u64> = 120..=600;

/// Defines parameters for sending old IBCed ASTRO to the Hub for burning.
#[cw_serde]
pub struct OutpostBurnParams {
    pub terra_burn_addr: String,
    pub old_astro_transfer_channel: String,
}

/// Main contract config.
/// `old_astro_asset_info` can be either cw20 contract or IBC denom depending on the chain.
/// `new_astro_denom` is always native coin either token factory or IBC denom.
/// `outpost_burn_params` must be None for old Hub and Some for all other outposts.
#[cw_serde]
pub struct Config {
    pub old_astro_asset_info: AssetInfo,
    pub new_astro_denom: String,
    pub outpost_burn_params: Option<OutpostBurnParams>,
}

/// Instantiate message. Fields meaning is the same as in Config.
#[cw_serde]
pub struct InstantiateMsg {
    pub old_astro_asset_info: AssetInfo,
    pub new_astro_denom: String,
    pub outpost_burn_params: Option<OutpostBurnParams>,
}

#[cw_serde]
pub struct Cw20HookMsg {
    pub receiver: Option<String>,
}

/// Available contract execute messages.
/// - `Convert` is used to convert old ASTRO to new ASTRO on outposts. New ASTRO sent to `receiver` if specified.
/// - `Receive` is used to process cw20 send hook from old cw20 ASTRO and release new ASTRO token on the old Hub.
/// Custom `receiver` is forwarded within Cw20HookMsg.
/// - `TransferForBurning` is used to send old ASTRO to the old Hub for burning. Is meant to be used by outposts.
/// - `Burn` is used to burn old cw20 ASTRO on the old Hub.
#[cw_serde]
pub enum ExecuteMsg {
    Convert { receiver: Option<String> },
    Receive(Cw20ReceiveMsg),
    TransferForBurning { timeout: Option<u64> },
    Burn {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(Config)]
    Config {},
}
