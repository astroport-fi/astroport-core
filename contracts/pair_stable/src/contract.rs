use crate::error::ContractError;
use crate::math::{
    calc_ask_amount, calc_offer_amount, calc_y, compute_d, AMP_PRECISION, MAX_AMP, MAX_AMP_CHANGE,
    MIN_AMP_CHANGING_TIME,
};
use crate::state::{get_precision, store_precisions, Config, CONFIG};
use std::collections::HashMap;

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, wasm_execute, wasm_instantiate, Addr, Binary,
    CosmosMsg, Decimal, Deps, DepsMut, Env, Fraction, MessageInfo, QuerierWrapper, Reply, Response,
    StdError, StdResult, SubMsg, Uint128, Uint64, WasmMsg,
};

use crate::response::MsgInstantiateContractResponse;
use astroport::asset::{
    addr_opt_validate, addr_validate_to_lower, format_lp_token_name, is_non_zero_liquidity, Asset,
    AssetInfo, PairInfo,
};
use astroport::factory::PairType;

use astroport::pair::{
    migration_check, ConfigResponse, InstantiateMsg, StablePoolParams, StablePoolUpdateParams,
    DEFAULT_SLIPPAGE, MAX_ALLOWED_SLIPPAGE, TWAP_PRECISION,
};

