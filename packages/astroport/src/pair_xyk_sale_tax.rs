use crate::asset::{validate_native_denom, AssetInfo};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{from_binary, Addr, Api, Binary, Decimal, StdError, StdResult};

#[cw_serde]
pub struct TaxConfig<T> {
    /// The tax rate to apply to token sales of `tax_denom`.
    pub tax_rate: Decimal,
    /// The address to send the tax to
    pub tax_recipient: T,
    /// The denom of the asset to tax sales of
    pub tax_denom: String,
}
pub type TaxConfigChecked = TaxConfig<Addr>;
pub type TaxConfigUnchecked = TaxConfig<String>;

impl From<TaxConfigChecked> for TaxConfigUnchecked {
    fn from(value: TaxConfigChecked) -> Self {
        TaxConfigUnchecked {
            tax_rate: value.tax_rate,
            tax_recipient: value.tax_recipient.to_string(),
            tax_denom: value.tax_denom,
        }
    }
}

/// Implement default for TaxConfig. Useful for testing
impl Default for TaxConfigChecked {
    fn default() -> Self {
        TaxConfigChecked {
            tax_rate: Decimal::percent(5),
            tax_recipient: Addr::unchecked("addr0000"),
            tax_denom: "uusd".to_string(),
        }
    }
}
impl Default for TaxConfigUnchecked {
    fn default() -> Self {
        TaxConfigChecked::default().into()
    }
}

impl TaxConfigUnchecked {
    /// Checks that the params are valid and returns a `TaxConfigChecked`.
    pub fn check(
        self,
        api: &dyn Api,
        pair_asset_infos: &[AssetInfo],
    ) -> StdResult<TaxConfigChecked> {
        // Tax rate cannot be more than 100%
        if self.tax_rate > Decimal::one() {
            return Err(StdError::generic_err("Tax rate cannot be more than 100%"));
        }

        // Tax recipient must be a valid address
        let tax_recipient = api.addr_validate(&self.tax_recipient)?;

        // Tax denom must be a valid denom
        validate_native_denom(&self.tax_denom)?;

        // Tax denom must be one of the pair assets
        if !pair_asset_infos.contains(&AssetInfo::native(&self.tax_denom)) {
            return Err(StdError::generic_err(
                "Tax denom must be one of the pair assets",
            ));
        }

        Ok(TaxConfigChecked {
            tax_rate: self.tax_rate,
            tax_recipient,
            tax_denom: self.tax_denom,
        })
    }
}

/// Allows updating the config
#[cw_serde]
#[derive(Default)]
pub struct SaleTaxConfigUpdates {
    /// The tax rate to apply to token sales of `tax_denom`.
    pub tax_rate: Option<Decimal>,
    /// The address to send the tax to
    pub tax_recipient: Option<String>,
    /// The denom of the asset to tax sales of
    pub tax_denom: Option<String>,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: Option<bool>,
}

impl TaxConfigChecked {
    pub fn apply_updates(
        &self,
        api: &dyn Api,
        pair_asset_infos: &[AssetInfo],
        updates: SaleTaxConfigUpdates,
    ) -> StdResult<Self> {
        TaxConfigUnchecked {
            tax_rate: updates.tax_rate.unwrap_or(self.tax_rate),
            tax_recipient: updates
                .tax_recipient
                .unwrap_or_else(|| self.tax_recipient.clone().into()),
            tax_denom: updates.tax_denom.unwrap_or_else(|| self.tax_denom.clone()),
        }
        .check(api, pair_asset_infos)
    }
}
/// Extra data embedded in the default pair InstantiateMsg
#[cw_serde]
#[derive(Default)]
pub struct SaleTaxInitParams {
    pub tax_config: TaxConfigUnchecked,
    /// Whether asset balances are tracked over blocks or not.
    /// They will not be tracked if the parameter is ignored.
    /// It can not be disabled later once enabled.
    pub track_asset_balances: bool,
}

impl SaleTaxInitParams {
    /// Deserializes the params from an `Option<Binary>`.
    pub fn from_binary(data: Option<Binary>) -> StdResult<Self> {
        data.as_ref()
            .map(from_binary::<SaleTaxInitParams>)
            .transpose()?
            .ok_or_else(|| StdError::generic_err("Missing Init params"))
    }
}
