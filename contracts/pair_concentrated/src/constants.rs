use std::ops::RangeInclusive;

use cosmwasm_std::Uint256;

/// ## Internal constants
pub const MULTIPLIER_U128: u128 = 1e18 as u128;
pub const MULTIPLIER: Uint256 = Uint256::from_u128(MULTIPLIER_U128);
pub const N_COINS: Uint256 = Uint256::from_u128(2u128);
pub const A_MULTIPLIER: Uint256 = Uint256::from_u128(10_000u128);
pub const A_MULTIPLIER_U128: u128 = 10_000u128;
pub const UINT256_E14: Uint256 = Uint256::from_u128(1e14 as u128);
pub const FEE_DENOMINATOR: Uint256 = Uint256::from_u128(1e10 as u128);

/// ## Adjustable constants
/// The precision to convert to
pub const PRECISION: Uint256 = MULTIPLIER;
/// The maximum number of calculation steps for numerical methods.
pub const ITERATIONS: u8 = 32;
/// Calculation precision for halfpow function
pub const EXP_PRECISION: Uint256 = Uint256::from_u128(1e10 as u128);
/// The minimum time interval for updating Amplifier or Gamma
pub const MIN_AMP_CHANGING_TIME: u64 = 86400;
/// The maximum allowed change of Amplifier or Gamma (in form of bps).
pub const MAX_CHANGE: u128 = 1000u128; // 10 %
/// Amplifier limits
// N_COINS**N_COINS * A_MULTIPLIER / 10 .. N_COINS**N_COINS * A_MULTIPLIER * 100000
pub const AMP_LIMITS: RangeInclusive<u128> =
    (4 * A_MULTIPLIER_U128 / 10)..=(4 * A_MULTIPLIER_U128 * 100000);
/// Gamma limits (0.0000001 .. 0.02) considering 10**18 as denominator.
pub const GAMMA_LIMITS: RangeInclusive<u128> = (1e10 as u128)..=(2 * 10e16 as u128);
/// Noise fee added on provide
pub const NOISE_FEE: Uint256 = Uint256::from_u128(1e5 as u128);
