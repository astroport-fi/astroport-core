use cosmwasm_std::{Addr, Decimal, DepsMut, Env, StdError, StdResult, Storage, Uint128, Uint256};
use cw_storage_plus::{Item, Map};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::cosmwasm_ext::AbsDiff;
use astroport::pair_concentrated::{PromoteParams, UpdatePoolParams};

use crate::constants::{
    AMP_LIMITS, A_MULTIPLIER, A_MULTIPLIER_U128, GAMMA_LIMITS, MAX_CHANGE, MIN_AMP_CHANGING_TIME,
    MULTIPLIER, MULTIPLIER_U128, N_COINS, PRECISION,
};
use crate::error::ContractError;
use crate::math::{geometric_mean, halfpow, newton_d, newton_y};

/// ## Description
/// This structure stores the main concentrated pair parameters.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The greatest precision of assets in the pool
    pub greatest_precision: u8,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// Pool parameters
    pub pool_params: PoolParams,
    /// Pool state
    pub pool_state: PoolState,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema, Default)]
pub struct PoolParams {
    pub mid_fee: Uint256,
    pub out_fee: Uint256,
    pub fee_gamma: Uint256,
    // Decimal value with MULTIPLIER denominator, e.g. 100_000_000_000 = 0.0000001
    pub allowed_extra_profit: Uint256,
    pub adjustment_step: Uint128,
    pub ma_half_time: u64,
}

impl PoolParams {
    pub fn update_params(&mut self, update_params: UpdatePoolParams) -> StdResult<()> {
        if let Some(mid_fee) = update_params.mid_fee {
            // TODO: validation
            self.mid_fee = Uint256::from(mid_fee);
        }

        if let Some(out_fee) = update_params.out_fee {
            // TODO: validation
            self.out_fee = Uint256::from(out_fee);
        }

        if let Some(fee_gamma) = update_params.fee_gamma {
            // TODO: validation
            self.fee_gamma = Uint256::from(fee_gamma);
        }

        if let Some(allowed_extra_profit) = update_params.allowed_extra_profit {
            // TODO: validation: 0 < allowed_extra_profit < 0.1
            self.allowed_extra_profit = allowed_extra_profit.into();
        }

        if let Some(adjustment_step) = update_params.adjustment_step {
            // TODO: validation
            self.adjustment_step = adjustment_step;
        }

        if let Some(ma_half_time) = update_params.ma_half_time {
            // TODO: validation
            self.ma_half_time = ma_half_time;
        }

        Ok(())
    }

    pub fn fee(&self, xp: &[Uint256]) -> Uint256 {
        let mut f = xp[0] + xp[1];
        f = MULTIPLIER * N_COINS * N_COINS * xp[0] / f * xp[1] / f;
        // f = MULTIPLIER * N_COINS * N_COINS * xp[0] * xp[1] / f / f;
        if !self.fee_gamma.is_zero() {
            f = self.fee_gamma * MULTIPLIER / (self.fee_gamma + MULTIPLIER - f)
        }

        (self.mid_fee * f + self.out_fee * (MULTIPLIER - f)) / MULTIPLIER
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, JsonSchema)]
pub struct AmpGamma {
    pub amp: Uint128,
    pub gamma: Uint128,
}

impl AmpGamma {
    pub fn new(new_amp: u128, gamma: u128) -> Result<Self, ContractError> {
        let amp = new_amp * A_MULTIPLIER_U128;
        if !AMP_LIMITS.contains(&amp) {
            return Err(ContractError::IncorrectPoolParam(
                "amp".to_string(),
                new_amp,
            ));
        }
        if !GAMMA_LIMITS.contains(&gamma) {
            return Err(ContractError::IncorrectPoolParam(
                "gamma".to_string(),
                gamma,
            ));
        }

        Ok(AmpGamma {
            amp: amp.into(),
            gamma: gamma.into(),
        })
    }

    pub fn ann(&self) -> Uint256 {
        (self.amp * Uint128::from(4u8)).into()
    }

