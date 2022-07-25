use std::cmp::Ordering;

use cosmwasm_std::{
    to_binary, wasm_execute, Addr, Api, CosmosMsg, Decimal, Env, QuerierWrapper, StdResult,
    Uint128, Uint256,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo};
use astroport::cosmwasm_ext::{AbsDiff, OneValue};
use astroport::querier::query_factory_config;

use crate::constants::{MULTIPLIER, NOISE_FEE, N_COINS, TWAP_PRECISION};
use crate::error::ContractError;
use crate::math::{newton_d, newton_y};
use crate::state::{Config, PoolParams};

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

/// ## Description
/// Returns the result of a swap in form of a [`SwapResult`] object. In case of error, returns [`ContractError`].
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
///
/// * **dx** is an offer amount.
///
/// * **offer_ind** is an index of offer pool.
///
/// * **ask_ind** is an index of ask pool.
///
/// * **xp** as a vector of [`Uint256`] which represents the amount in each pool.
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
    // TODO: cached D sometimes wrong
    // let d = config.pool_state.get_last_d(env, &old_xp)?;

    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &old_xp)?;
    let dy = xp[ask_ind] - newton_y(amp_gamma.ann(), amp_gamma.gamma(), &xp, d, ask_ind)?;

    Ok(dy)
}

/// ## Description
/// Accumulate token prices for the assets in the pool.
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
pub fn accumulate_prices(env: &Env, config: &mut Config) {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return;
    }

    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    let immut_config = config.clone();
    for (from, _, value) in config.cumulative_prices.iter_mut() {
        let price = if config.pair_info.asset_infos[0] == *from {
            MULTIPLIER * MULTIPLIER / immut_config.pool_state.price_state.last_prices
        } else {
            immut_config.pool_state.price_state.last_prices
        };
        // price max value = 1e24 which fits into Uint128 thus we use unwrap here
        let price: Uint128 = price
            .multiply_ratio(TWAP_PRECISION, MULTIPLIER)
            .try_into()
            .unwrap();
        // time_elapsed * price does not need checked_mul.
        // price max value = 1e24, u128 max value = 340282366920938463463374607431768211455
        // overflow is possible if time_elapsed > 340282366920939 seconds ~ 10790283 years
        *value = value.wrapping_add(time_elapsed * price);
    }

    config.block_time_last = block_time;
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

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_env;

    use astroport::asset::{native_asset_info, PairInfo};
    use astroport::factory::PairType;

    use crate::state::{AmpGamma, PoolState, PriceState};

    use super::*;

    #[test]
    fn test_accumulate_prices() {
        let mut env = mock_env();
        let asset_infos = vec![
            native_asset_info("test1".to_string()),
            native_asset_info("test2".to_string()),
        ];
        let mut config = Config {
            factory_addr: Addr::unchecked(""),
            block_time_last: env.block.time.seconds(),
            greatest_precision: 0,
            cumulative_prices: vec![
                (asset_infos[0].clone(), asset_infos[1].clone(), 0u8.into()),
                (asset_infos[1].clone(), asset_infos[0].clone(), 0u8.into()),
            ],
            pair_info: PairInfo {
                asset_infos,
                contract_addr: Addr::unchecked(""),
                liquidity_token: Addr::unchecked(""),
                pair_type: PairType::Concentrated {},
            },
            pool_params: Default::default(),
            pool_state: PoolState {
                initial: AmpGamma {
                    amp: Default::default(),
                    gamma: Default::default(),
                },
                future: AmpGamma {
                    amp: Default::default(),
                    gamma: Default::default(),
                },
                future_time: 0,
                initial_time: 0,
                price_state: PriceState {
                    price_oracle: Default::default(),
                    last_prices: MULTIPLIER,
                    price_scale: Default::default(),
                    last_price_update: 0,
                    xcp_profit: Default::default(),
                    virtual_price: Default::default(),
                    d: Default::default(),
                    not_adjusted: false,
                },
            },
        };

        env.block.time = env.block.time.plus_seconds(5000);
        accumulate_prices(&env, &mut config);
        assert_eq!(config.cumulative_prices[0].2.u128(), 10_000_000 * 5000);
        assert_eq!(config.cumulative_prices[1].2.u128(), 10_000_000 * 5000);

        config.pool_state.price_state.last_prices = MULTIPLIER.multiply_ratio(1u8, 2u8);
        env.block.time = env.block.time.plus_seconds(5000);
        accumulate_prices(&env, &mut config);
        assert_eq!(
            config.cumulative_prices[0].2.u128(),
            10_000_000 * 5000 + 2 * 10_000_000 * 5000
        );
        assert_eq!(
            config.cumulative_prices[1].2.u128(),
            10_000_000 * 5000 + 10_000_000 / 2 * 5000
        );
    }
}
