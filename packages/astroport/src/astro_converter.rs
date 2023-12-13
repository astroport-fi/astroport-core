use cosmwasm_schema::cw_serde;
use cw20::Cw20ReceiveMsg;

use crate::asset::AssetInfo;

/// Default timeout for IBC transfer (5 minutes)
pub const DEFAULT_TIMEOUT: u64 = 300;

#[cw_serde]
pub struct OutpostBurnParams {
    pub terra_burn_addr: String,
    pub old_astro_transfer_channel: String,
}

#[cw_serde]
pub struct Config {
    pub old_astro_asset_info: AssetInfo,
    pub new_astro_denom: String,
    pub outpost_burn_params: Option<OutpostBurnParams>,
}

#[cw_serde]
pub struct InstantiateMsg {
    pub old_astro_asset_info: AssetInfo,
    pub new_astro_denom: String,
    pub outpost_burn_params: Option<OutpostBurnParams>,
}

#[cw_serde]
pub enum ExecuteMsg {
    Convert {},
    Receive(Cw20ReceiveMsg),
    TransferForBurning { timeout: Option<u64> },
    Burn {},
}
