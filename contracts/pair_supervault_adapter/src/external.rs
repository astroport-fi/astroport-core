use cosmwasm_schema::cw_serde;
use cosmwasm_schema::serde::Deserialize;
use cosmwasm_std::{
    from_json, to_json_vec, Addr, Coin, Decimal, Decimal256, Empty, Env, QuerierWrapper,
    QueryRequest, StdError, StdResult, Uint128, Uint256,
};
use neutron_std::types::cosmos::base::v1beta1::Coin as ProtoCoin;
use neutron_std::types::neutron::dex::{
    MsgMultiHopSwap, MsgMultiHopSwapResponse, MultiHopRoute, QuerySimulateMultiHopSwapRequest,
    QuerySimulateMultiHopSwapResponse,
};

/// Max precision in CosmWasm limited to max precision of Decimal/Decimal256 types which is 18.
/// Precision in Duality is 27
/// (https://github.com/neutron-org/neutron/blob/8ee37dd582bdf640e4d3cfa0eb6fa59ffdd27e84/utils/math/prec_dec.go#L26).
/// Hence, while converting price from contract to Duality representation,
/// we must multiply it by 1e9.
/// In case Duality changes precision, this value should be updated accordingly.
pub const DUALITY_PRICE_ADJUSTMENT: Uint256 = Uint256::from_u128(1e9 as u128);

#[cw_serde]
/// SuperVault execute messages used in our adapter contract.
pub enum SVExecuteMsg {
    /// deposit funds to use for market making
    Deposit {},
    /// withdraw free unutilised funds
    Withdraw { amount: Uint128 },
}

#[cw_serde]
/// SuperVault query messages used in our adapter contract.
pub enum SVQueryMsg {
    /// response: SvConfig
    GetConfig {},
    /// response: Uint128
    SimulateProvideLiquidity {
        amount_0: Uint128,
        amount_1: Uint128,
        sender: Addr,
    },
    /// response: WithdrawLiquidityResponse
    SimulateWithdrawLiquidity { amount: Uint128 },
    /// response: Vec<Coin>
    GetBalance {},
}

#[derive(Deserialize)]
pub struct TokenData {
    pub denom: String,
    pub decimals: u8,
}

#[derive(Deserialize)]
pub struct PairData {
    pub token_0: TokenData,
    pub token_1: TokenData,
}

#[derive(Deserialize)]
/// SuperVault config.
/// It contains way more fields, but we ignore them for the sake of simplicity.
pub struct SvConfig {
    /// the denom of the contract's LP token
    pub lp_denom: String,
    /// token and denom information
    pub pair_data: PairData,
}

#[cw_serde]
pub struct WithdrawLiquidityResponse {
    pub withdraw_amount_0: Uint128,
    pub withdraw_amount_1: Uint128,
}

pub struct SvQuerier(pub Addr);

impl SvQuerier {
    pub fn new(addr: Addr) -> Self {
        SvQuerier(addr)
    }

    pub fn query_config(&self, querier: QuerierWrapper) -> StdResult<SvConfig> {
        querier.query_wasm_smart(&self.0, &SVQueryMsg::GetConfig {})
    }

    pub fn simulate_provide_liquidity(
        &self,
        querier: QuerierWrapper,
        amount_0: Uint128,
        amount_1: Uint128,
        sender: Addr,
    ) -> StdResult<Uint128> {
        let msg = SVQueryMsg::SimulateProvideLiquidity {
            amount_0,
            amount_1,
            sender,
        };
        querier.query_wasm_smart(&self.0, &msg)
    }

    pub fn simulate_withdraw_liquidity(
        &self,
        querier: QuerierWrapper,
        amount: Uint128,
    ) -> StdResult<WithdrawLiquidityResponse> {
        let msg = SVQueryMsg::SimulateWithdrawLiquidity { amount };
        querier.query_wasm_smart(&self.0, &msg)
    }

    pub fn query_balance(&self, querier: QuerierWrapper) -> StdResult<Vec<Coin>> {
        querier.query_wasm_smart(&self.0, &SVQueryMsg::GetBalance {})
    }
}

pub fn duality_swap(
    env: &Env,
    receiver: &Addr,
    coin_in: &Coin,
    denom_out: &str,
    belief_price: Option<Decimal>,
) -> StdResult<MsgMultiHopSwap> {
    let exit_limit_price = if let Some(price) = belief_price {
        // Adjusting the price to duality notation (float * 1e27)
        Decimal256::from(price)
            .atomics()
            .checked_mul(DUALITY_PRICE_ADJUSTMENT)?
            .to_string()
    } else {
        "1".to_string()
    };

    Ok(MsgMultiHopSwap {
        creator: env.contract.address.to_string(),
        receiver: receiver.to_string(),
        routes: vec![MultiHopRoute {
            hops: vec![coin_in.denom.clone(), denom_out.to_string()],
        }],
        amount_in: coin_in.amount.to_string(),
        exit_limit_price,
        pick_best_route: true,
    })
}

pub fn simulate_duality_swap(querier: QuerierWrapper, msg: MsgMultiHopSwap) -> StdResult<Uint128> {
    let query_msg = to_json_vec(&QueryRequest::<Empty>::Stargate {
        path: "/neutron.dex.Query/SimulateMultiHopSwap".to_string(),
        data: QuerySimulateMultiHopSwapRequest { msg: Some(msg) }.into(),
    })?;

    let response_raw = querier
        .raw_query(&query_msg)
        .into_result()
        .map_err(|err| StdError::generic_err(err.to_string()))?
        .into_result()
        .map_err(StdError::generic_err)?;

    match from_json::<QuerySimulateMultiHopSwapResponse>(&response_raw)? {
        QuerySimulateMultiHopSwapResponse {
            resp:
                Some(MsgMultiHopSwapResponse {
                    coin_out: Some(ProtoCoin { amount, .. }),
                    ..
                }),
        } => amount.parse(),
        _ => Err(StdError::generic_err(
            "Invalid response from duality swap simulation",
        )),
    }
}
