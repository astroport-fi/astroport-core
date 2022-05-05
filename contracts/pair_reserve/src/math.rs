use std::str::FromStr;

use cosmwasm_std::{
    Decimal, Decimal256, Fraction, QuerierWrapper, StdError, StdResult, Uint128, Uint256,
};

use astroport::asset::Asset;
use astroport::pair_reserve::{PoolParams, MAX_ALLOWED_SLIPPAGE};
use astroport::DecimalCheckedOps;

use crate::error::ContractError;
use crate::general::{get_oracle_price, RateDirection};

/// ## Description
/// Converts Decimal256 to Decimal. In case the numerator is greater than [`Uint128::MAX`]
/// the function throws an error.
fn decimal256_to_decimal(value: Decimal256) -> StdResult<Decimal> {
    let numerator: Uint128 = value.numerator().try_into()?;
    // Decimal256::DECIMAL_FRACTIONAL is always 10**18
    let denominator: Uint128 = value.denominator().try_into().unwrap();
    Ok(Decimal::from_ratio(numerator, denominator))
}

/// ## Description
/// Calculates the direct spread based on a given variables, where `offer_amount` is amount of offered coins,
/// `offer_pool` - the total amount of coins in the offer pool, `ask_pool` - the total amount of coins in the ask pool,
/// `cp` - constant product i.e. k in formula x*y=k. The function considers that all values are of one denom.
fn calc_direct_spread(
    offer_amount: impl Into<Uint256>,
    offer_pool: Uint256,
    ask_pool: Uint256,
    cp: Uint256,
) -> StdResult<Decimal> {
    let offer_amount = offer_amount.into();
    // ask_amount = ask_pool - cp / (offer_pool + offer_amount)
    let ask_amount = ask_pool.checked_sub(cp / (offer_pool + offer_amount))?;
    let spread = Decimal256::from_ratio(offer_amount - ask_amount, offer_amount);
    decimal256_to_decimal(spread)
}

/// ## Description
/// Calculates the reverse spread based on a given variables, where `ask_amount` is amount of coins asked,
/// `ask_pool` - the total amount of coins in the ask pool, `offer_pool` - the total amount of coins in the offer pool,
/// `cp` - constant product i.e. k in formula x*y=k. The function considers that all values are of one denom.
fn calc_reverse_spread(
    ask_amount: impl Into<Uint256>,
    ask_pool: Uint256,
    offer_pool: Uint256,
    cp: Uint256,
) -> StdResult<Decimal> {
    let ask_amount = ask_amount.into();
    // offer_amount = cp / (ask_pool - ask_amount) - offer_pool
    let offer_amount = (cp / (ask_pool - ask_amount)).checked_sub(offer_pool)?;
    let spread = Decimal256::from_ratio(offer_amount - ask_amount, offer_amount);
    decimal256_to_decimal(spread)
}

/// ## Description
/// Internal structure to store swap result.
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct SwapResult {
    /// Amount of return or offer coins (depends on the swap direction: direct or reverse).
    pub amount: Uint128,
    /// Spread fee denominated in UST
    pub spread_ust_fee: Uint128,
    /// Spread percentage
    pub spread: Decimal,
}

