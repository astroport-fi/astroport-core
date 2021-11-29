use crate::error::ContractError;
use crate::math::{calc_amount, compute_d, N_COINS};
use crate::state::{Config, CONFIG};

use cosmwasm_bignumber::Decimal256;
use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Env, MessageInfo, Reply, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128,
    WasmMsg,
};

use crate::response::MsgInstantiateContractResponse;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::generator::ExecuteMsg as GeneratorExecuteMsg;
use astroport::pair::{ConfigResponse, InstantiateMsg, StablePoolParams};
use astroport::pair::{
    CumulativePricesResponse, Cw20HookMsg, ExecuteMsg, MigrateMsg, PoolResponse, QueryMsg,
    ReverseSimulationResponse, SimulationResponse,
};
use astroport::querier::{
    query_factory_config, query_fee_info, query_supply, query_token_precision,
};
use astroport::{token::InstantiateMsg as TokenInstantiateMsg, U256};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use protobuf::Message;
use std::cmp::Ordering;
use std::vec;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-pair-stable";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTANTIATE_TOKEN_REPLY_ID: u64 = 1;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.asset_infos[0] == msg.asset_infos[1] {
        return Err(ContractError::DoublingAssets {});
    }

    let params: StablePoolParams = from_binary(&msg.init_params.unwrap())?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos,
            pair_type: PairType::Stable {},
        },
        factory_addr: deps.api.addr_validate(msg.factory_addr.as_str())?,
        block_time_last: 0,
        price0_cumulative_last: Uint128::zero(),
        price1_cumulative_last: Uint128::zero(),
        amp: params.amp,
    };

    CONFIG.save(deps.storage, &config)?;

    // Create LP token
    let sub_msg: Vec<SubMsg> = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            code_id: msg.token_code_id,
            msg: to_binary(&TokenInstantiateMsg {
                name: "Astroport LP token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
            })?,
            funds: vec![],
            admin: None,
            label: String::from("Astroport LP token"),
        }
        .into(),
        id: INSTANTIATE_TOKEN_REPLY_ID,
        gas_limit: None,
        reply_on: ReplyOn::Success,
    }];

    Ok(Response::new().add_submessages(sub_msg))
}

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

    config.pair_info.liquidity_token = deps.api.addr_validate(res.get_contract_address())?;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { params } => update_config(deps, info, params),
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
        } => {
            if !offer_asset.is_native_token() {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(&to_addr)?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info.clone(),
                info.sender,
                offer_asset,
                belief_price,
                max_spread,
                to_addr,
            )
        }
    }
}

pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let contract_addr = info.sender.clone();
    match from_binary(&cw20_msg.msg) {
        Ok(Cw20HookMsg::Swap {
            belief_price,
            max_spread,
            to,
        }) => {
            // only asset contract can execute this message
            let mut authorized: bool = false;
            let config: Config = CONFIG.load(deps.storage)?;

            for pool in config.pair_info.asset_infos {
                if let AssetInfo::Token { contract_addr, .. } = &pool {
                    if contract_addr == &info.sender {
                        authorized = true;
                    }
                }
            }

            if !authorized {
                return Err(ContractError::Unauthorized {});
            }

            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(to_addr.as_str())?)
            } else {
                None
            };

            swap(
                deps,
                env,
                info,
                Addr::unchecked(cw20_msg.sender),
                Asset {
                    info: AssetInfo::Token { contract_addr },
                    amount: cw20_msg.amount,
                },
                belief_price,
                max_spread,
                to_addr,
            )
        }
        Ok(Cw20HookMsg::WithdrawLiquidity {}) => withdraw_liquidity(
            deps,
            env,
            info,
            Addr::unchecked(cw20_msg.sender),
            cw20_msg.amount,
        ),
        Err(err) => Err(ContractError::Std(err)),
    }
}

