use cosmwasm_std::{
    to_binary, wasm_execute, Addr, Api, CosmosMsg, Decimal, Decimal256, Env, Fraction,
    QuerierWrapper, StdError, StdResult, Storage, Uint128, Uint256,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, DecimalAsset};
use astroport::cosmwasm_ext::AbsDiff;
use astroport::observation::Observation;
use astroport::querier::{query_factory_config, query_supply};
use astroport_circular_buffer::error::BufferResult;
use astroport_circular_buffer::BufferManager;
use astroport_factory::state::pair_key;

use crate::consts::{DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, N, OFFER_PERCENT};
use crate::error::ContractError;
use crate::math::{calc_d, calc_y};
use crate::state::{Config, PoolParams, Precisions, OBSERVATIONS};

/// Helper function to check the given asset infos are valid.
pub(crate) fn check_asset_infos(
    api: &dyn Api,
    asset_infos: &[AssetInfo],
) -> Result<(), ContractError> {
    if !asset_infos.iter().all_unique() {
        return Err(ContractError::DoublingAssets {});
    }

    asset_infos
        .iter()
        .try_for_each(|asset_info| asset_info.check(api))
        .map_err(Into::into)
}

/// Helper function to check that the assets in a given array are valid.
pub(crate) fn check_assets(api: &dyn Api, assets: &[Asset]) -> Result<(), ContractError> {
    let asset_infos = assets.iter().map(|asset| asset.info.clone()).collect_vec();
    check_asset_infos(api, &asset_infos)
}

/// Checks that cw20 token is part of the pool.
///
/// * **cw20_sender** is cw20 token address which is being checked.
pub(crate) fn check_cw20_in_pool(config: &Config, cw20_sender: &Addr) -> Result<(), ContractError> {
    for asset_info in &config.pair_info.asset_infos {
        match asset_info {
            AssetInfo::Token { contract_addr } if contract_addr == cw20_sender => return Ok(()),
            _ => {}
        }
    }

    Err(ContractError::Unauthorized {})
}

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Generator contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **amount** amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
/// be automatically staked in the Generator on behalf of the recipient.
pub(crate) fn mint_liquidity_token_message(
    querier: QuerierWrapper,
    config: &Config,
    contract_address: &Addr,
    recipient: &Addr,
    amount: Uint128,
    auto_stake: bool,
) -> Result<Vec<CosmosMsg>, ContractError> {
    let lp_token = &config.pair_info.liquidity_token;

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(vec![wasm_execute(
            lp_token,
            &Cw20ExecuteMsg::Mint {
                recipient: recipient.to_string(),
                amount,
            },
            vec![],
        )?
        .into()]);
    }

    // Mint for the pair contract and stake into the Generator contract
    let generator = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    if let Some(generator) = generator {
        Ok(vec![
            wasm_execute(
                lp_token,
                &Cw20ExecuteMsg::Mint {
                    recipient: contract_address.to_string(),
                    amount,
                },
                vec![],
            )?
            .into(),
            wasm_execute(
                lp_token,
                &Cw20ExecuteMsg::Send {
                    contract: generator.to_string(),
                    amount,
                    msg: to_binary(&astroport::generator::Cw20HookMsg::DepositFor(
                        recipient.to_string(),
                    ))?,
                },
                vec![],
            )?
            .into(),
        ])
    } else {
        Err(ContractError::AutoStakeError {})
    }
}

