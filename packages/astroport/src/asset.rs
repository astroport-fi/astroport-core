use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::cosmwasm_ext::DecimalToInteger;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, from_slice, to_binary, Addr, Api, BankMsg, Coin, ConversionOverflowError, CosmosMsg,
    CustomMsg, CustomQuery, Decimal256, Fraction, MessageInfo, QuerierWrapper, StdError, StdResult,
    Uint128, Uint256, WasmMsg,
};
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use cw_utils::must_pay;
use itertools::Itertools;

use crate::factory::PairType;
use crate::pair::QueryMsg as PairQueryMsg;
use crate::querier::{
    query_balance, query_token_balance, query_token_precision, query_token_symbol,
};
use crate::token::is_valid_symbol;

/// UST token denomination
pub const UUSD_DENOM: &str = "uusd";
/// LUNA token denomination
pub const ULUNA_DENOM: &str = "uluna";
/// Minimum initial LP share
pub const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
/// Maximum denom length
pub const DENOM_MAX_LENGTH: usize = 60;

/// This enum describes a Terra asset (native or CW20).
#[cw_serde]
pub struct Asset {
    /// Information about an asset stored in a [`AssetInfo`] struct
    pub info: AssetInfo,
    /// A token amount
    pub amount: Uint128,
}

/// This struct describes a Terra asset as decimal.
#[cw_serde]
pub struct DecimalAsset {
    pub info: AssetInfo,
    pub amount: Decimal256,
}

impl DecimalAsset {
    pub fn into_asset(self, precision: impl Into<u32> + Sized) -> StdResult<Asset> {
        Ok(Asset {
            info: self.info,
            amount: self.amount.to_uint(precision)?,
        })
    }
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.info)
    }
}

impl Asset {
    /// Returns true if the token is native. Otherwise returns false.
    pub fn is_native_token(&self) -> bool {
        self.info.is_native_token()
    }

    /// For native tokens of type [`AssetInfo`] uses the default method [`BankMsg::Send`] to send a
    /// token amount to a recipient.
    /// For a token of type [`AssetInfo`] we use the default method [`Cw20ExecuteMsg::Transfer`].
    pub fn into_msg<T>(self, recipient: impl Into<String>) -> StdResult<CosmosMsg<T>>
    where
        T: CustomMsg,
    {
        let recipient = recipient.into();
        match &self.info {
            AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient,
                    amount: self.amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::NativeToken { .. } => Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient,
                amount: vec![self.as_coin()?],
            })),
        }
    }

    /// Validates an amount of native tokens being sent.
    pub fn assert_sent_native_token_balance(&self, message_info: &MessageInfo) -> StdResult<()> {
        if let AssetInfo::NativeToken { denom } = &self.info {
            let amount = must_pay(message_info, denom)
                .map_err(|err| StdError::generic_err(err.to_string()))?;
            if self.amount == amount {
                Ok(())
            } else {
                Err(StdError::generic_err(
                    "Native token balance mismatch between the argument and the transferred",
                ))
            }
        } else {
            Ok(())
        }
    }

    pub fn to_decimal_asset(&self, precision: impl Into<u32>) -> StdResult<DecimalAsset> {
        Ok(DecimalAsset {
            info: self.info.clone(),
            amount: Decimal256::with_precision(self.amount, precision.into())?,
        })
    }

    pub fn as_coin(&self) -> StdResult<Coin> {
        match &self.info {
            AssetInfo::Token { .. } => {
                Err(StdError::generic_err("Cannot convert token asset to coin"))
            }
            AssetInfo::NativeToken { denom } => Ok(coin(self.amount.u128(), denom)),
        }
    }
}

pub trait CoinsExt {
    fn assert_coins_properly_sent(
        &self,
        assets: &[Asset],
        pool_asset_infos: &[AssetInfo],
    ) -> StdResult<()>;
}

impl CoinsExt for Vec<Coin> {
    fn assert_coins_properly_sent(
        &self,
        input_assets: &[Asset],
        pool_asset_infos: &[AssetInfo],
    ) -> StdResult<()> {
        let pool_coins = pool_asset_infos
            .iter()
            .filter_map(|asset_info| match asset_info {
                AssetInfo::NativeToken { denom } => Some(denom.to_string()),
                _ => None,
            })
            .collect::<HashSet<_>>();

        let input_coins = input_assets
            .iter()
            .filter_map(|asset| match &asset.info {
                AssetInfo::NativeToken { denom } => Some((denom.to_string(), asset.amount)),
                _ => None,
            })
            .map(|pair| {
                if pool_coins.contains(&pair.0) {
                    Ok(pair)
                } else {
                    Err(StdError::generic_err(format!(
                        "Asset {} is not in the pool",
                        pair.0
                    )))
                }
            })
            .collect::<StdResult<HashMap<_, _>>>()?;

        self.iter().try_for_each(|coin| {
            if input_coins.contains_key(&coin.denom) {
                if input_coins[&coin.denom] == coin.amount {
                    Ok(())
                } else {
                    Err(StdError::generic_err(
                        "Native token balance mismatch between the argument and the transferred",
                    ))
                }
            } else {
                Err(StdError::generic_err(format!(
                    "Supplied coins contain {} that is not in the input asset vector",
                    coin.denom
                )))
            }
        })
    }
}

