use astroport::asset::AssetInfo;
use astroport::querier::query_token_precision;
use cosmwasm_std::{Addr, QuerierWrapper, StdResult};

/// Validates array of assets. If asset is native coin then this function checks whether
/// it has been registered in registry or not.
pub(crate) fn is_native_registered(
    querier: &QuerierWrapper,
    asset_infos: &[AssetInfo],
    factory_addr: &Addr,
) -> StdResult<()> {
    for asset_info in asset_infos {
        query_token_precision(querier, asset_info, factory_addr)?;
    }

    Ok(())
}
