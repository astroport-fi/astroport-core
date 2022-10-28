use cosmwasm_schema::cw_serde;

pub use ap_pair::{
    check_swap_parameters, format_lp_token_name, migration_check, ConfigResponse,
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, PairInfo, PairType,
    PoolResponse, QueryMsg, ReverseSimulationResponse, SimulationResponse, DEFAULT_SLIPPAGE,
    MAX_ALLOWED_SLIPPAGE, MINIMUM_LIQUIDITY_AMOUNT, TWAP_PRECISION,
};
use cosmwasm_std::Decimal;

/// This structure describes a migration message for Stable pair type.
/// We currently take no arguments for migrations.
#[cw_serde]
pub struct MigrateMsg {}

/// This structure holds stableswap pool parameters.
#[cw_serde]
pub struct StablePoolParams {
    /// The current stableswap pool amplification
    pub amp: u64,
    /// The contract owner
    pub owner: Option<String>,
}

/// This structure stores a stableswap pool's configuration.
#[cw_serde]
pub struct StablePoolConfig {
    /// The stableswap pool amplification
    pub amp: Decimal,
}

/// This enum stores the options available to start and stop changing a stableswap pool's amplification.
#[cw_serde]
pub enum StablePoolUpdateParams {
    StartChangingAmp { next_amp: u64, next_amp_time: u64 },
    StopChangingAmp {},
}

#[cfg(test)]
mod tests {
    use ap_pair::ConfigResponse;
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::{from_binary, to_binary, Binary, Decimal};

    use crate::StablePoolConfig;

    #[cw_serde]
    pub struct LegacyConfigResponse {
        pub block_time_last: u64,
        pub params: Option<Binary>,
    }

    #[test]
    fn test_config_response_compatability() {
        let ser_msg = to_binary(&LegacyConfigResponse {
            block_time_last: 12,
            params: Some(
                to_binary(&StablePoolConfig {
                    amp: Decimal::one(),
                })
                .unwrap(),
            ),
        })
        .unwrap();

        let _: ConfigResponse = from_binary(&ser_msg).unwrap();
    }
}