use crate::utils::{
    adjust_precision, check_asset_infos, check_assets, check_cw20_in_pool, compute_current_amp,
    compute_swap, get_share_in_assets, mint_liquidity_token_message, select_pools, SwapResult,
};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolResponse, QueryMsg,
    ReverseSimulationResponse, SimulationResponse, StablePoolConfig,
};
use astroport::querier::{
    query_factory_config, query_fee_info, query_supply, query_token_precision,
};
use astroport::token::InstantiateMsg as TokenInstantiateMsg;
use astroport::DecimalCheckedOps;
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use itertools::Itertools;
use protobuf::Message;
use std::str::FromStr;
use std::vec;

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

    if msg.asset_infos.len() > 5 || msg.asset_infos.len() < 2 {
        return Err(ContractError::InvalidNumberOfAssets {});
    }

    if msg.init_params.is_none() {
        return Err(ContractError::InitParamsNotFound {});
    }

    let params: StablePoolParams = from_binary(&msg.init_params.unwrap())?;

    if params.amp == 0 || params.amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp {});
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let greatest_precision = store_precisions(deps.branch(), &msg.asset_infos)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos.clone(),
            pair_type: PairType::Stable {},
        },
        factory_addr: addr_validate_to_lower(deps.api, msg.factory_addr)?,
        block_time_last: 0,
        price0_cumulative_last: Uint128::zero(),
        price1_cumulative_last: Uint128::zero(),
        init_amp: params.amp * AMP_PRECISION,
        init_amp_time: env.block.time.seconds(),
        next_amp: params.amp * AMP_PRECISION,
        next_amp_time: env.block.time.seconds(),
        greatest_precision,
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
    let mut config: Config = CONFIG.load(deps.storage)?;

    if config.pair_info.liquidity_token != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    let data = msg.result.unwrap().data.unwrap();
    let res: MsgInstantiateContractResponse =
        Message::parse_from_bytes(data.as_slice()).map_err(|_| {
            StdError::parse_err("MsgInstantiateContractResponse", "failed to parse data")
        })?;

    config.pair_info.liquidity_token =
        addr_validate_to_lower(deps.api, res.get_contract_address())?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
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
            ask_asset_info,
            belief_price,
            max_spread,
            to,
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
                ask_asset_info,
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
            ask_asset_info,
            belief_price,
            max_spread,
            to,
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
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: info.sender,
                    },
                    amount: cw20_msg.amount,
                },
                ask_asset_info,
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

    let auto_stake = auto_stake.unwrap_or(false);
    let mut config = CONFIG.load(deps.storage)?;

    if assets.len() > config.pair_info.asset_infos.len() {
        return Err(ContractError::InvalidNumberOfAssets {});
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut non_zero_flag = false;

    let mut assets_collection = assets
        .clone()
        .into_iter()
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

    // If some assets are omitted then add them explicitly with 0 deposit
    pools.into_iter().for_each(|(pool_info, pool_amount)| {
        if !assets.iter().any(|asset| asset.info == pool_info) {
            assets_collection.push((
                Asset {
                    amount: Uint128::zero(),
                    info: pool_info,
                },
                pool_amount,
            ));
        }
    });

    if !non_zero_flag {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut messages = vec![];
    for (deposit, pool) in assets_collection.iter_mut() {
        // We cannot put a zero amount into an empty pool.
        if deposit.amount.is_zero() && pool.is_zero() {
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
                *pool = pool.checked_sub(deposit.amount)?;
            }
        }

        // Adjusting to the greatest precision
        let coin_precision = get_precision(deps.storage, &deposit.info)?;
        deposit.amount =
            adjust_precision(deposit.amount, coin_precision, config.greatest_precision)?;
        *pool = adjust_precision(*pool, coin_precision, config.greatest_precision)?;
    }

    let n_coins = config.pair_info.asset_infos.len() as u8;

    let amp = compute_current_amp(&config, &env)?.checked_mul(n_coins.into())?;

    // Initial invariant (D)
    let old_balances = assets_collection
        .iter()
        .map(|(_, pool)| *pool)
        .collect_vec();
    let init_d = compute_d(amp, &old_balances)?;

    // Invariant (D) after deposit added
    let mut new_balances = assets_collection
        .iter()
        .map(|(deposit, pool)| Ok(pool.checked_add(deposit.amount)?))
        .collect::<StdResult<Vec<_>>>()?;
    let deposit_d = compute_d(amp, &new_balances)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    let mint_amount = if total_share.is_zero() {
        deposit_d
    } else {
        // Get fee info from the factory
        let fee_info = query_fee_info(
            &deps.querier,
            &config.factory_addr,
            config.pair_info.pair_type.clone(),
        )
        .unwrap_or_default(); // There may no fee exist thus 0 is a default fee.

        // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
        let fee = fee_info
            .total_fee_rate
            .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

        for i in 0..n_coins as usize {
            let ideal_balance = deposit_d.checked_multiply_ratio(old_balances[i], init_d)?;
            let difference = if ideal_balance > new_balances[i] {
                ideal_balance - new_balances[i]
            } else {
                new_balances[i] - ideal_balance
            };
            // Fee will be charged only during imbalanced provide i.e. if invariant D was changed
            new_balances[i] = new_balances[i].checked_sub(fee.checked_mul_uint128(difference)?)?;
        }

        let after_fee_d = compute_d(amp, &new_balances)?;

        total_share.checked_multiply_ratio(after_fee_d - init_d, init_d)?
    };

    let mint_amount = adjust_precision(mint_amount, config.greatest_precision, LP_TOKEN_PRECISION)?;

    if mint_amount.is_zero() {
        return Err(ContractError::LiquidityAmountTooSmall {});
    }

    // Mint LP token for the caller (or for the receiver if it was set)
    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    messages.extend(mint_liquidity_token_message(
        deps.querier,
        &config,
        &env.contract.address,
        &receiver,
        mint_amount,
        auto_stake,
    )?);

    // TODO: accumulate prices for multiple assets
    // // Accumulate prices assets in the pool
    // if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
    //     env,
    //     &config,
    //     pools[0].amount,
    //     token_precision_0,
    //     pools[1].amount,
    //     token_precision_1,
    // )? {
    //     config.price0_cumulative_last = price0_cumulative_new;
    //     config.price1_cumulative_last = price1_cumulative_new;
    //     config.block_time_last = block_time;
    //     CONFIG.save(deps.storage, &config)?;
    // }

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

    if assets.is_empty() {
        let (pools, total_share) = pool_info(deps.querier, &config)?;
        burn_amount = amount;
        refund_assets = get_share_in_assets(&pools, amount, total_share);
    } else {
        // Imbalanced withdraw
        burn_amount = imbalanced_withdraw(deps.as_ref(), &env, &config, amount, &assets)?;
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

    // // Accumulate prices for the assets in the pool
    // if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
    //     env,
    //     &config,
    //     pools[0].amount,
    //     query_token_precision(&deps.querier, &pools[0].info)?,
    //     pools[1].amount,
    //     query_token_precision(&deps.querier, &pools[1].info)?,
    // )? {
    //     config.price0_cumulative_last = price0_cumulative_new;
    //     config.price1_cumulative_last = price1_cumulative_new;
    //     config.block_time_last = block_time;
    //     CONFIG.save(deps.storage, &config)?;
    // }
    //
    // let messages = vec![
    //     refund_assets[0].clone().into_msg(&deps.querier, &sender)?,
    //     refund_assets[1].clone().into_msg(&deps.querier, &sender)?,
    //     CosmosMsg::Wasm(WasmMsg::Execute {
    //         contract_addr: config.pair_info.liquidity_token.to_string(),
    //         msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
    //         funds: vec![],
    //     }),
    // ];

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
    config: &Config,
    provided_amount: Uint128,
    assets: &[Asset],
) -> Result<Uint128, ContractError> {
    check_assets(deps.api, assets)?;

    if assets.len() > config.pair_info.asset_infos.len() {
        return Err(ContractError::InvalidNumberOfAssets {});
    }

    let pools: HashMap<_, _> = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|pool| (pool.info, pool.amount))
        .collect();

    let mut assets_collection = assets
        .into_iter()
        .cloned()
        .map(|asset| {
            // Get appropriate pool
            let mut pool = pools
                .get(&asset.info)
                .copied()
                .ok_or_else(|| ContractError::InvalidAsset(asset.info.to_string()))?;

            // Adjusting to the greatest precision
            let coin_precision = get_precision(deps.storage, &asset.info)?;
            pool = adjust_precision(pool, coin_precision, config.greatest_precision)?;

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
                pool_amount,
            ));
        }
    });

    let n_coins = config.pair_info.asset_infos.len() as u8;

    let amp = compute_current_amp(&config, &env)?.checked_mul(n_coins.into())?;

    // Initial invariant (D)
    let old_balances = assets_collection
        .iter()
        .map(|(_, pool)| *pool)
        .collect_vec();
    let init_d = compute_d(amp, &old_balances)?;

    // Invariant (D) after assets withdrawn
    let mut new_balances = assets_collection
        .iter()
        .map(|(withdraw, pool)| Ok(pool.checked_sub(withdraw.amount)?))
        .collect::<StdResult<Vec<_>>>()?;
    let withdraw_d = compute_d(amp, &new_balances)?;

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )
    .unwrap_or_default(); // There may no fee exist thus 0 is a default fee.

    // total_fee_rate * N_COINS / (4 * (N_COINS - 1))
    let fee = fee_info
        .total_fee_rate
        .checked_mul(Decimal::from_ratio(n_coins, 4 * (n_coins - 1)))?;

    for i in 0..n_coins as usize {
        let ideal_balance = withdraw_d.checked_multiply_ratio(old_balances[i], init_d)?;
        let difference = if ideal_balance > new_balances[i] {
            ideal_balance - new_balances[i]
        } else {
            new_balances[i] - ideal_balance
        };
        new_balances[i] = new_balances[i].checked_sub(fee.checked_mul_uint128(difference)?)?;
    }

    let after_fee_d = compute_d(amp, &new_balances)?;

    let total_share = query_supply(&deps.querier, &config.pair_info.liquidity_token)?;
    // How many tokens do we need to burn to withdraw asked assets?
    let burn_amount = total_share
        .checked_multiply_ratio(init_d - after_fee_d, init_d)?
        .checked_add(Uint128::from(1u8))?; // In case of rounding errors - make it unfavorable for the "attacker"

    let burn_amount = adjust_precision(burn_amount, LP_TOKEN_PRECISION, config.greatest_precision)?;

    if burn_amount > provided_amount {
        return Err(StdError::generic_err(format!(
            "Not enough LP tokens. You need {} LP tokens.",
            burn_amount
        ))
        .into());
    } else if burn_amount.is_zero() {
        return Err(StdError::generic_err("Failed to calculate necessary LP tokens amount").into());
    }

    Ok(burn_amount)
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
    ask_asset_info: Option<AssetInfo>,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    // If the asset balance already increased
    // We should subtract the user deposit from the pool offer asset amount
    let pools = config
        .pair_info
        .query_pools(&deps.querier, &env.contract.address)?
        .into_iter()
        .map(|mut p| {
            if p.info.equal(&offer_asset.info) {
                p.amount = p.amount.checked_sub(offer_asset.amount)?;
            }
            Ok(p)
        })
        .collect::<StdResult<Vec<_>>>()?;

    let (offer_pool, ask_pool) = select_pools(&config, &offer_asset.info, ask_asset_info, &pools)?;

    // Check if the liquidity is non-zero
    is_non_zero_liquidity(offer_pool.amount, ask_pool.amount)?;

    let pools = pools
        .into_iter()
        .map(|pool| {
            let token_precision = query_token_precision(&deps.querier, &pool.info)?;
            Ok(Asset {
                amount: adjust_precision(pool.amount, token_precision, config.greatest_precision)?,
                ..pool
            })
        })
        .collect::<StdResult<Vec<_>>>()?;

    let token_precision = query_token_precision(&deps.querier, &offer_asset.info)?;

    let new_ask_pool = calc_y(
        &offer_asset.info,
        &ask_pool.info,
        adjust_precision(
            offer_asset.amount,
            token_precision,
            config.greatest_precision,
        )?,
        &pools,
        compute_current_amp(&config, &env)?,
    )?;

    let token_precision = query_token_precision(&deps.querier, &ask_pool.info)?;
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

    // // Check the max spread limit (if it was specified)
    // assert_max_spread(
    //     belief_price,
    //     max_spread,
    //     offer_amount,
    //     return_amount + commission_amount,
    //     spread_amount,
    // )?;

    let spread_amount = Uint128::zero();
    let commission_amount = Uint128::zero();

    let receiver = to.unwrap_or_else(|| sender.clone());

    let mut messages = vec![Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    }
    .into_msg(&deps.querier, &receiver)?];

    // Compute the Maker fee
    let mut maker_fee_amount = Uint128::zero();
    // if let Some(fee_address) = fee_info.fee_address {
    //     if let Some(f) =
    //         calculate_maker_fee(&ask_pool.info, commission_amount, fee_info.maker_fee_rate)
    //     {
    //         maker_fee_amount = f.amount;
    //         messages.push(f.into_msg(&deps.querier, fee_address)?);
    //     }
    // }

    /* TODO: support multiple assets
    // Accumulate prices for the assets in the pool
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
        env,
        &config,
        offer_pool.amount,
        query_token_precision(&deps.querier, &offer_pool.info)?,
        ask_pool.amount,
        query_token_precision(&deps.querier, &ask_pool.info)?,
    )? {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }*/

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
            attr("ask_asset", ask_pool.info.to_string()),
            attr("offer_amount", offer_asset.amount),
            attr("return_amount", return_amount),
            attr("spread_amount", spread_amount),
            attr("commission_amount", commission_amount),
            attr("maker_fee_amount", maker_fee_amount),
        ]))
}