/// CONTRACT - should approve contract to use the amount of token
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
    receiver: Option<String>,
) -> Result<Response, ContractError> {
    let auto_stake = auto_stake.unwrap_or(false);
    for asset in assets.iter() {
        asset.assert_sent_native_token_balance(&info)?;
    }

    let mut config: Config = CONFIG.load(deps.storage)?;
    let mut pools: [Asset; 2] = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address.clone())?;
    let deposits: [Uint128; 2] = [
        assets
            .iter()
            .find(|a| a.info.equal(&pools[0].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
        assets
            .iter()
            .find(|a| a.info.equal(&pools[1].info))
            .map(|a| a.amount)
            .expect("Wrong asset info is given"),
    ];

    if deposits[0].is_zero() || deposits[1].is_zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut messages: Vec<CosmosMsg> = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.to_string(),
                msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                    owner: info.sender.to_string(),
                    recipient: env.contract.address.to_string(),
                    amount: deposits[i],
                })?,
                funds: vec![],
            }))
        } else {
            // If the asset is native token, balance is already increased
            // To calculated properly we should subtract user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    // assert slippage tolerance
    assert_slippage_tolerance(&slippage_tolerance, &deposits, &pools)?;

    let token_precision_0 = query_token_precision(&deps.querier, pools[0].info.clone())?;
    let token_precision_1 = query_token_precision(&deps.querier, pools[1].info.clone())?;

    let greater_precision = token_precision_0.max(token_precision_1);

    let deposit_amount_0 = adjust_precision(deposits[0], token_precision_0, greater_precision)?;
    let deposit_amount_1 = adjust_precision(deposits[1], token_precision_1, greater_precision)?;

    let total_share = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;
    let share = if total_share.is_zero() {
        let liquidity_token_precision = query_token_precision(
            &deps.querier,
            AssetInfo::Token {
                contract_addr: config.pair_info.liquidity_token.clone(),
            },
        )?;

        // Initial share = collateral amount
        adjust_precision(
            Uint128::new(
                (U256::from(deposit_amount_0.u128()) * U256::from(deposit_amount_1.u128()))
                    .integer_sqrt()
                    .as_u128(),
            ),
            greater_precision,
            liquidity_token_precision,
        )?
    } else {
        let leverage = config.amp.checked_mul(u64::from(N_COINS)).unwrap();

        let mut pool_amount_0 =
            adjust_precision(pools[0].amount, token_precision_0, greater_precision)?;
        let mut pool_amount_1 =
            adjust_precision(pools[1].amount, token_precision_1, greater_precision)?;

        let d_before_addition_liquidity =
            compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

        pool_amount_0 = pool_amount_0.checked_add(deposit_amount_0)?;
        pool_amount_1 = pool_amount_1.checked_add(deposit_amount_1)?;

        let d_after_addition_liquidity =
            compute_d(leverage, pool_amount_0.u128(), pool_amount_1.u128()).unwrap();

        // d after addition liquidity may be less than or equal to d before addition liquidity due to rounding
        if d_before_addition_liquidity >= d_after_addition_liquidity {
            return Err(ContractError::LiquidityAmountTooSmall {});
        }

        total_share.multiply_ratio(
            d_after_addition_liquidity - d_before_addition_liquidity,
            d_before_addition_liquidity,
        )
    };

    if share.is_zero() {
        return Err(ContractError::LiquidityAmountTooSmall {});
    }

    // mint LP token for sender or receiver if set
    let receiver = receiver.unwrap_or_else(|| info.sender.to_string());
    messages.extend(mint_liquidity_token_message(
        deps.as_ref(),
        &config,
        env.clone(),
        deps.api.addr_validate(receiver.as_str())?,
        share,
        auto_stake,
    )?);

    // Accumulate prices for oracle
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
        env,
        &config,
        pools[0].amount,
        token_precision_0,
        pools[1].amount,
        token_precision_1,
    )? {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new().add_messages(messages).add_attributes(vec![
        attr("action", "provide_liquidity"),
        attr("sender", info.sender.as_str()),
        attr("receiver", receiver.as_str()),
        attr("assets", format!("{}, {}", assets[0], assets[1])),
        attr("share", share.to_string()),
    ]))
}