    pub fn gamma(&self) -> Uint256 {
        self.gamma.into()
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, JsonSchema, Default)]
pub struct PriceState {
    pub price_oracle: Uint256,
    pub last_prices: Uint256,
    pub price_scale: Uint256,
    pub last_price_update: u64,
    pub xcp_profit: Uint256,
    pub virtual_price: Uint256,
    pub d: Uint256,
    pub not_adjusted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct PoolState {
    pub initial: AmpGamma,
    pub future: AmpGamma,
    pub future_time: u64,
    pub initial_time: u64,
    pub price_state: PriceState,
}

impl PoolState {
    pub fn promote_params(
        &mut self,
        env: &Env,
        params: PromoteParams,
    ) -> Result<(), ContractError> {
        let block_time = env.block.time.seconds();

        // Validate time interval
        if block_time < self.initial_time + MIN_AMP_CHANGING_TIME
            || params.future_time < block_time + MIN_AMP_CHANGING_TIME
        {
            return Err(ContractError::MinChangingTimeAssertion {});
        }

        // Validate amp and gamma
        let next_amp_gamma = AmpGamma::new(params.next_amp, params.next_gamma)?;

        // Calculate current amp and gamma
        let cur_amp_gamma = self.get_amp_gamma(env);

        // Validate amp and gamma values are changed by <= 10%
        let max_change = Decimal::from_ratio(MAX_CHANGE, 10000u16);
        let one = Decimal::one();
        let ratio = Decimal::checked_from_ratio(next_amp_gamma.amp, cur_amp_gamma.amp)?;
        if ratio.diff(one) > max_change {
            return Err(ContractError::MaxChangeAssertion(
                "Amp".to_string(),
                MAX_CHANGE / 1000,
            ));
        }
        let ratio = Decimal::checked_from_ratio(next_amp_gamma.gamma, cur_amp_gamma.gamma)?;
        if ratio.diff(one) > max_change {
            return Err(ContractError::MaxChangeAssertion(
                "Gamma".to_string(),
                MAX_CHANGE / 1000,
            ));
        }

        self.initial = cur_amp_gamma;
        self.initial_time = block_time;

        self.future = next_amp_gamma;
        self.future_time = params.future_time;

        Ok(())
    }

    pub fn stop_promotion(&mut self, env: &Env) {
        self.future_time = env.block.time.seconds();
        self.future = self.get_amp_gamma(env);
    }

    pub fn get_amp_gamma(&self, env: &Env) -> AmpGamma {
        let block_time = env.block.time.seconds();
        if block_time < self.future_time {
            let total = Uint128::from(self.future_time - self.initial_time);
            let passed = Uint128::from(block_time - self.initial_time);
            let left = Uint128::from(total - passed);

            // A1 = A0 + (A1 - A0) * (block_time - t_init) / (t_end - t_init) -> simplified to:
            // A1 = ( A0 * (t_end - block_time) + A1 * (block_time - t_init) ) / (t_end - t_init)
            // This formula handles upgrade as well as downgrade.
            let amp = (self.initial.amp * left + self.future.amp * passed) / total;
            let gamma = (self.initial.gamma * left + self.future.gamma * passed) / total;

            AmpGamma { amp, gamma }
        } else {
            AmpGamma {
                amp: self.future.amp,
                gamma: self.future.gamma,
            }
        }
    }

    pub fn get_last_d(&self, env: &Env, xp: &[Uint256]) -> StdResult<Uint256> {
        let block_time = env.block.time.seconds();
        if block_time >= self.future_time {
            // Amp and gamma are stable
            Ok(self.price_state.d)
        } else {
            // Amp and gamma are being changed
            let amp_gamma = self.get_amp_gamma(env);
            newton_d(amp_gamma.ann(), amp_gamma.gamma(), xp)
        }
    }
}

pub const CONFIG: Item<Config> = Item::new("config");

/// Stores map of AssetInfo (as String) -> precision
const PRECISIONS: Map<String, u8> = Map::new("precisions");

/// ## Description
/// Store all token precisions and return the greatest one.
pub(crate) fn store_precisions(deps: DepsMut, asset_infos: &[AssetInfo]) -> StdResult<u8> {
    let mut max = 0u8;

    for asset_info in asset_infos {
        let precision = asset_info.query_token_precision(&deps.querier)?;
        max = max.max(precision);
        PRECISIONS.save(deps.storage, asset_info.to_string(), &precision)?;
    }

    Ok(max)
}

/// ## Description
/// Loads precision of the given asset info.
pub(crate) fn get_precision(storage: &dyn Storage, asset_info: &AssetInfo) -> StdResult<u8> {
    PRECISIONS.load(storage, asset_info.to_string())
}

#[cfg(test)]
mod test {
    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::{Timestamp, Uint128};
    use sim::model::{ConcentratedPairModel, MUL_E18};

    use super::*;