/// ## Description
/// Accumulate token prices for the assets in the pool.
/// Note that this function shifts **block_time** when any of the token prices is zero in order to not
/// fill an accumulator with a null price for that period.
/// ## Params
/// * **env** is an object of type [`Env`].
///
/// * **config** is an object of type [`Config`].
///
/// * **x** is an object of type [`Uint128`]. This is the balance of asset\[\0] in the pool.
///
/// * **x_precision** is an object of type [`u8`]. This is the precision for the x token.
///
/// * **y** is an object of type [`Uint128`]. This is the balance of asset\[\1] in the pool.
///
/// * **y_precision** is an object of type [`u8`]. This is the precision for the y token.
pub fn accumulate_prices(
    env: Env,
    config: &Config,
    x: Uint128,
    x_precision: u8,
    y: Uint128,
    y_precision: u8,
) -> StdResult<Option<(Uint128, Uint128, u64)>> {
    let block_time = env.block.time.seconds();
    if block_time <= config.block_time_last {
        return Ok(None);
    }

    // We have to shift block_time when any price is zero in order to not fill an accumulator with a null price for that period
    let greater_precision = x_precision.max(y_precision).max(TWAP_PRECISION);
    let x = adjust_precision(x, x_precision, greater_precision)?;
    let y = adjust_precision(y, y_precision, greater_precision)?;

    let time_elapsed = Uint128::from(block_time - config.block_time_last);

    let mut pcl0 = config.price0_cumulative_last;
    let mut pcl1 = config.price1_cumulative_last;

    if !x.is_zero() && !y.is_zero() {
        let current_amp = compute_current_amp(config, &env)?;
        pcl0 = config.price0_cumulative_last.wrapping_add(adjust_precision(
            time_elapsed.checked_mul(calc_ask_amount(
                x,
                y,
                adjust_precision(Uint128::new(1), 0, greater_precision)?,
                current_amp,
            )?)?,
            greater_precision,
            TWAP_PRECISION,
        )?);
        pcl1 = config.price1_cumulative_last.wrapping_add(adjust_precision(
            time_elapsed.checked_mul(calc_ask_amount(
                y,
                x,
                adjust_precision(Uint128::new(1), 0, greater_precision)?,
                current_amp,
            )?)?,
            greater_precision,
            TWAP_PRECISION,
        )?)
    };

    Ok(Some((pcl0, pcl1, block_time)))
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
    let maker_fee: Uint128 = commission_amount * maker_commission_rate;
    if maker_fee.is_zero() {
        return None;
    }

    Some(Asset {
        info: pool_info.clone(),
        amount: maker_fee,
    })
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
        } => to_binary(&query_simulation(deps, env, offer_asset)?),
        QueryMsg::ReverseSimulation {
            offer_asset_info,
            ask_asset,
        } => to_binary(&query_reverse_simulation(deps, env, ask_asset)?),
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
pub fn query_simulation(deps: Deps, env: Env, offer_asset: Asset) -> StdResult<SimulationResponse> {
    let config = CONFIG.load(deps.storage)?;
    let contract_addr = config.pair_info.contract_addr.clone();

    let pools = config.pair_info.query_pools(&deps.querier, contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given offer asset doesn't belong to pairs",
        ));
    }

    // Get fee info from factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    // Check if the liquidity is non-zero
    is_non_zero_liquidity(offer_pool.amount, ask_pool.amount)?;

    let SwapResult {
        return_amount,
        spread_amount,
        commission_amount,
    } = compute_swap(
        deps.querier,
        &offer_pool,
        &ask_pool,
        offer_asset.amount,
        fee_info.total_fee_rate,
        compute_current_amp(&config, &env)?,
    )?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
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
pub fn query_reverse_simulation(
    deps: Deps,
    env: Env,
    ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let config = CONFIG.load(deps.storage)?;
    let contract_addr = config.pair_info.contract_addr.clone();

    let pools = config.pair_info.query_pools(&deps.querier, contract_addr)?;

    let offer_pool: Asset;
    let ask_pool: Asset;
    if ask_asset.info.equal(&pools[0].info) {
        ask_pool = pools[0].clone();
        offer_pool = pools[1].clone();
    } else if ask_asset.info.equal(&pools[1].info) {
        ask_pool = pools[1].clone();
        offer_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err(
            "Given ask asset doesn't belong to pairs",
        ));
    }

    // Get fee info from the factory
    let fee_info = query_fee_info(
        &deps.querier,
        &config.factory_addr,
        config.pair_info.pair_type.clone(),
    )?;

    // Check if the liquidity is non-zero
    is_non_zero_liquidity(offer_pool.amount, ask_pool.amount)?;

    let (offer_amount, spread_amount, commission_amount) = compute_offer_amount(
        offer_pool.amount,
        query_token_precision(&deps.querier, &offer_pool.info)?,
        ask_pool.amount,
        query_token_precision(&deps.querier, &ask_pool.info)?,
        ask_asset.amount,
        fee_info.total_fee_rate,
        compute_current_amp(&config, &env)?,
    )?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
    })
}