/// Mint LP token to beneficiary or auto deposit into generator if set
fn mint_liquidity_token_message(
    deps: Deps,
    config: &Config,
    env: Env,
    recipient: Addr,
    amount: Uint128,
    auto_stake: bool,
) -> StdResult<Vec<CosmosMsg>> {
    let lp_token = config.pair_info.liquidity_token.clone();

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(vec![CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: recipient.to_string(),
                amount,
            })?,
            funds: vec![],
        })]);
    }

    // Mint to contract and stake to generator
    let generator =
        query_factory_config(&deps.querier, config.clone().factory_addr)?.generator_address;

    Ok(vec![
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: env.contract.address.to_string(),
                amount,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::IncreaseAllowance {
                spender: generator.to_string(),
                amount,
                expires: None,
            })?,
            funds: vec![],
        }),
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: generator.to_string(),
            msg: to_binary(&GeneratorExecuteMsg::DepositFor {
                lp_token,
                amount,
                beneficiary: recipient,
            })?,
            funds: vec![],
        }),
    ])
}

pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage).unwrap();

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let (pools, total_share) = pool_info(deps.as_ref(), config.clone())?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    // Accumulate prices for oracle
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
        env,
        &config,
        pools[0].amount,
        query_token_precision(&deps.querier, pools[0].info.clone())?,
        pools[1].amount,
        query_token_precision(&deps.querier, pools[1].info.clone())?,
    )? {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    let messages: Vec<CosmosMsg> = vec![
        refund_assets[0]
            .clone()
            .into_msg(&deps.querier, sender.clone())?,
        refund_assets[1]
            .clone()
            .into_msg(&deps.querier, sender.clone())?,
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: config.pair_info.liquidity_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
            funds: vec![],
        }),
    ];

    let attributes = vec![
        attr("action", "withdraw_liquidity"),
        attr("sender", sender.as_str()),
        attr("withdrawn_share", &amount.to_string()),
        attr(
            "refund_assets",
            format!("{}, {}", refund_assets[0], refund_assets[1]),
        ),
    ];

    Ok(Response::new()
        .add_messages(messages)
        .add_attributes(attributes))
}

