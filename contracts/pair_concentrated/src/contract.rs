use std::collections::HashMap;
use std::str::FromStr;
use std::vec;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, from_binary, to_binary, wasm_execute, wasm_instantiate, Addr, Binary, CosmosMsg, Decimal,
    Deps, DepsMut, Env, Fraction, MessageInfo, QuerierWrapper, Reply, Response, StdError,
    StdResult, SubMsg, SubMsgResponse, SubMsgResult, Uint128, Uint256, WasmMsg,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use cw_utils::parse_instantiate_response_data;
use itertools::Itertools;

use astroport::asset::{
    addr_opt_validate, addr_validate_to_lower, check_swap_parameters, format_lp_token_name,
    token_asset, Asset, AssetInfo, PairInfo,
};
use astroport::cosmwasm_ext::{AbsDiff, OneValue};
use astroport::factory::PairType;
use astroport::pair::{
    migration_check, ConfigResponse, InstantiateMsg, StablePoolUpdateParams, DEFAULT_SLIPPAGE,
    MAX_ALLOWED_SLIPPAGE,
};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolResponse, QueryMsg,
    ReverseSimulationResponse, SimulationResponse, StablePoolConfig,
};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, UpdatePoolParams,
};
use astroport::querier::{
    query_factory_config, query_fee_info, query_supply, query_token_precision,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport::DecimalCheckedOps;

use crate::constants::{FEE_DENOMINATOR, MULTIPLIER, N_COINS, PRECISION};
use crate::error::ContractError;
use crate::math::{geometric_mean, newton_d, update_price};
use crate::state::{
    get_precision, store_precisions, AmpGamma, Config, PoolParams, PoolState, PriceState, CONFIG,
};
use crate::utils::{
    accumulate_prices, adjust_precision, calc_provide_fee, check_asset_infos, check_assets,
    check_cw20_in_pool, compute_swap, get_share_in_assets, mint_liquidity_token_message,
    select_pools, SwapResult,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-pair-stable";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID of sub-message.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;
/// An LP token precision.
const LP_TOKEN_PRECISION: u8 = 6;

/// ## Description
/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is a message of type [`InstantiateMsg`] which contains the parameters for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    check_asset_infos(deps.api, &msg.asset_infos)?;

    if msg.asset_infos.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }

    if msg.init_params.is_none() {
        return Err(ContractError::InitParamsNotFound {});
    }

    let params: ConcentratedPoolParams = from_binary(&msg.init_params.unwrap())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let greatest_precision = store_precisions(deps.branch(), &msg.asset_infos)?;

    // Initializing cumulative prices
    let mut cumulative_prices = vec![];
    for from_pool in &msg.asset_infos {
        for to_pool in &msg.asset_infos {
            if !from_pool.eq(to_pool) {
                cumulative_prices.push((from_pool.clone(), to_pool.clone(), Uint128::zero()))
            }
        }
    }

    let mut pool_params = PoolParams::default();
    pool_params.update_params(UpdatePoolParams {
        mid_fee: Some(params.mid_fee),
        out_fee: Some(params.out_fee),
        fee_gamma: Some(params.fee_gamma),
        allowed_extra_profit: Some(params.allowed_extra_profit),
        adjustment_step: Some(params.adjustment_step),
        ma_half_time: Some(params.ma_half_time),
    })?;

    let pool_state = PoolState {
        initial: AmpGamma {
            amp: Default::default(),
            gamma: Default::default(),
        },
        future: AmpGamma::new(params.amp, params.gamma)?,
        future_time: env.block.time.seconds(),
        initial_time: 0,
        price_state: PriceState {
            price_oracle: MULTIPLIER,
            last_prices: MULTIPLIER,
            price_scale: MULTIPLIER,
            last_price_update: env.block.time.seconds(),
            xcp_profit: MULTIPLIER,
            virtual_price: MULTIPLIER,
            d: Default::default(),
            not_adjusted: false,
        },
    };

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Concentrated {},
        },
        factory_addr: addr_validate_to_lower(deps.api, msg.factory_addr)?,
        block_time_last: env.block.time.seconds(),
        greatest_precision,
        cumulative_prices,
        pool_params,
        pool_state,
    };

    CONFIG.save(deps.storage, &config)?;

    let token_name = format_lp_token_name(&msg.asset_infos, &deps.querier)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        wasm_instantiate(
            msg.token_code_id,
            &TokenInstantiateMsg {
                name: token_name,
                symbol: "uLP".to_string(),
                decimals: LP_TOKEN_PRECISION,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                marketing: None,
            },
            vec![],
            String::from("Astroport LP token"),
        )?,
        INSTANTIATE_TOKEN_REPLY_ID,
    );

    Ok(Response::new().add_submessage(sub_msg))
}

