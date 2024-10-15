use std::ops::RangeInclusive;

use cosmwasm_std::Decimal;

/// Validation limits for order size.
pub const ORDER_SIZE_LIMITS: RangeInclusive<u8> = 1..=30;

/// Min liquidity percent for orders to be placed in the orderbook. (0.01%)
pub const MIN_LIQUIDITY_PERCENT: Decimal = Decimal::raw(1e16 as u128);
/// Max liquidity percent for orders to be placed in the orderbook. (50%)
pub const MAX_LIQUIDITY_PERCENT: Decimal = Decimal::raw(5e17 as u128);