/// This enum describes available Token types.
/// ## Examples
/// ```
/// # use cosmwasm_std::Addr;
/// # use astroport::asset::AssetInfo::{NativeToken, Token};
/// Token { contract_addr: Addr::unchecked("stake...") };
/// NativeToken { denom: String::from("uluna") };
/// ```
#[cw_serde]
#[derive(Hash, Eq)]
pub enum AssetInfo {
    /// Non-native Token
    Token { contract_addr: Addr },
    /// Native token
    NativeToken { denom: String },
}

impl<'a> PrimaryKey<'a> for &AssetInfo {
    type Prefix = ();

    type SubPrefix = ();

    type Suffix = Self;

    type SuperSuffix = Self;

    fn key(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl<'a> Prefixer<'a> for &AssetInfo {
    fn prefix(&self) -> Vec<Key> {
        vec![Key::Ref(self.as_bytes())]
    }
}

impl KeyDeserialize for &AssetInfo {
    type Output = AssetInfo;

    #[inline(always)]
    fn from_vec(value: Vec<u8>) -> StdResult<Self::Output> {
        from_slice(&value)
    }
}

impl fmt::Display for AssetInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AssetInfo::NativeToken { denom } => write!(f, "{denom}"),
            AssetInfo::Token { contract_addr } => write!(f, "{contract_addr}"),
        }
    }
}

impl AssetInfo {
    /// Returns true if the caller is a native token. Otherwise returns false.
    pub fn is_native_token(&self) -> bool {
        match self {
            AssetInfo::NativeToken { .. } => true,
            AssetInfo::Token { .. } => false,
        }
    }

    /// Checks whether the native coin is IBCed token or not.
    pub fn is_ibc(&self) -> bool {
        match self {
            AssetInfo::NativeToken { denom } => denom.to_lowercase().starts_with("ibc/"),
            AssetInfo::Token { .. } => false,
        }
    }

    /// Returns the balance of token in a pool.
    ///
    /// * **pool_addr** is the address of the contract whose token balance we check.
    pub fn query_pool<C>(
        &self,
        querier: &QuerierWrapper<C>,
        pool_addr: impl Into<String>,
    ) -> StdResult<Uint128>
    where
        C: CustomQuery,
    {
        match self {
            AssetInfo::Token { contract_addr, .. } => {
                query_token_balance(querier, contract_addr, pool_addr)
            }
            AssetInfo::NativeToken { denom } => query_balance(querier, pool_addr, denom),
        }
    }

    /// Returns the number of decimals that a token has.
    pub fn decimals<C>(&self, querier: &QuerierWrapper<C>, factory_addr: &Addr) -> StdResult<u8>
    where
        C: CustomQuery,
    {
        query_token_precision(querier, self, factory_addr)
    }

    /// Returns **true** if the calling token is the same as the token specified in the input parameters.
    /// Otherwise returns **false**.
    pub fn equal(&self, asset: &AssetInfo) -> bool {
        match (self, asset) {
            (AssetInfo::NativeToken { denom }, AssetInfo::NativeToken { denom: other_denom }) => {
                denom == other_denom
            }
            (
                AssetInfo::Token { contract_addr },
                AssetInfo::Token {
                    contract_addr: other_contract_addr,
                },
            ) => contract_addr == other_contract_addr,
            _ => false,
        }
    }

    /// If the caller object is a native token of type [`AssetInfo`] then his `denom` field converts to a byte string.
    ///
    /// If the caller object is a token of type [`AssetInfo`] then its `contract_addr` field converts to a byte string.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            AssetInfo::NativeToken { denom } => denom.as_bytes(),
            AssetInfo::Token { contract_addr } => contract_addr.as_bytes(),
        }
    }

    /// Checks that the tokens' denom or contract addr is valid.
    pub fn check(&self, api: &dyn Api) -> StdResult<()> {
        match self {
            AssetInfo::Token { contract_addr } => {
                api.addr_validate(contract_addr.as_str())?;
            }
            AssetInfo::NativeToken { denom } => {
                if !is_valid_symbol(denom, Some(DENOM_MAX_LENGTH)) {
                    return Err(StdError::generic_err(format!(
                        "Native denom is not in expected format [a-zA-Z\\-][3,{DENOM_MAX_LENGTH}]: {denom}",
                    )));
                }
            }
        }

        Ok(())
    }
}

