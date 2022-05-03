use std::str::FromStr;

use cosmwasm_std::{
    Decimal, Decimal256, Fraction, QuerierWrapper, StdError, StdResult, Uint128, Uint256,
};

use astroport::asset::Asset;
use astroport::pair_reserve::{PoolParams, MAX_ALLOWED_SLIPPAGE};
use astroport::DecimalCheckedOps;

use crate::error::ContractError;
use crate::general::{get_oracle_price, RateDirection};

fn decimal256_to_decimal(value: Decimal256) -> StdResult<Decimal> {
    let numerator: Uint128 = value.numerator().try_into()?;
    // Decimal256::DECIMAL_FRACTIONAL is always 10**18
    let denominator: Uint128 = value.denominator().try_into().unwrap();
    Ok(Decimal::from_ratio(numerator, denominator))
}

fn calc_direct_spread(
    offer_amount: impl Into<Uint256>,
    offer_pool: Uint256,
    ask_pool: Uint256,
    cp: Uint256,
) -> StdResult<Decimal> {
    let offer_amount = offer_amount.into();
    // ask_amount = ask_pool - cp / (offer_pool + offer_amount)
    let ask_amount = ask_pool.checked_sub(cp / (offer_pool + offer_amount))?;
    let spread = Decimal256::from_ratio(offer_amount - ask_amount, offer_pool);
    decimal256_to_decimal(spread)
}

fn calc_reverse_spread(
    ask_amount: impl Into<Uint256>,
    ask_pool: Uint256,
    offer_pool: Uint256,
    cp: Uint256,
) -> StdResult<Decimal> {
    let ask_amount = ask_amount.into();
    // offer_amount = cp / (ask_pool - ask_amount) - offer_pool
    let offer_amount = (cp / (ask_pool - ask_amount)).checked_sub(offer_pool)?;
    let spread = Decimal256::from_ratio(offer_amount - ask_amount, offer_pool);
    decimal256_to_decimal(spread)
}

/// Internal structure to store swap result
#[derive(PartialEq, Debug, Copy, Clone)]
pub struct SwapResult {
    pub amount: Uint128,
    pub spread_ust_fee: Uint128,
    pub spread: Decimal,
}

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
        return Err(ContractError::SwapZeroAmount {})?;
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

/// The idea is based on on-chain LUNA<>UST swap.
/// 'delta_decay' can reduce delta to zero only in case of replenish_pools()
/// will never be called during a recovery period. Otherwise there always will be a small residual.
pub(crate) fn replenish_pools(pool_params: &mut PoolParams, cur_block: u64) -> StdResult<()> {
    for flow_params in [&mut pool_params.exit, &mut pool_params.entry] {
        let blocks_passed = cur_block.saturating_sub(pool_params.last_repl_block);
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
        assert_eq!(&direct_spread.to_string(), "0.009");
        let ask_amount = offer_amount * (Decimal::one() - direct_spread);
        let reverse_spread =
            calc_reverse_spread(ask_amount, base_pool.into(), base_pool.into(), cp).unwrap();
        // A small residual appears because of rounding during Uint128 and Decimal multiplication
        assert_eq!(
            offer_amount,
            ask_amount * (Decimal::one() + reverse_spread) - Uint128::from(1u8)
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
            amount: Uint128::from(2_000_000_000000u128), // $2MM
        };
        let direct_swap = compute_swap(&querier, &offer_asset, &mut pool_params.clone()).unwrap();
        assert_eq!(
            direct_swap,
            SwapResult {
                amount: Uint128::from(49_975000u128),
                spread_ust_fee: Uint128::from(1000_000000u128), // $1k spread fee
                spread: Decimal::from_str("0.0005").unwrap(),   // 0.0005% spread (default)
            }
        );
        let ask_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(50_000000u128),
        };
        // Here we use exit flow parameters thus min spread is different
        let reverse_swap =
            compute_reverse_swap(&querier, &ask_asset, &pool_params.clone()).unwrap();
        assert_eq!(
            reverse_swap,
            SwapResult {
                amount: Uint128::from(2_020_000_000000u128),
                spread_ust_fee: Uint128::from(20000_000000u128), // $20k spread fee
                spread: Decimal::from_str("0.01").unwrap(),      // 1% spread (default)
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
                amount: Uint128::from(483_333333u128),
                spread_ust_fee: Uint128::from(666666_666666_u128), // $666.66k spread fee
                spread: Decimal::from_str("0.03333333333333").unwrap(), // $3.33% spread
            }
        );

        // Next swap will be more expensive than before
        let offer_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
            amount: Uint128::from(2_000_000_000000u128), // $2M
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(49_735883_u128),            // 4.73 BTC,
                spread_ust_fee: Uint128::from(10564_663023_u128), // $10564.66k spread fee
                spread: Decimal::from_str("0.005282331511841666").unwrap(), // 0.53% spread
            }
        );

        // ----------- checking BTC -> UST swap -----------------

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
                amount: Uint128::from(3960000_000000_u128),
                spread_ust_fee: Uint128::from(40000_000000_u128), // 4k$ spread fee
                spread: Decimal::from_str("0.01").unwrap(),       // 1% spread
            }
        );

        let offer_asset = Asset {
            info: AssetInfo::Token {
                contract_addr: Addr::unchecked("cw20_btc"),
            },
            amount: Uint128::from(1000_000000u128),
        };
        let res = compute_swap(&querier, &offer_asset, &mut pool_params).unwrap();
        assert_eq!(
            res,
            SwapResult {
                amount: Uint128::from(34_868161_849711u128),
                spread_ust_fee: Uint128::from(5_131_838_150289_u128), // $5.13MM spread fee
                spread: Decimal::from_str("0.128295953757226421").unwrap(), // 12.82% spread
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
                amount: Uint128::from(3_937_882_942098_u128),
                spread_ust_fee: Uint128::from(62_117_057902_u128), // 62.1k$ spread fee
                spread: Decimal::from_str("0.015529264475743249").unwrap(), // 1.5% spread
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