/// ## Description
/// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
pub fn query_cumulative_prices(deps: Deps, env: Env) -> StdResult<CumulativePricesResponse> {
    let config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps.querier, &config)?;

    let mut price0_cumulative_last = config.price0_cumulative_last;
    let mut price1_cumulative_last = config.price1_cumulative_last;

    if let Some((price0_cumulative_new, price1_cumulative_new, _)) = accumulate_prices(
        env,
        &config,
        assets[0].amount,
        query_token_precision(&deps.querier, &assets[0].info)?,
        assets[1].amount,
        query_token_precision(&deps.querier, &assets[1].info)?,
    )? {
        price0_cumulative_last = price0_cumulative_new;
        price1_cumulative_last = price1_cumulative_new;
    }

    let resp = CumulativePricesResponse {
        assets,
        total_share,
        price0_cumulative_last,
        price1_cumulative_last,
    };

    Ok(resp)
}

/// ## Description
/// Returns the pair contract configuration in a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(deps: Deps, env: Env) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_binary(&StablePoolConfig {
            amp: Decimal::from_ratio(compute_current_amp(&config, &env)?, AMP_PRECISION),
        })?),
    })
}

/// ## Description
/// Returns an amount of offer assets for a specified amount of ask assets.
/// ## Params
/// * **offer_pool** is an object of type [`Uint128`]. This is the total amount of offer assets in the pool.
///
/// * **offer_precision** is an object of type [`u8`]. This is the token precision used for the offer amount.
///
/// * **ask_pool** is an object of type [`Uint128`]. This is the total amount of ask assets in the pool.
///
/// * **ask_precision** is an object of type [`u8`]. This is the token precision used for the ask amount.
///
/// * **ask_amount** is an object of type [`Uint128`]. This is the amount of ask assets to swap to.
///
/// * **commission_rate** is an object of type [`Decimal`]. This is the total amount of fees charged for the swap.
fn compute_offer_amount(
    offer_pool: Uint128,
    offer_precision: u8,
    ask_pool: Uint128,
    ask_precision: u8,
    ask_amount: Uint128,
    commission_rate: Decimal,
    amp: Uint64,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer

    let greater_precision = offer_precision.max(ask_precision);
    let offer_pool = adjust_precision(offer_pool, offer_precision, greater_precision)?;
    let ask_pool = adjust_precision(ask_pool, ask_precision, greater_precision)?;
    let ask_amount = adjust_precision(ask_amount, ask_precision, greater_precision)?;

    let one_minus_commission = Decimal::one() - commission_rate;
    let inv_one_minus_commission = Decimal::one() / one_minus_commission;
    let before_commission_deduction = ask_amount * inv_one_minus_commission;

    let offer_amount = calc_offer_amount(offer_pool, ask_pool, before_commission_deduction, amp)?;

    // We assume the assets should stay in a 1:1 ratio, so the true exchange rate is 1. Any exchange rate < 1 could be considered the spread
    let spread_amount = offer_amount.saturating_sub(before_commission_deduction);

    let commission_amount = before_commission_deduction * commission_rate;

    let offer_amount = adjust_precision(offer_amount, greater_precision, offer_precision)?;
    let spread_amount = adjust_precision(spread_amount, greater_precision, ask_precision)?;
    let commission_amount = adjust_precision(commission_amount, greater_precision, ask_precision)?;

    Ok((offer_amount, spread_amount, commission_amount))
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
    let config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, &config.factory_addr)?;

    if info.sender != factory_config.owner {
        return Err(ContractError::Unauthorized {});
    }

    match from_binary::<StablePoolUpdateParams>(&params)? {
        StablePoolUpdateParams::StartChangingAmp {
            next_amp,
            next_amp_time,
        } => start_changing_amp(config, deps, env, next_amp, next_amp_time)?,
        StablePoolUpdateParams::StopChangingAmp {} => stop_changing_amp(config, deps, env)?,
    }

    Ok(Response::default())
}

