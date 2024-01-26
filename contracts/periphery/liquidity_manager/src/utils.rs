use std::collections::HashMap;

use cosmwasm_std::{
    from_json, Addr, Decimal, Decimal256, Env, QuerierWrapper, StdError, StdResult, Uint128,
};

use astroport::asset::{Asset, Decimal256Ext, DecimalAsset, PairInfo, MINIMUM_LIQUIDITY_AMOUNT};
use astroport::generator::QueryMsg as GeneratorQueryMsg;
use astroport::liquidity_manager::CompatPairStableConfig;
use astroport::querier::{query_supply, query_token_balance};
use astroport::U256;
use astroport_pair::{
    contract::assert_slippage_tolerance, error::ContractError as PairContractError,
};
use astroport_pair_stable::error::ContractError as StableContractError;
use astroport_pair_stable::math::compute_d;
use astroport_pair_stable::state::Config as PairStableConfig;
use astroport_pair_stable::utils::compute_current_amp;

pub fn query_lp_amount(
    querier: QuerierWrapper,
    lp_token_addr: String,
    factory_addr: Addr,
    staked_in_generator: bool,
    user: &String,
) -> StdResult<Uint128> {
    if staked_in_generator {
        let maybe_generator = astroport_factory::state::CONFIG
            .query(&querier, factory_addr)?
            .generator_address;
        if let Some(generator_addr) = maybe_generator {
            querier.query_wasm_smart(
                generator_addr,
                &GeneratorQueryMsg::Deposit {
                    lp_token: lp_token_addr,
                    user: user.to_string(),
                },
            )
        } else {
            Err(StdError::generic_err(
                "Generator address is not set in factory config",
            ))
        }
    } else {
        query_token_balance(&querier, lp_token_addr, user)
    }
}

pub fn query_cw20_minter(querier: QuerierWrapper, lp_token_addr: Addr) -> StdResult<Addr> {
    cw20_base::state::TOKEN_INFO
        .query(&querier, lp_token_addr.clone())?
        .mint
        .map(|info| info.minter)
        .ok_or_else(|| StdError::generic_err(format!("Minter for {lp_token_addr} is not set")))
}