/// ## Description
/// The entry point to the contract for processing replies from submessages.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`Reply`]. This is the reply from the submessage.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: INSTANTIATE_TOKEN_REPLY_ID,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let mut config = CONFIG.load(deps.storage)?;

            if config.pair_info.liquidity_token != Addr::unchecked("") {
                return Err(ContractError::Unauthorized {});
            }

            let init_response = parse_instantiate_response_data(data.as_slice())
                .map_err(|e| StdError::generic_err(format!("{e}")))?;
            config.pair_info.liquidity_token =
                addr_validate_to_lower(deps.api, &init_response.contract_address)?;
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::new()
                .add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
        }
        _ => Err(ContractError::FailedToParseReply {}),
    }
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Updates the contract configuration with the specified
/// input parameters.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ProvideLiquidity {
///             assets,
///             slippage_tolerance,
///             auto_stake,
///             receiver,
///         }** Provides liquidity in the pair using the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs an swap using the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    if migration_check(deps.querier, &cfg.factory_addr, &env.contract.address)? {
        return Err(ContractError::PairIsNotMigrated {});
    }

    match msg {
        ExecuteMsg::UpdateConfig { params } => update_config(deps, env, info, params),
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets,
            auto_stake,
            receiver,
            ..
        } => provide_liquidity(deps, env, info, assets, auto_stake, receiver),
        ExecuteMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
            ..
        } => {
            offer_asset.info.check(deps.api)?;
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }
            offer_asset.assert_sent_native_token_balance(&info)?;

            let to_addr = addr_opt_validate(deps.api, &to)?;

            swap(
                deps,
                env,
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If no template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`]. This is the CW20 receive message to process.
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
            ..
        } => {
            let config = CONFIG.load(deps.storage)?;

            // Only asset contract can execute this message
            check_cw20_in_pool(&config, &info.sender)?;

            let to_addr = addr_opt_validate(deps.api, &to)?;
            let sender = addr_validate_to_lower(deps.api, cw20_msg.sender)?;
            swap(
                deps,
                env,
                sender,
                token_asset(info.sender, cw20_msg.amount),
                belief_price,
                max_spread,
                to_addr,
            )
        }
        Cw20HookMsg::WithdrawLiquidity { assets } => {
            let sender = addr_validate_to_lower(deps.api, cw20_msg.sender)?;
            withdraw_liquidity(deps, env, info, sender, cw20_msg.amount, assets)
        }
    }
}

/// ## Description
/// Provides liquidity with the specified input parameters.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the
/// specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **assets** is an array with two objects of type [`Asset`]. These are the assets available in the pool.
///
/// * **auto_stake** is object of type [`Option<bool>`]. Determines whether the resulting LP tokens are automatically staked in
/// the Generator contract to receive token incentives.
///
/// * **receiver** is object of type [`Option<String>`]. This is the address that receives LP tokens.
/// If this address isn't specified, the function will default to the caller.
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    check_assets(deps.api, &assets)?;

    let mut config = CONFIG.load(deps.storage)?;

    if assets.len() > config.pair_info.asset_infos.len() {
        return Err(ContractError::InvalidNumberOfAssets(
            config.pair_info.asset_infos.len(),
        ));
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut non_zero_flag = false;

    let mut assets_collection = assets
        .iter()
        .cloned()
        .map(|asset| {
            asset.assert_sent_native_token_balance(&info)?;

            // Check that at least one asset is non-zero
            if !asset.amount.is_zero() {
                non_zero_flag = true;
            }

            // Get appropriate pool
            let pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

            Ok((asset, pool))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

    if !non_zero_flag {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // If some assets are omitted then add them explicitly with 0 deposit
    pools.iter().for_each(|(pool_info, pool_amount)| {
        if !assets.iter().any(|asset| asset.info.eq(pool_info)) {
            assets_collection.push((
                Asset {
                    amount: Uint128::zero(),
                    info: pool_info.clone(),
                },
                *pool_amount,
            ));
        }
    });

    let mut messages = vec![];
    let assets_collection = assets_collection
        .into_iter()
        .map(|(deposit, mut pool_amount)| {
            // We cannot put a zero amount into an empty pool.
            if deposit.amount.is_zero() && pool_amount.is_zero() {
                return Err(ContractError::InvalidProvideLPsWithSingleToken {});
            }

            // Transfer only non-zero amount
            if !deposit.amount.is_zero() {
                // If the pool is a token contract, then we need to execute a TransferFrom msg to receive funds
                if let AssetInfo::Token { contract_addr } = &deposit.info {
                    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                        contract_addr: contract_addr.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                            owner: info.sender.to_string(),
                            recipient: env.contract.address.to_string(),
                            amount: deposit.amount,
                        })?,
                        funds: vec![],
                    }))
                } else {
                    // If the asset is a native token, the pool balance already increased
                    // To calculate the pool balance properly, we should subtract the user deposit from the recorded pool token amount
                    pool_amount = pool_amount.checked_sub(deposit.amount)?;
                }
            }

            // Adjusting to the greatest precision
            let coin_precision = get_precision(deps.storage, &deposit.info)?;
            let deposit_amount =
                adjust_precision(deposit.amount, coin_precision, config.greatest_precision)?;
            let pool_amount =
                adjust_precision(pool_amount, coin_precision, config.greatest_precision)?;

            Ok((deposit_amount, pool_amount))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

    let mut xp = assets_collection
        .iter()
        .map(|(deposit, pool_amount)| Ok(deposit.checked_add(*pool_amount)?))
        .collect::<StdResult<Vec<_>>>()?;
    let xp_for_prices = xp.clone();
    let mut xp_old = assets_collection
        .iter()
        .map(|(_, pool_amount)| *pool_amount)
        .collect_vec();

    // We convert 2nd pool amount into the 1st asset
    xp[1] = xp[1] * config.pool_state.price_state.price_scale / PRECISION;
    xp_old[1] = xp_old[1] * config.pool_state.price_state.price_scale / PRECISION;

    let old_d = config.pool_state.get_last_d(&env, &xp_old)?;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let new_d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &xp)?;

    let total_share: Uint256 =
        query_supply(&deps.querier, &config.pair_info.liquidity_token)?.into();

    let mut mint_amount = if !old_d.is_zero() {
        (total_share * new_d / old_d).saturating_sub(total_share)
    } else {
        let tmp_xp = [
            new_d / N_COINS,
            new_d * PRECISION / (config.pool_state.price_state.price_scale * N_COINS),
        ];
        geometric_mean(&tmp_xp)
    };

    let deposits = assets_collection
        .iter()
        .map(|(deposit, _)| *deposit)
        .collect_vec();
    if !old_d.is_zero() {
        let provide_fee = calc_provide_fee(&config.pool_params, &deposits, &xp)? * mint_amount
            / FEE_DENOMINATOR
            + Uint256::one();
        mint_amount -= provide_fee;

        let mut price = Uint256::zero();

        // TODO: not sure we need this check
        if mint_amount > Uint256::from_u128(1e5 as u128) {
            // TODO: I believe here we need to check that the deposits are imbalanced, not just one of them is zero.
            if deposits[0].is_zero() || deposits[1].is_zero() {
                // How much the user spent to receive share in pool which he didn't deposit in
                // share in X / provide fees denominated in Y
                let covered_share_ind = if deposits[0].is_zero() { 1 } else { 0 };
                let uncovered_share =
                    xp_for_prices[1 - covered_share_ind].multiply_ratio(mint_amount, total_share);
                let provide_fees = deposits[covered_share_ind]
                    - xp_for_prices[covered_share_ind].multiply_ratio(mint_amount, total_share);
                price = uncovered_share / provide_fees;
                // Invert the price if the covered share is the first pool
                if covered_share_ind == 0 {
                    price = MULTIPLIER * MULTIPLIER / price;
                }
            }
        }

        update_price(
            &mut config.pool_state,
            &env,
            xp,
            price,
            new_d,
            &config.pool_params,
            total_share.into(),
        )?;
    } else {
        config.pool_state.price_state.d = new_d;
    }

    let mut mint_amount: Uint128 =
        adjust_precision(mint_amount, config.greatest_precision, LP_TOKEN_PRECISION)?.try_into()?;

    if mint_amount.is_zero() {
        return Err(ContractError::LiquidityAmountTooSmall {});
    }

    // TODO: assert slippage?

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        mint_amount,
        auto_stake.unwrap_or(false),
    )?);

    accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", assets.iter().join(", ")),
        attr("share", mint_amount),
    ]))
}

/// ## Description
/// Withdraw liquidity from the pool. Returns a [`ContractError`] on failure,
/// otherwise returns a [`Response`] with the specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **sender** is an object of type [`Addr`]. This is the address that will receive the withdrawn liquidity.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of provided LP tokens to withdraw liquidity with.
///
/// * **assets** is an optional array of type [`Vec<Asset>`]. It specifies the assets amount to withdraw.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
    assets: Vec<Asset>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let burn_amount;
    let refund_assets;
    let mut messages = vec![];

    let (pools, total_share) = pool_info(deps.querier, &config)?;
    if assets.is_empty() {
        // Usual withdraw (balanced)
        burn_amount = amount;
        refund_assets = get_share_in_assets(&pools, amount, total_share);
    } else {
        // Imbalanced withdraw
        burn_amount = imbalanced_withdraw(
            deps.as_ref(),
            &env,
            &mut config,
            amount,
            &assets,
            total_share,
        )?;
        if burn_amount < amount {
            // Returning unused LP tokens back to the user
            messages.push(
                wasm_execute(
                    &config.pair_info.liquidity_token,
                    &Cw20ExecuteMsg::Transfer {
                        recipient: sender.to_string(),
                        amount: amount - burn_amount,
                    },
                    vec![],
                )?
                .into(),
            )
        }
        refund_assets = assets;
    }

    // Reducing cached D invariant
    let d = config.pool_state.price_state.d;
    config.pool_state.price_state.d = d.saturating_sub(d.multiply_ratio(burn_amount, total_share));

    messages.extend(
        refund_assets
            .clone()
            .into_iter()
            .map(|asset| asset.into_msg(&deps.querier, &sender))
            .collect::<StdResult<Vec<_>>>()?,
    );
    messages.push(
        wasm_execute(
            &config.pair_info.liquidity_token,
            &Cw20ExecuteMsg::Burn {
                amount: burn_amount,
            },
            vec![],
        )?
        .into(),
    );

    accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}

/// ## Description
/// Imbalanced withdraw liquidity from the pool. Returns a [`ContractError`] on failure,
/// otherwise returns the number of LP tokens to burn.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
///
/// * **provided_amount** is an object of type [`Uint128`]. This is the amount of provided LP tokens to withdraw liquidity with.
///
/// * **assets** is array with objects of type [`Asset`]. It specifies the assets amount to withdraw.
fn imbalanced_withdraw(
    deps: Deps,
    env: &Env,
    config: &mut Config,
    provided_amount: Uint128,
    assets: &[Asset],
    total_lp: impl Into<Uint256>,
) -> Result<Uint128, ContractError> {
    check_assets(deps.api, assets)?;

    let n_coins = config.pair_info.asset_infos.len();
    if assets.len() > n_coins {
        return Err(ContractError::InvalidNumberOfAssets(
            config.pair_info.asset_infos.len(),
        ));
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut assets_collection = assets
        .iter()
        .cloned()
        .map(|asset| {
            // Get appropriate pool
            let mut pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

            // Adjusting to the greatest precision
            let coin_precision = get_precision(deps.storage, &asset.info)?;
            let pool = adjust_precision(pool, coin_precision, config.greatest_precision)?;

            Ok((asset, pool))
        })
        .collect::<Result<Vec<_>, ContractError>>()?;

    // If some assets are omitted then add them explicitly with 0 withdraw amount
    pools.into_iter().for_each(|(pool_info, pool_amount)| {
        if !assets.iter().any(|asset| asset.info == pool_info) {
            assets_collection.push((
                Asset {
                    amount: Uint128::zero(),
                    info: pool_info,
                },
                pool_amount.into(),
            ));
        }
    });

    // Initial invariant (D)
    let old_balances = assets_collection
        .iter()
        .map(|(_, pool)| *pool)
        .collect_vec();
    let amp_gamma = config.pool_state.get_amp_gamma(env);
    let init_d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &old_balances)?;

    // Invariant (D) after assets withdrawn
    let mut new_balances = assets_collection
        .iter()
        .map(|(withdraw, pool)| Ok(pool.checked_sub(withdraw.amount.into())?))
        .collect::<StdResult<Vec<_>>>()?;
    let withdraw_d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &new_balances)?;

    let fee = config.pool_params.fee(&old_balances);

    for i in 0..n_coins as usize {
        let ideal_balance = withdraw_d.checked_multiply_ratio(old_balances[i], init_d)?;
        let diff = ideal_balance.diff(new_balances[i]);
        new_balances[i] = new_balances[i].checked_sub(fee * diff / FEE_DENOMINATOR)?;
    }

    let after_fee_d = newton_d(amp_gamma.ann(), amp_gamma.gamma(), &new_balances)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    // How many tokens do we need to burn to withdraw asked assets?
    let diff_d = init_d - after_fee_d;
    let burn_amount = Uint256::from(total_share)
        .checked_multiply_ratio(diff_d, init_d)?
        .checked_add(Uint256::one())?; // In case of rounding errors - make it unfavorable for the "attacker"

    let burn_amount = adjust_precision(burn_amount, config.greatest_precision, LP_TOKEN_PRECISION)?;

    if burn_amount > Uint256::from(provided_amount) {
        return Err(StdError::generic_err(format!(
            "Not enough LP tokens. You need {} LP tokens.",
            burn_amount
        ))
        .into());
    }

    // What if a user would do a balanced withdraw instead?
    let balanced_withdraw = old_balances
        .iter()
        .map(|old_bal| old_bal - old_bal.multiply_ratio(diff_d, total_share))
        .collect_vec();
    let withdraw_amounts = assets_collection
        .iter()
        .map(|(withdraw, _)| Uint256::from(withdraw.amount))
        .collect_vec();
    // Calculate how much a user spent to make imbalanced withdraw thus calculate the price
    let dx = balanced_withdraw[0].diff(withdraw_amounts[0]);
    let dy =
        balanced_withdraw[1].diff(withdraw_amounts[1]) * config.pool_state.price_state.price_scale;
    let price = dy * MULTIPLIER / dx;

    update_price(
        &mut config.pool_state,
        &env,
        new_balances.clone(),
        price,
        after_fee_d,
        &config.pool_params,
        total_lp.into(),
    )?;

    Ok(burn_amount.try_into()?)
}

/// ## Description
/// Performs a swap with the specified parameters.
/// Returns a [`ContractError`] on failure, otherwise returns a [`Response`] with the
/// specified attributes if the operation was successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **sender** is an object of type [`Addr`]. This is the default recipient of the swap operation.
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset to swap.
///
/// * **ask_asset_info** is an object of type [`Option<AssetInfo>`]. It must contain ask asset info always if pool has > 2 assets.
///
/// * **belief_price** is an object of type [`Option<Decimal>`]. This is used to calculate the maximum spread.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. This is the maximum spread allowed for the swap.
///
/// * **to** is an object of type [`Option<Addr>`]. This is the address that receives ask tokens.
/// NOTE - the address that wants to swap should approve the pair contract to pull the offer token.
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // Offer pool balance already increased and this is good.
    let mut pools = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?;

    let (offer_ind, _) = pools
        .iter()
        .find_position(|pool| pool.info.eq(&offer_asset.info))
        .ok_or(ContractError::InvalidAsset(offer_asset.info.to_string()))?;
    let ask_ind = 1 - offer_ind;

    check_swap_parameters(
        pools[offer_ind].amount.checked_sub(offer_asset.amount)?,
        pools[ask_ind].amount,
        offer_asset.amount,
    )?;

    // Converting according to token precisions and price_scale
    let mut xp = pools
        .iter()
        .map(|pool| {
            let precision = get_precision(deps.storage, &pool.info)?;
            let adjusted = adjust_precision(pool.amount, precision, config.greatest_precision)?;
            Ok(adjusted)
        })
        .collect::<StdResult<Vec<_>>>()?;
    xp[1] *= config.pool_state.price_state.price_scale / PRECISION;

    let precision = get_precision(deps.storage, &offer_asset.info)?;
    let mut dx = adjust_precision(offer_asset.amount, precision, config.greatest_precision)?.into();
    if offer_ind == 0 {
        dx *= config.pool_state.price_state.price_scale / PRECISION;
    }
    let mut return_amount = compute_swap(&env, &config, dx, offer_ind, ask_ind, &xp)?;

    xp[ask_ind] -= return_amount;
    return_amount -= Uint256::one(); // Reduce by 1 just for safety reasons.
    let mut commission_amount = config.pool_params.fee(&xp) * return_amount / MULTIPLIER;
    xp[ask_ind] += commission_amount;
    let mut spread_amount = dx.saturating_sub(return_amount);
    return_amount = return_amount.saturating_sub(commission_amount);

    if ask_ind > 0 {
        return_amount = return_amount
            .checked_multiply_ratio(PRECISION, config.pool_state.price_state.price_scale)?;
        commission_amount = commission_amount
            .checked_multiply_ratio(PRECISION, config.pool_state.price_state.price_scale)?;
        spread_amount = spread_amount
            .checked_multiply_ratio(PRECISION, config.pool_state.price_state.price_scale)?;
    }
    let new_price = if offer_ind == 0 {
        Uint256::from(offer_asset.amount) * MULTIPLIER / return_amount
    } else {
        return_amount * MULTIPLIER / Uint256::from(offer_asset.amount)
    };

    let total_lp = query_supply(&deps.querier, &config.pair_info.liquidity_token)?.into();
    update_price(
        &mut config.pool_state,
        &env,
        xp.clone(),
        new_price,
        Uint256::zero(),
        &config.pool_params,
        total_lp,
    )?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let receiver = to.unwrap_or_else(|| sender.clone());

    // Resolving precisions
    let ask_precision = get_precision(&*deps.storage, &pools[ask_ind].info)?;
    let return_amount =
        adjust_precision(return_amount, config.greatest_precision, ask_precision)?.try_into()?;
    let commission_amount =
        adjust_precision(commission_amount, config.greatest_precision, ask_precision)?
            .try_into()?;
    let spread_amount: Uint128 =
        adjust_precision(spread_amount, config.greatest_precision, ask_precision)?.try_into()?;

    // Check the max spread limit (if it was specified)
    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset.amount,
        return_amount,
        spread_amount + commission_amount,
    )?;

    let mut messages = vec![Asset {
        info: pools[ask_ind].info.clone(),
        amount: return_amount,
    }
    .into_msg(&deps.querier, &receiver)?];

    // Compute the Maker fee
    let mut maker_fee_amount = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        if let Some(f) = calculate_maker_fee(
            &pools[ask_ind].info,
            commission_amount,
            fee_info.maker_fee_rate,
        ) {
            maker_fee_amount = f.amount;
            messages.push(f.into_msg(&deps.querier, fee_address)?);
        }
    }

    accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_messages(
            // 1. send collateral tokens from the contract to a user
            // 2. send inactive commission fees to the Maker contract
            messages,
        )
        .add_attributes(vec![
            attr("action", "swap"),
            attr("sender", sender),
            attr("receiver", receiver),
            attr("offer_asset", offer_asset.info.to_string()),
            attr("ask_asset", pools[ask_ind].info.to_string()),
            attr("offer_amount", offer_asset.amount),
            attr("return_amount", return_amount),
            attr("spread_amount", spread_amount),
            attr("commission_amount", commission_amount),
            attr("maker_fee_amount", maker_fee_amount),
        ]))
}

/// ## Description
/// Calculates the amount of fees the Maker contract gets according to specified pair parameters.
/// Returns a [`None`] if the Maker fee is zero, otherwise returns a [`Asset`] struct with the specified attributes.
/// ## Params
/// * **pool_info** is an object of type [`AssetInfo`]. Contains information about the pool asset for which the commission will be calculated.
///
/// * **commission_amount** is an object of type [`Env`]. This is the total amount of fees charged for a swap.
///
/// * **maker_commission_rate** is an object of type [`MessageInfo`]. This is the percentage of fees that go to the Maker contract.
pub fn calculate_maker_fee(
    pool_info: &AssetInfo,
    commission_amount: Uint128,
    maker_commission_rate: Decimal,
) -> Option<Asset> {
    let maker_fee = commission_amount * maker_commission_rate;
    if maker_fee.is_zero() {
        None
    } else {
        Some(Asset {
            info: pool_info.clone(),
            amount: maker_fee,
        })
    }
}

/// ## Description
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Pair {}** Returns information about the pair in an object of type [`PairInfo`].
///
/// * **QueryMsg::Pool {}** Returns information about the amount of assets in the pair contract as
/// well as the amount of LP tokens issued using an object of type [`PoolResponse`].
///
/// * **QueryMsg::Share { amount }** Returns the amount of assets that could be withdrawn from the pool
/// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
///
/// * **QueryMsg::Simulation { offer_asset }** Returns the result of a swap simulation using a [`SimulationResponse`] object.
///
/// * **QueryMsg::ReverseSimulation { ask_asset }** Returns the result of a reverse swap simulation using
/// a [`ReverseSimulationResponse`] object.
///
/// * **QueryMsg::CumulativePrices {}** Returns information about cumulative prices for the assets in the
/// pool using a [`CumulativePricesResponse`] object.
///
/// * **QueryMsg::Config {}** Returns the configuration for the pair contract using a [`ConfigResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&CONFIG.load(deps.storage)?.pair_info),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation {
            offer_asset,
            ask_asset_info,
        } => to_binary(
            &query_simulation(deps, env, offer_asset, ask_asset_info)
                .map_err(|err| StdError::generic_err(format!("{err}")))?,
        ),
        QueryMsg::ReverseSimulation {
            offer_asset_info,
            ask_asset,
        } => to_binary(
            &query_reverse_simulation(deps, env, ask_asset, offer_asset_info)
                .map_err(|err| StdError::generic_err(format!("{err}")))?,
        ),
        QueryMsg::CumulativePrices {} => to_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::QueryComputeD {} => to_binary(&query_compute_d(deps, env)?),
    }
}

