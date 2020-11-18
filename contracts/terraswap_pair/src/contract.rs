use crate::math::{decimal_multiplication, decimal_subtraction, reverse_decimal};
use crate::msg::{
    Cw20HookMsg, HandleMsg, MigrateMsg, PoolResponse, QueryMsg, ReverseSimulationResponse,
    SimulationResponse,
};
use crate::state::{read_pair_info, store_pair_info};

use cosmwasm_std::{
    from_binary, log, to_binary, Api, Binary, CanonicalAddr, Coin, CosmosMsg, Decimal, Env, Extern,
    HandleResponse, HandleResult, HumanAddr, InitResponse, MigrateResponse, MigrateResult, Querier,
    StdError, StdResult, Storage, Uint128, WasmMsg,
};

use cw20::{Cw20HandleMsg, Cw20ReceiveMsg, MinterResponse};
use integer_sqrt::IntegerSquareRoot;
use std::str::FromStr;
use terraswap::{
    query_supply, Asset, AssetInfo, InitHook, PairInfo, PairInfoRaw, PairInitMsg, TokenInitMsg,
};

/// Commission rate == 0.3%
const COMMISSION_RATE: &str = "0.003";
pub fn init<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: PairInitMsg,
) -> StdResult<InitResponse> {
    let pair_info: &PairInfoRaw = &PairInfoRaw {
        contract_addr: deps.api.canonical_address(&env.contract.address)?,
        liquidity_token: CanonicalAddr::default(),
        asset_infos: [
            msg.asset_infos[0].to_raw(&deps)?,
            msg.asset_infos[1].to_raw(&deps)?,
        ],
    };

    store_pair_info(&mut deps.storage, &pair_info)?;

    // Create LP token
    let mut messages: Vec<CosmosMsg> = vec![CosmosMsg::Wasm(WasmMsg::Instantiate {
        code_id: msg.token_code_id,
        msg: to_binary(&TokenInitMsg {
            name: "terraswap liquidity token".to_string(),
            symbol: "uLP".to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: env.contract.address.clone(),
                cap: None,
            }),
            init_hook: Some(InitHook {
                msg: to_binary(&HandleMsg::PostInitialize {})?,
                contract_addr: env.contract.address,
            }),
        })?,
        send: vec![],
        label: None,
    })];

    if let Some(hook) = msg.init_hook {
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: hook.contract_addr,
            msg: hook.msg,
            send: vec![],
        }));
    }

    Ok(InitResponse {
        messages,
        log: vec![],
    })
}

pub fn handle<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    msg: HandleMsg,
) -> HandleResult {
    match msg {
        HandleMsg::Receive(msg) => receive_cw20(deps, env, msg),
        HandleMsg::PostInitialize {} => try_post_initialize(deps, env),
        HandleMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
        } => try_provide_liquidity(deps, env, assets, slippage_tolerance),
        HandleMsg::Swap {
            offer_asset,
            belief_price,
            max_spread,
            to,
        } => {
            if !offer_asset.is_native_token() {
                return Err(StdError::unauthorized());
            }

            try_swap(
                deps,
                env.clone(),
                env.message.sender,
                offer_asset,
                belief_price,
                max_spread,
                to,
            )
        }
    }
}

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    let contract_addr = env.message.sender.clone();
    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            Cw20HookMsg::Swap {
                belief_price,
                max_spread,
                to,
            } => {
                // only asset contract can execute this message
                let mut authorized: bool = false;
                let config: PairInfoRaw = read_pair_info(&deps.storage)?;
                let pools: [Asset; 2] = config.query_pools(deps, &env.contract.address)?;
                for pool in pools.iter() {
                    if let AssetInfo::Token { contract_addr, .. } = &pool.info {
                        if contract_addr == &env.message.sender {
                            authorized = true;
                        }
                    }
                }

                if !authorized {
                    return Err(StdError::unauthorized());
                }

                try_swap(
                    deps,
                    env,
                    cw20_msg.sender,
                    Asset {
                        info: AssetInfo::Token { contract_addr },
                        amount: cw20_msg.amount,
                    },
                    belief_price,
                    max_spread,
                    to,
                )
            }
            Cw20HookMsg::WithdrawLiquidity {} => {
                let config: PairInfoRaw = read_pair_info(&deps.storage)?;
                if deps.api.canonical_address(&env.message.sender)? != config.liquidity_token {
                    return Err(StdError::unauthorized());
                }

                try_withdraw_liquidity(deps, env, cw20_msg.sender, cw20_msg.amount)
            }
        }
    } else {
        Err(StdError::generic_err("data should be given"))
    }
}

