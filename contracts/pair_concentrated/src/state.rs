use std::fmt::Display;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    Addr, Decimal, Decimal256, DepsMut, Env, Order, StdError, StdResult, Storage, Uint128,
};
use cw_storage_plus::{Item, Map, SnapshotMap};

use astroport::asset::{AssetInfo, PairInfo};
use astroport::common::OwnershipProposal;
use astroport::cosmwasm_ext::{AbsDiff, IntegerToDecimal};
use astroport::pair_concentrated::{PromoteParams, UpdatePoolParams};

use crate::consts::{
    AMP_MAX, AMP_MIN, FEE_GAMMA_MAX, FEE_GAMMA_MIN, FEE_TOL, GAMMA_MAX, GAMMA_MIN, MAX_CHANGE,
    MAX_FEE, MA_HALF_TIME_LIMITS, MIN_AMP_CHANGING_TIME, MIN_FEE, N_POW2, PRICE_SCALE_DELTA_MAX,
    PRICE_SCALE_DELTA_MIN, REPEG_PROFIT_THRESHOLD_MAX, REPEG_PROFIT_THRESHOLD_MIN, TWO,
};
use crate::error::ContractError;
use crate::math::{calc_d, get_xcp, half_float_pow};

/// This structure stores the concentrated pair parameters.
#[cw_serde]
pub struct Config {
    /// The pair information stored in a [`PairInfo`] struct
    pub pair_info: PairInfo,
    /// The factory contract address
    pub factory_addr: Addr,
    /// The last timestamp when the pair contract updated the asset cumulative prices
    pub block_time_last: u64,
    /// The vector contains cumulative prices for each pair of assets in the pool
    pub cumulative_prices: Vec<(AssetInfo, AssetInfo, Uint128)>,
    /// Pool parameters
    pub pool_params: PoolParams,
    /// Pool state
    pub pool_state: PoolState,
    /// Pool's owner
    pub owner: Option<Addr>,
    /// Whether asset balances are tracked over blocks or not.
    pub track_asset_balances: bool,
}

/// This structure stores the pool parameters which may be adjusted via the `update_pool_params`.
#[cw_serde]
#[derive(Default)]
pub struct PoolParams {
    /// The minimum fee, charged when pool is fully balanced
    pub mid_fee: Decimal,
    /// The maximum fee, charged when pool is imbalanced
    pub out_fee: Decimal,
    /// Parameter that defines how gradual the fee changes from fee_mid to fee_out based on
    /// distance from price_scale
    pub fee_gamma: Decimal,
    /// Minimum profit before initiating a new repeg
    pub repeg_profit_threshold: Decimal,
    /// Minimum amount to change price_scale when repegging
    pub min_price_scale_delta: Decimal,
    /// Half-time used for calculating the price oracle
    pub ma_half_time: u64,
}

/// Validates input value against its limits.
fn validate_param<T>(name: &str, val: T, min: T, max: T) -> Result<(), ContractError>
where
    T: PartialOrd + Display,
{
    if val >= min && val <= max {
        Ok(())
    } else {
        Err(ContractError::IncorrectPoolParam(
            name.to_string(),
            min.to_string(),
            max.to_string(),
        ))
    }
}

