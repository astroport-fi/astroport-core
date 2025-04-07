use cosmwasm_std::{
    coin, wasm_execute, Addr, Api, CosmosMsg, CustomMsg, CustomQuery, Decimal, Decimal256, Env,
    Fraction, QuerierWrapper, StdError, StdResult, Uint128,
};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, Decimal256Ext, DecimalAsset};
use astroport::cosmwasm_ext::AbsDiff;
use astroport::incentives::ExecuteMsg as IncentiveExecuteMsg;
use astroport::querier::query_factory_config;
use astroport::token_factory::tf_mint_msg;
use astroport_factory::state::pair_key;

use crate::consts::{
    DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, N, OFFER_PERCENT, TWAP_PRECISION_DEC, TWO,
};
use crate::error::PclError;
use crate::state::{Config, PoolParams, PriceState};
use crate::{calc_d, calc_y};

#[cfg(any(feature = "injective", feature = "sei"))]
use cosmwasm_std::BankMsg;

/// Helper function to check the given asset infos are valid.
pub fn check_asset_infos(api: &dyn Api, asset_infos: &[AssetInfo]) -> Result<(), PclError> {
    if !asset_infos.iter().all_unique() {
        return Err(PclError::DoublingAssets {});
    }

    asset_infos
        .iter()
        .try_for_each(|asset_info| asset_info.check(api))
        .map_err(Into::into)
}

/// Helper function to check that the assets in a given array are valid.
pub fn check_assets(api: &dyn Api, assets: &[Asset]) -> Result<(), PclError> {
    let asset_infos = assets.iter().map(|asset| asset.info.clone()).collect_vec();
    check_asset_infos(api, &asset_infos)
}

/// Checks that cw20 token is part of the pool.
///
/// * **cw20_sender** is cw20 token address which is being checked.
pub fn check_cw20_in_pool(config: &Config, cw20_sender: &Addr) -> Result<(), PclError> {
    for asset_info in &config.pair_info.asset_infos {
        match asset_info {
            AssetInfo::Token { contract_addr } if contract_addr == cw20_sender => return Ok(()),
            _ => {}
        }
    }

    Err(PclError::Unauthorized {})
}

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Incentive contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **coin** denom and amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
/// be automatically staked in the Incentives Contract on behalf of the recipient.
pub fn mint_liquidity_token_message<T, C>(
    querier: QuerierWrapper<C>,
    config: &Config,
    contract_address: &Addr,
    recipient: &Addr,
    amount: Uint128,
    auto_stake: bool,
) -> Result<Vec<CosmosMsg<T>>, PclError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    let coin = coin(amount.into(), config.pair_info.liquidity_token.to_string());

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(tf_mint_msg(contract_address, coin, recipient));
    }

    // Mint for the pair contract and stake into the Incentives contract
    let incentives_addr = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    if let Some(address) = incentives_addr {
        let mut msgs = tf_mint_msg(contract_address, coin.clone(), contract_address);
        msgs.push(
            wasm_execute(
                address,
                &IncentiveExecuteMsg::Deposit {
                    recipient: Some(recipient.to_string()),
                },
                vec![coin],
            )?
            .into(),
        );
        Ok(msgs)
    } else {
        Err(PclError::AutoStakeError {})
    }
}

/// Return the amount of tokens that a specific amount of LP tokens would withdraw.
///
/// * **pools** assets available in the pool.
///
/// * **amount** amount of LP tokens to calculate underlying amounts for.
///
/// * **total_share** total amount of LP tokens currently issued by the pool.
pub fn get_share_in_assets(
    pools: &[DecimalAsset],
    amount: Uint128,
    total_share: Uint128,
) -> Vec<DecimalAsset> {
    let share_ratio = if !total_share.is_zero() {
        Decimal256::from_ratio(amount, total_share)
    } else {
        Decimal256::zero()
    };

    pools
        .iter()
        .map(|pool| DecimalAsset {
            info: pool.info.clone(),
            amount: pool.amount * share_ratio,
        })
        .collect()
}

/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
///
/// * **belief_price** belief price used in the swap.
///
/// * **max_spread** max spread allowed so that the swap can be executed successfuly.
///
/// * **offer_amount** amount of assets to swap.
///
/// * **return_amount** amount of assets  a user wants to receive from the swap.
///
/// * **spread_amount** spread used in the swap.
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), PclError> {
    let max_spread = max_spread.map(Decimal256::from).unwrap_or(DEFAULT_SLIPPAGE);
    if max_spread > MAX_ALLOWED_SLIPPAGE {
        return Err(PclError::AllowedSpreadAssertion {});
    }

    if let Some(belief_price) = belief_price {
        let expected_return = offer_amount
            * belief_price.inv().ok_or_else(|| {
                StdError::generic_err("Invalid belief_price. Check the input values.")
            })?;

        let spread_amount = expected_return.saturating_sub(return_amount);

        if return_amount < expected_return
            && Decimal256::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(PclError::MaxSpreadAssertion {});
        }
    } else if Decimal256::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
        return Err(PclError::MaxSpreadAssertion {});
    }

    Ok(())
}

