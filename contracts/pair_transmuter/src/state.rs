use std::collections::HashMap;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, QuerierWrapper, StdResult, Uint128};
use cw_storage_plus::Item;

use astroport::asset::{Asset, AssetInfoExt, PairInfo};

use crate::error::ContractError;

/// This structure stores the main pair parameters.
#[cw_serde]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// Map of normalization coefficients (stringified asset info -> normalization coefficient)
    pub norm_coeff: HashMap<String, Uint128>,
}

impl Config {
    pub fn new(
        querier: QuerierWrapper,
        pair_info: PairInfo,
        factory_addr: Addr,
    ) -> StdResult<Self> {
        let decimals = pair_info
            .asset_infos
            .iter()
            .map(|info| Ok((info.to_string(), info.decimals(&querier, &factory_addr)?)))
            .collect::<StdResult<Vec<_>>>()?;
        let max_decimals = *decimals.iter().map(|(_, decimals)| decimals).max().unwrap() as u32;
        let max_norm_factor = 10u64.pow(max_decimals);
        let norm_coeff = decimals
            .into_iter()
            .map(|(key, dec)| (key, (max_norm_factor / 10u64.pow(dec as u32)).into()))
            .collect();

        Ok(Self {
            pair_info,
            factory_addr,
            norm_coeff,
        })
    }

    pub fn normalize(&self, asset: &Asset) -> Result<Asset, ContractError> {
        let norm_coeff = self
            .norm_coeff
            .get(&asset.info.to_string())
            .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

        Ok(asset.info.with_balance(asset.amount * norm_coeff))
    }

    pub fn denormalize(&self, asset: &Asset) -> Result<Asset, ContractError> {
        let norm_coeff = self
            .norm_coeff
            .get(&asset.info.to_string())
            .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

        Ok(asset.info.with_balance(asset.amount / norm_coeff))
    }
}

pub const CONFIG: Item<Config> = Item::new("config");