/// This structure stores the main parameters for an Astroport pair
#[cw_serde]
pub struct PairInfo {
    /// Asset information for the assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// Pair contract address
    pub contract_addr: Addr,
    /// Pair LP token address
    pub liquidity_token: Addr,
    /// The pool type (xyk, stableswap etc) available in [`PairType`]
    pub pair_type: PairType,
}

impl PairInfo {
    /// Returns the balance for each asset in the pool.
    ///
    /// * **contract_addr** is pair's pool address.
    pub fn query_pools<C>(
        &self,
        querier: &QuerierWrapper<C>,
        contract_addr: impl Into<String>,
    ) -> StdResult<Vec<Asset>>
    where
        C: CustomQuery,
    {
        let contract_addr = contract_addr.into();
        self.asset_infos
            .iter()
            .map(|asset_info| {
                Ok(Asset {
                    info: asset_info.clone(),
                    amount: asset_info.query_pool(querier, &contract_addr)?,
                })
            })
            .collect()
    }

    /// Returns the balance for each asset in the pool in decimal.
    ///
    /// * **contract_addr** is pair's pool address.
    pub fn query_pools_decimal(
        &self,
        querier: &QuerierWrapper,
        contract_addr: impl Into<String>,
        factory_addr: &Addr,
    ) -> StdResult<Vec<DecimalAsset>> {
        let contract_addr = contract_addr.into();
        self.asset_infos
            .iter()
            .map(|asset_info| {
                Ok(DecimalAsset {
                    info: asset_info.clone(),
                    amount: Decimal256::from_atomics(
                        asset_info.query_pool(querier, &contract_addr)?,
                        asset_info.decimals(querier, factory_addr)?.into(),
                    )
                    .map_err(|_| StdError::generic_err("Decimal256RangeExceeded"))?,
                })
            })
            .collect()
    }
}

/// Returns a lowercased, validated address upon success if present.
#[inline]
pub fn addr_opt_validate(api: &dyn Api, addr: &Option<String>) -> StdResult<Option<Addr>> {
    addr.as_ref()
        .map(|addr| api.addr_validate(addr))
        .transpose()
}

const TOKEN_SYMBOL_MAX_LENGTH: usize = 4;

/// Returns a formatted LP token name
pub fn format_lp_token_name<C>(
    asset_infos: &[AssetInfo],
    querier: &QuerierWrapper<C>,
) -> StdResult<String>
where
    C: CustomQuery,
{
    let mut short_symbols: Vec<String> = vec![];
    for asset_info in asset_infos {
        let short_symbol = match &asset_info {
            AssetInfo::NativeToken { denom } => {
                denom.chars().take(TOKEN_SYMBOL_MAX_LENGTH).collect()
            }
            AssetInfo::Token { contract_addr } => {
                let token_symbol = query_token_symbol(querier, contract_addr)?;
                token_symbol.chars().take(TOKEN_SYMBOL_MAX_LENGTH).collect()
            }
        };
        short_symbols.push(short_symbol);
    }
    Ok(format!("{}-LP", short_symbols.iter().join("-")).to_uppercase())
}

/// Returns an [`Asset`] object representing a native token and an amount of tokens.
///
/// * **denom** native asset denomination.
///
/// * **amount** amount of native assets.
pub fn native_asset(denom: String, amount: Uint128) -> Asset {
    Asset {
        info: AssetInfo::NativeToken { denom },
        amount,
    }
}

/// Returns an [`Asset`] object representing a non-native token and an amount of tokens.
/// ## Params
/// * **contract_addr** iaddress of the token contract.
///
/// * **amount** amount of tokens.
pub fn token_asset(contract_addr: Addr, amount: Uint128) -> Asset {
    Asset {
        info: AssetInfo::Token { contract_addr },
        amount,
    }
}

/// Returns an [`AssetInfo`] object representing the denomination for native asset.
pub fn native_asset_info(denom: String) -> AssetInfo {
    AssetInfo::NativeToken { denom }
}

/// Returns an [`AssetInfo`] object representing the address of a token contract.
pub fn token_asset_info(contract_addr: Addr) -> AssetInfo {
    AssetInfo::Token { contract_addr }
}

/// Returns [`PairInfo`] by specified pool address.
///
/// * **pool_addr** address of the pool.
pub fn pair_info_by_pool(querier: &QuerierWrapper, pool: impl Into<String>) -> StdResult<PairInfo> {
    let minter_info: MinterResponse = querier.query_wasm_smart(pool, &Cw20QueryMsg::Minter {})?;

    let pair_info: PairInfo =
        querier.query_wasm_smart(minter_info.minter, &PairQueryMsg::Pair {})?;

    Ok(pair_info)
}