/// ## Description
/// Returns the amounts of assets in the pair contract as well as the amount of LP
/// tokens currently minted in an object of type [`PoolResponse`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

/// ## Description
/// Returns the amount of assets that could be withdrawn from the pool using a specific amount of LP tokens.
/// The result is returned in a vector that contains objects of type [`Asset`].
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of LP tokens for which we calculate associated amounts of assets.
pub fn query_share(deps: Deps, amount: Uint128) -> StdResult<Vec<Asset>> {
    let config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(deps.querier, &config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(refund_assets)
}

/// ## Description
/// Returns information about a swap simulation in a [`SimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **offer_asset** is an object of type [`Asset`]. This is the asset to swap as well as an amount of the said asset.
pub fn query_simulation(
    deps: Deps,
    env: Env,
    offer_asset: Asset,
    ask_asset_info: Option<AssetInfo>,
) -> Result<SimulationResponse, ContractError> {
    // let config = CONFIG.load(deps.storage)?;
    // let pools = config
    //     .pair_info
    //     .query_pools(&deps.querier, &config.pair_info.contract_addr)?
    //     .into_iter()
    //     .map(|pool| {
    //         let token_precision = get_precision(deps.storage, &pool.info)?;
    //         Ok(Asset {
    //             amount: adjust_precision(pool.amount, token_precision, config.greatest_precision)?,
    //             ..pool
    //         })
    //     })
    //     .collect::<StdResult<Vec<_>>>()?;
    //
    // let (offer_pool, ask_pool) =
    //     select_pools(Some(&offer_asset.info), ask_asset_info.as_ref(), &pools)?;
    //
    // if check_swap_parameters(offer_pool.amount, ask_pool.amount, offer_asset.amount).is_err() {
    //     return Ok(SimulationResponse {
    //         return_amount: Uint128::zero(),
    //         spread_amount: Uint128::zero(),
    //         commission_amount: Uint128::zero(),
    //     });
    // }
    //
    // let SwapResult {
    //     return_amount,
    //     spread_amount,
    // } = compute_swap(
    //     deps.storage,
    //     &env,
    //     &config,
    //     &offer_asset,
    //     &offer_pool,
    //     &ask_pool,
    //     &pools,
    // )?;
    //
    // // Get fee info from factory
    // let fee_info = query_fee_info(
    //     &deps.querier,
    //     &config.factory_addr,
    //     config.pair_info.pair_type.clone(),
    // )?;
    //
    // let commission_amount = fee_info.total_fee_rate.checked_mul_uint128(return_amount)?;
    // let return_amount = return_amount.saturating_sub(commission_amount);
    //
    // Ok(SimulationResponse {
    //     return_amount,
    //     spread_amount,
    //     commission_amount,
    // })

    todo!()
}

/// ## Description
/// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **ask_asset** is an object of type [`Asset`]. This is the asset to swap to as well as the desired
/// amount of ask assets to receive from the swap.
///
/// * **offer_asset_info** is an object of type [`Option<AssetInfo>`]. This is optional field which specifies the asset to swap from.
/// May be omitted only in case the pool length is 2.
pub fn query_reverse_simulation(
    deps: Deps,
    env: Env,
    ask_asset: Asset,
    offer_asset_info: Option<AssetInfo>,
) -> Result<ReverseSimulationResponse, ContractError> {
    // let config = CONFIG.load(deps.storage)?;
    // let pools = config
    //     .pair_info
    //     .query_pools(&deps.querier, &config.pair_info.contract_addr)?
    //     .into_iter()
    //     .map(|pool| {
    //         let token_precision = query_token_precision(&deps.querier, &pool.info)?;
    //         Ok(Asset {
    //             amount: adjust_precision(pool.amount, token_precision, config.greatest_precision)?,
    //             ..pool
    //         })
    //     })
    //     .collect::<StdResult<Vec<_>>>()?;
    // let (offer_pool, ask_pool) =
    //     select_pools(offer_asset_info.as_ref(), Some(&ask_asset.info), &pools)?;
    //
    // // Check the swap parameters are valid
    // if check_swap_parameters(offer_pool.amount, ask_pool.amount, ask_asset.amount).is_err() {
    //     return Ok(ReverseSimulationResponse {
    //         offer_amount: Uint128::zero(),
    //         spread_amount: Uint128::zero(),
    //         commission_amount: Uint128::zero(),
    //     });
    // }
    //
    // // Get fee info from factory
    // let fee_info = query_fee_info(
    //     &deps.querier,
    //     &config.factory_addr,
    //     config.pair_info.pair_type.clone(),
    // )?;
    // let before_commission = (Decimal::one() - fee_info.total_fee_rate)
    //     .inv()
    //     .unwrap_or_else(Decimal::one)
    //     .checked_mul_uint128(ask_asset.amount)?;
    //
    // let token_precision = get_precision(deps.storage, &ask_pool.info)?;
    // let offer_amount = calc_y(
    //     &ask_pool.info,
    //     &offer_pool.info,
    //     adjust_precision(
    //         ask_pool.amount.checked_sub(before_commission)?,
    //         token_precision,
    //         config.greatest_precision,
    //     )?,
    //     &pools,
    //     compute_current_amp(&config, &env)?,
    // )?
    // .checked_sub(offer_pool.amount)?;
    //
    // let token_precision = get_precision(deps.storage, &offer_pool.info)?;
    // let offer_amount = adjust_precision(offer_amount, config.greatest_precision, token_precision)?;
    //
    // Ok(ReverseSimulationResponse {
    //     offer_amount,
    //     spread_amount: offer_amount.saturating_sub(before_commission),
    //     commission_amount: fee_info
    //         .total_fee_rate
    //         .checked_mul_uint128(before_commission)?,
    // })

    todo!()
}

/// ## Description
/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
pub fn query_cumulative_prices(deps: Deps, env: Env) -> StdResult<CumulativePricesResponse> {
    let mut config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;
    accumulate_prices(&env, &mut config);

    Ok(CumulativePricesResponse {
        assets,
        total_share,
        cumulative_prices: config.cumulative_prices,
    })
}

/// ## Description
/// Returns the pair contract configuration in a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(deps: Deps, env: Env) -> StdResult<ConfigResponse> {
    // let config = CONFIG.load(deps.storage)?;
    // Ok(ConfigResponse {
    //     block_time_last: config.block_time_last,
    //     params: Some(to_binary(&StablePoolConfig {
    //         amp: Decimal::from_ratio(compute_current_amp(&config, &env)?, AMP_PRECISION),
    //     })?),
    // })

    todo!()
}

