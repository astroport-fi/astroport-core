use astroport::asset::AssetInfo;
use astroport::common::OwnershipProposal;
use astroport::maker::{Config, RouteStep, SeizeConfig};
use cosmwasm_std::Uint128;
use cw_storage_plus::{Item, Map};

/// Config is the general settings of the contract.
pub const CONFIG: Item<Config> = Item::new("config");
/// Stores the latest proposal to change contract ownership
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
/// Stores the latest timestamp when fees were collected
pub const LAST_COLLECT_TS: Item<u64> = Item::new("last_collect_ts");
/// Stores seize config
pub const SEIZE_CONFIG: Item<SeizeConfig> = Item::new("seize_config");
/// Routes is a map of asset_in and asset_out to pool address.
/// Key: (asset_in) binary representing [`AssetInfo`] converted with [`crate::utils::asset_info_key`],
/// Value: RouteStep object {asset_out, pool_addr}
pub const ROUTES: Map<&[u8], RouteStep> = Map::new("routes");
/// Temporary storage for pre reply dev fund asset amount. Used for a fair dev fund cut distribution.
pub const TMP_REPLY_DATA: Item<Uint128> = Item::new("pre_reply_dev_fund_amount");
