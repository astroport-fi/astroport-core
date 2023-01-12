use cosmwasm_std::{
    to_binary, wasm_execute, Addr, Api, CosmosMsg, Decimal, Decimal256, Env, Fraction,
    QuerierWrapper, StdError, StdResult, Uint128,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, Decimal256Ext, DecimalAsset};
use astroport::querier::{query_factory_config, query_supply};

use crate::consts::{DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, TWAP_PRECISION_DEC};
use crate::error::ContractError;
use crate::math::{calc_d, calc_y};
use crate::state::{Config, Precisions};

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
                        recipient.clone(),
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
) -> StdResult<Vec<DecimalAsset>> {
    let share_ratio = if !total_share.is_zero() {
        Decimal256::from_ratio(amount, total_share)
    } else {
        Decimal256::zero()
    };

    pools
        .iter()
        .map(|pool| {
            Ok(DecimalAsset {
                info: pool.info.clone(),
                amount: pool.amount * share_ratio,
            })
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
    offer_amount: Decimal256,
    return_amount: Decimal256,
    spread_amount: Decimal256,
) -> Result<(), ContractError> {
    let max_spread = max_spread.map(Decimal256::from).unwrap_or(DEFAULT_SLIPPAGE);
    if max_spread > MAX_ALLOWED_SLIPPAGE {
        return Err(ContractError::AllowedSpreadAssertion {});
    }

    if let Some(belief_price) = belief_price.map(Decimal256::from) {
        let expected_return = offer_amount
            * belief_price.inv().ok_or_else(|| {
                ContractError::Std(StdError::generic_err(
                    "Invalid belief_price. Check the input values.",
                ))
            })?;

        let spread_amount = expected_return.saturating_sub(return_amount);

        if return_amount < expected_return && spread_amount / expected_return > max_spread {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if spread_amount / (return_amount + spread_amount) > max_spread {
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

/// Returns current pool's volumes where amoint is in [`Decimal256`] form.
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

/// Calculate swap result.
pub fn compute_swap(
    xs: &[Decimal256],
    offer_amount: Decimal256,
    ask_ind: usize,
    config: &Config,
    env: &Env,
    maker_fee_share: Decimal256,
) -> StdResult<SwapResult> {
    let offer_ind = 1 - ask_ind;

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
    let offer_ind = 1 - ask_ind;

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

    let mut spread_fee = dy - before_fee;
    if offer_ind == 1 {
        dy /= config.pool_state.price_state.price_scale;
        spread_fee /= config.pool_state.price_state.price_scale;
        fee /= config.pool_state.price_state.price_scale;
    }

    Ok((dy, spread_fee, fee))
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

/// Calculates balanced share and excess tokens.
/// * **deposits** deposit values as they came in the contract (real values).
/// * **price_scale** x0 -> x1 exchange rate
///
/// Returns:
/// (
///     balanced share (internal representation),
///     array with excess tokens (real values)
/// )
pub fn balanced_deposits(
    deposits: &[Decimal256],
    price_scale: Decimal256,
) -> (Decimal256, Vec<Decimal256>) {
    let min_dep = deposits[0].min(deposits[1] * price_scale);

    (
        min_dep,
        vec![
            deposits[0] - min_dep,
            (deposits[1] * price_scale - min_dep) / price_scale,
        ],
    )
}