/// Checks swap parameters.
///
/// * **pools** amount of tokens in pools.
///
/// * **swap_amount** amount to swap.
pub fn check_swap_parameters(pools: Vec<Uint128>, swap_amount: Uint128) -> StdResult<()> {
    if pools.iter().any(|pool| pool.is_zero()) {
        return Err(StdError::generic_err("One of the pools is empty"));
    }

    if swap_amount.is_zero() {
        return Err(StdError::generic_err("Swap amount must not be zero"));
    }

    Ok(())
}

/// Trait extension for AssetInfo to produce [`Asset`] objects from [`AssetInfo`].
pub trait AssetInfoExt {
    fn with_balance(&self, balance: impl Into<Uint128>) -> Asset;
    fn with_dec_balance(&self, balance: Decimal256) -> DecimalAsset;
}

impl AssetInfoExt for AssetInfo {
    fn with_balance(&self, balance: impl Into<Uint128>) -> Asset {
        Asset {
            info: self.clone(),
            amount: balance.into(),
        }
    }

    fn with_dec_balance(&self, balance: Decimal256) -> DecimalAsset {
        DecimalAsset {
            info: self.clone(),
            amount: balance,
        }
    }
}

/// Trait extension for Decimal256 to work with token precisions more accurately.
pub trait Decimal256Ext {
    fn to_uint256(&self) -> Uint256;

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128>;

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256>;

    fn from_integer(i: impl Into<Uint256>) -> Self;

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256>;

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256>;
}

impl Decimal256Ext for Decimal256 {
    fn to_uint256(&self) -> Uint256 {
        self.numerator() / self.denominator()
    }

    fn to_uint128_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint128> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())?
            .try_into()
            .map_err(|o: ConversionOverflowError| {
                StdError::generic_err(format!("Error converting {}", o.value))
            })
    }

    fn to_uint256_with_precision(&self, precision: impl Into<u32>) -> StdResult<Uint256> {
        let value = self.atomics();
        let precision = precision.into();

        value
            .checked_div(10u128.pow(self.decimal_places() - precision).into())
            .map_err(|_| StdError::generic_err("DivideByZeroError"))
    }

    fn from_integer(i: impl Into<Uint256>) -> Self {
        Decimal256::from_ratio(i.into(), 1u8)
    }

    fn checked_multiply_ratio(
        &self,
        numerator: Decimal256,
        denominator: Decimal256,
    ) -> StdResult<Decimal256> {
        Ok(Decimal256::new(
            self.atomics()
                .checked_multiply_ratio(numerator.atomics(), denominator.atomics())
                .map_err(|_| StdError::generic_err("CheckedMultiplyRatioError"))?,
        ))
    }

    fn with_precision(
        value: impl Into<Uint256>,
        precision: impl Into<u32>,
    ) -> StdResult<Decimal256> {
        Decimal256::from_atomics(value, precision.into())
            .map_err(|_| StdError::generic_err("Decimal256 range exceeded"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::{coin, coins};

    #[test]
    fn test_native_coins_sent() {
        let asset = native_asset_info("uusd".to_string()).with_balance(1000u16);

        let info = mock_info("addr0000", &coins(1000, "random"));
        let err = asset.assert_sent_native_token_balance(&info).unwrap_err();
        assert_eq!(err, StdError::generic_err("Must send reserve token 'uusd'"));

        let info = mock_info("addr0000", &coins(100, "uusd"));
        let err = asset.assert_sent_native_token_balance(&info).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(
                "Native token balance mismatch between the argument and the transferred"
            )
        );

        let info = mock_info("addr0000", &coins(1000, "uusd"));
        asset.assert_sent_native_token_balance(&info).unwrap();
    }

    #[test]
    fn test_proper_native_coins_sent() {
        let pool_asset_infos = [
            native_asset_info("uusd".to_string()),
            native_asset_info("uluna".to_string()),
        ];

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(100u16),
        ];
        let err = vec![coin(1000, "uusd"), coin(1000, "random")]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(
                "Supplied coins contain random that is not in the input asset vector"
            )
        );

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            native_asset_info("random".to_string()).with_balance(100u16),
        ];
        let err = vec![coin(1000, "uusd"), coin(100, "random")]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("Asset random is not in the pool")
        );

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(1000u16),
        ];
        let err = vec![coin(1000, "uusd"), coin(100, "uluna")]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(
                "Native token balance mismatch between the argument and the transferred"
            )
        );

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(1000u16),
        ];
        vec![coin(1000, "uusd"), coin(1000, "uluna")]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap();

        let pool_asset_infos = [
            token_asset_info(Addr::unchecked("addr0000")),
            token_asset_info(Addr::unchecked("addr0001")),
        ];
        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(1000u16),
        ];
        let err = vec![coin(1000, "uusd"), coin(1000, "uluna")]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(
                "Supplied coins contain uusd that is not in the input asset vector"
            )
        );
    }
}