/// ## Description
/// The main calculation function of reserve pool. It selects flow parameters based on the offered asset,
/// converts offered asset to the UST denom, calculates the spread and sets it to the default
/// if it is less than the minimum spread limit. Return asset is converted to ask denom based on the oracles price.
/// The function also increases certain flow delta based on the offer asset.
pub(crate) fn compute_swap(
    querier: &QuerierWrapper,
    offer_asset: &Asset,
    pool_params: &mut PoolParams,
) -> Result<SwapResult, ContractError> {
    let flow_params;
    let ask_exchange_rate;
    let offer_ust_amount;
    let ust_pool;
    if offer_asset.is_native_token() {
        // UST -> BTC
        flow_params = &mut pool_params.entry;
        offer_ust_amount = offer_asset.amount;
        ask_exchange_rate =
            get_oracle_price(querier, RateDirection::USD2BTC, &pool_params.oracles)?;
        ust_pool = flow_params
            .base_pool
            .checked_add(flow_params.pool_delta * Uint128::from(1_u8))?
            .into();
    } else {
        // BTC -> UST
        flow_params = &mut pool_params.exit;
        offer_ust_amount = get_oracle_price(querier, RateDirection::BTC2USD, &pool_params.oracles)?
            .checked_mul(offer_asset.amount)?;
        ask_exchange_rate = Decimal::one();
        ust_pool = flow_params
            .base_pool
            .checked_sub(flow_params.pool_delta * Uint128::from(1_u8))?
            .into();
    }

    let cp = flow_params.base_pool.full_mul(flow_params.base_pool);
    let btc_pool = cp / ust_pool;
    let mut spread = if offer_asset.is_native_token() {
        calc_direct_spread(offer_ust_amount, ust_pool, btc_pool, cp)?
    } else {
        calc_direct_spread(offer_ust_amount, btc_pool, ust_pool, cp)?
    };
    let flow_min_spread = Decimal::from_ratio(flow_params.min_spread, 10000u16);
    if spread < flow_min_spread {
        spread = flow_min_spread;
    }
    let spread_ust_fee = spread.checked_mul(offer_ust_amount)?;
    let ask_amount = ask_exchange_rate.checked_mul(offer_ust_amount - spread_ust_fee)?;
    if ask_amount.is_zero() {
        return Err(ContractError::SwapZeroAmount {});
    }
    flow_params.pool_delta = flow_params
        .pool_delta
        .checked_add(Decimal::from_ratio(offer_ust_amount, 1_u8))?;

    Ok(SwapResult {
        amount: ask_amount,
        spread_ust_fee,
        spread,
    })
}

/// ## Description
/// This function is reversed [`compute_swap`].
/// It calculates the amount of the offer asset based on the amount of the ask asset and flow parameters.
/// NOTE: this function does not mutate flow delta and is intended for querying purposes.
pub(crate) fn compute_reverse_swap(
    querier: &QuerierWrapper,
    ask_asset: &Asset,
    pool_params: &PoolParams,
) -> Result<SwapResult, ContractError> {
    let flow_params;
    let offer_exchange_rate;
    let ask_ust_amount;
    let ust_pool;
    if ask_asset.is_native_token() {
        // BTC -> UST
        flow_params = &pool_params.entry;
        ask_ust_amount = ask_asset.amount;
        offer_exchange_rate =
            get_oracle_price(querier, RateDirection::USD2BTC, &pool_params.oracles)?;
        ust_pool = flow_params
            .base_pool
            .checked_add(flow_params.pool_delta * Uint128::from(1_u8))?
            .into();
    } else {
        // UST -> BTC
        flow_params = &pool_params.exit;
        ask_ust_amount = get_oracle_price(querier, RateDirection::BTC2USD, &pool_params.oracles)?
            .checked_mul(ask_asset.amount)?;
        offer_exchange_rate = Decimal::one();
        ust_pool = flow_params
            .base_pool
            .checked_sub(flow_params.pool_delta * Uint128::from(1_u8))?
            .into();
    }

    let cp = flow_params.base_pool.full_mul(flow_params.base_pool);
    let btc_pool = cp / ust_pool;
    let mut spread = if ask_asset.is_native_token() {
        calc_reverse_spread(ask_ust_amount, ust_pool, btc_pool, cp)?
    } else {
        calc_reverse_spread(ask_ust_amount, btc_pool, ust_pool, cp)?
    };
    let flow_min_spread = Decimal::from_ratio(flow_params.min_spread, 10000u16);
    if spread < flow_min_spread {
        spread = flow_min_spread;
    }
    let spread_ust_fee = spread.checked_mul(ask_ust_amount)?;
    let offer_amount = offer_exchange_rate.checked_mul(ask_ust_amount + spread_ust_fee)?;

    Ok(SwapResult {
        amount: offer_amount,
        spread_ust_fee,
        spread,
    })
}

