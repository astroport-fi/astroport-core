use cosmwasm_std::{Addr, Api, Decimal, Fraction, QuerierWrapper, StdError, StdResult, Uint128};

use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo};
use astroport::pair_reserve::{FlowParams, PoolParams};
use astroport::querier::query_supply;
use astroport::DecimalCheckedOps;

use crate::error::ContractError;
use crate::state::Config;

pub(crate) trait AssetsValidator {
    fn validate(&self, api: &dyn Api) -> Result<(), ContractError>;
}

impl AssetsValidator for [Asset; 2] {
    fn validate(&self, api: &dyn Api) -> Result<(), ContractError> {
        let asset_infos: [AssetInfo; 2] = self
            .iter()
            .map(|Asset { info, .. }| info.clone())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        asset_infos.validate(api)
    }
}

impl AssetsValidator for [AssetInfo; 2] {
    fn validate(&self, api: &dyn Api) -> Result<(), ContractError> {
        self.iter()
            .map(|asset_info| check_asset_info(api, asset_info))
            .collect::<StdResult<Vec<_>>>()?;
        if self[0] == self[1] {
            return Err(ContractError::DoublingAssets {});
        }

        if self[0].is_native_token() && !self[1].is_native_token()
            || self[1].is_native_token() && !self[0].is_native_token()
        {
            Ok(())
        } else {
            Err(
                StdError::generic_err("Reserve pool accepts (native token, CW20 token) pairs only")
                    .into(),
            )
        }
    }
}

pub(crate) trait ParametersValidator {
    fn validate(&self, direction: Option<&str>) -> Result<(), ContractError>;
}

impl ParametersValidator for FlowParams {
    fn validate(&self, direction: Option<&str>) -> Result<(), ContractError> {
        let direction = direction.unwrap().to_string();
        if self.base_pool.is_zero() {
            return Err(ContractError::ValidationError(
                direction,
                "base_pool cannot be zero".to_string(),
            ));
        }
        if !(1..=10000).contains(&self.min_spread) {
            return Err(ContractError::ValidationError(
                direction,
                "Min spread must be within [1, 10000] limit".to_string(),
            ));
        }
        if self.recovery_period == 0 {
            return Err(ContractError::ValidationError(
                direction,
                "Recovery period cannot be zero".to_string(),
            ));
        }
        Ok(())
    }
}

impl ParametersValidator for PoolParams {
    fn validate(&self, _: Option<&str>) -> Result<(), ContractError> {
        self.entry
            .validate(Some("Inflow"))
            .and(self.exit.validate(Some("Outflow")))
    }
}

pub(crate) fn check_asset_info(api: &dyn Api, asset_info: &AssetInfo) -> StdResult<()> {
    match asset_info {
        AssetInfo::Token { contract_addr } => {
            addr_validate_to_lower(api, contract_addr.as_str())?;
            Ok(())
        }
        AssetInfo::NativeToken { denom } => {
            if denom.clone() != denom.to_lowercase() {
                Err(StdError::generic_err("Native tokens must be lowercase"))
            } else if denom.starts_with("ibc/") {
                Err(StdError::generic_err("IBC tokens are forbidden"))
            } else {
                Ok(())
            }
        }
    }
}

/// ## Description
/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **config** is an object of type [`Config`].
pub(crate) fn pool_info(
    querier: &QuerierWrapper,
    config: &Config,
) -> StdResult<([Asset; 2], Uint128)> {
    let contract_addr = config.pair_info.contract_addr.clone();
    let pools = config.pair_info.query_pools(querier, contract_addr)?;
    let total_share = query_supply(querier, config.pair_info.liquidity_token.clone())?;

    Ok((pools, total_share))
}

/// ## Description
/// Returns the amount of pool assets that correspond to an amount of LP tokens.
/// ## Params
/// * **pools** are an array of [`Asset`] type items. These are the assets in the pool.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to compute a corresponding amount of assets for.
///
/// * **total_share** is an object of type [`Uint128`]. This is the total amount of LP tokens currently minted.
pub(crate) fn get_share_in_assets(
    pools: &[Asset; 2],
    amount: Uint128,
    total_share: Uint128,
) -> Vec<Asset> {
    let mut share_ratio = Decimal::zero();
    if !total_share.is_zero() {
        share_ratio = Decimal::from_ratio(amount, total_share);
    }

    pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect()
}

/// Bulk validation and conversion between [`String`] -> [`Addr`] for an array of addresses.
/// If any address is invalid, the function returns [`StdError`].
pub(crate) fn validate_addresses(api: &dyn Api, addresses: &[String]) -> StdResult<Vec<Addr>> {
    addresses
        .iter()
        .map(|addr| addr_validate_to_lower(api, addr))
        .collect()
}

pub(crate) enum RateDirection {
    BTC2USD,
    USD2BTC,
}

pub(crate) fn get_oracle_price(
    querier: &QuerierWrapper,
    direction: RateDirection,
    oracles: &[Addr],
) -> Result<Decimal, ContractError> {
    let prices = oracles
        .iter()
        .filter_map(|oracle_addr| querier.query_wasm_smart(oracle_addr, &()).ok())
        .collect::<Vec<_>>();
    if prices.is_empty() {
        Err(ContractError::OraclesError {})
    } else {
        let sum = prices
            .iter()
            .try_fold(Decimal::zero(), |acc, &x| acc.checked_add(x))?;
        if sum.is_zero() {
            Err(ContractError::OraclesError {})
        } else {
            let exchange_rate = sum / Uint128::from(prices.len() as u128);
            match direction {
                RateDirection::BTC2USD => Ok(exchange_rate),
                RateDirection::USD2BTC => Ok(exchange_rate.inv().unwrap()),
            }
        }
    }
}
