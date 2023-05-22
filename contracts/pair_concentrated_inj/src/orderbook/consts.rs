use crate::consts::OBSERVATIONS_SIZE;
use std::ops::RangeInclusive;

/// Validation limits for order size.
pub const ORDER_SIZE_LIMITS: RangeInclusive<u8> = 1..=30;

/// Validation limits for minimal number of trades to average price. See [`crate::utils::accumulate_swap_sizes`]
/// why we need such exotic limits.
pub const MIN_TRADES_TO_AVG_LIMITS: RangeInclusive<u32> = 1..=(OBSERVATIONS_SIZE - 1);

/// Starting from v1.10 injective uses default subaccount (nonce = 0) to automatically transfer
/// funds from bank module when creating an order. We need to avoid it.
pub const SUBACC_NONCE: u32 = 1;
