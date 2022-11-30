use std::vec;

use cosmwasm_std::{
    attr, entry_point, from_binary, wasm_execute, wasm_instantiate, Addr, Binary, CosmosMsg,
    Decimal, Decimal256, DepsMut, Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg,
    SubMsgResponse, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use cw_utils::parse_instantiate_response_data;
use itertools::Itertools;

use astroport::asset::{
    addr_opt_validate, addr_validate_to_lower, format_lp_token_name, token_asset, Asset, AssetInfo,
    Decimal256Ext, PairInfo, MINIMUM_LIQUIDITY_AMOUNT,
};
use astroport::cosmwasm_ext::{DecimalToInteger, IntegerToDecimal};
use astroport::factory::PairType;
use astroport::pair::migration_check;
use astroport::pair::{Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg};
use astroport::pair_concentrated::{
    ConcentratedPoolParams, ConcentratedPoolUpdateParams, UpdatePoolParams,
};
use astroport::querier::{query_factory_config, query_fee_info, query_supply};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;

use crate::error::ContractError;
use crate::math::{calc_d, get_xcp};
use crate::state::{
    store_precisions, AmpGamma, Config, PoolParams, PoolState, Precisions, PriceState, CONFIG,
};
use crate::utils::{
    accumulate_prices, assert_max_spread, before_swap_check, calculate_maker_fee,
    check_asset_infos, check_assets, check_cw20_in_pool, compute_swap, get_share_in_assets,
    mint_liquidity_token_message, pool_info, query_pools,
};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// A `reply` call code ID used for sub-messages.
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;
/// An LP token's precision.
pub(crate) const LP_TOKEN_PRECISION: u8 = 6;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
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

    let params: ConcentratedPoolParams = from_binary(
        &msg.init_params
            .ok_or(ContractError::InitParamsNotFound {})?,
    )?;

    if params.initial_price_scale.is_zero() {
        return Err(StdError::generic_err("Initial price scale can not be zero").into());
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    store_precisions(deps.branch(), &msg.asset_infos)?;

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
        repeg_profit_threshold: Some(params.repeg_profit_threshold),
        min_price_scale_delta: Some(params.min_price_scale_delta),
        ma_half_time: Some(params.ma_half_time),
    })?;

    let pool_state = PoolState {
        initial: AmpGamma::default(),
        future: AmpGamma::new(params.amp, params.gamma)?,
        future_time: env.block.time.seconds(),
        initial_time: 0,
        price_state: PriceState {
            oracle_price: params.initial_price_scale.into(),
            last_price: params.initial_price_scale.into(),
            price_scale: params.initial_price_scale.into(),
            last_price_update: env.block.time.seconds(),
            xcp_profit: Decimal256::zero(),
            xcp: Decimal256::zero(),
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
        cumulative_prices,
        pool_params,
        pool_state,
        owner: addr_opt_validate(deps.api, &params.owner)?,
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

/// The entry point to the contract for processing replies from submessages.
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

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::UpdateConfig { params: Binary }** Not supported.
///
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ProvideLiquidity {
///             assets,
///             slippage_tolerance,
///             auto_stake,
///             receiver,
///         }** Provides liquidity in the pair with the specified input parameters.
///
/// * **ExecuteMsg::Swap {
///             offer_asset,
///             belief_price,
///             max_spread,
///             to,
///         }** Performs a swap operation with the specified parameters.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if migration_check(deps.querier, &config.factory_addr, &env.contract.address)? {
        return Err(ContractError::PairIsNotMigrated {});
    }

    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
        } => provide_liquidity(
            deps,
            env,
            info,
            assets,
            slippage_tolerance,
            auto_stake,
            receiver,
        ),
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

            if !config.pair_info.asset_infos.contains(&offer_asset.info) {
                return Err(ContractError::InvalidAsset(offer_asset.info.to_string()));
            }

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
        ExecuteMsg::UpdateConfig { params } => update_config(deps, env, info, params),
        _ => Err(ContractError::NonSupported {}),
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
fn receive_cw20(
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

/// Provides liquidity in the pair with the specified input parameters.
///
/// * **assets** is an array with assets available in the pool.
///
/// * **slippage_tolerance** is an optional parameter which is used to specify how much
/// the pool price can move until the provide liquidity transaction goes through.
///
/// * **auto_stake** is an optional parameter which determines whether the LP tokens minted after
/// liquidity provision are automatically staked in the Generator contract on behalf of the LP token receiver.
///
/// * **receiver** is an optional parameter which defines the receiver of the LP tokens.
/// If no custom receiver is specified, the pair will mint LP tokens for the function caller.
///
/// NOTE - the address that wants to provide liquidity should approve the pair contract to pull its relevant tokens.
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<Asset>,
    _slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    check_assets(deps.api, &assets)?;

    let config = CONFIG.load(deps.storage)?;

    if assets.len() > config.pair_info.asset_infos.len() {
        return Err(ContractError::InvalidNumberOfAssets(
            config.pair_info.asset_infos.len(),
        ));
    }

    assets
        .iter()
        .try_for_each(|asset| asset.assert_sent_native_token_balance(&info))?;

    let precisions = Precisions::new(deps.storage)?;

    let mut pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

    // TODO: fix this in case imbalanced provide is allowed
    let deposits = [
        Decimal256::with_precision(assets[0].amount, precisions.get_precision(&assets[0].info)?)?,
        Decimal256::with_precision(assets[1].amount, precisions.get_precision(&assets[1].info)?)?,
    ];

    if deposits[0].is_zero() || deposits[1].is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut messages = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the asset is a token contract, then we need to execute a TransferFrom msg to receive assets
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(wasm_execute(
                contract_addr,
                &Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: deposits[i].to_uint(precisions.get_precision(&assets[i].info)?)?,
                },
                vec![],
            )?));
        } else {
            // If the asset is native token, the pool balance is already increased
            // To calculate the total amount of deposits properly, we should subtract the user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    let mut new_xp = pools
        .iter()
        .zip(deposits.iter())
        .map(|(pool, deposit)| pool.amount + deposit)
        .collect_vec();
    new_xp[1] *= config.pool_state.price_state.price_scale;
    let amp_gamma = config.pool_state.get_amp_gamma(&env);
    let new_d = calc_d(&new_xp, &amp_gamma)?;
    let auto_stake = auto_stake.unwrap_or(false);
    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;

    let mint_amount = if total_share.is_zero() {
        let xcp = get_xcp(new_d, config.pool_state.price_state.price_scale);
        let mint_amount = xcp
            .to_uint(LP_TOKEN_PRECISION)?
            .checked_sub(MINIMUM_LIQUIDITY_AMOUNT)
            .map_err(|_| ContractError::MinimumLiquidityAmountError {})?;

        messages.extend(mint_liquidity_token_message(
            deps.querier,
            &config,
            &env.contract.address,
            &env.contract.address,
            MINIMUM_LIQUIDITY_AMOUNT,
            false,
        )?);

        // share cannot become zero after minimum liquidity subtraction
        if mint_amount.is_zero() {
            return Err(ContractError::MinimumLiquidityAmountError {});
        }

        mint_amount
    } else {
        // TODO: Assert slippage tolerance if needed
        // TODO: calculate provide fee or allow only balanced provide

        let mut old_xp = pools.iter().map(|a| a.amount).collect_vec();
        old_xp[1] *= config.pool_state.price_state.price_scale;
        let old_d = calc_d(&old_xp, &amp_gamma)?;
        let total_share = total_share.to_decimal256(LP_TOKEN_PRECISION)?;

        (total_share * new_d / old_d)
            .saturating_sub(total_share)
            .to_uint(LP_TOKEN_PRECISION)?
    };

    // TODO: we will need to update price in case imbalanced provide is allowed
    // let last_price = config.pool_state.price_state.price_scale * new_xp[0] / new_xp[1];
    // config.pool_state.update_price(
    //     &config.pool_params,
    //     &env,
    //     total_share + mint_amount.to_decimal256(0u8)?,
    //     new_d,
    //     &new_xp,
    //     last_price,
    // )?;

    // Mint LP tokens for the sender or for the receiver (if set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        mint_amount,
        auto_stake,
    )?);

    // TODO: accumulate prices only if imbalanced provide is allowed
    // accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender),
        attr("receiver", receiver),
        attr("assets", format!("{}, {}", assets[0], assets[1])),
        attr("share", mint_amount),
    ]))
}

