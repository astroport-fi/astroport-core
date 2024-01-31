use cosmwasm_std::{Decimal, Decimal256};
use std::ops::RangeInclusive;

/// ## Adjustable constants
/// 0.05
pub const DEFAULT_SLIPPAGE: Decimal256 = Decimal256::raw(50000000000000000);
/// 0.5
pub const MAX_ALLOWED_SLIPPAGE: Decimal256 = Decimal256::raw(500000000000000000);
/// Percentage of 1st pool volume used as offer amount to forecast last price (0.01% or 0.0001).
pub const OFFER_PERCENT: Decimal256 = Decimal256::raw(100000000000000);

/// ## Internal constants
/// Number of coins. (2.0)
pub const N: Decimal256 = Decimal256::raw(2000000000000000000);
/// Defines fee tolerance. If k coefficient is small enough then k = 0. (0.001)
pub const FEE_TOL: Decimal256 = Decimal256::raw(1000000000000000);
/// N ^ 2
pub const N_POW2: Decimal256 = Decimal256::raw(4000000000000000000);
/// 1e-5
pub const TOL: Decimal256 = Decimal256::raw(10000000000000);
/// halfpow tolerance (1e-10)
pub const HALFPOW_TOL: Decimal256 = Decimal256::raw(100000000);
/// 2.0
pub const TWO: Decimal256 = Decimal256::raw(2000000000000000000);
/// Iterations limit for Newton's method
pub const MAX_ITER: usize = 64;

/// ## Validation constants
/// 0.00005 (0.005%)
pub const MIN_FEE: Decimal = Decimal::raw(50000000000000);
/// 0.01 (1%)
pub const MAX_FEE: Decimal = Decimal::raw(10000000000000000);

pub const FEE_GAMMA_MIN: Decimal = Decimal::zero();
pub const FEE_GAMMA_MAX: Decimal = Decimal::one();

pub const REPEG_PROFIT_THRESHOLD_MIN: Decimal = Decimal::zero();
/// 0.01
pub const REPEG_PROFIT_THRESHOLD_MAX: Decimal = Decimal::raw(10000000000000000);

pub const PRICE_SCALE_DELTA_MIN: Decimal = Decimal::zero();
pub const PRICE_SCALE_DELTA_MAX: Decimal = Decimal::one();

pub const MA_HALF_TIME_LIMITS: RangeInclusive<u64> = 1..=(7 * 86400);

/// 0.1
pub const AMP_MIN: Decimal = Decimal::raw(1e17 as u128);
/// 100000
pub const AMP_MAX: Decimal = Decimal::raw(1e23 as u128);

/// 0.00000001
pub const GAMMA_MIN: Decimal = Decimal::raw(10000000000);
/// 0.02
pub const GAMMA_MAX: Decimal = Decimal::raw(20000000000000000);

/// The minimum time interval for updating Amplifier or Gamma
pub const MIN_AMP_CHANGING_TIME: u64 = 86400;
/// The maximum allowed change of Amplifier or Gamma (1000%).
pub const MAX_CHANGE: Decimal = Decimal::raw(1e19 as u128);
