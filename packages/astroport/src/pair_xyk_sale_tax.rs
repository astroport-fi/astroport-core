use std::collections::HashMap;

use crate::asset::{validate_native_denom, AssetInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_json, Addr, Api, Binary, Decimal, StdError, StdResult};

#[cw_serde]
pub struct TaxConfig<T> {
    /// The tax rate to apply to token sales of `tax_denom`.
    pub tax_rate: Decimal,
    /// The address to send the tax to
    pub tax_recipient: T,
}
pub type TaxConfigChecked = TaxConfig<Addr>;
pub type TaxConfigUnchecked = TaxConfig<String>;

/// A map of tax configs, keyed by the denom of the asset to tax sales of. E.g. in the pair
/// APOLLO-USDC, the can have one tax rate and recipient when swapping APOLLO for USDC, and another
/// when swapping USDC for APOLLO.
#[cw_serde]
pub struct TaxConfigs<T>(HashMap<String, TaxConfig<T>>);
pub type TaxConfigsChecked = TaxConfigs<Addr>;
pub type TaxConfigsUnchecked = TaxConfigs<String>;

impl From<TaxConfigChecked> for TaxConfigUnchecked {
    fn from(value: TaxConfigChecked) -> Self {
        TaxConfigUnchecked {
            tax_rate: value.tax_rate,
            tax_recipient: value.tax_recipient.to_string(),
        }
    }
}
impl From<TaxConfigsChecked> for TaxConfigsUnchecked {
    fn from(value: TaxConfigsChecked) -> Self {
        TaxConfigs(
            value
                .0
                .into_iter()
                .map(|(k, v)| (k, v.into()))
                .collect::<HashMap<_, _>>(),
        )
    }
}
impl From<Vec<(&str, TaxConfigUnchecked)>> for TaxConfigsUnchecked {
    fn from(value: Vec<(&str, TaxConfigUnchecked)>) -> Self {
        TaxConfigs(value.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}
impl From<Vec<(&str, TaxConfigChecked)>> for TaxConfigsChecked {
    fn from(value: Vec<(&str, TaxConfigChecked)>) -> Self {
        TaxConfigs(value.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
    }
}

/// Implement default for TaxConfig. Useful for testing
impl Default for TaxConfigChecked {
    fn default() -> Self {
        TaxConfigChecked {
            tax_rate: Decimal::percent(5),
            tax_recipient: Addr::unchecked("addr0000"),
        }
    }
}
impl Default for TaxConfigUnchecked {
    fn default() -> Self {
        TaxConfigChecked::default().into()
    }
}
impl Default for TaxConfigsChecked {
    fn default() -> Self {
        vec![("uusd", TaxConfigChecked::default())].into()
    }
}
impl Default for TaxConfigsUnchecked {
    fn default() -> Self {
        TaxConfigsChecked::default().into()
    }
}

impl TaxConfigUnchecked {
    /// Checks that the params are valid and returns a `TaxConfigChecked`.
    pub fn check(self, api: &dyn Api) -> StdResult<TaxConfigChecked> {
        // Tax rate cannot be more than 50% to avoid blocking swaps if set to 100% or errors if
        // set to more than 100%.
        if self.tax_rate > Decimal::percent(50) {
            return Err(StdError::generic_err("Tax rate cannot be more than 50%"));
        }

        // Tax recipient must be a valid address
        let tax_recipient = api.addr_validate(&self.tax_recipient)?;

        Ok(TaxConfigChecked {
            tax_rate: self.tax_rate,
            tax_recipient,
        })
    }
}

impl TaxConfigsUnchecked {
    /// Creates a new empty `TaxConfigsUnchecked`.
    pub fn new() -> Self {
        TaxConfigs(HashMap::new())
    }

    /// Checks that the params are valid and returns a `TaxConfigsChecked`.
    pub fn check(
        self,
        api: &dyn Api,
        pair_asset_infos: &[AssetInfo],
    ) -> StdResult<TaxConfigsChecked> {
        let mut tax_configs = HashMap::new();
        for (tax_denom, tax_config) in self.0.into_iter() {
            // Tax denom must be a valid denom
            validate_native_denom(&tax_denom)?;

            // Tax denom must be one of the pair assets
            if !pair_asset_infos.contains(&AssetInfo::native(&tax_denom)) {
                return Err(StdError::generic_err(
                    "Tax denom must be one of the pair assets",
                ));
            }

            let tax_config = tax_config.check(api)?;
            tax_configs.insert(tax_denom, tax_config);
        }
        Ok(TaxConfigs(tax_configs))
    }
}

impl TaxConfigsChecked {
    /// Returns the tax config for the given tax denom if it exists.
    pub fn get(&self, tax_denom: &str) -> Option<&TaxConfigChecked> {
        self.0.get(tax_denom)
    }
}

/// Allows updating the config
#[cw_serde]
#[derive(Default)]
pub struct SaleTaxConfigUpdates {
    /// The new tax configs to apply to the pair.
    pub tax_configs: Option<TaxConfigsUnchecked>,
    /// The new address that is allowed to updated the tax configs.
    pub tax_config_admin: Option<String>,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: Option<bool>,
}

/// Extra data embedded in the default pair InstantiateMsg
#[cw_serde]
pub struct SaleTaxInitParams {
    /// The configs of the trade taxes for the pair.
    pub tax_configs: TaxConfigs<String>,
    /// The address that is allowed to updated the tax configs.
    pub tax_config_admin: String,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: bool,
}

impl Default for SaleTaxInitParams {
    fn default() -> Self {
        Self {
            tax_config_admin: "addr0000".to_string(),
            tax_configs: TaxConfigs::default(),
            track_asset_balances: false,
        }
    }
}

impl SaleTaxInitParams {
    /// Deserializes the params from an `Option<Binary>`.
    pub fn from_json(data: Option<Binary>) -> StdResult<Self> {
        data.as_ref()
            .map(from_json::<SaleTaxInitParams>)
            .transpose()?
            .ok_or_else(|| StdError::generic_err("Missing Init params"))
    }
}

#[cw_serde]
/// Message used when migrating the contract from the standard XYK pair.
pub struct MigrateMsg {
    /// The configs of the trade taxes for the pair.
    pub tax_configs: TaxConfigs<String>,
    /// The address that is allowed to updated the tax configs.
    pub tax_config_admin: String,
}