/// Withdraw liquidity from the pool.
/// * **sender** is the address that will receive assets back from the pair contract
///
/// * **amount** is the amount of provided LP tokens
///
/// * **assets** array with [`Asset`] objects which define number of coins a user wants to withdraw
fn withdraw_liquidity(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
    assets: Vec<Asset>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let (pools, total_share) = pool_info(deps.querier, &config)?;

    let burn_amount;
    let refund_assets;
    let mut messages = vec![];

    if assets.is_empty() {
        // Usual withdraw (balanced)
        burn_amount = amount;
        refund_assets = get_share_in_assets(&pools, amount, total_share)?;
    } else {
        todo!("Imbalanced withdraw + update prices")
    }

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

    // TODO: accumulate prices only if imbalanced provide is allowed
    // accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr("refund_assets", refund_assets.iter().join(", ")),
    ]))
}

/// Performs an swap operation with the specified parameters. The trader must approve the
/// pool contract to transfer offer assets from their wallet.
///
/// * **sender** is the sender of the swap operation.
///
/// * **offer_asset** proposed asset for swapping.
///
/// * **belief_price** is used to calculate the maximum swap spread.
///
/// * **max_spread** sets the maximum spread of the swap operation.
///
/// * **to** sets the recipient of the swap operation.
fn swap(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let precisions = Precisions::new(deps.storage)?;
    let offer_asset_prec = precisions.get_precision(&offer_asset.info)?;
    let offer_asset_dec = offer_asset.to_decimal_asset(offer_asset_prec)?;
    let mut config = CONFIG.load(deps.storage)?;

    let mut pools = query_pools(deps.querier, &env.contract.address, &config, &precisions)?;

    let (offer_ind, _) = pools
        .iter()
        .find_position(|asset| asset.info == offer_asset_dec.info)
        .ok_or_else(|| ContractError::InvalidAsset(offer_asset_dec.info.to_string()))?;
    let ask_ind = 1 - offer_ind;
    let ask_asset_prec = precisions.get_precision(&pools[ask_ind].info)?;

    pools[offer_ind].amount -= offer_asset_dec.amount;

    before_swap_check(&pools, offer_asset_dec.amount)?;

    let mut xs = pools.iter().map(|asset| asset.amount).collect_vec();

    let swap_result = compute_swap(&xs, offer_asset_dec.amount, ask_ind, &config, &env)?;
    xs[offer_ind] += offer_asset_dec.amount;
    xs[ask_ind] -= swap_result.dy;

    assert_max_spread(
        belief_price,
        max_spread,
        offer_asset_dec.amount,
        swap_result.dy,
        swap_result.spread_fee,
    )?;
    let spread_amount = swap_result.spread_fee.to_uint(ask_asset_prec)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?
        .to_decimal256(LP_TOKEN_PRECISION)?;
    let last_price = config.pool_state.price_state.price_scale * xs[0] / xs[1];

    // .update_price() works only with internal representation
    xs[1] *= config.pool_state.price_state.price_scale;
    config.pool_state.update_price(
        &config.pool_params,
        &env,
        total_share,
        swap_result.d,
        &xs,
        last_price,
    )?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;
    let receiver = to.unwrap_or_else(|| sender.clone());

    let return_amount = swap_result.dy.to_uint(ask_asset_prec)?;
    let mut messages = vec![Asset {
        info: pools[ask_ind].info.clone(),
        amount: return_amount,
    }
    .into_msg(&deps.querier, &receiver)?];

    let commission_amount = swap_result.fee.to_uint(ask_asset_prec)?;
    let mut maker_fee_amount = Uint128::zero();
    if let Some(fee_address) = fee_info.fee_address {
        if let Some(fee) = calculate_maker_fee(
            &pools[ask_ind].info,
            commission_amount,
            fee_info.maker_fee_rate,
        ) {
            maker_fee_amount = fee.amount;
            messages.push(fee.into_msg(&deps.querier, fee_address)?);
        }
    }

    accumulate_prices(&env, &mut config);

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "swap"),
        attr("sender", sender),
        attr("receiver", receiver),
        attr("offer_asset", offer_asset_dec.info.to_string()),
        attr("ask_asset", pools[ask_ind].info.to_string()),
        attr("offer_amount", offer_asset.amount),
        attr("return_amount", return_amount),
        attr("spread_amount", spread_amount),
        attr("commission_amount", commission_amount),
        attr("maker_fee_amount", maker_fee_amount),
    ]))
}

/// Updates the pool configuration with the specified parameters in the `params` variable.
///
/// * **params** new parameter values in [`Binary`] form.
fn update_config(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender != factory_config.owner && Some(info.sender) != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let action = match from_binary::<ConcentratedPoolUpdateParams>(&params)? {
        ConcentratedPoolUpdateParams::Update(update_params) => {
            config.pool_params.update_params(update_params)?;
            "update_params"
        }
        ConcentratedPoolUpdateParams::Promote(promote_params) => {
            config.pool_state.promote_params(&env, promote_params)?;
            "promote_params"
        }
        ConcentratedPoolUpdateParams::StopChangingAmpGamma {} => {
            config.pool_state.stop_promotion(&env);
            "stop_changing_amp_gamma"
        }
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default().add_attribute("action", action))
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
