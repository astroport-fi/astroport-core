use crate::state::PriceSourceUnchecked;
use astroport::asset::AssetInfo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub owner: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    /// Update contract config
    UpdateConfig { owner: Option<String> },
    /// Specify parameters to query asset price
    SetAssetInfo {
        asset_info: AssetInfo,
        price_source: PriceSourceUnchecked,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    /// Query contract config
    Config {},
    /// Query asset price given it's internal reference
    /// (meant to be used by protocol contracts only)
    AssetPriceByReference { asset_reference: Vec<u8> },
    /// Query asset price given an asset
    AssetPrice { asset_info: AssetInfo },
    /// Get asset's info price config
    AssetPriceConfig { asset_info: AssetInfo },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ConfigResponse {
    pub owner: String,
}