/// Checks whether it possible to make a swap or not.
pub fn before_swap_check(pools: &[DecimalAsset], offer_amount: Decimal256) -> StdResult<()> {
    if offer_amount.is_zero() {
        return Err(StdError::generic_err("Swap amount must not be zero"));
    }
    if pools.iter().any(|a| a.amount.is_zero()) {
        return Err(StdError::generic_err("One of the pools is empty"));
    }

    Ok(())
}

/// This structure is for internal use only. Represents swap's result.
#[derive(Debug)]
pub struct SwapResult {
    pub dy: Decimal256,
    pub spread_fee: Decimal256,
    pub maker_fee: Decimal256,
    pub share_fee: Decimal256,
    pub total_fee: Decimal256,
}

impl SwapResult {
    /// Calculates **last price** for PCL repeg algo
    pub fn calc_last_price(&self, offer_amount: Decimal256, offer_ind: usize) -> Decimal256 {
        if offer_ind == 0 {
            offer_amount / (self.dy + self.maker_fee + self.share_fee)
        } else {
            (self.dy + self.maker_fee + self.share_fee) / offer_amount
        }
    }
}

/// Performs swap simulation to calculate a price.
pub fn calc_last_prices(xs: &[Decimal256], config: &Config, env: &Env) -> StdResult<Decimal256> {
    let mut offer_amount = Decimal256::one().min(xs[0] * OFFER_PERCENT);
    if offer_amount.is_zero() {
        offer_amount = Decimal256::raw(1u128);
    }

    let last_price = compute_swap(
        xs,
        offer_amount,
        1,
        config,
        env,
        Decimal256::zero(),
        Decimal256::zero(),
    )?
    .calc_last_price(offer_amount, 0);

    Ok(last_price)
}

/// Accumulate token prices for the assets in the pool.
pub fn accumulate_prices(env: &Env, config: &mut Config, last_real_price: Decimal256) {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return;
    }

    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    for (from, _, value) in config.cumulative_prices.iter_mut() {
        let price = if &config.pair_info.asset_infos[0] == from {
            last_real_price.inv().unwrap()
        } else {
            last_real_price
        };
        // Price max value = 1e18 bc smallest value in Decimal is 1e-18.
        // Thus highest inverted price is 1/1e-18.
        // (price * twap) max value = 1e24 which fits into Uint128 thus we use unwrap here
        let price: Uint128 = (price * TWAP_PRECISION_DEC)
            .to_uint128_with_precision(0u8)
            .unwrap();
        // time_elapsed * price does not need checked_mul.
        // price max value = 1e24, u128 max value = 340282366920938463463374607431768211455
        // overflow is possible if time_elapsed > 340282366920939 seconds ~ 10790283 years
        *value = value.wrapping_add(time_elapsed * price);
    }

    config.block_time_last = block_time;
}

/// Calculate swap result.
pub fn compute_swap(
    xs: &[Decimal256],
    offer_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    env: &Env,
    maker_fee_share: Decimal256,
    share_fee_share: Decimal256,
) -> StdResult<SwapResult> {
    let offer_ind = 1 ^ ask_ind;

    let mut ixs = xs.to_vec();
    ixs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let d = calc_d(&ixs, &amp_gamma)?;

    if offer_ind == 1 {
        ixs[offer_ind] += offer_amount * config.pool_state.price_state.price_scale;
    } else {
        ixs[offer_ind] += offer_amount;
    }

    let new_y = calc_y(&ixs, d, &amp_gamma, ask_ind)?;
    let mut dy = ixs[ask_ind] - new_y;
    ixs[ask_ind] = new_y;

    // Derive spread using oracle price
    let spread_fee = if ask_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
        (offer_amount / config.pool_state.price_state.oracle_price).saturating_sub(dy)
    } else {
        (offer_amount * config.pool_state.price_state.oracle_price).saturating_sub(dy)
    };

    let fee_rate = config.pool_params.fee(&ixs);
    let total_fee = fee_rate * dy;
    dy -= total_fee;

    let share_fee = total_fee * share_fee_share;

    Ok(SwapResult {
        dy,
        spread_fee,
        maker_fee: (total_fee - share_fee) * maker_fee_share,
        share_fee,
        total_fee,
    })
}

