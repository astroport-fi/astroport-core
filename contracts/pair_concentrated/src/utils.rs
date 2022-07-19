use std::cmp::Ordering;

use cosmwasm_std::{
    to_binary, wasm_execute, Addr, Api, CosmosMsg, Decimal, Deps, Env, QuerierWrapper, StdError,
    StdResult, Storage, Uint128, Uint256, Uint64,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::cosmwasm_ext::{AbsDiff, OneValue};
use astroport::pair::TWAP_PRECISION;
use astroport::querier::{query_factory_config, query_fee_info};
use astroport::DecimalCheckedOps;

use crate::constants::{NOISE_FEE, N_COINS, PRECISION};
use crate::error::ContractError;
use crate::math::{newton_d, newton_y};
use crate::state::{get_precision, Config, PoolParams};

/// ## Description
/// Helper function to check if the given asset infos are valid.
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

/// ## Description
/// Helper function to check that the assets in a given array are valid.
pub(crate) fn check_assets(api: &dyn Api, assets: &[Asset]) -> Result<(), ContractError> {
    let asset_infos = assets.iter().map(|asset| asset.info.clone()).collect_vec();
    check_asset_infos(api, &asset_infos)
}

/// ## Description
/// Checks that cw20 token is part of the pool. Returns [`Ok(())`] in case of success,
/// otherwise [`ContractError`].
/// ## Params
/// * **config** is an object of type [`Config`].
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

/// ## Description
/// Select offer and ask pools based on given offer info.
/// Returns (offer_pool, ask_pool) in case of success.
/// If it is impossible to define offer and ask pools, returns [`ContractError`].
/// ## Params
/// * **offer_asset_info** - asset info of the offer asset.
///
/// * **pools** - list of pools.
pub(crate) fn select_pools(
    offer_asset_info: &AssetInfo,
    pools: &[Asset],
) -> Result<(Asset, Asset), ContractError> {
    let (offer_ind, _) = pools
        .iter()
        .find_position(|pool| pool.info.eq(offer_asset_info))
        .ok_or(ContractError::InvalidAsset(offer_asset_info.to_string()))?;
    Ok((pools[offer_ind].clone(), pools[1 - offer_ind].clone()))
}

/// ## Description
/// Returns a value using a newly specified precision.
/// ## Params
/// * **value** is an object of type [`Uint128`]. This is the value that will have its precision adjusted.
///
/// * **current_precision** is an object of type [`u8`]. This is the `value`'s current precision
///
/// * **new_precision** is an object of type [`u8`]. This is the new precision to use when returning the `value`.
pub(crate) fn adjust_precision(
    value: impl Into<Uint256>,
    current_precision: u8,
    new_precision: u8,
) -> StdResult<Uint256> {
    let value: Uint256 = value.into();
    let res = match current_precision.cmp(&new_precision) {
        Ordering::Equal => value,
        Ordering::Less => value.checked_mul(Uint256::from(
            10_u128.pow((new_precision - current_precision) as u32),
        ))?,
        Ordering::Greater => value.checked_div(Uint256::from(
            10_u128.pow((current_precision - new_precision) as u32),
        ))?,
    };

    Ok(res)
}

/// ## Description
/// Mint LP tokens for a beneficiary and auto stake the tokens in the Generator contract (if auto staking is specified).
/// # Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **config** is an object of type [`Config`].
///
/// * **contract_address** is an object of type [`Addr`].
///
/// * **recipient** is an object of type [`Addr`]. This is the LP token recipient.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** is the field of type [`bool`]. Determines whether the newly minted LP tokens will
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

/// ## Description
/// Return the amount of tokens that a specific amount of LP tokens would withdraw.
/// ## Params
/// * **pools** is an array of [`Asset`] type items. These are the assets available in the pool.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens to calculate underlying amounts for.
///
/// * **total_share** is an object of type [`Uint128`]. This is the total amount of LP tokens currently issued by the pool.
pub(crate) fn get_share_in_assets(
    pools: &[Asset],
    amount: Uint128,
    total_share: Uint128,
) -> Vec<Asset> {
    let mut share_ratio = Decimal::zero();
    if !total_share.is_zero() {
        share_ratio = Decimal::from_ratio(amount, total_share);
    }

    pools
        .iter()
        .map(|pool| Asset {
            info: pool.info.clone(),
            amount: pool.amount * share_ratio,
        })
        .collect()
}

/// Structure for internal use which represents swap result.
pub(crate) struct SwapResult {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
}

/// ## Description
/// Returns the result of a swap in form of a [`SwapResult`] object. In case of error, returns [`ContractError`].
/// ## Params
/// * **storage** is an object of type [`Storage`].
///
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset that is being offered.
///
/// * **offer_pool** is an object of type [`Uint128`]. This is the total amount of offer assets in the pool.
///
/// * **ask_pool** is an object of type [`Uint128`]. This is the total amount of ask assets in the pool.
///
/// * **pools** is an array of [`Asset`] type items. These are the assets available in the pool.
pub(crate) fn compute_swap(
    env: &Env,
    config: &Config,
    dx: Uint256,
    offer_ind: usize,
    ask_ind: usize,
    xp: &[Uint256],
) -> Result<Uint256, ContractError> {
    let xp = xp.to_vec();

    let mut old_xp = xp.clone();
    old_xp[offer_ind] -= dx;
    let d = config.pool_state.get_last_d(&env, &old_xp)?;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let dy = xp[ask_ind] - newton_y(amp_gamma.ann(), amp_gamma.gamma(), &xp, d, ask_ind)?;

    Ok(dy)
}

/// ## Description
/// Accumulate token prices for the assets in the pool.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
///
/// * **pools** is an array of [`Asset`] type items. These are the assets available in the pool.
pub fn accumulate_prices(
    deps: Deps,
    env: Env,
    config: &mut Config,
    pools: &[Asset],
) -> Result<(), ContractError> {
    // let block_time = env.block.time.seconds();
    // if block_time <= config.block_time_last {
    //     return Ok(());
    // }
    //
    // let greater_precision = config.greatest_precision.max(TWAP_PRECISION);
    //
    // let time_elapsed = Uint128::from(block_time - config.block_time_last);
    //
    // let immut_config = config.clone();
    // for (from, to, value) in config.cumulative_prices.iter_mut() {
    //     let offer_asset = from.with_balance(adjust_precision(
    //         Uint128::from(1u8),
    //         0u8,
    //         greater_precision,
    //     )?);
    //
    //     let (offer_pool, ask_pool) = select_pools(Some(from), Some(to), pools)?;
    //     let SwapResult { return_amount, .. } = compute_swap(
    //         deps.storage,
    //         &env,
    //         &immut_config,
    //         &offer_asset,
    //         &offer_pool,
    //         &ask_pool,
    //         pools,
    //     )?;
    //
    //     // Get fee info from factory
    //     let fee_info = query_fee_info(
    //         &deps.querier,
    //         &config.factory_addr,
    //         immut_config.pair_info.pair_type.clone(),
    //     )?;
    //
    //     let commission_amount = fee_info.total_fee_rate.checked_mul_uint128(return_amount)?;
    //     let return_amount = return_amount.saturating_sub(commission_amount);
    //
    //     *value = value.wrapping_add(time_elapsed.checked_mul(return_amount)?);
    // }
    //
    // config.block_time_last = block_time;
    //
    // Ok(())

    todo!()
}

pub(crate) fn calc_provide_fee(
    params: &PoolParams,
    provide_amounts: &[Uint256],
    xp: &[Uint256],
) -> StdResult<Uint256> {
    let fee = params.fee(xp) * N_COINS / (Uint256::from(4u8) * (N_COINS - Uint256::one()));
    let sum: Uint256 = provide_amounts.iter().sum();
    let avg = sum / N_COINS;
    let s_diff = provide_amounts
        .iter()
        .try_fold(Uint256::zero(), |acc, x| acc.checked_add(avg.diff(*x)))?;

    Ok(fee * s_diff / sum + NOISE_FEE)
}