/// ## Description
/// Start changing the AMP value. Returns a [`ContractError`] on failure, otherwise returns [`Ok`].
/// ## Params
/// * **mut config** is an object of type [`Config`]. This is a mutable reference to the pool configuration.
///
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **next_amp** is an object of type [`u64`]. This is the new value for AMP.
///
/// * **next_amp_time** is an object of type [`u64`]. This is the end time when the pool amplification will be equal to `next_amp`.
fn start_changing_amp(
    mut config: Config,
    deps: DepsMut,
    env: Env,
    next_amp: u64,
    next_amp_time: u64,
) -> Result<(), ContractError> {
    if next_amp == 0 || next_amp > MAX_AMP {
        return Err(ContractError::IncorrectAmp {});
    }

    let current_amp = compute_current_amp(&config, &env)?.u64();

    let next_amp_with_precision = next_amp * AMP_PRECISION;

    if next_amp_with_precision * MAX_AMP_CHANGE < current_amp
        || next_amp_with_precision > current_amp * MAX_AMP_CHANGE
    {
        return Err(ContractError::MaxAmpChangeAssertion {});
    }

    let block_time = env.block.time.seconds();

    if block_time < config.init_amp_time + MIN_AMP_CHANGING_TIME
        || next_amp_time < block_time + MIN_AMP_CHANGING_TIME
    {
        return Err(ContractError::MinAmpChangingTimeAssertion {});
    }

    config.init_amp = current_amp;
    config.next_amp = next_amp_with_precision;
    config.init_amp_time = block_time;
    config.next_amp_time = next_amp_time;

    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

/// ## Description
/// Stop changing the AMP value. Returns [`Ok`].
/// ## Params
/// * **mut config** is an object of type [`Config`]. This is a mutable reference to the pool configuration.
///
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
fn stop_changing_amp(mut config: Config, deps: DepsMut, env: Env) -> StdResult<()> {
    let current_amp = compute_current_amp(&config, &env)?;
    let block_time = env.block.time.seconds();

    config.init_amp = current_amp.u64();
    config.next_amp = current_amp.u64();
    config.init_amp_time = block_time;
    config.next_amp_time = block_time;

    // now (block_time < next_amp_time) is always False, so we return the saved AMP
    CONFIG.save(deps.storage, &config)?;

    Ok(())
}

/// ## Description
/// Compute the current pool D value.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
fn query_compute_d(deps: Deps, env: Env) -> StdResult<Uint128> {
    let config = CONFIG.load(deps.storage)?;

    let amp = compute_current_amp(&config, &env)?;
    let pools = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address)?
        .into_iter()
        .map(|p| p.amount)
        .collect_vec();
    let n_coins = config.pair_info.asset_infos.len() as u8;
    let leverage = amp.checked_mul(Uint64::from(n_coins))?;

    compute_d(leverage, &pools).map_err(|_| StdError::generic_err("Failed to calculate the D"))
}
