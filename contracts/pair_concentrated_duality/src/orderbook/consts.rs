use std::ops::RangeInclusive;

use cosmwasm_std::Decimal;

/// Validation limits for the number of orders.
/// Be very cautious when changing these values and always check current max gas per block on Neutron.
/// Keep in mind that
/// - 15 orders incur ~3.27M gas on swap, ~3.21M on provide and ~3.26M on withdraw,
/// - 30 orders incur ~6.15M gas on swap, ~6.08M on provide and ~6.14M on withdraw.
pub const ORDERS_NUMBER_LIMITS: RangeInclusive<u8> = 1..=15;

/// Min liquidity percent for orders to be placed in the orderbook. (1%)
pub const MIN_LIQUIDITY_PERCENT: Decimal = Decimal::raw(1e16 as u128);
/// Max liquidity percent for orders to be placed in the orderbook. (50%)
pub const MAX_LIQUIDITY_PERCENT: Decimal = Decimal::raw(5e17 as u128);