// Must token contract execute it
pub fn try_post_initialize<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
) -> HandleResult {
    let config: PairInfoRaw = read_pair_info(&deps.storage)?;

    // permission check
    if config.liquidity_token != CanonicalAddr::default() {
        return Err(StdError::unauthorized());
    }

    store_pair_info(
        &mut deps.storage,
        &PairInfoRaw {
            liquidity_token: deps.api.canonical_address(&env.message.sender)?,
            ..config
        },
    )?;

    Ok(HandleResponse {
        messages: vec![],
        log: vec![log("liquidity_token_addr", env.message.sender.as_str())],
        data: None,
    })
}

/// CONTRACT - should approve contract to use the amount of token
pub fn try_provide_liquidity<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
) -> HandleResult {
    for asset in assets.iter() {
        asset.assert_sent_native_token_balance(&env)?;
    }

    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;
    let mut pools: [Asset; 2] = pair_info.query_pools(deps, &env.contract.address)?;
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

    let mut i = 0;
    let mut messages: Vec<CosmosMsg> = vec![];
    for pool in pools.iter_mut() {
        // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract_addr.clone(),
                msg: to_binary(&Cw20HandleMsg::TransferFrom {
                    owner: env.message.sender.clone(),
                    recipient: env.contract.address.clone(),
                    amount: deposits[i],
                })?,
                send: vec![],
            }));
        } else {
            // If the asset is native token, balance is already increased
            // To calculated properly we should subtract user deposit from the pool
            pool.amount = (pool.amount - deposits[i])?;
        }

        i += 1;
    }

    // assert slippage tolerance
    assert_slippage_tolerance(&slippage_tolerance, &deposits, &pools)?;

    let liquidity_token = deps.api.human_address(&pair_info.liquidity_token)?;
    let total_share = query_supply(&deps, &liquidity_token)?;
    let share = if total_share == Uint128::zero() {
        // Initial share = collateral amount
        Uint128((deposits[0].u128() * deposits[1].u128()).integer_sqrt())
    } else {
        // min(1, 2)
        // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_share / sqrt(pool_0 * pool_1))
        // == deposit_0 * total_share / pool_0
        // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_share / sqrt(pool_1 * pool_1))
        // == deposit_1 * total_share / pool_1
        std::cmp::min(
            deposits[0].multiply_ratio(total_share, pools[0].amount),
            deposits[1].multiply_ratio(total_share, pools[1].amount),
        )
    };

    // mint LP token to sender
    messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: deps.api.human_address(&pair_info.liquidity_token)?,
        msg: to_binary(&Cw20HandleMsg::Mint {
            recipient: env.message.sender,
            amount: share,
        })?,
        send: vec![],
    }));
    Ok(HandleResponse {
        messages,
        log: vec![
            log("action", "provide_liquidity"),
            log("assets", format!("{}, {}", assets[0], assets[1])),
            log("share", &share),
        ],
        data: None,
    })
}

pub fn try_withdraw_liquidity<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    sender: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;
    let liquidity_addr: HumanAddr = deps.api.human_address(&pair_info.liquidity_token)?;

    let pools: [Asset; 2] = pair_info.query_pools(&deps, &env.contract.address)?;
    let total_share: Uint128 = query_supply(&deps, &liquidity_addr)?;

    let share_ratio: Decimal = Decimal::from_ratio(amount, total_share);
    let refund_assets: Vec<Asset> = pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect();

    // update pool info
    Ok(HandleResponse {
        messages: vec![
            // refund asset tokens
            refund_assets[0].clone().into_msg(
                deps,
                env.contract.address.clone(),
                sender.clone(),
            )?,
            refund_assets[1]
                .clone()
                .into_msg(&deps, env.contract.address, sender)?,
            // burn liquidity token
            CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: deps.api.human_address(&pair_info.liquidity_token)?,
                msg: to_binary(&Cw20HandleMsg::Burn { amount })?,
                send: vec![],
            }),
        ],
        log: vec![
            log("action", "withdraw_liquidity"),
            log("withdrawn_share", &amount.to_string()),
            log(
                "refund_assets",
                format!("{}, {}", refund_assets[0], refund_assets[1]),
            ),
        ],
        data: None,
    })
}