pub fn xyk_provide_simulation(
    querier: QuerierWrapper,
    pool_balances: &[Asset],
    pair_info: &PairInfo,
    slippage_tolerance: Option<Decimal>,
    deposits: Vec<Asset>,
) -> Result<Uint128, PairContractError> {
    let deposits = [
        deposits
            .iter()
            .find(|a| a.info.equal(&pool_balances[0].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
        deposits
            .iter()
            .find(|a| a.info.equal(&pool_balances[1].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
    ];

    if deposits[0].is_zero() || deposits[1].is_zero() {
        return Err(StdError::generic_err("Wrong asset info is given").into());
    }

    let total_share = query_supply(&querier, &pair_info.liquidity_token)?;
    let share = if total_share.is_zero() {
        // Initial share = collateral amount
        let share = Uint128::new(
            (U256::from(deposits[0].u128()) * U256::from(deposits[1].u128()))
                .integer_sqrt()
                .as_u128(),
        )
        .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
        .map_err(|_| PairContractError::MinimumLiquidityAmountError {})?;

        // share cannot become zero after minimum liquidity subtraction
        if share.is_zero() {
            return Err(PairContractError::MinimumLiquidityAmountError {});
        }

        share
    } else {
        // Assert slippage tolerance
        assert_slippage_tolerance(slippage_tolerance, &deposits, pool_balances)?;

        // min(1, 2)
        // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_0))
        // == deposit_0 * total_share / pool_0
        // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
        // == deposit_1 * total_share / pool_1
        std::cmp::min(
            deposits[0].multiply_ratio(total_share, pool_balances[0].amount),
            deposits[1].multiply_ratio(total_share, pool_balances[1].amount),
        )
    };

    Ok(share)
}

pub fn stableswap_provide_simulation(
    querier: QuerierWrapper,
    env: Env,
    config: PairStableConfig,
    _slippage_tolerance: Option<Decimal>,
    deposits: Vec<Asset>,
) -> Result<Uint128, StableContractError> {
    if deposits.len() != config.pair_info.asset_infos.len() {
        return Err(StableContractError::InvalidNumberOfAssets(
            config.pair_info.asset_infos.len(),
        ));
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut non_zero_flag = false;

    let mut assets_collection = deposits
        .clone()
        .into_iter()
        .map(|asset| {
            // Check that at least one asset is non-zero
            if !asset.amount.is_zero() {
                non_zero_flag = true;
            }

            // Get appropriate pool
            let pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| StableContractError::InvalidAsset(asset.info.to_string()))?;

            Ok((asset, pool))
        })
        .collect::<Result<Vec<_>, StableContractError>>()?;

    // If some assets are omitted then add them explicitly with 0 deposit
    pools.iter().for_each(|(pool_info, pool_amount)| {
        if !deposits.iter().any(|asset| asset.info.eq(pool_info)) {
            assets_collection.push((
                Asset {
                    amount: Uint128::zero(),
                    info: pool_info.clone(),
                },
                *pool_amount,
            ));
        }
    });

    if !non_zero_flag {
        return Err(StableContractError::InvalidZeroAmount {});
    }

    for (deposit, pool) in assets_collection.iter_mut() {
        // We cannot put a zero amount into an empty pool.
        if deposit.amount.is_zero() && pool.is_zero() {
            return Err(StableContractError::InvalidProvideLPsWithSingleToken {});
        }
    }

    let assets_collection = assets_collection
        .iter()
        .cloned()
        .map(|(asset, pool)| {
            let coin_precision = astroport_pair_stable::state::PRECISIONS
                .query(
                    &querier,
                    config.pair_info.contract_addr.clone(),
                    asset.info.to_string(),
                )?
                .or_else(|| asset.info.decimals(&querier, &config.factory_addr).ok())
                .ok_or_else(|| {
                    StdError::generic_err(format!("Asset {asset} precision not found"))
                })?;
            Ok((
                asset.to_decimal_asset(coin_precision)?,
                Decimal256::with_precision(pool, coin_precision)?,
            ))
        })
        .collect::<StdResult<Vec<(DecimalAsset, Decimal256)>>>()?;

    let amp = compute_current_amp(&config, &env)?;

    // Invariant (D) after deposit added
    let new_balances = assets_collection
        .iter()
        .map(|(deposit, pool)| Ok(pool + deposit.amount))
        .collect::<StdResult<Vec<_>>>()?;
    let deposit_d = compute_d(amp, &new_balances)?;

    let total_share = query_supply(&querier, &config.pair_info.liquidity_token)?;
    let share = if total_share.is_zero() {
        let share = deposit_d
            .to_uint128_with_precision(config.greatest_precision)?
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
            .map_err(|_| StableContractError::MinimumLiquidityAmountError {})?;

        // share cannot become zero after minimum liquidity subtraction
        if share.is_zero() {
            return Err(StableContractError::MinimumLiquidityAmountError {});
        }

        share
    } else {
        // Initial invariant (D)
        let old_balances = assets_collection
            .iter()
            .map(|(_, pool)| *pool)
            .collect::<Vec<_>>();
        let init_d = compute_d(amp, &old_balances)?;

        let share = Decimal256::with_precision(total_share, config.greatest_precision)?
            .checked_multiply_ratio(deposit_d.saturating_sub(init_d), init_d)?
            .to_uint128_with_precision(config.greatest_precision)?;

        if share.is_zero() {
            return Err(StableContractError::LiquidityAmountTooSmall {});
        }

        share
    };

    Ok(share)
}

pub fn convert_config(
    querier: QuerierWrapper,
    config_data: Vec<u8>,
) -> StdResult<PairStableConfig> {
    let compat_config: CompatPairStableConfig = from_json(config_data)?;

    let greatest_precision = if let Some(prec) = compat_config.greatest_precision {
        prec
    } else {
        let mut greatest_precision = 0u8;
        for asset_info in &compat_config.pair_info.asset_infos {
            let precision = asset_info.decimals(&querier, &compat_config.factory_addr)?;
            greatest_precision = greatest_precision.max(precision);
        }
        greatest_precision
    };

    Ok(PairStableConfig {
        owner: compat_config.owner,
        pair_info: compat_config.pair_info,
        factory_addr: compat_config.factory_addr,
        block_time_last: compat_config.block_time_last,
        init_amp: compat_config.init_amp,
        init_amp_time: compat_config.init_amp_time,
        next_amp: compat_config.next_amp,
        next_amp_time: compat_config.next_amp_time,
        greatest_precision,
        fee_share: None,
    })
}