/// Returns an amount of offer assets for a specified amount of ask assets.
pub fn compute_offer_amount(
    xs: &[Decimal256],
    mut want_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    env: &Env,
) -> StdResult<(Decimal256, Decimal256, Decimal256)> {
    let offer_ind = 1 ^ ask_ind;

    if ask_ind == 1 {
        want_amount *= config.pool_state.price_state.price_scale
    }

    let mut ixs = xs.to_vec();
    ixs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let d = calc_d(&ixs, &amp_gamma)?;

    // It's hard to predict fee rate thus we use maximum possible fee rate
    let before_fee = want_amount
        * (Decimal256::one() - Decimal256::from(config.pool_params.out_fee))
            .inv()
            .unwrap();
    let mut fee = before_fee - want_amount;

    ixs[ask_ind] -= before_fee;

    let new_y = calc_y(&ixs, d, &amp_gamma, offer_ind)?;
    let mut dy = new_y - ixs[offer_ind];

    let mut spread_fee = dy.saturating_sub(before_fee);
    if offer_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
        spread_fee /= config.pool_state.price_state.price_scale;
        fee /= config.pool_state.price_state.price_scale;
    }

    Ok((dy, spread_fee, fee))
}

/// Calculate provide fee applied on the amount of LP tokens. Only charged for imbalanced provide.
/// * `deposits` - internal repr of deposit
/// * `xp` - internal repr of pools
pub fn calc_provide_fee(
    deposits: &[Decimal256],
    xp: &[Decimal256],
    params: &PoolParams,
) -> Decimal256 {
    let sum = deposits[0] + deposits[1];
    let avg = sum / N;

    deposits[0].diff(avg) * params.fee(xp) / sum
}

/// This is an internal function that enforces slippage tolerance for provides. Returns actual slippage.
pub fn assert_slippage_tolerance(
    deposits: &[Decimal256],
    actual_share: Decimal256,
    price_state: &PriceState,
    slippage_tolerance: Option<Decimal>,
) -> Result<Decimal256, PclError> {
    let slippage_tolerance = slippage_tolerance
        .map(Into::into)
        .unwrap_or(DEFAULT_SLIPPAGE);
    if slippage_tolerance > MAX_ALLOWED_SLIPPAGE {
        return Err(PclError::AllowedSpreadAssertion {});
    }

    let deposit_value = deposits[0] + deposits[1] * price_state.price_scale;
    let lp_expected = (deposit_value / TWO * deposit_value / (TWO * price_state.price_scale))
        .sqrt()
        / price_state.xcp_profit_real;
    let slippage = lp_expected.saturating_sub(actual_share) / lp_expected;

    if slippage > slippage_tolerance {
        return Err(PclError::MaxSpreadAssertion {});
    }

    Ok(slippage)
}

/// Checks whether the pair is registered in the factory or not.
pub fn check_pair_registered<C>(
    querier: QuerierWrapper<C>,
    factory: &Addr,
    asset_infos: &[AssetInfo],
) -> StdResult<bool>
where
    C: CustomQuery,
{
    astroport_factory::state::PAIRS
        .query(&querier, factory.clone(), &pair_key(asset_infos))
        .map(|inner| inner.is_some())
}

#[cfg(test)]
mod tests {
    use crate::state::PoolParams;
    use astroport_test::convert::{dec_to_f64, f64_to_dec};

    use super::*;

    #[test]
    fn test_provide_fees() {
        let params = PoolParams {
            mid_fee: f64_to_dec(0.0026),
            out_fee: f64_to_dec(0.0045),
            fee_gamma: f64_to_dec(0.00023),
            ..PoolParams::default()
        };

        let fee_rate = calc_provide_fee(
            &[f64_to_dec(50_000f64), f64_to_dec(50_000f64)],
            &[f64_to_dec(100_000f64), f64_to_dec(100_000f64)],
            &params,
        );
        assert_eq!(dec_to_f64(fee_rate), 0.0);

        let fee_rate = calc_provide_fee(
            &[f64_to_dec(99_000f64), f64_to_dec(1_000f64)],
            &[f64_to_dec(100_000f64), f64_to_dec(100_000f64)],
            &params,
        );
        assert_eq!(dec_to_f64(fee_rate), 0.001274);

        let fee_rate = calc_provide_fee(
            &[f64_to_dec(99_000f64), f64_to_dec(1_000f64)],
            &[f64_to_dec(1_000f64), f64_to_dec(99_000f64)],
            &params,
        );
        assert_eq!(dec_to_f64(fee_rate), 0.002205);
    }
}