// CONTRACT - a user must do token approval
pub fn try_swap<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    sender: HumanAddr,
    offer_asset: Asset,
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    to: Option<HumanAddr>,
) -> HandleResult {
    offer_asset.assert_sent_native_token_balance(&env)?;

    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;

    let pools: [Asset; 2] = pair_info.query_pools(&deps, &env.contract.address)?;

    let offer_pool: Asset;
    let ask_pool: Asset;

    // If the asset balance is already increased
    // To calculated properly we should subtract user deposit from the pool
    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = Asset {
            amount: (pools[0].amount - offer_asset.amount)?,
            info: pools[0].info.clone(),
        };
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = Asset {
            amount: (pools[1].amount - offer_asset.amount)?,
            info: pools[1].info.clone(),
        };
        ask_pool = pools[0].clone();
    } else {
        return Err(StdError::generic_err("Wrong asset info is given"));
    }

    let offer_amount = offer_asset.amount;
    let (return_amount, spread_amount, commission_amount) =
        compute_swap(offer_pool.amount, ask_pool.amount, offer_amount)?;

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

    let tax_amount = return_asset.compute_tax(&deps)?;

    // 1. send collateral token from the contract to a user
    // 2. send inactive commission to collector
    Ok(HandleResponse {
        messages: vec![return_asset.into_msg(
            &deps,
            env.contract.address.clone(),
            to.unwrap_or(sender),
        )?],
        log: vec![
            log("action", "swap"),
            log("offer_asset", offer_asset.info.to_string()),
            log("ask_asset", ask_pool.info.to_string()),
            log("offer_amount", offer_amount.to_string()),
            log("return_amount", return_amount.to_string()),
            log("tax_amount", tax_amount.to_string()),
            log("spread_amount", spread_amount.to_string()),
            log("commission_amount", commission_amount.to_string()),
        ],
        data: None,
    })
}

pub fn query<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&query_pair_info(&deps)?),
        QueryMsg::Pool {} => to_binary(&query_pool(&deps)?),
        QueryMsg::Simulation { offer_asset } => to_binary(&query_simulation(&deps, offer_asset)?),
        QueryMsg::ReverseSimulation { ask_asset } => {
            to_binary(&query_reverse_simulation(&deps, ask_asset)?)
        }
    }
}

pub fn query_pair_info<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<PairInfo> {
    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;
    pair_info.to_normal(&deps)
}

pub fn query_pool<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<PoolResponse> {
    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;
    let contract_addr = deps.api.human_address(&pair_info.contract_addr)?;
    let assets: [Asset; 2] = pair_info.query_pools(&deps, &contract_addr)?;
    let total_share: Uint128 =
        query_supply(&deps, &deps.api.human_address(&pair_info.liquidity_token)?)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
}

pub fn query_simulation<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    offer_asset: Asset,
) -> StdResult<SimulationResponse> {
    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;

    let contract_addr = deps.api.human_address(&pair_info.contract_addr)?;
    let pools: [Asset; 2] = pair_info.query_pools(&deps, &contract_addr)?;

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
            "Given offer asset is not blong to pairs",
        ));
    }

    let (return_amount, spread_amount, commission_amount) =
        compute_swap(offer_pool.amount, ask_pool.amount, offer_asset.amount)?;

    Ok(SimulationResponse {
        return_amount,
        spread_amount,
        commission_amount,
    })
}

