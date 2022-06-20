use std::cmp::Ordering;

use cosmwasm_std::{
    to_binary, wasm_execute, Addr, Api, CosmosMsg, Decimal, Env, QuerierWrapper, StdResult,
    Storage, Uint128, Uint64,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

use astroport::asset::{is_non_zero_liquidity, Asset, AssetInfo};
use astroport::querier::query_factory_config;

use crate::error::ContractError;
use crate::math::calc_y;
use crate::state::{get_precision, Config};

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

pub(crate) fn check_assets(api: &dyn Api, assets: &[Asset]) -> Result<(), ContractError> {
    let asset_infos = assets.iter().map(|asset| asset.info.clone()).collect_vec();
    check_asset_infos(api, &asset_infos)
}

pub(crate) fn check_cw20_in_pool(config: &Config, cw20_sender: &Addr) -> Result<(), ContractError> {
    for asset_info in &config.pair_info.asset_infos {
        match asset_info {
            AssetInfo::Token { contract_addr } if contract_addr == cw20_sender => return Ok(()),
            _ => {}
        }
    }

    Err(ContractError::Unauthorized {})
}

/// Returns: (offer_pool, ask_pool)
pub(crate) fn select_pools(
    offer_asset_info: Option<&AssetInfo>,
    ask_asset_info: Option<&AssetInfo>,
    pools: &[Asset],
) -> Result<(Asset, Asset), ContractError> {
    if pools.len() == 2 {
        match (offer_asset_info, ask_asset_info) {
            (Some(offer_asset_info), _) => {
                let (offer_ind, offer_pool) = pools
                    .iter()
                    .find_position(|pool| pool.info.eq(offer_asset_info))
                    .ok_or(ContractError::AssetMismatch {})?;
                Ok((
                    offer_pool.clone(),
                    pools[(offer_ind + 1) % pools.len()].clone(),
                ))
            }
            (_, Some(ask_asset_info)) => {
                let (ask_ind, ask_pool) = pools
                    .iter()
                    .find_position(|pool| pool.info.eq(ask_asset_info))
                    .ok_or(ContractError::AssetMismatch {})?;
                Ok((ask_pool.clone(), pools[(ask_ind + 1) % pools.len()].clone()))
            }
            _ => Err(ContractError::VariableAssetMissed {}), // Should always be unreachable
        }
    } else {
        if let (Some(offer_asset_info), Some(ask_asset_info)) = (offer_asset_info, ask_asset_info) {
            if ask_asset_info.eq(offer_asset_info) {
                return Err(ContractError::SameAssets {});
            }

            let offer_pool = pools
                .iter()
                .find(|pool| pool.info.eq(offer_asset_info))
                .ok_or(ContractError::AssetMismatch {})?;
            let ask_pool = pools
                .iter()
                .find(|pool| pool.info.eq(ask_asset_info))
                .ok_or(ContractError::AssetMismatch {})?;

            Ok((offer_pool.clone(), ask_pool.clone()))
        } else {
            Err(ContractError::VariableAssetMissed {}) // Should always be unreachable
        }
    }
}

/// ## Description
/// Compute the current pool amplification coefficient (AMP).
/// ## Params
/// * **config** is an object of type [`Config`].
///
/// * **env** is an object of type [`Env`].
pub(crate) fn compute_current_amp(config: &Config, env: &Env) -> StdResult<Uint64> {
    let block_time = env.block.time.seconds();
    if block_time < config.next_amp_time {
        let elapsed_time: Uint128 = block_time.saturating_sub(config.init_amp_time).into();
        let time_range = config
            .next_amp_time
            .saturating_sub(config.init_amp_time)
            .into();
        let init_amp = Uint128::from(config.init_amp);
        let next_amp = Uint128::from(config.next_amp);

        if next_amp > init_amp {
            let amp_range = next_amp - init_amp;
            let res = init_amp + (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.try_into()?)
        } else {
            let amp_range = init_amp - next_amp;
            let res = init_amp - (amp_range * elapsed_time).checked_div(time_range)?;
            Ok(res.try_into()?)
        }
    } else {
        Ok(Uint64::from(config.next_amp))
    }
}

/// ## Description
/// Return a value using a newly specified precision.
/// ## Params
/// * **value** is an object of type [`Uint128`]. This is the value that will have its precision adjusted.
///
/// * **current_precision** is an object of type [`u8`]. This is the `value`'s current precision
///
/// * **new_precision** is an object of type [`u8`]. This is the new precision to use when returning the `value`.
pub(crate) fn adjust_precision(
    value: Uint128,
    current_precision: u8,
    new_precision: u8,
) -> StdResult<Uint128> {
    Ok(match current_precision.cmp(&new_precision) {
        Ordering::Equal => value,
        Ordering::Less => value.checked_mul(Uint128::new(
            10_u128.pow((new_precision - current_precision) as u32),
        ))?,
        Ordering::Greater => value.checked_div(Uint128::new(
            10_u128.pow((current_precision - new_precision) as u32),
        ))?,
    })
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

/// Structure for internal use.
pub(crate) struct SwapResult {
    pub return_amount: Uint128,
    pub spread_amount: Uint128,
}

/// ## Description
/// Returns the result of a swap.
/// ## Params
/// * **storage** is an object of type [`Storage`].
///
/// * **offer_pool** is an object of type [`Uint128`]. This is the total amount of offer assets in the pool.
///
/// * **ask_pool** is an object of type [`Uint128`]. This is the total amount of ask assets in the pool.
///
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of offer assets to swap.
///
/// * **commission_rate** is an object of type [`Decimal`]. This is the total amount of fees charged for the swap.
///
/// * **amp** is an object of type [`Uint64`]. This is the pool amplification used to calculate the swap result.
pub(crate) fn compute_swap(
    storage: &dyn Storage,
    env: &Env,
    config: &Config,
    offer_asset: &Asset,
    offer_pool: &Asset,
    ask_pool: &Asset,
    pools: &[Asset],
) -> Result<SwapResult, ContractError> {
    // Check if the liquidity is non-zero
    is_non_zero_liquidity(offer_pool.amount, ask_pool.amount)?;

    let token_precision = get_precision(storage, &offer_asset.info)?;

    let new_ask_pool = calc_y(
        &offer_asset.info,
        &ask_pool.info,
        adjust_precision(
            offer_pool.amount.checked_add(offer_asset.amount)?,
            token_precision,
            config.greatest_precision,
        )?,
        &pools,
        compute_current_amp(&config, &env)?,
    )?;

    let token_precision = get_precision(storage, &ask_pool.info)?;
    let new_ask_pool = adjust_precision(new_ask_pool, config.greatest_precision, token_precision)?;
    let return_amount = ask_pool.amount.checked_sub(new_ask_pool)?;

    // // Get fee info from the factory
    // let fee_info = query_fee_info(
    //     &deps.querier,
    //     &config.factory_addr,
    //     config.pair_info.pair_type.clone(),
    // )?;
    //
    // let offer_amount = offer_asset.amount;
    //
    // let SwapResult {
    //     return_amount,
    //     spread_amount,
    //     commission_amount,
    // } = compute_swap(
    //     deps.querier,
    //     &offer_pool,
    //     &ask_pool,
    //     offer_asset.amount,
    //     fee_info.total_fee_rate,
    //     compute_current_amp(&config, &env)?,
    // )?;

    let spread_amount = offer_asset.amount.saturating_sub(return_amount);

    Ok(SwapResult {
        return_amount,
        spread_amount,
    })
}