pub fn get_share_in_assets(
    pools: &[Asset; 2],
    amount: Uint128,
    total_share: Uint128,
) -> [Asset; 2] {
    let share_ratio: Decimal = Decimal::from_ratio(amount, total_share);

    [
        Asset {
            info: pools[0].info.clone(),
            amount: pools[0].amount * share_ratio,
        },
        Asset {
            info: pools[1].info.clone(),
            amount: pools[1].amount * share_ratio,
        },
    ]
}
// CONTRACT - a user must do token approval
#[allow(clippy::too_many_arguments)]
pub fn swap(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<Addr>,
) -> Result<Response, ContractError> {
    offer_asset.assert_sent_native_token_balance(&info)?;

    let mut config: Config = CONFIG.load(deps.storage)?;

    // If the asset balance is already increased
    // To calculated properly we should subtract user deposit from the pool
    let pools: Vec<Asset> = config
        .pair_info
        .query_pools(&deps.querier, env.clone().contract.address)?
        .iter()
        .map(|p| {
            let mut p = p.clone();
            if p.info.equal(&offer_asset.info) {
                p.amount = p.amount.checked_sub(offer_asset.amount).unwrap();
            }

            p
        })
        .collect();

    let offer_pool: Asset;
    let ask_pool: Asset;

    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = pools[0].clone();
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = pools[1].clone();
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
    }

    // Get fee info from factory
    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;

    let offer_amount = offer_asset.amount;
    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        query_token_precision(&deps.querier, offer_pool.info)?,
        ask_pool.amount,
        query_token_precision(&deps.querier, ask_pool.info.clone())?,
        offer_amount,
        fee_info.total_fee_rate,
        config.amp,
    )?;

    // check max spread limit if exist
    assert_max_spread(
        belief_price,
        max_spread,
        offer_amount,
        return_amount + commission_amount,
        spread_amount,
    )?;

    // compute tax
    let return_asset = Asset {
        info: ask_pool.info.clone(),
        amount: return_amount,
    };

    let tax_amount = return_asset.compute_tax(&deps.querier)?;

    let receiver = to.unwrap_or_else(|| sender.clone());

    let mut messages: Vec<CosmosMsg> =
        vec![return_asset.into_msg(&deps.querier, receiver.clone())?];

    // Maker fee
    let mut maker_fee_amount = Uint128::new(0);
    if let Some(fee_address) = fee_info.fee_address {
        if let Some(f) = calculate_maker_fee(
            ask_pool.info.clone(),
            commission_amount,
            fee_info.maker_fee_rate,
        ) {
            messages.push(f.clone().into_msg(&deps.querier, fee_address)?);
            maker_fee_amount = f.amount;
        }
    }

    // Accumulate prices for oracle
    if let Some((price0_cumulative_new, price1_cumulative_new, block_time)) = accumulate_prices(
        env,
        &config,
        pools[0].amount,
        query_token_precision(&deps.querier, pools[0].info.clone())?,
        pools[1].amount,
        query_token_precision(&deps.querier, pools[1].info.clone())?,
    )? {
        config.price0_cumulative_last = price0_cumulative_new;
        config.price1_cumulative_last = price1_cumulative_new;
        config.block_time_last = block_time;
        CONFIG.save(deps.storage, &config)?;
    }

    Ok(Response::new()
        .add_messages(
            // 1. send collateral token from the contract to a user
            // 2. send inactive commission to collector
            messages,
        )
        .add_attribute("action", "swap")
        .add_attribute("sender", sender.as_str())
        .add_attribute("receiver", receiver.as_str())
        .add_attribute("offer_asset", offer_asset.info.to_string())
        .add_attribute("ask_asset", ask_pool.info.to_string())
        .add_attribute("offer_amount", offer_amount.to_string())
        .add_attribute("return_amount", return_amount.to_string())
        .add_attribute("tax_amount", tax_amount.to_string())
        .add_attribute("spread_amount", spread_amount.to_string())
        .add_attribute("commission_amount", commission_amount.to_string())
        .add_attribute("maker_fee_amount", maker_fee_amount.to_string()))
}

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

    // we have to shift block_time when any price is zero to not fill an accumulator with a new price to that period

    let greater_precision = x_precision.max(y_precision);
    let x = adjust_precision(x, x_precision, greater_precision)?;
    let y = adjust_precision(y, y_precision, greater_precision)?;

    let time_elapsed = Uint128::new((block_time - config.block_time_last) as u128);

    let mut pcl0 = config.price0_cumulative_last;
    let mut pcl1 = config.price1_cumulative_last;

    if !x.is_zero() && !y.is_zero() {
        pcl0 = config.price0_cumulative_last.wrapping_add(adjust_precision(
            time_elapsed.checked_mul(Uint128::new(
                calc_amount(
                    x.u128(),
                    y.u128(),
                    adjust_precision(Uint128::new(1), 0, greater_precision)?.u128(),
                    config.amp,
                )
                .unwrap(),
            ))?,
            greater_precision,
            0,
        )?);
        pcl1 = config.price1_cumulative_last.wrapping_add(adjust_precision(
            time_elapsed.checked_mul(Uint128::new(
                calc_amount(
                    y.u128(),
                    x.u128(),
                    adjust_precision(Uint128::new(1), 0, greater_precision)?.u128(),
                    config.amp,
                )
                .unwrap(),
            ))?,
            greater_precision,
            0,
        )?)
    };

    Ok(Some((pcl0, pcl1, block_time)))
}