pub fn query_reverse_simulation<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    ask_asset: Asset,
) -> StdResult<ReverseSimulationResponse> {
    let pair_info: PairInfoRaw = read_pair_info(&deps.storage)?;

    let contract_addr = deps.api.human_address(&pair_info.contract_addr)?;
    let pools: [Asset; 2] = pair_info.query_pools(&deps, &contract_addr)?;

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
            "Given ask asset is not blong to pairs",
        ));
    }

    let (offer_amount, spread_amount, commission_amount) =
        compute_offer_amount(offer_pool.amount, ask_pool.amount, ask_asset.amount)?;

    Ok(ReverseSimulationResponse {
        offer_amount,
        spread_amount,
        commission_amount,
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
    ask_pool: Uint128,
    offer_amount: Uint128,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // offer => ask
    // ask_amount = (ask_pool - cp / (offer_pool + offer_amount)) * (1 - commission_rate)
    let cp = Uint128(offer_pool.u128() * ask_pool.u128());
    let return_amount = (ask_pool - cp.multiply_ratio(1u128, offer_pool + offer_amount))?;

    // calculate spread & commission
    let spread_amount: Uint128 = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool)
        - return_amount)
        .unwrap_or_else(|_| Uint128::zero());
    let commission_amount: Uint128 = return_amount * Decimal::from_str(&COMMISSION_RATE).unwrap();

    // commission will be absorbed to pool
    let return_amount: Uint128 = (return_amount - commission_amount).unwrap();

    Ok((return_amount, spread_amount, commission_amount))
}

fn compute_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer
    // offer_amount = cp / (ask_pool - ask_amount / (1 - commission_rate)) - offer_pool
    let cp = Uint128(offer_pool.u128() * ask_pool.u128());
    let one_minus_commission =
        decimal_subtraction(Decimal::one(), Decimal::from_str(&COMMISSION_RATE).unwrap())?;

    let offer_amount: Uint128 = (cp.multiply_ratio(
        1u128,
        (ask_pool - ask_amount * reverse_decimal(one_minus_commission))?,
    ) - offer_pool)?;

    let before_commission_deduction = ask_amount * reverse_decimal(one_minus_commission);
    let spread_amount = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool)
        - before_commission_deduction)
        .unwrap_or_else(|_| Uint128::zero());
    let commission_amount =
        before_commission_deduction * Decimal::from_str(&COMMISSION_RATE).unwrap();
    Ok((offer_amount, spread_amount, commission_amount))
}

/// If `belief_price` and `max_spread` both are given,
/// we compute new spread else we just use terraswap
/// spread to check `max_spread`
pub fn assert_max_spread(
    belief_price: Option<Decimal>,
    max_spread: Option<Decimal>,
    offer_amount: Uint128,
    return_amount: Uint128,
    spread_amount: Uint128,
) -> StdResult<()> {
    if let (Some(max_spread), Some(belief_price)) = (max_spread, belief_price) {
        let expected_return = offer_amount * reverse_decimal(belief_price);
        let spread_amount = (expected_return - return_amount).unwrap_or_else(|_| Uint128::zero());

        if return_amount < expected_return
            && Decimal::from_ratio(spread_amount, expected_return) > max_spread
        {
            return Err(StdError::generic_err("Operation exceeds max spread limit"));
        }
    } else if let Some(max_spread) = max_spread {
        if Decimal::from_ratio(spread_amount, return_amount + spread_amount) > max_spread {
            return Err(StdError::generic_err("Operation exceeds max spread limit"));
        }
    }

    Ok(())
}

fn assert_slippage_tolerance(
    slippage_tolerance: &Option<Decimal>,
    deposits: &[Uint128; 2],
    pools: &[Asset; 2],
) -> StdResult<()> {
    if let Some(slippage_tolerance) = *slippage_tolerance {
        let one_minus_slippage_tolerance = decimal_subtraction(Decimal::one(), slippage_tolerance)?;

        // Ensure each prices are not dropped as much as slippage tolerance rate
        if decimal_multiplication(
            Decimal::from_ratio(deposits[0], deposits[1]),
            one_minus_slippage_tolerance,
        ) > Decimal::from_ratio(pools[0].amount, pools[1].amount)
            || decimal_multiplication(
                Decimal::from_ratio(deposits[1], deposits[0]),
                one_minus_slippage_tolerance,
            ) > Decimal::from_ratio(pools[1].amount, pools[0].amount)
        {
            return Err(StdError::generic_err(
                "Operation exceeds max splippage tolerance",
            ));
        }
    }

    Ok(())
}

pub fn migrate<S: Storage, A: Api, Q: Querier>(
    _deps: &mut Extern<S, A, Q>,
    _env: Env,
    _msg: MigrateMsg,
) -> MigrateResult {
    Ok(MigrateResponse::default())
}
