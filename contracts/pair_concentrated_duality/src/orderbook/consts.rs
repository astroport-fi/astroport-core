use std::ops::RangeInclusive;

use cosmwasm_std::{Decimal, Uint256};

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
/// Min average price adjustment percent (1bps)
pub const MIN_AVG_PRICE_ADJ_PERCENT: Decimal = Decimal::raw(1e14 as u128);
/// Max average price adjustment percent (30bps)
pub const MAX_AVG_PRICE_ADJ_PERCENT: Decimal = Decimal::raw(3e15 as u128);
/// Max precision in CosmWasm limited to max precision of Decimal/Decimal256 types which is 18.
/// Precision in Duality is 27
/// (https://github.com/neutron-org/neutron/blob/8ee37dd582bdf640e4d3cfa0eb6fa59ffdd27e84/utils/math/prec_dec.go#L26).
/// Hence, while converting price from contract to Duality representation,
/// we must multiply it by 1e9.
/// In case Duality changes precision, this value should be updated accordingly.
pub const DUALITY_PRICE_ADJUSTMENT: Uint256 = Uint256::from_u128(1e9 as u128);