impl PoolParams {
    /// Intended to update current pool parameters. Performs validation of the new parameters.
    ///
    /// * `update_params` - an object which contains new pool parameters. Any of the parameters may be omitted.
    pub fn update_params(&mut self, update_params: UpdatePoolParams) -> Result<(), ContractError> {
        if let Some(mid_fee) = update_params.mid_fee {
            validate_param("mid_fee", mid_fee, MIN_FEE, MAX_FEE)?;
            self.mid_fee = mid_fee;
        }

        if let Some(out_fee) = update_params.out_fee {
            validate_param("out_fee", out_fee, MIN_FEE, MAX_FEE)?;
            if out_fee <= self.mid_fee {
                return Err(StdError::generic_err(format!(
                    "out_fee {out_fee} must be more {}",
                    self.mid_fee
                ))
                .into());
            }
            self.out_fee = out_fee;
        }

        if let Some(fee_gamma) = update_params.fee_gamma {
            validate_param("fee_gamma", fee_gamma, FEE_GAMMA_MIN, FEE_GAMMA_MAX)?;
            self.fee_gamma = fee_gamma;
        }

        if let Some(repeg_profit_threshold) = update_params.repeg_profit_threshold {
            validate_param(
                "repeg_profit_threshold",
                repeg_profit_threshold,
                REPEG_PROFIT_THRESHOLD_MIN,
                REPEG_PROFIT_THRESHOLD_MAX,
            )?;
            self.repeg_profit_threshold = repeg_profit_threshold;
        }

        if let Some(min_price_scale_delta) = update_params.min_price_scale_delta {
            validate_param(
                "min_price_scale_delta",
                min_price_scale_delta,
                PRICE_SCALE_DELTA_MIN,
                PRICE_SCALE_DELTA_MAX,
            )?;
            self.min_price_scale_delta = min_price_scale_delta;
        }

        if let Some(ma_half_time) = update_params.ma_half_time {
            validate_param(
                "ma_half_time",
                ma_half_time,
                *MA_HALF_TIME_LIMITS.start(),
                *MA_HALF_TIME_LIMITS.end(),
            )?;
            self.ma_half_time = ma_half_time;
        }

        Ok(())
    }

    pub fn fee(&self, xp: &[Decimal256]) -> Decimal256 {
        let fee_gamma: Decimal256 = self.fee_gamma.into();
        let sum = xp[0] + xp[1];
        let mut k = xp[0] * xp[1] * N_POW2 / sum.pow(2);
        k = fee_gamma / (fee_gamma + Decimal256::one() - k);

        if k <= FEE_TOL {
            k = Decimal256::zero()
        }

        k * Decimal256::from(self.mid_fee)
            + (Decimal256::one() - k) * Decimal256::from(self.out_fee)
    }
}

/// Structure which stores Amp and Gamma.
#[cw_serde]
#[derive(Default, Copy)]
pub struct AmpGamma {
    pub amp: Decimal,
    pub gamma: Decimal,
}

impl AmpGamma {
    /// Validates the parameters and creates a new object of the [`AmpGamma`] structure.
    pub fn new(amp: Decimal, gamma: Decimal) -> Result<Self, ContractError> {
        validate_param("amp", amp, AMP_MIN, AMP_MAX)?;
        validate_param("gamma", gamma, GAMMA_MIN, GAMMA_MAX)?;

        Ok(AmpGamma { amp, gamma })
    }
}

/// Internal structure which stores the price state.
/// This structure cannot be updated via update_config.
#[cw_serde]
#[derive(Default)]
pub struct PriceState {
    /// Internal oracle price
    pub oracle_price: Decimal256,
    /// The last saved price
    pub last_price: Decimal256,
    /// Current price scale between 1st and 2nd assets.
    /// I.e. such C that x = C * y where x - 1st asset, y - 2nd asset.
    pub price_scale: Decimal256,
    /// Last timestamp when the price_oracle was updated.
    pub last_price_update: u64,
    /// Keeps track of positive change in xcp due to fees accruing
    pub xcp_profit: Decimal256,
    /// Amount of liquidity if price returns to price_scale.
    /// Used to measure increases in pool value from collected fees.
    pub xcp: Decimal256,
}

/// Internal structure which stores the pool's state.
#[cw_serde]
pub struct PoolState {
    /// Initial Amp and Gamma
    pub initial: AmpGamma,
    /// Future Amp and Gamma
    pub future: AmpGamma,
    /// Timestamp when Amp and Gamma should become equal to self.future
    pub future_time: u64,
    /// Timestamp when Amp and Gamma started being changed
    pub initial_time: u64,
    /// Current price state
    pub price_state: PriceState,
}

impl PoolState {
    /// Validates Amp and Gamma promotion parameters.
    /// Saves current values in self.initial and setups self.future.
    /// If amp and gamma are being changed then current values will be used as initial values.
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