pub fn calculate_maker_fee(
    pool_info: AssetInfo,
    commission_amount: Uint128,
    maker_commission_rate: Decimal,
) -> Option<Asset> {
    let maker_fee: Uint128 = commission_amount * maker_commission_rate;
    if maker_fee.is_zero() {
        return None;
    }

    Some(Asset {
        info: pool_info,
        amount: maker_fee,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&query_pair_info(deps)?),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
        QueryMsg::Share { amount } => to_binary(&query_share(deps, amount)?),
        QueryMsg::Simulation { offer_asset } => to_binary(&query_simulation(deps, offer_asset)?),
        QueryMsg::ReverseSimulation { ask_asset } => {
            to_binary(&query_reverse_simulation(deps, ask_asset)?)
        }
        QueryMsg::CumulativePrices {} => to_binary(&query_cumulative_prices(deps, env)?),
        QueryMsg::Config {} => to_binary(&query_config(deps)?),
    }
}

pub fn query_pair_info(deps: Deps) -> StdResult<PairInfo> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(config.pair_info)
}

pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, config)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

pub fn query_share(deps: Deps, amount: Uint128) -> StdResult<[Asset; 2]> {
    let config: Config = CONFIG.load(deps.storage)?;
    let (pools, total_share) = pool_info(deps, config)?;
    let refund_assets = get_share_in_assets(&pools, amount, total_share);

    Ok(refund_assets)
}

pub fn query_simulation(deps: Deps, offer_asset: Asset) -> StdResult<SimulationResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let contract_addr = config.pair_info.contract_addr.clone();

    let pools: [Asset; 2] = config.pair_info.query_pools(&deps.querier, contract_addr)?;

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
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;

    let (return_amount, spread_amount, commission_amount) = compute_swap(
        offer_pool.amount,
        query_token_precision(&deps.querier, offer_pool.info)?,
        ask_pool.amount,
        query_token_precision(&deps.querier, ask_pool.info)?,
        offer_asset.amount,
        fee_info.total_fee_rate,
        config.amp,
    )?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
}

pub fn query_reverse_simulation(
    deps: Deps,
    ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let contract_addr = config.pair_info.contract_addr.clone();

    let pools: [Asset; 2] = config.pair_info.query_pools(&deps.querier, contract_addr)?;

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

    // Get fee info from factory
    let fee_info = query_fee_info(
        &deps.querier,
        config.factory_addr.clone(),
        config.pair_info.pair_type.clone(),
    )?;

    let (offer_amount, spread_amount, commission_amount) = compute_offer_amount(
        offer_pool.amount,
        query_token_precision(&deps.querier, offer_pool.info)?,
        ask_pool.amount,
        query_token_precision(&deps.querier, ask_pool.info)?,
        ask_asset.amount,
        fee_info.total_fee_rate,
        config.amp,
    )?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
    })
}

pub fn query_cumulative_prices(deps: Deps, env: Env) -> StdResult<CumulativePricesResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let (assets, total_share) = pool_info(deps, config.clone())?;

    let mut price0_cumulative_last = config.price0_cumulative_last;
    let mut price1_cumulative_last = config.price1_cumulative_last;

    if let Ok(Some((price0_cumulative_new, price1_cumulative_new, _))) = accumulate_prices(
        env,
        &config,
        assets[0].amount,
        query_token_precision(&deps.querier, assets[0].info.clone())?,
        assets[1].amount,
        query_token_precision(&deps.querier, assets[1].info.clone())?,
    ) {
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

pub fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        block_time_last: config.block_time_last,
        params: Some(to_binary(&StablePoolParams { amp: config.amp })?),
    })
}

pub fn amount_of(coins: &[Coin], denom: String) -> Uint128 {
    match coins.iter().find(|x| x.denom == denom) {
        Some(coin) => coin.amount,
        None => Uint128::zero(),
    }
}

