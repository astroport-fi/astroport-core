use std::fmt;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    coin, coins, ensure, to_json_binary, wasm_execute, Addr, Api, BankMsg, Coin,
    ConversionOverflowError, CosmosMsg, CustomMsg, CustomQuery, Decimal256, Fraction, MessageInfo,
    QuerierWrapper, ReplyOn, StdError, StdResult, SubMsg, Uint128, Uint256, WasmMsg,
};
use cw20::{Cw20Coin, Cw20CoinVerified, Cw20ExecuteMsg, Cw20QueryMsg, Denom, MinterResponse};
use cw_asset::{Asset as CwAsset, AssetInfo as CwAssetInfo};
use cw_storage_plus::{Key, KeyDeserialize, Prefixer, PrimaryKey};
use cw_utils::must_pay;
use itertools::Itertools;

use crate::cosmwasm_ext::DecimalToInteger;
use crate::factory::PairType;
use crate::pair::QueryMsg as PairQueryMsg;
use crate::querier::{
    query_balance, query_token_balance, query_token_precision, query_token_symbol,
};

/// UST token denomination
pub const UUSD_DENOM: &str = "uusd";
/// LUNA token denomination
pub const ULUNA_DENOM: &str = "uluna";
/// Minimum initial LP share
pub const MINIMUM_LIQUIDITY_AMOUNT: Uint128 = Uint128::new(1_000);
/// Maximum denom length
pub const DENOM_MAX_LENGTH: usize = 128;

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
    pub fn into_asset(self, precision: impl Into<u32>) -> StdResult<Asset> {
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

impl From<Coin> for Asset {
    fn from(coin: Coin) -> Self {
        Asset::native(coin.denom, coin.amount)
    }
}

impl From<&Coin> for Asset {
    fn from(coin: &Coin) -> Self {
        coin.clone().into()
    }
}

impl TryFrom<Asset> for Coin {
    type Error = StdError;

    fn try_from(asset: Asset) -> Result<Self, Self::Error> {
        match asset.info {
            AssetInfo::NativeToken { denom } => Ok(Self {
                denom,
                amount: asset.amount,
            }),
            _ => Err(StdError::parse_err(
                "Asset",
                "Cannot convert non-native asset to Coin",
            )),
        }
    }
}

impl TryFrom<&Asset> for Coin {
    type Error = StdError;

    fn try_from(asset: &Asset) -> Result<Self, Self::Error> {
        asset.clone().try_into()
    }
}

impl From<Cw20CoinVerified> for Asset {
    fn from(coin: Cw20CoinVerified) -> Self {
        Asset::cw20(coin.address, coin.amount)
    }
}

impl TryFrom<Asset> for Cw20CoinVerified {
    type Error = StdError;

    fn try_from(asset: Asset) -> Result<Self, Self::Error> {
        match asset.info {
            AssetInfo::Token { contract_addr } => Ok(Self {
                address: contract_addr,
                amount: asset.amount,
            }),
            _ => Err(StdError::generic_err(
                "Cannot convert non-CW20 asset to Cw20Coin",
            )),
        }
    }
}

impl TryFrom<Asset> for Cw20Coin {
    type Error = StdError;

    fn try_from(asset: Asset) -> Result<Self, Self::Error> {
        let verified: Cw20CoinVerified = asset.try_into()?;
        Ok(Self {
            address: verified.address.to_string(),
            amount: verified.amount,
        })
    }
}

impl From<Asset> for CwAsset {
    fn from(asset: Asset) -> CwAsset {
        Self::new(Into::<CwAssetInfo>::into(asset.info), asset.amount)
    }
}

impl TryFrom<CwAsset> for Asset {
    type Error = StdError;

    fn try_from(cw_asset: CwAsset) -> StdResult<Self> {
        cw_asset
            .info
            .try_into()
            .map(|cw_asset_info| Self::new(cw_asset_info, cw_asset.amount))
    }
}

impl Asset {
    /// Constructs a new [`Asset`] object.
    pub fn new<A: Into<Uint128>>(info: AssetInfo, amount: A) -> Self {
        Self {
            info,
            amount: amount.into(),
        }
    }

    /// Returns an [`Asset`] object representing a native token with a given amount.
    pub fn native<A: Into<String>, B: Into<Uint128>>(denom: A, amount: B) -> Self {
        native_asset(denom.into(), amount.into())
    }

    /// Returns an [`Asset`] object representing a CW20 token with a given amount.
    pub fn cw20<A: Into<Uint128>>(contract_addr: Addr, amount: A) -> Self {
        token_asset(contract_addr, amount.into())
    }

    /// Returns an [`Asset`] object representing a CW20 token with a given amount, bypassing the
    /// address validation.
    pub fn cw20_unchecked<A: Into<String>, B: Into<Uint128>>(contract_addr: A, amount: B) -> Self {
        token_asset(Addr::unchecked(contract_addr.into()), amount.into())
    }

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
                msg: to_json_binary(&Cw20ExecuteMsg::Transfer {
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

    /// Same as [`Asset::into_msg`] but allows handling errors/msg response data and
    /// enforcing gas limit in contract's reply endpoint.
    /// If `reply_params` is None then the reply is disabled.
    /// Returns a [`SubMsg`] object.
    pub fn into_submsg<T>(
        self,
        recipient: impl Into<String>,
        reply_params: Option<(ReplyOn, u64)>,
        gas_limit: Option<u64>,
    ) -> StdResult<SubMsg<T>>
    where
        T: CustomMsg,
    {
        let recipient = recipient.into();
        let (reply_on, reply_id) = reply_params.unwrap_or((ReplyOn::Never, 0));

        match &self.info {
            AssetInfo::Token { contract_addr } => {
                let inner_msg = wasm_execute(
                    contract_addr,
                    &Cw20ExecuteMsg::Transfer {
                        recipient,
                        amount: self.amount,
                    },
                    vec![],
                )?;

                Ok(SubMsg {
                    id: reply_id,
                    msg: inner_msg.into(),
                    gas_limit,
                    reply_on,
                })
            }
            AssetInfo::NativeToken { denom } => {
                let bank_msg = BankMsg::Send {
                    to_address: recipient,
                    amount: coins(self.amount.u128(), denom),
                }
                .into();

                Ok(SubMsg {
                    id: reply_id,
                    msg: bank_msg,
                    gas_limit,
                    reply_on,
                })
            }
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
        ensure!(
            !input_assets.is_empty(),
            StdError::generic_err("Empty input assets")
        );

        ensure!(
            input_assets.iter().map(|asset| &asset.info).all_unique(),
            StdError::generic_err("Duplicated assets in the input")
        );

        input_assets.iter().try_for_each(|input| {
            if pool_asset_infos.contains(&input.info) {
                match &input.info {
                    AssetInfo::NativeToken { denom } => {
                        let coin = self
                            .iter()
                            .find(|coin| coin.denom == *denom)
                            .cloned()
                            .unwrap_or_else(|| coin(0, denom));
                        if coin.amount != input.amount {
                            Err(StdError::generic_err(
                                format!("Native token balance mismatch between the argument ({}{denom}) and the transferred ({}{denom})", input.amount, coin.amount),
                            ))
                        } else {
                            Ok(())
                        }
                    }
                    AssetInfo::Token { .. } => Ok(())
                }
            } else {
                Err(StdError::generic_err(format!(
                    "Asset {} is not in the pool",
                    input.info
                )))
            }
        })?;

        self.iter().try_for_each(|coin| {
            if pool_asset_infos.contains(&AssetInfo::NativeToken {
                denom: coin.denom.clone(),
            }) {
                Ok(())
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
    fn from_vec(_value: Vec<u8>) -> StdResult<Self::Output> {
        unimplemented!("Due to lack of knowledge of enum variant in binary there is no way to determine correct AssetInfo")
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

impl From<Denom> for AssetInfo {
    fn from(denom: Denom) -> Self {
        match denom {
            Denom::Cw20(contract_addr) => token_asset_info(contract_addr),
            Denom::Native(denom) => native_asset_info(denom),
        }
    }
}

impl From<AssetInfo> for Denom {
    fn from(asset_info: AssetInfo) -> Self {
        match asset_info {
            AssetInfo::Token { contract_addr } => Denom::Cw20(contract_addr),
            AssetInfo::NativeToken { denom } => Denom::Native(denom),
        }
    }
}

impl TryFrom<AssetInfo> for Addr {
    type Error = StdError;

    fn try_from(asset_info: AssetInfo) -> StdResult<Self> {
        match asset_info {
            AssetInfo::Token { contract_addr } => Ok(contract_addr),
            AssetInfo::NativeToken { denom: _ } => Err(StdError::generic_err("Not a CW20 token")),
        }
    }
}

impl From<Addr> for AssetInfo {
    fn from(contract_addr: Addr) -> Self {
        token_asset_info(contract_addr)
    }
}

impl From<AssetInfo> for CwAssetInfo {
    fn from(asset_info: AssetInfo) -> Self {
        match asset_info {
            AssetInfo::Token { contract_addr } => Self::Cw20(contract_addr),
            AssetInfo::NativeToken { denom } => Self::Native(denom),
        }
    }
}

impl TryFrom<CwAssetInfo> for AssetInfo {
    type Error = StdError;

    fn try_from(cw_asset_info: CwAssetInfo) -> StdResult<Self> {
        match cw_asset_info {
            CwAssetInfo::Native(denom) => Ok(Self::native(denom)),
            CwAssetInfo::Cw20(contract_addr) => Ok(Self::cw20(contract_addr)),
            _ => Err(StdError::generic_err("CwAssetInfo variant unknown")),
        }
    }
}

impl AssetInfo {
    /// Returns an [`AssetInfo`] object representing the denomination for native asset.
    pub fn native<A: Into<String>>(denom: A) -> Self {
        native_asset_info(denom.into())
    }

    /// Returns an [`AssetInfo`] object representing the address of a CW20 token contract.
    pub fn cw20(contract_addr: Addr) -> Self {
        token_asset_info(contract_addr)
    }

    /// Returns an [`AssetInfo`] object representing the address of a CW20 token contract, bypassing
    /// the address validation.
    pub fn cw20_unchecked<A: Into<String>>(contract_addr: A) -> Self {
        AssetInfo::Token {
            contract_addr: Addr::unchecked(contract_addr.into()),
        }
    }

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
                validate_native_denom(denom)?;
            }
        }

        Ok(())
    }
}

/// Taken from https://github.com/mars-protocol/red-bank/blob/5bb0fe145588352b281803f7b870103bc6832621/packages/utils/src/helpers.rs#L68
/// Follows cosmos SDK validation logic where denom can be 3 - 128 characters long
/// and starts with a letter, followed but either a letter, number, or separator ( ‘/' , ‘:' , ‘.’ , ‘_’ , or '-')
/// reference: https://github.com/cosmos/cosmos-sdk/blob/7728516abfab950dc7a9120caad4870f1f962df5/types/coin.go#L865-L867
pub fn validate_native_denom(denom: &str) -> StdResult<()> {
    if denom.len() < 3 || denom.len() > DENOM_MAX_LENGTH {
        return Err(StdError::generic_err(format!(
            "Invalid denom length [3,{DENOM_MAX_LENGTH}]: {denom}"
        )));
    }

    let mut chars = denom.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(StdError::generic_err(format!(
            "First character is not ASCII alphabetic: {denom}"
        )));
    }

    let set = ['/', ':', '.', '_', '-'];
    for c in chars {
        if !(c.is_ascii_alphanumeric() || set.contains(&c)) {
            return Err(StdError::generic_err(format!(
                "Not all characters are ASCII alphanumeric or one of:  /  :  .  _  -: {denom}"
            )));
        }
    }

    Ok(())
}

/// This structure stores the main parameters for an Astroport pair
#[cw_serde]
pub struct PairInfo {
    /// Asset information for the assets in the pool
    pub asset_infos: Vec<AssetInfo>,
    /// Pair contract address
    pub contract_addr: Addr,
    /// Pair LP token denom
    pub liquidity_token: String,
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

/// This function tries to determine asset info from the given input.  
///
/// **NOTE**
/// - this function relies on the fact that chain doesn't allow to mint native tokens in the form of bech32 addresses.
/// For example, if it is allowed to mint native token `wasm1xxxxxxx` then [`AssetInfo`] will be determined incorrectly;
/// - if you intend to test this functionality in cw-multi-test you must implement [`Api`] trait for your test App
/// with conjunction with [AddressGenerator](https://docs.rs/cw-multi-test/0.17.0/cw_multi_test/trait.AddressGenerator.html)
pub fn determine_asset_info(maybe_asset_info: &str, api: &dyn Api) -> StdResult<AssetInfo> {
    if api.addr_validate(maybe_asset_info).is_ok() {
        Ok(AssetInfo::Token {
            contract_addr: Addr::unchecked(maybe_asset_info),
        })
    } else if validate_native_denom(maybe_asset_info).is_ok() {
        Ok(AssetInfo::NativeToken {
            denom: maybe_asset_info.to_string(),
        })
    } else {
        Err(StdError::generic_err(format!(
            "Cannot determine asset info from {maybe_asset_info}"
        )))
    }
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
    use cosmwasm_std::testing::mock_info;
    use cosmwasm_std::{coin, coins};
    use test_case::test_case;

    use super::*;

    fn mock_cw20() -> Asset {
        Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("mock_token"),
            },
            amount: Uint128::new(123456u128),
        }
    }

    fn mock_native() -> Asset {
        Asset {
            info: AssetInfo::NativeToken {
                denom: String::from("uusd"),
            },
            amount: Uint128::new(123456u128),
        }
    }

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
            StdError::generic_err("Native token balance mismatch between the argument (100uluna) and the transferred (0uluna)")
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
                "Native token balance mismatch between the argument (1000uluna) and the transferred (100uluna)"
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

    #[test]
    fn test_empty_funds() {
        let pool_asset_infos = [
            native_asset_info("uusd".to_string()),
            native_asset_info("uluna".to_string()),
        ];

        let err = vec![]
            .assert_coins_properly_sent(&[], &pool_asset_infos)
            .unwrap_err();
        assert_eq!(err.to_string(), "Generic error: Empty input assets");

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(100u16),
        ];
        let err = vec![]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Native token balance mismatch between the argument (1000uusd) and the transferred (0uusd)"
        );

        let err = vec![assets[0].as_coin().unwrap()]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Native token balance mismatch between the argument (100uluna) and the transferred (0uluna)"
        );
    }

    #[test]
    fn test_duplicated_funds() {
        let pool_asset_infos = [
            native_asset_info("uusd".to_string()),
            native_asset_info("uusd".to_string()),
        ];

        let assets = [
            pool_asset_infos[0].with_balance(1000u16),
            pool_asset_infos[1].with_balance(100u16),
        ];
        let err = vec![assets[0].as_coin().unwrap(), assets[1].as_coin().unwrap()]
            .assert_coins_properly_sent(&assets, &pool_asset_infos)
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Duplicated assets in the input"
        );
    }

    #[test]
    fn native_denom_validation() {
        let err = validate_native_denom("ab").unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("Invalid denom length [3,128]: ab")
        );
        let err = validate_native_denom("1usd").unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("First character is not ASCII alphabetic: 1usd")
        );
        let err = validate_native_denom("wow@usd").unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err(
                "Not all characters are ASCII alphanumeric or one of:  /  :  .  _  -: wow@usd"
            )
        );
        let long_denom: String = ['a'].repeat(129).iter().collect();
        let err = validate_native_denom(&long_denom).unwrap_err();
        assert_eq!(
            err,
            StdError::generic_err("Invalid denom length [3,128]: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );

        validate_native_denom("uusd").unwrap();
        validate_native_denom(
            "ibc/EBD5A24C554198EBAF44979C5B4D2C2D312E6EBAB71962C92F735499C7575839",
        )
        .unwrap();
        validate_native_denom("factory/wasm1jdppe6fnj2q7hjsepty5crxtrryzhuqsjrj95y/uusd").unwrap();
    }

    #[test]
    fn test_native_asset_info() {
        let info = AssetInfo::native("uusd");
        assert_eq!(
            AssetInfo::NativeToken {
                denom: "uusd".to_string()
            },
            info
        );
    }

    #[test]
    fn cw20_unchecked_asset_info() {
        let info = AssetInfo::cw20_unchecked(Addr::unchecked("mock_token"));
        assert_eq!(
            AssetInfo::Token {
                contract_addr: Addr::unchecked("mock_token")
            },
            info
        );
    }

    #[test]
    fn cw20_asset_info() {
        let info = AssetInfo::cw20(Addr::unchecked("mock_token"));
        assert_eq!(
            AssetInfo::Token {
                contract_addr: Addr::unchecked("mock_token")
            },
            info
        );
    }

    #[test]
    fn from_cw20coinverified_for_asset() {
        let coin = Cw20CoinVerified {
            address: Addr::unchecked("mock_token"),
            amount: Uint128::new(123456u128),
        };
        assert_eq!(mock_cw20(), Asset::from(coin));
    }

    #[test_case(mock_native() => matches Err(_) ; "native")]
    #[test_case(mock_cw20() => Ok(Cw20CoinVerified {
                    address: Addr::unchecked("mock_token"),
                    amount: 123456u128.into()
                }) ; "cw20")]
    fn try_from_asset_for_cw20coinverified(asset: Asset) -> StdResult<Cw20CoinVerified> {
        Cw20CoinVerified::try_from(asset)
    }

    #[test_case(mock_native() => matches Err(_) ; "native")]
    #[test_case(mock_cw20() => Ok(Cw20Coin {
                    address: "mock_token".to_string(),
                    amount: 123456u128.into()
                }) ; "cw20")]
    fn try_from_asset_for_cw20coin(asset: Asset) -> StdResult<Cw20Coin> {
        Cw20Coin::try_from(asset)
    }

    #[test]
    fn test_from_coin_for_asset() {
        let coin = coin(123456u128, "uusd");
        assert_eq!(mock_native(), Asset::from(coin));
    }

    #[test]
    fn test_try_from_asset_for_coin() {
        let coin = coin(123456u128, "uusd");
        let asset = Asset::from(&coin);
        let coin2: Coin = asset.try_into().unwrap();
        assert_eq!(coin, coin2);
    }

    #[test]
    fn test_from_addr_for_asset_info() {
        let addr = Addr::unchecked("mock_token");
        let info = AssetInfo::from(addr.clone());
        assert_eq!(info, AssetInfo::cw20(addr));
    }

    #[test]
    fn test_try_from_asset_info_for_addr() {
        let addr = Addr::unchecked("mock_token");
        let info = AssetInfo::cw20(addr.clone());
        let addr2: Addr = info.try_into().unwrap();
        assert_eq!(addr, addr2);
    }

    #[test]
    fn test_from_denom_for_asset_info() {
        let denom = Denom::Native("uusd".to_string());
        let info = AssetInfo::from(denom.clone());
        assert_eq!(info, AssetInfo::native("uusd"));
    }

    #[test]
    fn test_try_from_asset_info_for_denom() {
        let denom = Denom::Native("uusd".to_string());
        let info = AssetInfo::native("uusd");
        let denom2: Denom = info.try_into().unwrap();
        assert_eq!(denom, denom2);
    }

    #[test]
    fn test_from_asset_info_for_cw_asset_info() {
        let asset_info_native = AssetInfo::native("denom");
        let asset_info_cw20 = AssetInfo::cw20(Addr::unchecked("cw20"));
        assert_eq!(CwAssetInfo::native("denom"), asset_info_native.into());
        assert_eq!(
            CwAssetInfo::cw20(Addr::unchecked("cw20")),
            asset_info_cw20.into()
        )
    }

    #[test]
    fn test_try_from_from_cw_asset_info_for_asset_info() {
        let cw_asset_info_native = CwAssetInfo::native("denom");
        let cw_asset_info_cw20 = CwAssetInfo::cw20(Addr::unchecked("cw20"));
        assert_eq!(
            AssetInfo::native("denom"),
            cw_asset_info_native.try_into().unwrap()
        );
        assert_eq!(
            AssetInfo::cw20(Addr::unchecked("cw20")),
            cw_asset_info_cw20.try_into().unwrap()
        )
    }

    #[test]
    fn test_from_asset_for_cw_asset() {
        let asset_native = Asset::new(AssetInfo::native("denom"), Uint128::one());
        let asset_cw20 = Asset::new(
            AssetInfo::cw20(Addr::unchecked("cw20")),
            Uint128::from(2_u128),
        );
        assert_eq!(
            CwAsset::new(CwAssetInfo::native("denom"), Uint128::one()),
            Into::<CwAsset>::into(asset_native)
        );
        assert_eq!(
            CwAsset::new(
                CwAssetInfo::cw20(Addr::unchecked("cw20")),
                Uint128::from(2_u128)
            ),
            Into::<CwAsset>::into(asset_cw20)
        )
    }

    #[test]
    fn test_try_from_cw_asset_for_asset() {
        let asset_native = CwAsset::new(CwAssetInfo::native("denom"), Uint128::one());
        let asset_cw20 = CwAsset::new(
            CwAssetInfo::cw20(Addr::unchecked("cw20")),
            Uint128::from(2_u128),
        );
        assert_eq!(
            Asset::new(AssetInfo::native("denom"), Uint128::one()),
            asset_native.try_into().unwrap()
        );
        assert_eq!(
            Asset::new(
                AssetInfo::cw20(Addr::unchecked("cw20")),
                Uint128::from(2_u128)
            ),
            asset_cw20.try_into().unwrap()
        )
    }
}