/// ## Description
/// Returns a [`ContractError`] on failure.
/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
/// ## Params
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of assets to swap.
///
/// * **swap_result** is an object of type [`SwapResult`]. This is the result of the swap calculation.
///
/// * **belief_price** is an object of type [`Option<Decimal>`]. This is the belief price used in the swap.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. This is the
/// max spread allowed so that the swap can be executed successfully.
pub(crate) fn assert_max_spread(
    offer_amount: Uint128,
    swap_result: SwapResult,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
) -> Result<(), ContractError> {
    let max_allowed_spread = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;
    let max_spread = max_spread.unwrap_or(max_allowed_spread);
    if max_spread > max_allowed_spread {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    if let Some(belief_price) = belief_price {
        let belief_rate = belief_price
            .inv()
            .ok_or_else(|| StdError::generic_err("Division by zero"))?;
        let expected_return = belief_rate.checked_mul(offer_amount)?;
        if swap_result.amount < expected_return {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    }
    if swap_result.spread > max_spread {
        return Err(ContractError::MaxSpreadAssertion {});
    }

    Ok(())
}

/// ## Description
/// Replenishes both pool flows towards equilibrium.
/// The idea is based on the on-chain LUNA<>UST swap.
/// 'delta_decay' can reduce delta to zero only in case of replenish_pools()
/// will never be called during a recovery period. Otherwise there always will be a small residual.
pub(crate) fn replenish_pools(pool_params: &mut PoolParams, cur_block: u64) -> StdResult<()> {
    if pool_params.last_repl_block == cur_block {
        return Ok(());
    }
    let blocks_passed = cur_block.saturating_sub(pool_params.last_repl_block);

    for flow_params in [&mut pool_params.exit, &mut pool_params.entry] {
        let delta_decay = Decimal::from_ratio(
            flow_params.pool_delta.checked_mul(blocks_passed.into())?,
            flow_params.recovery_period,
        );
        flow_params.pool_delta = if flow_params.pool_delta > delta_decay {
            flow_params.pool_delta - delta_decay
        } else {
            Decimal::zero()
        };
    }
    pool_params.last_repl_block = cur_block;

    Ok(())
}

#[cfg(test)]
mod testing {
    use std::str::FromStr;

    use cosmwasm_std::Addr;

    use crate::mock_querier::{CustomQuerier, ORACLE_ADDR1, ORACLE_ADDR2};
    use astroport::asset::AssetInfo;
    use astroport::pair_reserve::FlowParams;

    use super::*;

    #[test]
    fn test_calc_spread() {
        let base_pool = Uint128::from(10000u128);
        let offer_amount = Uint128::from(1000u128);
        let cp = base_pool.full_mul(base_pool);
        let direct_spread =
            calc_direct_spread(offer_amount, base_pool.into(), base_pool.into(), cp).unwrap();
        assert_eq!(&direct_spread.to_string(), "0.09");
        let ask_amount = offer_amount * (Decimal::one() - direct_spread);
        let reverse_spread =
            calc_reverse_spread(ask_amount, base_pool.into(), base_pool.into(), cp).unwrap();
        // A small residual appears because of rounding during Uint128 and Decimal multiplication
        assert_eq!(
            offer_amount * (Decimal::one() - reverse_spread) + Uint128::from(1u8),
            ask_amount
        );
    }

    #[test]
    fn test_compute_swap() {
        let querier = QuerierWrapper::new(&CustomQuerier);
        let base_pool = 100_000_000_000000u128; // $100MM
        let mut pool_params = PoolParams {
            entry: FlowParams {
                base_pool: Uint128::from(base_pool),
                min_spread: 5,
                ..Default::default()
            },
            exit: FlowParams {
                base_pool: Uint128::from(base_pool),
                min_spread: 100,
                ..Default::default()
            },
            last_repl_block: 0,
            oracles: vec![Addr::unchecked(ORACLE_ADDR1), Addr::unchecked(ORACLE_ADDR2)],
        };

        let offer_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(40_000_000000u128), // $40k
        };
        let direct_swap = compute_swap(&querier, &offer_asset, &mut pool_params.clone()).unwrap();
        assert_eq!(
            direct_swap,
            SwapResult {
                amount: Uint128::from(999_500u128),
                spread_ust_fee: Uint128::from(20_000000u128), // $20 spread fee
                spread: Decimal::from_str("0.0005").unwrap(), // 0.0005% spread (default)
            }
        );
        let ask_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(1_000000u128),
        };
        // Here we use exit flow parameters thus min spread is different
        let reverse_swap =
            compute_reverse_swap(&querier, &ask_asset, &pool_params.clone()).unwrap();
        assert_eq!(
            reverse_swap,
            SwapResult {
                amount: Uint128::from(40_400_000000u128), // a user needs to offer $40.4k to get 1 BTC
                spread_ust_fee: Uint128::from(400_000000u128), // $400 spread fee
                spread: Decimal::from_str("0.01").unwrap(), // 1% spread (default)
            }
        );

        // Simulate a big drain of btc liquidity
        let offer_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(20_000_000_000000u128), // $20MM
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(416_666666u128),
                spread_ust_fee: Uint128::from(3_333_333_333333u128), // $3.333MM spread fee
                spread: Decimal::from_str("0.16666666666665").unwrap(), // 16.66% spread
            }
        );

        // Next swap will be more expensive than before
        let offer_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(40_000_000000u128), // $40k
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(694213u128),                  // 0.69 BTC,
                spread_ust_fee: Uint128::from(12_231_478396u128),   // $12.23k spread fee
                spread: Decimal::from_str("0.3057869599").unwrap(), // 30.5% spread
            }
        );

        // ----------- checking BTC -> UST swap -----------------

        let offer_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(1_000000u128),
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(39600_000000u128),
                spread_ust_fee: Uint128::from(400_000000_u128), // 400$ spread fee
                spread: Decimal::from_str("0.01").unwrap(),     // 1% spread
            }
        );

        let offer_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(100_000000u128),
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(3_843_136_663641u128),
                spread_ust_fee: Uint128::from(156_863_336359u128), // $156.8k spread fee
                spread: Decimal::from_str("0.03921583408975").unwrap(), // 3.9% spread
            }
        );

        let offer_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(1_000000u128),
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(36819_153736u128),
                spread_ust_fee: Uint128::from(3180_846264u128), // $3180 spread fee
                spread: Decimal::from_str("0.0795211566").unwrap(), // 7.9% spread
            }
        );
    }

    #[test]
    fn test_assert_spread() {
        let offer_amount = Uint128::from(100_000000u128);

        let swap_result = SwapResult {
            amount: Uint128::from(2_000000u128),
            spread_ust_fee: Uint128::from(1_000000u128),
            spread: Decimal::from_str("0.01").unwrap(),
        };
        assert_max_spread(
            offer_amount,
            swap_result,
            Decimal::from_str("50.0").ok(),
            None,
        )
        .unwrap();

        assert_max_spread(
            offer_amount,
            swap_result,
            None,
            Decimal::from_str("0.11").ok(),
        )
        .unwrap();

        let err = assert_max_spread(
            offer_amount,
            swap_result,
            Decimal::from_str("49.9").ok(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::MaxSpreadAssertion {});

        let err = assert_max_spread(
            offer_amount,
            swap_result,
            None,
            Decimal::from_str("0.005").ok(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::MaxSpreadAssertion {});

        let err = assert_max_spread(
            offer_amount,
            swap_result,
            None,
            Decimal::from_str("0.6").ok(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::AllowedSpreadAssertion {});
    }

    #[test]
    fn test_replenishment() {
        let mut pool_params = PoolParams {
            entry: FlowParams {
                recovery_period: 10,
                pool_delta: Decimal::from_ratio(9u8, 1u8),
                ..Default::default()
            },
            exit: FlowParams {
                recovery_period: 100,
                pool_delta: Decimal::from_ratio(60u8, 1u8),
                ..Default::default()
            },
            last_repl_block: 0,
            oracles: vec![Addr::unchecked(ORACLE_ADDR2), Addr::unchecked(ORACLE_ADDR1)],
        };

        replenish_pools(&mut pool_params, 1).unwrap();
        assert_eq!(&pool_params.entry.pool_delta.to_string(), "8.1");
        assert_eq!(&pool_params.exit.pool_delta.to_string(), "59.4");

        pool_params.entry.pool_delta =
            pool_params.entry.pool_delta + Decimal::from_str("11.9").unwrap();
        pool_params.exit.pool_delta =
            pool_params.exit.pool_delta + Decimal::from_str("40.6").unwrap();
        replenish_pools(&mut pool_params, 2).unwrap();
        assert_eq!(&pool_params.entry.pool_delta.to_string(), "18");
        assert_eq!(&pool_params.exit.pool_delta.to_string(), "99");

        replenish_pools(&mut pool_params, 12).unwrap();
        assert_eq!(&pool_params.entry.pool_delta.to_string(), "0");
        assert_eq!(&pool_params.exit.pool_delta.to_string(), "89.1");

        replenish_pools(&mut pool_params, 112).unwrap();
        assert_eq!(&pool_params.entry.pool_delta.to_string(), "0");
        assert_eq!(&pool_params.exit.pool_delta.to_string(), "0");
    }
}