        // Validate amp and gamma values are being changed by <= 10%
        let one = Decimal::one();
        if (next_amp_gamma.amp / cur_amp_gamma.amp).diff(one) > MAX_CHANGE {
            return Err(ContractError::MaxChangeAssertion(
                "Amp".to_string(),
                MAX_CHANGE,
            ));
        }
        if (next_amp_gamma.gamma / cur_amp_gamma.gamma).diff(one) > MAX_CHANGE {
            return Err(ContractError::MaxChangeAssertion(
                "Gamma".to_string(),
                MAX_CHANGE,
            ));
        }

        self.initial = cur_amp_gamma;
        self.initial_time = block_time;

        self.future = next_amp_gamma;
        self.future_time = params.future_time;

        Ok(())
    }

    /// Stops amp and gamma promotion. Saves current values in self.future.
    pub fn stop_promotion(&mut self, env: &Env) {
        self.future = self.get_amp_gamma(env);
        self.future_time = env.block.time.seconds();
    }

    /// Calculates current amp and gamma.
    /// This function handles parameters upgrade as well as downgrade.
    /// If block time >= self.future_time then it returns self.future parameters.
    pub fn get_amp_gamma(&self, env: &Env) -> AmpGamma {
        let block_time = env.block.time.seconds();
        if block_time < self.future_time {
            let total = (self.future_time - self.initial_time).to_decimal();
            let passed = (block_time - self.initial_time).to_decimal();
            let left = total - passed;

            // A1 = A0 + (A1 - A0) * (block_time - t_init) / (t_end - t_init) -> simplified to:
            // A1 = ( A0 * (t_end - block_time) + A1 * (block_time - t_init) ) / (t_end - t_init)
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

    /// The function is responsible for repegging mechanism.
    /// It updates internal oracle price and adjusts price scale.
    ///
    /// * **total_lp** total LP tokens were minted
    /// * **cur_xs** - internal representation of pool volumes
    /// * **cur_price** - last price happened in the previous action (swap, provide or withdraw)
    pub fn update_price(
        &mut self,
        pool_params: &PoolParams,
        env: &Env,
        total_lp: Decimal256,
        cur_xs: &[Decimal256],
        cur_price: Decimal256,
    ) -> StdResult<()> {
        let amp_gamma = self.get_amp_gamma(env);
        let block_time = env.block.time.seconds();
        let price_state = &mut self.price_state;

        if price_state.last_price_update < block_time {
            let arg = Decimal256::from_ratio(
                block_time - price_state.last_price_update,
                pool_params.ma_half_time,
            );
            let alpha = half_float_pow(arg)?;
            price_state.oracle_price = price_state.last_price * (Decimal256::one() - alpha)
                + price_state.oracle_price * alpha;
            price_state.last_price_update = block_time;
        }
        price_state.last_price = cur_price;

        let cur_d = calc_d(cur_xs, &amp_gamma)?;
        let xcp = get_xcp(cur_d, price_state.price_scale);

        let mut virtual_price = Decimal256::one();
        if !price_state.xcp.is_zero() {
            // If xcp dropped and no ramping happens then this swap makes loss
            if xcp < price_state.xcp && block_time >= self.future_time {
                return Err(StdError::generic_err(
                    "XCP value dropped. This action makes loss",
                ));
            }

            price_state.xcp_profit = price_state.xcp_profit * xcp / price_state.xcp;
            virtual_price = xcp / total_lp;
        }

        price_state.xcp = xcp;

        let xcp_profit = price_state.xcp_profit;

        let norm = (price_state.oracle_price / price_state.price_scale).diff(Decimal256::one());
        let scale_delta = Decimal256::from(pool_params.min_price_scale_delta)
            .max(norm * Decimal256::from_ratio(1u8, 10u8));

        if norm >= scale_delta
            && virtual_price - Decimal256::one()
                > (xcp_profit - Decimal256::one()) / TWO
                    + Decimal256::from(pool_params.repeg_profit_threshold)
        {
            let numerator = price_state.price_scale * (norm - scale_delta)
                + scale_delta * price_state.oracle_price;
            let price_scale_new = numerator / norm;

            let xs = [
                cur_xs[0],
                cur_xs[1] * price_scale_new / price_state.price_scale,
            ];
            let new_d = calc_d(&xs, &amp_gamma)?;

            let new_xcp = get_xcp(new_d, price_scale_new);
            let new_virtual_price = new_xcp / total_lp;

            if TWO * new_virtual_price > xcp_profit + Decimal256::one() {
                price_state.price_scale = price_scale_new;
                price_state.xcp = new_xcp;
            };
        }

        Ok(())
    }
}

/// Store all token precisions.
pub(crate) fn store_precisions(
    deps: DepsMut,
    asset_infos: &[AssetInfo],
    factory_addr: &Addr,
) -> StdResult<()> {
    for asset_info in asset_infos {
        let precision = asset_info.decimals(&deps.querier, factory_addr)?;
        PRECISIONS.save(deps.storage, asset_info.to_string(), &precision)?;
    }

    Ok(())
}

pub(crate) struct Precisions(Vec<(String, u8)>);

impl Precisions {
    pub(crate) fn new(storage: &dyn Storage) -> StdResult<Self> {
        let items = PRECISIONS
            .range(storage, None, None, Order::Ascending)
            .collect::<StdResult<Vec<_>>>()?;

        Ok(Self(items))
    }

    pub(crate) fn get_precision(&self, asset_info: &AssetInfo) -> Result<u8, ContractError> {
        self.0
            .iter()
            .find_map(|(info, prec)| {
                if info == &asset_info.to_string() {
                    Some(*prec)
                } else {
                    None
                }
            })
            .ok_or_else(|| ContractError::InvalidAsset(asset_info.to_string()))
    }
}

/// Stores pool parameters and state.
pub const CONFIG: Item<Config> = Item::new("config");

/// Stores map of AssetInfo (as String) -> precision
const PRECISIONS: Map<String, u8> = Map::new("precisions");

/// Stores the latest contract ownership transfer proposal
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

/// Stores asset balances to query them later at any block height
pub const BALANCES: SnapshotMap<&AssetInfo, Uint128> = SnapshotMap::new(
    "balances",
    "balances_check",
    "balances_change",
    cw_storage_plus::Strategy::EveryBlock,
);

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use cosmwasm_std::testing::mock_env;
    use cosmwasm_std::Timestamp;

    use crate::math::calc_y;

    use super::*;

    fn f64_to_dec(val: f64) -> Decimal {
        Decimal::from_str(&val.to_string()).unwrap()
    }
    fn f64_to_dec256(val: f64) -> Decimal256 {
        Decimal256::from_str(&val.to_string()).unwrap()
    }
    fn dec_to_f64(val: Decimal256) -> f64 {
        f64::from_str(&val.to_string()).unwrap()
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn test_validator_odd_behaviour() {
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(86400);

        let mut state = PoolState {
            initial: AmpGamma {
                amp: Decimal::zero(),
                gamma: Decimal::zero(),
            },
            future: AmpGamma {
                amp: f64_to_dec(100_f64),
                gamma: f64_to_dec(0.0000001_f64),
            },
            future_time: 0,
            initial_time: 0,
            price_state: Default::default(),
        };

        // Increase values
        let promote_params = PromoteParams {
            next_amp: f64_to_dec(110_f64),
            next_gamma: f64_to_dec(0.00000011_f64),
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(100_f64));
        assert_eq!(gamma, f64_to_dec(0.0000001_f64));

        // Simulating validator odd behavior
        env.block.time = env.block.time.minus_seconds(1000);
        state.get_amp_gamma(&env);
    }

    #[test]
    fn test_pool_state() {
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(86400);

        let mut state = PoolState {
            initial: AmpGamma {
                amp: Decimal::zero(),
                gamma: Decimal::zero(),
            },
            future: AmpGamma {
                amp: f64_to_dec(100_f64),
                gamma: f64_to_dec(0.0000001_f64),
            },
            future_time: 0,
            initial_time: 0,
            price_state: Default::default(),
        };

        // Trying to promote params with future time in the past
        let promote_params = PromoteParams {
            next_amp: f64_to_dec(110_f64),
            next_gamma: f64_to_dec(0.00000011_f64),
            future_time: env.block.time.seconds() - 10000,
        };
        let err = state.promote_params(&env, promote_params).unwrap_err();
        assert_eq!(err, ContractError::MinChangingTimeAssertion {});

        // Increase values
        let promote_params = PromoteParams {
            next_amp: f64_to_dec(110_f64),
            next_gamma: f64_to_dec(0.00000011_f64),
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(100_f64));
        assert_eq!(gamma, f64_to_dec(0.0000001_f64));

        env.block.time = env.block.time.plus_seconds(50_000);

        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(105_f64));
        assert_eq!(gamma, f64_to_dec(0.000000105_f64));

        env.block.time = env.block.time.plus_seconds(100_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(110_f64));
        assert_eq!(gamma, f64_to_dec(0.00000011_f64));

        // Decrease values
        let promote_params = PromoteParams {
            next_amp: f64_to_dec(108_f64),
            next_gamma: f64_to_dec(0.000000106_f64),
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        env.block.time = env.block.time.plus_seconds(50_000);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(109_f64));
        assert_eq!(gamma, f64_to_dec(0.000000108_f64));

        env.block.time = env.block.time.plus_seconds(50_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(108_f64));
        assert_eq!(gamma, f64_to_dec(0.000000106_f64));

        // Increase amp only
        let promote_params = PromoteParams {
            next_amp: f64_to_dec(118_f64),
            next_gamma: f64_to_dec(0.000000106_f64),
            future_time: env.block.time.seconds() + 100_000,
        };
        state.promote_params(&env, promote_params).unwrap();

        env.block.time = env.block.time.plus_seconds(50_000);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(113_f64));
        assert_eq!(gamma, f64_to_dec(0.000000106_f64));

        env.block.time = env.block.time.plus_seconds(50_001);
        let AmpGamma { amp, gamma } = state.get_amp_gamma(&env);
        assert_eq!(amp, f64_to_dec(118_f64));
        assert_eq!(gamma, f64_to_dec(0.000000106_f64));
    }

    #[test]
    fn check_fee_update() {
        let mid_fee = 0.25f64;
        let out_fee = 0.46f64;
        let fee_gamma = 0.0002f64;

        let params = PoolParams {
            mid_fee: f64_to_dec(mid_fee),
            out_fee: f64_to_dec(out_fee),
            fee_gamma: f64_to_dec(fee_gamma),
            repeg_profit_threshold: Default::default(),
            min_price_scale_delta: Default::default(),
            ma_half_time: 0,
        };

        let xp = vec![f64_to_dec256(1_000_000f64), f64_to_dec256(1_000_000f64)];
        let result = params.fee(&xp);
        assert_eq!(dec_to_f64(result), mid_fee);

        let xp = vec![f64_to_dec256(990_000f64), f64_to_dec256(1_000_000f64)];
        let result = params.fee(&xp);
        assert_eq!(dec_to_f64(result), 0.2735420730476899);

        let xp = vec![f64_to_dec256(100_000f64), f64_to_dec256(1_000_000_f64)];
        let result = params.fee(&xp);
        assert_eq!(dec_to_f64(result), out_fee);
    }

    /// (cur_d, total_lp, new_price)
    fn swap(
        ext_xs: &mut [Decimal256],
        offer_amount: Decimal256,
        price_scale: Decimal256,
        ask_ind: usize,
        amp_gamma: &AmpGamma,
        pool_params: &PoolParams,
    ) -> (Decimal256, Decimal256, Decimal256) {
        let offer_ind = 1 - ask_ind;

        let mut xs = ext_xs.to_vec();
        println!("Before swap: {} {}", xs[0], xs[1]);

        // internal repr
        xs[1] *= price_scale;
        println!("Before swap (internal): {} {}", xs[0], xs[1]);

        let cur_d = calc_d(&xs, amp_gamma).unwrap();

        let total_lp = get_xcp(cur_d, price_scale);

        let mut offer_amount_internal = offer_amount;
        // internal repr
        if offer_ind == 1 {
            offer_amount_internal *= price_scale;
        }

        xs[offer_ind] += offer_amount_internal;
        let mut ask_amount = xs[ask_ind] - calc_y(&xs, cur_d, amp_gamma, ask_ind).unwrap();
        xs[ask_ind] -= ask_amount;
        let fee = ask_amount * pool_params.fee(&xs);
        println!("fee {fee} ({}%)", pool_params.fee(&xs));
        xs[ask_ind] += fee;
        ask_amount -= fee;

        println!(
            "Internal Swap {} x[{}] for {} x[{}] by {} price",
            offer_amount_internal,
            offer_ind,
            ask_amount,
            ask_ind,
            ask_amount / offer_amount_internal
        );

        // external repr
        let new_price = if ask_ind == 1 {
            ask_amount /= price_scale;
            offer_amount / ask_amount
        } else {
            ask_amount / offer_amount
        };

        println!(
            "Swap {} x[{}] for {} x[{}] by {new_price} price",
            offer_amount, offer_ind, ask_amount, ask_ind
        );

        ext_xs[offer_ind] += offer_amount;
        ext_xs[ask_ind] -= ask_amount;

        let ext_d = calc_d(ext_xs, amp_gamma).unwrap();
        let cur_d = calc_d(&xs, amp_gamma).unwrap();

        println!("Internal: d {cur_d}",);
        println!("External: d {ext_d}",);

        println!("After swap: {} {}", ext_xs[0], ext_xs[1]);
        println!(
            "After swap (internal): {} {}",
            ext_xs[0],
            ext_xs[1] * price_scale
        );

        (cur_d, total_lp, new_price)
    }

    fn to_future(env: &mut Env, by_secs: u64) {
        env.block.time = env.block.time.plus_seconds(by_secs)
    }

    fn to_internal_repr(xs: &[Decimal256], price_scale: Decimal256) -> Vec<Decimal256> {
        vec![xs[0], xs[1] * price_scale]
    }

    #[test]
    fn check_repeg() {
        let (amp, gamma) = (40f64, 0.000145);
        let amp_gamma = AmpGamma {
            amp: f64_to_dec(amp),
            gamma: f64_to_dec(gamma),
        };
        let mut env = mock_env();
        env.block.time = Timestamp::from_seconds(0);

        let pool_params = PoolParams {
            mid_fee: f64_to_dec(0.0026),
            out_fee: f64_to_dec(0.0045),
            fee_gamma: f64_to_dec(0.00023),
            repeg_profit_threshold: f64_to_dec(0.000002),
            min_price_scale_delta: f64_to_dec(0.000146),
            ma_half_time: 600,
        };

        let mut pool_state = PoolState {
            initial: AmpGamma::default(),
            future: amp_gamma,
            future_time: 0,
            initial_time: 0,
            price_state: PriceState {
                oracle_price: f64_to_dec256(2f64),
                last_price: f64_to_dec256(2f64),
                price_scale: f64_to_dec256(2f64),
                last_price_update: env.block.time.seconds(),
                xcp_profit: Decimal256::one(),
                xcp: Decimal256::zero(),
            },
        };

        to_future(&mut env, 1);

        // external repr
        let mut ext_xs = [f64_to_dec256(1_000_000f64), f64_to_dec256(500_000f64)];

        let offer_amount = f64_to_dec256(1000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            0,
            &amp_gamma,
            &pool_params,
        );
        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();

        to_future(&mut env, 600);

        let offer_amount = f64_to_dec256(10000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            0,
            &amp_gamma,
            &pool_params,
        );
        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();

        to_future(&mut env, 600);

        let offer_amount = f64_to_dec256(200_000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            0,
            &amp_gamma,
            &pool_params,
        );
        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();

        to_future(&mut env, 12000);

        let offer_amount = f64_to_dec256(1_000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            0,
            &amp_gamma,
            &pool_params,
        );

        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();

        to_future(&mut env, 600);

        let offer_amount = f64_to_dec256(200_000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            1,
            &amp_gamma,
            &pool_params,
        );

        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();

        to_future(&mut env, 60);

        let offer_amount = f64_to_dec256(2_000_f64);
        let (_cur_d, total_lp, price) = swap(
            &mut ext_xs,
            offer_amount,
            pool_state.price_state.price_scale,
            1,
            &amp_gamma,
            &pool_params,
        );

        pool_state
            .update_price(
                &pool_params,
                &env,
                total_lp,
                &to_internal_repr(&ext_xs, pool_state.price_state.price_scale),
                price,
            )
            .unwrap();
    }
}
