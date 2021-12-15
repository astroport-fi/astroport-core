use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::factory::PairType;
use crate::querier::{query_balance, query_token_balance, query_token_symbol};
use cosmwasm_std::{
    to_binary, Addr, Api, BankMsg, Coin, CosmosMsg, Decimal, MessageInfo, QuerierWrapper, StdError,
    StdResult, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use terra_cosmwasm::TerraQuerier;

/// ## Description
/// This enum describes asset.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Asset {
    /// the available type of asset from [`AssetInfo`]
    pub info: AssetInfo,
    /// the amount of an asset
    pub amount: Uint128,
}

impl fmt::Display for Asset {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", self.amount, self.info)
    }
}

/// the decimal fraction
static DECIMAL_FRACTION: Uint128 = Uint128::new(1_000_000_000_000_000_000u128);

impl Asset {
    /// ## Description
    /// Returns true if token is native token. Otherwise returns false.
    /// ## Params
    /// * **self** is the type of the caller object.
    pub fn is_native_token(&self) -> bool {
        self.info.is_native_token()
    }

    /// ## Description
    /// Calculates and returns computed tax for native token. For other tokens returns zero.
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **querier** is the object of type [`QuerierWrapper`]
    pub fn compute_tax(&self, querier: &QuerierWrapper) -> StdResult<Uint128> {
        let amount = self.amount;
        if let AssetInfo::NativeToken { denom } = &self.info {
            let terra_querier = TerraQuerier::new(querier);
            let tax_rate: Decimal = (terra_querier.query_tax_rate()?).rate;
            let tax_cap: Uint128 = (terra_querier.query_tax_cap(denom.to_string())?).cap;
            Ok(std::cmp::min(
                (amount.checked_sub(amount.multiply_ratio(
                    DECIMAL_FRACTION,
                    DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
                )))?,
                tax_cap,
            ))
        } else {
            Ok(Uint128::zero())
        }
    }

    /// ## Description
    /// Calculates and returns deducted tax for native token. For other tokens returns an [`Err`].
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **querier** is the object of type [`QuerierWrapper`]
    pub fn deduct_tax(&self, querier: &QuerierWrapper) -> StdResult<Coin> {
        let amount = self.amount;
        if let AssetInfo::NativeToken { denom } = &self.info {
            Ok(Coin {
                denom: denom.to_string(),
                amount: amount.checked_sub(self.compute_tax(querier)?)?,
            })
        } else {
            Err(StdError::generic_err("cannot deduct tax from token asset"))
        }
    }

    /// ## Description
    /// Returns a message of type [`CosmosMsg`].
    ///
    /// For native tokens of type [`AssetInfo`] used default method [`BankMsg::Send`] to send amount to recipient,
    /// before sent we need to deduct tax.
    ///
    /// For token of type [`AssetInfo`] used default method [`Cw20ExecuteMsg::Transfer`] and no need to deduct any tax.
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **querier** is the object of type [`QuerierWrapper`]
    ///
    /// * **recepient** is the address where the funds will be sent.
    pub fn into_msg(self, querier: &QuerierWrapper, recipient: Addr) -> StdResult<CosmosMsg> {
        let amount = self.amount;

        match &self.info {
            AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::Transfer {
                    recipient: recipient.to_string(),
                    amount,
                })?,
                funds: vec![],
            })),
            AssetInfo::NativeToken { .. } => Ok(CosmosMsg::Bank(BankMsg::Send {
                to_address: recipient.to_string(),
                amount: vec![self.deduct_tax(querier)?],
            })),
        }
    }

    /// ## Description
    /// Approves the amount of native tokens. Returns [`Ok`] if successful, otherwise returns [`Err`].
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **message_info** is the object of type [`MessageInfo`]
    pub fn assert_sent_native_token_balance(&self, message_info: &MessageInfo) -> StdResult<()> {
        if let AssetInfo::NativeToken { denom } = &self.info {
            match message_info.funds.iter().find(|x| x.denom == *denom) {
                Some(coin) => {
                    if self.amount == coin.amount {
                        Ok(())
                    } else {
                        Err(StdError::generic_err("Native token balance mismatch between the argument and the transferred"))
                    }
                }
                None => {
                    if self.amount.is_zero() {
                        Ok(())
                    } else {
                        Err(StdError::generic_err("Native token balance mismatch between the argument and the transferred"))
                    }
                }
            }
        } else {
            Ok(())
        }
    }
}

/// ## Description
/// This enum describes available types of Token.
/// ## Examples
/// ```
/// # use cosmwasm_std::Addr;
/// # use astroport::asset::AssetInfo::{NativeToken, Token};
/// Token { contract_addr: Addr::unchecked("terra...") };
/// NativeToken { denom: String::from("uluna") };
/// ```
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetInfo {
    /// Token
    Token { contract_addr: Addr },
    /// Native token
    NativeToken { denom: String },
}

impl fmt::Display for AssetInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AssetInfo::NativeToken { denom } => write!(f, "{}", denom),
            AssetInfo::Token { contract_addr } => write!(f, "{}", contract_addr),
        }
    }
}

impl AssetInfo {
    /// ## Description
    /// Returns true if the caller is a native token. Otherwise returns false.
    /// ## Params
    /// * **self** is the type of the caller object
    pub fn is_native_token(&self) -> bool {
        match self {
            AssetInfo::NativeToken { .. } => true,
            AssetInfo::Token { .. } => false,
        }
    }