    #[test]
    fn test_pool_state() {
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(86400);

        let mut state = PoolState {
            initial: AmpGamma {
                amp: Uint128::zero(),
                gamma: Uint128::zero(),
            },
            future: AmpGamma {
                amp: Uint128::from(100 * A_MULTIPLIER_U128),
                gamma: Uint128::from(1e10 as u128),
            },
            future_time: 0,
            initial_time: 0,
            price_state: Default::default(),
        };

        // Increase values
        let promote_params = PromoteParams {
            next_amp: 110,
            next_gamma: 1.1e10 as u128,
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 100 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1e10 as u128);

        env.block.time = env.block.time.plus_seconds(50_000);

        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 105u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.05e10 as u128);

        env.block.time = env.block.time.plus_seconds(100_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 110u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.1e10 as u128);

        // Decrease values
        let promote_params = PromoteParams {
            next_amp: 108u128,
            next_gamma: 1.06e10 as u128,
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        env.block.time = env.block.time.plus_seconds(50_000);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 109u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.08e10 as u128);

        env.block.time = env.block.time.plus_seconds(50_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 108u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.06e10 as u128);

        // Increase amp only
        let promote_params = PromoteParams {
            next_amp: 118u128,
            next_gamma: 1.06e10 as u128,
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        env.block.time = env.block.time.plus_seconds(50_000);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 113u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.06e10 as u128);

        env.block.time = env.block.time.plus_seconds(50_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp.u128(), 118u128 * A_MULTIPLIER_U128);
        assert_eq!(gamma.u128(), 1.06e10 as u128);
    }

    #[test]
    fn check_fee_update() {
        fn check_fee(result: Uint256, model_result: f64) {
            assert_eq!(result, Uint256::from(model_result as u128));
        }

        let mid_fee = 0.26f64;
        let out_fee = 0.45f64;
        let fee_gamma = (0.00023f64 * 1e18) as u128;
        let mut xp = vec![1_000_000 * MUL_E18, 1_000_000 * MUL_E18];

        let get_fee = |xp: Vec<u128>, mid_fee: f64, out_fee: f64, fee_gamma: u128| -> f64 {
            // Initialize python model
            let model = ConcentratedPairModel::new(
                100 * A_MULTIPLIER_U128,
                100000,
                xp,
                2,
                vec![MUL_E18, MUL_E18],
                mid_fee,
                out_fee,
                fee_gamma,
                0f64,
                0,
            )
            .unwrap();
            model.call("fee", ()).unwrap()
        };

        let params = PoolParams {
            mid_fee: Uint256::from((mid_fee * 1e10) as u128),
            out_fee: Uint256::from((out_fee * 1e10) as u128),
            fee_gamma: Uint256::from(fee_gamma),
            allowed_extra_profit: Default::default(),
            adjustment_step: Default::default(),
            ma_half_time: 0,
        };
        let mut xp_u256: Vec<Uint256> = xp
            .iter()
            .map(|amount| Uint256::from_u128(*amount))
            .collect();

        let result = params.fee(&xp_u256);
        let model_fee = get_fee(xp.clone(), mid_fee, out_fee, fee_gamma);
        check_fee(result, model_fee);

        xp[0] = 500_000 * MUL_E18;
        xp[1] = 1_500_000 * MUL_E18;
        xp_u256[0] = Uint256::from_u128(xp[0]);
        xp_u256[1] = Uint256::from_u128(xp[1]);
        let result = params.fee(&xp_u256);
        let model_fee = get_fee(xp.clone(), mid_fee, out_fee, fee_gamma);
        check_fee(result, model_fee);

        xp[0] = 10_000 * MUL_E18;
        xp[1] = 1_980_000 * MUL_E18;
        xp_u256[0] = Uint256::from_u128(xp[0]);
        xp_u256[1] = Uint256::from_u128(xp[1]);
        let result = params.fee(&xp_u256);
        let model_fee = get_fee(xp.clone(), mid_fee, out_fee, fee_gamma);
        check_fee(result, model_fee);

        // No fee check
        let mid_fee = 0f64;
        let out_fee = 0f64;
        let fee_gamma = 0u128;
        let params = PoolParams {
            mid_fee: Uint256::from((mid_fee * 1e10) as u128),
            out_fee: Uint256::from((out_fee * 1e10) as u128),
            fee_gamma: Uint256::from(fee_gamma),
            allowed_extra_profit: Default::default(),
            adjustment_step: Default::default(),
            ma_half_time: 0,
        };
        xp[0] = 1_000_000 * MUL_E18;
        xp[1] = 1_000_000 * MUL_E18;
        xp_u256[0] = Uint256::from_u128(xp[0]);
        xp_u256[1] = Uint256::from_u128(xp[1]);
        let result = params.fee(&xp_u256);
        let model_fee = get_fee(xp.clone(), mid_fee, out_fee, fee_gamma);
        check_fee(result, model_fee);
    }
}