/// Return the amount of tokens that a specific amount of LP tokens would withdraw.
///
/// * **pools** assets available in the pool.
///
/// * **amount** amount of LP tokens to calculate underlying amounts for.
///
/// * **total_share** total amount of LP tokens currently issued by the pool.
pub(crate) fn get_share_in_assets(
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
pub(crate) fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    let max_spread = max_spread.map(Decimal256::from).unwrap_or(DEFAULT_SLIPPAGE);
    if max_spread > MAX_ALLOWED_SLIPPAGE {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    if let Some(belief_price) = belief_price {
        let expected_return = offer_amount
            * belief_price.inv().ok_or_else(|| {
                ContractError::Std(StdError::generic_err(
                    "Invalid belief_price. Check the input values.",
                ))
            })?;

        let spread_amount = expected_return.saturating_sub(return_amount);

        if return_amount < expected_return
            && Decimal256::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if Decimal256::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
        return Err(ContractError::MaxSpreadAssertion {});
    }

    Ok(())
}

/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
pub(crate) fn pool_info(
    querier: QuerierWrapper,
    config: &Config,
) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;
    let total_share = query_supply(&querier, &config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

/// Returns current pool's volumes where amount is in [`Decimal256`] form.
pub(crate) fn query_pools(
    querier: QuerierWrapper,
    addr: &Addr,
    config: &Config,
    precisions: &Precisions,
) -> Result<Vec<DecimalAsset>, ContractError> {
    config
        .pair_info
        .query_pools(&querier, addr)?
        .into_iter()
        .map(|asset| {
            asset
                .to_decimal_asset(precisions.get_precision(&asset.info)?)
                .map_err(Into::into)
        })
        .collect()
}

/// Checks whether it possible to make a swap or not.
pub(crate) fn before_swap_check(pools: &[DecimalAsset], offer_amount: Decimal256) -> StdResult<()> {
    if offer_amount.is_zero() {
        return Err(StdError::generic_err("Swap amount must not be zero"));
    }
    if pools.iter().any(|a| a.amount.is_zero()) {
        return Err(StdError::generic_err("One of the pools is empty"));
    }

    Ok(())
}

/// This structure is for internal use only. Represents swap's result.
pub struct SwapResult {
    pub dy: Decimal256,
    pub spread_fee: Decimal256,
    pub maker_fee: Decimal256,
    pub total_fee: Decimal256,
}

impl SwapResult {
    /// Calculates and returns **last price** where:
    /// - last_price is a price for repeg algo
    pub fn calc_last_prices(&self, offer_amount: Decimal256, offer_ind: usize) -> Decimal256 {
        if offer_ind == 0 {
            offer_amount / (self.dy + self.maker_fee)
        } else {
            (self.dy + self.maker_fee) / offer_amount
        }
    }
}

/// Performs swap simulation to calculate price.
pub fn calc_last_prices(xs: &[Decimal256], config: &Config, env: &Env) -> StdResult<Decimal256> {
    let mut offer_amount = Decimal256::one().min(xs[0] * OFFER_PERCENT);
    if offer_amount.is_zero() {
        offer_amount = Decimal256::raw(1u128);
    }

    let last_price = compute_swap(xs, offer_amount, 1, config, env, Decimal256::zero())?
        .calc_last_prices(offer_amount, 0);

    Ok(last_price)
}

/// Calculate swap result.
pub fn compute_swap(
    xs: &[Decimal256],
    offer_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    env: &Env,
    maker_fee_share: Decimal256,
) -> StdResult<SwapResult> {
    let offer_ind = 1 ^ ask_ind;

    let mut ixs = xs.to_vec();
    ixs[1] *= config.pool_state.price_state.price_scale;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let d = calc_d(&ixs, &amp_gamma)?;

    let offer_amount = if offer_ind == 1 {
        offer_amount * config.pool_state.price_state.price_scale
    } else {
        offer_amount
    };

    ixs[offer_ind] += offer_amount;

    let new_y = calc_y(&ixs, d, &amp_gamma, ask_ind)?;
    let mut dy = ixs[ask_ind] - new_y;
    ixs[ask_ind] = new_y;

    let price = if ask_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
        config.pool_state.price_state.price_scale.inv().unwrap()
    } else {
        config.pool_state.price_state.price_scale
    };

    // Since price_scale moves slower than real price spread fee may become negative
    let spread_fee = (offer_amount * price).saturating_sub(dy);

    let fee_rate = config.pool_params.fee(&ixs);
    let total_fee = fee_rate * dy;
    dy -= total_fee;

    Ok(SwapResult {
        dy,
        spread_fee,
        maker_fee: total_fee * maker_fee_share,
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

/// This is an internal function that enforces slippage tolerance for swaps.
pub fn assert_slippage_tolerance(
    old_price: Decimal256,
    new_price: Decimal256,
    slippage_tolerance: Option<Decimal>,
) -> Result<(), ContractError> {
    let slippage_tolerance = slippage_tolerance
        .map(Into::into)
        .unwrap_or(DEFAULT_SLIPPAGE);
    if slippage_tolerance > MAX_ALLOWED_SLIPPAGE {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    // Ensure price was not changed more than the slippage tolerance allows
    if Decimal256::one().diff(new_price / old_price) > slippage_tolerance {
        return Err(ContractError::MaxSpreadAssertion {});
    }

    Ok(())
}

// Checks whether the pair is registered in the factory or not.
pub fn check_pair_registered(
    querier: QuerierWrapper,
    factory: &Addr,
    asset_infos: &[AssetInfo],
) -> StdResult<bool> {
    astroport_factory::state::PAIRS
        .query(&querier, factory.clone(), &pair_key(asset_infos))
        .map(|inner| inner.is_some())
}

/// Internal function to calculate new moving average using Uint256.
/// Overflow is possible only if new average order size is greater than 2^128 - 1 which is unlikely.
fn safe_sma_calculation(
    sma: Uint128,
    oldest_amount: Uint128,
    count: u32,
    new_amount: Uint128,
) -> StdResult<Uint128> {
    let res = (sma.full_mul(count) + Uint256::from(new_amount) - Uint256::from(oldest_amount))
        .checked_div(count.into())?;
    res.try_into().map_err(StdError::from)
}

/// Calculate and save moving averages of swap sizes.
pub fn accumulate_swap_sizes(
    storage: &mut dyn Storage,
    env: &Env,
    base_amount: Uint128,
    quote_amount: Uint128,
) -> BufferResult<()> {
    let mut buffer = BufferManager::new(storage, OBSERVATIONS)?;

    let new_observation;
    if let Some(last_obs) = buffer.read_last(storage)? {
        // Since this is circular buffer the next index contains the oldest value
        let count = buffer.capacity();
        if let Some(oldest_obs) = buffer.read_single(storage, buffer.head() + 1)? {
            let new_base_sma = safe_sma_calculation(
                last_obs.base_sma,
                oldest_obs.base_amount,
                count,
                base_amount,
            )?;
            let new_quote_sma = safe_sma_calculation(
                last_obs.quote_sma,
                oldest_obs.quote_amount,
                count,
                quote_amount,
            )?;
            new_observation = Observation {
                base_amount,
                quote_amount,
                base_sma: new_base_sma,
                quote_sma: new_quote_sma,
                timestamp: env.block.time.seconds(),
            };
        } else {
            // Buffer is not full yet
            let count = Uint128::from(buffer.head());
            let new_base_sma = (last_obs.base_sma * count + base_amount) / (count + Uint128::one());
            let new_quote_sma =
                (last_obs.quote_sma * count + quote_amount) / (count + Uint128::one());
            new_observation = Observation {
                base_amount,
                quote_amount,
                base_sma: new_base_sma,
                quote_sma: new_quote_sma,
                timestamp: env.block.time.seconds(),
            };
        }
    } else {
        // Buffer is empty
        new_observation = Observation {
            timestamp: env.block.time.seconds(),
            base_sma: base_amount,
            base_amount,
            quote_sma: quote_amount,
            quote_amount,
        };
    }

    buffer.instant_push(storage, &new_observation)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmwasm_std::testing::{mock_env, MockStorage};
    use std::error::Error;
    use std::fmt::Display;
    use std::str::FromStr;

    pub fn f64_to_dec<T>(val: f64) -> T
    where
        T: FromStr,
        T::Err: Error,
    {
        T::from_str(&val.to_string()).unwrap()
    }

    pub fn dec_to_f64(val: impl Display) -> f64 {
        f64::from_str(&val.to_string()).unwrap()
    }

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

    #[test]
    fn test_swap_obeservations() {
        let mut store = MockStorage::new();
        let env = mock_env();

        BufferManager::init(&mut store, OBSERVATIONS, 10).unwrap();

        for _ in 0..50 {
            accumulate_swap_sizes(
                &mut store,
                &env,
                Uint128::from(1000u128),
                Uint128::from(500u128),
            )
            .unwrap();
        }

        let buffer = BufferManager::new(&store, OBSERVATIONS).unwrap();

        assert_eq!(buffer.head(), 0);
        assert_eq!(
            buffer.read_last(&store).unwrap().unwrap().base_sma.u128(),
            1000u128
        );
        assert_eq!(
            buffer.read_last(&store).unwrap().unwrap().quote_sma.u128(),
            500u128
        );
    }
}