/// ## Description
/// Returns a [`ContractError`] on failure.
/// If `belief_price` and `max_spread` are both specified, we compute a new spread,
/// otherwise we just use the swap spread to check `max_spread`.
/// ## Params
/// * **belief_price** is an object of type [`Option<Decimal>`]. This is the belief price used in the swap.
///
/// * **max_spread** is an object of type [`Option<Decimal>`]. This is the
/// max spread allowed so that the swap can be executed successfuly.
///
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of assets to swap.
///
/// * **return_amount** is an object of type [`Uint128`]. This is the amount of assets to receive from the swap.
///
/// * **spread_amount** is an object of type [`Uint128`]. This is the spread used in the swap.
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    let default_spread = Decimal::from_str(DEFAULT_SLIPPAGE)?;
    let max_allowed_spread = Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?;

    let max_spread = max_spread.unwrap_or(default_spread);
    if max_spread.gt(&max_allowed_spread) {
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
            && Decimal::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if Decimal::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
        return Err(ContractError::MaxSpreadAssertion {});
    }

    Ok(())
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

/// ## Description
/// Returns the total amount of assets in the pool as well as the total amount of LP tokens currently minted.
/// ## Params
/// * **querier** is an object of type [`QuerierWrapper`].
///
/// * **config** is an object of type [`Config`].
pub fn pool_info(querier: QuerierWrapper, config: &Config) -> StdResult<(Vec<Asset>, Uint128)> {
    let pools = config
        .pair_info
        .query_pools(&querier, &config.pair_info.contract_addr)?;
    let total_share = query_supply(&querier, &config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

/// ## Description
/// Updates the pool configuration with the specified parameters in the `params` variable.
/// Returns a [`ContractError`] as a failure, otherwise returns a [`Response`] with the specified
/// attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **params** is an object of type [`Binary`]. These are the the new parameter values.
pub fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender != factory_config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let action;
    match from_binary::<ConcentratedPoolUpdateParams>(&params)? {
        ConcentratedPoolUpdateParams::Update(update_params) => {
            config.pool_params.update_params(update_params)?;
            action = "update_params";
        }
        ConcentratedPoolUpdateParams::Promote(promote_params) => {
            config.pool_state.promote_params(&env, promote_params)?;
            action = "promote_params";
        }
        ConcentratedPoolUpdateParams::StopChangingAmpGamma {} => {
            config.pool_state.stop_promotion(&env);
            action = "stop_changing_amp_gamma";
        }
    }
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", action))
}

/// ## Description
/// Compute the current pool D value.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
fn query_compute_d(deps: Deps, env: Env) -> StdResult<Uint128> {
    // let config = CONFIG.load(deps.storage)?;
    //
    // let amp = compute_current_amp(&config, &env)?;
    // let pools = config
    //     .pair_info
    //     .query_pools(&deps.querier, env.contract.address)?
    //     .into_iter()
    //     .map(|pool| {
    //         let token_precision = query_token_precision(&deps.querier, &pool.info)?;
    //         adjust_precision(pool.amount, token_precision, config.greatest_precision)
    //     })
    //     .collect::<StdResult<Vec<_>>>()?;
    // let n_coins = config.pair_info.asset_infos.len() as u8;
    // let leverage = amp.checked_mul(n_coins.into())?;
    //
    // compute_d(leverage, &pools).map_err(|_| StdError::generic_err("Failed to calculate the D"))

    todo!()
}