    /// ## Description
    /// Returns balance of token in a pool.
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **pool_addr** is the address of the contract from which the balance is requested.
    pub fn query_pool(&self, querier: &QuerierWrapper, pool_addr: Addr) -> StdResult<Uint128> {
        match self {
            AssetInfo::Token { contract_addr, .. } => {
                query_token_balance(querier, contract_addr.clone(), pool_addr)
            }
            AssetInfo::NativeToken { denom, .. } => {
                query_balance(querier, pool_addr, denom.to_string())
            }
        }
    }

    /// ## Description
    /// Returns True if the calling token is equal to the token specified in the input parameters.
    /// Otherwise returns False.
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **asset** is object of type [`AssetInfo`].
    pub fn equal(&self, asset: &AssetInfo) -> bool {
        match self {
            AssetInfo::Token { contract_addr, .. } => {
                let self_contract_addr = contract_addr;
                match asset {
                    AssetInfo::Token { contract_addr, .. } => self_contract_addr == contract_addr,
                    AssetInfo::NativeToken { .. } => false,
                }
            }
            AssetInfo::NativeToken { denom, .. } => {
                let self_denom = denom;
                match asset {
                    AssetInfo::Token { .. } => false,
                    AssetInfo::NativeToken { denom, .. } => self_denom == denom,
                }
            }
        }
    }

    /// ## Description
    /// If caller object is a native token of type ['AssetInfo`] then his `denom` field convert to a byte string.
    ///
    /// If caller object is a token of type ['AssetInfo`] then his `contract_addr` field convert to a byte string.
    /// ## Params
    /// * **self** is the type of the caller object.
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            AssetInfo::NativeToken { denom } => denom.as_bytes(),
            AssetInfo::Token { contract_addr } => contract_addr.as_bytes(),
        }
    }

    /// ## Description
    /// Returns [`Ok`] if the token of type [`AssetInfo`] is in lowercase and valid. Otherwise returns [`Err`].
    /// ## Params
    /// * **self** is the type of the caller object.
    ///
    /// * **api** is a object of type [`Api`]
    pub fn check(&self, api: &dyn Api) -> StdResult<()> {
        match self {
            AssetInfo::Token { contract_addr } => {
                addr_validate_to_lower(api, contract_addr.as_str())?;
            }
            AssetInfo::NativeToken { denom } => {
                if denom != &denom.to_lowercase() {
                    return Err(StdError::generic_err(format!(
                        "Native token denom {} should be lowercase",
                        denom
                    )));
                }
            }
        }
        Ok(())
    }
}

/// ## Description
/// This structure describes the main controls configs of pair
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PairInfo {
    /// the type of asset infos available in [`AssetInfo`]
    pub asset_infos: [AssetInfo; 2],
    /// pair contract address
    pub contract_addr: Addr,
    /// pair liquidity token
    pub liquidity_token: Addr,
    /// the type of pair available in [`PairType`]
    pub pair_type: PairType,
}

impl PairInfo {
    /// ## Description
    /// Returns balance for each asset in the pool.
    /// ## Params
    /// * **self** is the type of the caller object
    ///
    /// * **querier** is the object of type [`QuerierWrapper`]
    ///
    /// * **contract_addr** is the pool address of the pair.
    pub fn query_pools(
        &self,
        querier: &QuerierWrapper,
        contract_addr: Addr,
    ) -> StdResult<[Asset; 2]> {
        Ok([
            Asset {
                amount: self.asset_infos[0].query_pool(querier, contract_addr.clone())?,
                info: self.asset_infos[0].clone(),
            },
            Asset {
                amount: self.asset_infos[1].query_pool(querier, contract_addr)?,
                info: self.asset_infos[1].clone(),
            },
        ])
    }
}

/// ## Description
/// Returns the validated address in lowercase on success. Otherwise returns [`Err`]
/// ## Params
/// * **api** is a object of type [`Api`]
///
/// * **addr** is the object of type [`Addr`]
pub fn addr_validate_to_lower(api: &dyn Api, addr: &str) -> StdResult<Addr> {
    if addr.to_lowercase() != addr {
        return Err(StdError::generic_err(format!(
            "Address {} should be lowercase",
            addr
        )));
    }
    api.addr_validate(addr)
}

const TOKEN_SYMBOL_MAX_LENGTH: usize = 4;

/// ## Description
/// Returns formatted liquidity token name
/// ## Params
/// * **asset_infos** is array with two items the type of [`AssetInfo`].
///
/// * **querier** is the object of type [`QuerierWrapper`].
pub fn format_lp_token_name(
    asset_infos: [AssetInfo; 2],
    querier: &QuerierWrapper,
) -> StdResult<String> {
    let mut short_symbols: Vec<String> = vec![];
    for asset_info in asset_infos {
        let short_symbol: String;
        match asset_info {
            AssetInfo::NativeToken { denom } => {
                short_symbol = denom.chars().take(TOKEN_SYMBOL_MAX_LENGTH).collect();
            }
            AssetInfo::Token { contract_addr } => {
                let token_symbol = query_token_symbol(querier, contract_addr)?;
                short_symbol = token_symbol.chars().take(TOKEN_SYMBOL_MAX_LENGTH).collect();
            }
        }
        short_symbols.push(short_symbol);
    }
    Ok(format!("{}-{}-LP", short_symbols[0], short_symbols[1]).to_uppercase())
}