fn compute_swap(
    offer_pool: Uint128,
    offer_precision: u8,
    ask_pool: Uint128,
    ask_precision: u8,
    offer_amount: Uint128,
    commission_rate: Decimal,
    amp: u64,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // offer => ask

    let greater_precision = offer_precision.max(ask_precision);
    let offer_pool = adjust_precision(offer_pool, offer_precision, greater_precision)?;
    let ask_pool = adjust_precision(ask_pool, ask_precision, greater_precision)?;
    let offer_amount = adjust_precision(offer_amount, offer_precision, greater_precision)?;

    let return_amount = adjust_precision(
        Uint128::new(
            calc_amount(offer_pool.u128(), ask_pool.u128(), offer_amount.u128(), amp).unwrap(),
        ),
        greater_precision,
        ask_precision,
    )?;

    // We assume the assets should stay in a 1:1 ratio, the true exchange rate is 1. So any exchange rate <1 could be considered the spread
    let spread_amount = offer_amount.saturating_sub(return_amount);

    let commission_amount: Uint128 = return_amount * commission_rate;

    // commission will be absorbed to pool
    let return_amount: Uint128 = return_amount.checked_sub(commission_amount).unwrap();

    Ok((return_amount, spread_amount, commission_amount))
}

fn compute_offer_amount(
    offer_pool: Uint128,
    offer_precision: u8,
    ask_pool: Uint128,
    ask_precision: u8,
    ask_amount: Uint128,
    commission_rate: Decimal,
    amp: u64,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer

    let greater_precision = offer_precision.max(ask_precision);
    let offer_pool = adjust_precision(offer_pool, offer_precision, greater_precision)?;
    let ask_pool = adjust_precision(ask_pool, ask_precision, greater_precision)?;
    let ask_amount = adjust_precision(ask_amount, ask_precision, greater_precision)?;

    let one_minus_commission = Decimal256::one() - Decimal256::from(commission_rate);
    let inv_one_minus_commission: Decimal = (Decimal256::one() / one_minus_commission).into();
    let before_commission_deduction = ask_amount * inv_one_minus_commission;

    let offer_amount = adjust_precision(
        Uint128::new(
            calc_amount(
                ask_pool.u128(),
                offer_pool.u128(),
                before_commission_deduction.u128(),
                amp,
            )
            .unwrap(),
        ),
        greater_precision,
        offer_precision,
    )?;

    // We assume the assets should stay in a 1:1 ratio, the true exchange rate is 1. So any exchange rate <1 could be considered the spread
    let spread_amount = offer_amount.saturating_sub(before_commission_deduction);

    let commission_amount = before_commission_deduction * commission_rate;

    Ok((offer_amount, spread_amount, commission_amount))
}

fn adjust_precision(
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

/// If `belief_price` and `max_spread` both are given,
/// we compute new spread else we just use swap
/// spread to check `max_spread`
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> Result<(), ContractError> {
    if let (Some(max_spread), Some(belief_price)) = (max_spread, belief_price) {
        let expected_return =
            offer_amount * Decimal::from(Decimal256::one() / Decimal256::from(belief_price));
        let spread_amount = expected_return
            .checked_sub(return_amount)
            .unwrap_or_else(|_| Uint128::zero());

        if return_amount < expected_return
            && Decimal::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    } else if let Some(max_spread) = max_spread {
        if Decimal::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
            return Err(ContractError::MaxSpreadAssertion {});
        }
    }

    Ok(())
}

fn assert_slippage_tolerance(
    _slippage_tolerance: &Option<Decimal>,
    _deposits: &[Uint128; 2],
    _pools: &[Asset; 2],
) -> Result<(), ContractError> {
    //There is no slippage in the stable pool
    Ok(())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

pub fn pool_info(deps: Deps, config: Config) -> StdResult<([Asset; 2], Uint128)> {
    let contract_addr = config.pair_info.contract_addr.clone();
    let pools: [Asset; 2] = config.pair_info.query_pools(&deps.querier, contract_addr)?;
    let total_share: Uint128 = query_supply(&deps.querier, config.pair_info.liquidity_token)?;

    Ok((pools, total_share))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    params: Binary,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    let factory_config = query_factory_config(&deps.querier, config.factory_addr.clone())?;

    if info.sender != factory_config.owner {
        return Err(ContractError::Unauthorized {});
    }

    let params: StablePoolParams = from_binary(&params)?;

    config.amp = params.amp;

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default())
}
