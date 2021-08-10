use crate::error::ContractError;
use crate::math::{decimal_multiplication, decimal_subtraction, reverse_decimal};
use crate::state::{Config, CONFIG};

use cosmwasm_std::{
    attr, entry_point, from_binary, to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env,
    MessageInfo, QueryRequest, ReplyOn, Response, StdError, StdResult, SubMsg, Uint128, WasmMsg,
    WasmQuery,
};

use cw20::{Cw20ExecuteMsg, Cw20ReceiveMsg, MinterResponse};
use integer_sqrt::IntegerSquareRoot;
use std::ops::Add;
use std::str::FromStr;
use std::vec;
use terraswap::asset::{Asset, AssetInfo, PairInfo};
use terraswap::factory::QueryMsg as FactoryQueryMsg;
use terraswap::hook::InitHook;
use terraswap::pair::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, PoolResponse, QueryMsg,
    ReverseSimulationResponse, SimulationResponse,
};
use terraswap::querier::query_supply;
use terraswap::token::InstantiateMsg as TokenInstantiateMsg;

/// Commission rate == 0.3%
const COMMISSION_RATE: &str = "0.003";
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: Addr::unchecked(""),
            asset_infos: msg.asset_infos,
        },
        k_last: Uint128::new(0),
        factory_addr: msg.factory_addr,
    };

    CONFIG.save(deps.storage, &config)?;

    // Create LP token
    let mut messages: Vec<SubMsg> = vec![SubMsg {
        msg: WasmMsg::Instantiate {
            code_id: msg.token_code_id,
            msg: to_binary(&TokenInstantiateMsg {
                name: "terraswap liquidity token".to_string(),
                symbol: "uLP".to_string(),
                decimals: 6,
                initial_balances: vec![],
                mint: Some(MinterResponse {
                    minter: env.contract.address.to_string(),
                    cap: None,
                }),
                init_hook: Some(InitHook {
                    msg: to_binary(&ExecuteMsg::PostInitialize {})?,
                    contract_addr: env.contract.address.to_string(),
                }),
            })?,
            funds: vec![],
            admin: None,
            label: String::new(),
        }
        .into(),
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    }];

    if let Some(hook) = msg.init_hook {
        messages.push(SubMsg {
            msg: WasmMsg::Execute {
                contract_addr: hook.contract_addr,
                msg: hook.msg,
                funds: vec![],
            }
            .into(),
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        });
    }

    Ok(Response {
        events: vec![],
        messages,
        attributes: vec![],
        data: None,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::PostInitialize {} => post_initialize(deps, env, info),
        ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
        } => provide_liquidity(deps, env, info, assets, slippage_tolerance),
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
            let pools: [Asset; 2] = config
                .pair_info
                .query_pools(&deps.querier, env.contract.address.clone())?;
            for pool in pools.iter() {
                if let AssetInfo::Token { contract_addr, .. } = &pool.info {
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

// Must token contract execute it
pub fn post_initialize(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
) -> Result<Response, ContractError> {
    let mut config: Config = CONFIG.load(deps.storage)?;

    // permission check
    if config.pair_info.liquidity_token != Addr::unchecked("") {
        return Err(ContractError::Unauthorized {});
    }

    config.pair_info.liquidity_token = info.sender.clone();

    CONFIG.save(deps.storage, &config)?;

    Ok(Response {
        events: vec![],
        messages: vec![],
        attributes: vec![attr("liquidity_token_addr", info.sender.as_str())],
        data: None,
    })
}

/// CONTRACT - should approve contract to use the amount of token
pub fn provide_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: [Asset; 2],
    slippage_tolerance: Option<Decimal>,
) -> Result<Response, ContractError> {
    for asset in assets.iter() {
        asset.assert_sent_native_token_balance(&info)?;
    }

    let config: Config = CONFIG.load(deps.storage)?;
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

    let mut messages: Vec<SubMsg> = vec![];
    for (i, pool) in pools.iter_mut().enumerate() {
        // If the pool is token contract, then we need to execute TransferFrom msg to receive funds
        if let AssetInfo::Token { contract_addr, .. } = &pool.info {
            messages.push(SubMsg {
                msg: WasmMsg::Execute {
                    contract_addr: contract_addr.to_string(),
                    msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                        owner: info.sender.to_string(),
                        recipient: env.contract.address.to_string(),
                        amount: deposits[i],
                    })?,
                    funds: vec![],
                }
                .into(),
                id: 0,
                gas_limit: None,
                reply_on: ReplyOn::Never,
            });
        } else {
            // If the asset is native token, balance is already increased
            // To calculated properly we should subtract user deposit from the pool
            pool.amount = pool.amount.checked_sub(deposits[i])?;
        }
    }

    // assert slippage tolerance
    assert_slippage_tolerance(&slippage_tolerance, &deposits, &pools)?;

    let total_supply = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;

    // Mint fee
    let (amount_minted, fee_msg) = mint_fee(
        deps.as_ref(),
        config.clone(),
        pools[0].amount,
        pools[1].amount,
        total_supply,
    );

    if let Some(f) = fee_msg {
        messages.push(f)
    }

    // Add minted fee to total supply for further calculations
    let total_supply = total_supply.add(amount_minted);

    let share = if total_supply.is_zero() {
        // Initial share = collateral amount
        Uint128::new((deposits[0].u128() * deposits[1].u128()).integer_sqrt())
    } else {
        // min(1, 2)
        // 1. sqrt(deposit_0 * exchange_rate_0_to_1 * deposit_0) * (total_supply / sqrt(pool_0 * pool_1))
        // == deposit_0 * total_supply / pool_0
        // 2. sqrt(deposit_1 * exchange_rate_1_to_0 * deposit_1) * (total_supply / sqrt(pool_1 * pool_1))
        // == deposit_1 * total_supply / pool_1
        std::cmp::min(
            deposits[0].multiply_ratio(total_supply, pools[0].amount),
            deposits[1].multiply_ratio(total_supply, pools[1].amount),
        )
    };

    // Update kLast
    let config = update_k_last(
        deps,
        config,
        pools[0].amount + deposits[0],
        pools[1].amount + deposits[1],
    )?;

    // mint LP token to sender
    messages.push(SubMsg {
        msg: WasmMsg::Execute {
            contract_addr: config.pair_info.liquidity_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: info.sender.to_string(),
                amount: share,
            })?,
            funds: vec![],
        }
        .into(),
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    Ok(Response {
        events: vec![],
        messages,
        attributes: vec![
            attr("action", "provide_liquidity"),
            attr("assets", format!("{}, {}", assets[0], assets[1])),
            attr("share", &share),
        ],
        data: None,
    })
}

pub fn withdraw_liquidity(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    sender: Addr,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let config: Config = CONFIG.load(deps.storage).unwrap();

    if info.sender != config.pair_info.liquidity_token {
        return Err(ContractError::Unauthorized {});
    }

    let pools: [Asset; 2] = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address)?;

    let total_supply = query_supply(&deps.querier, config.pair_info.liquidity_token.clone())?;

    let mut messages: Vec<SubMsg> = vec![];

    // Mint fee
    let (amount_minted, fee_msg) = mint_fee(
        deps.as_ref(),
        config.clone(),
        pools[0].amount,
        pools[1].amount,
        total_supply,
    );

    if let Some(f) = fee_msg {
        messages.push(f)
    }

    // Add minted fee to total supply for further calculations
    let total_supply = total_supply.add(amount_minted);

    let share_ratio: Decimal = Decimal::from_ratio(amount, total_supply);
    let refund_assets: Vec<Asset> = pools
        .iter()
        .map(|a| Asset {
            info: a.info.clone(),
            amount: a.amount * share_ratio,
        })
        .collect();

    // Update kLast
    update_k_last(
        deps.branch(),
        config.clone(),
        refund_assets[0].amount,
        refund_assets[1].amount,
    )?;

    // refund asset tokens
    messages.push(SubMsg {
        msg: refund_assets[0]
            .clone()
            .into_msg(&deps.querier, sender.clone())?,
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    messages.push(SubMsg {
        msg: refund_assets[1].clone().into_msg(&deps.querier, sender)?,
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    // burn liquidity token
    messages.push(SubMsg {
        msg: WasmMsg::Execute {
            contract_addr: config.pair_info.liquidity_token.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Burn { amount })?,
            funds: vec![],
        }
        .into(),
        id: 0,
        gas_limit: None,
        reply_on: ReplyOn::Never,
    });

    // update pool info
    Ok(Response {
        events: vec![],
        messages,
        attributes: vec![
            attr("action", "withdraw_liquidity"),
            attr("withdrawn_share", &amount.to_string()),
            attr(
                "refund_assets",
                format!("{}, {}", refund_assets[0], refund_assets[1]),
            ),
        ],
        data: None,
    })
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

    let config: Config = CONFIG.load(deps.storage)?;

    let pools: [Asset; 2] = config
        .pair_info
        .query_pools(&deps.querier, env.contract.address)?;

    let offer_pool: Asset;
    let ask_pool: Asset;

    // If the asset balance is already increased
    // To calculated properly we should subtract user deposit from the pool
    if offer_asset.info.equal(&pools[0].info) {
        offer_pool = Asset {
            amount: pools[0].amount.checked_sub(offer_asset.amount)?,
            info: pools[0].info.clone(),
        };
        ask_pool = pools[1].clone();
    } else if offer_asset.info.equal(&pools[1].info) {
        offer_pool = Asset {
            amount: pools[1].amount.checked_sub(offer_asset.amount)?,
            info: pools[1].info.clone(),
        };
        ask_pool = pools[0].clone();
    } else {
        return Err(ContractError::AssetMismatch {});
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

    let tax_amount = return_asset.compute_tax(&deps.querier)?;

    // 1. send collateral token from the contract to a user
    // 2. send inactive commission to collector
    Ok(Response {
        events: vec![],
        messages: vec![SubMsg {
            msg: return_asset.into_msg(&deps.querier, to.unwrap_or(sender))?,
            id: 0,
            gas_limit: None,
            reply_on: ReplyOn::Never,
        }],
        attributes: vec![
            attr("action", "swap"),
            attr("offer_asset", offer_asset.info.to_string()),
            attr("ask_asset", ask_pool.info.to_string()),
            attr("offer_amount", offer_amount.to_string()),
            attr("return_amount", return_amount.to_string()),
            attr("tax_amount", tax_amount.to_string()),
            attr("spread_amount", spread_amount.to_string()),
            attr("commission_amount", commission_amount.to_string()),
        ],
        data: None,
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Pair {} => to_binary(&query_pair_info(deps)?),
        QueryMsg::Pool {} => to_binary(&query_pool(deps)?),
        QueryMsg::KLast {} => to_binary(&query_k_last(deps)?),
        QueryMsg::Simulation { offer_asset } => to_binary(&query_simulation(deps, offer_asset)?),
        QueryMsg::ReverseSimulation { ask_asset } => {
            to_binary(&query_reverse_simulation(deps, ask_asset)?)
        }
    }
}

pub fn query_pair_info(deps: Deps) -> StdResult<PairInfo> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(config.pair_info)
}

pub fn query_k_last(deps: Deps) -> StdResult<Uint128> {
    let config: Config = CONFIG.load(deps.storage)?;
    Ok(config.k_last)
}

pub fn query_pool(deps: Deps) -> StdResult<PoolResponse> {
    let config: Config = CONFIG.load(deps.storage)?;
    let contract_addr = config.pair_info.contract_addr.clone();
    let assets: [Asset; 2] = config.pair_info.query_pools(&deps.querier, contract_addr)?;
    let total_share: Uint128 = query_supply(&deps.querier, config.pair_info.liquidity_token)?;

    let resp = PoolResponse {
        assets,
        total_share,
    };

    Ok(resp)
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
    let cp = Uint128::new(offer_pool.u128() * ask_pool.u128());
    let return_amount =
        ask_pool.checked_sub(cp.multiply_ratio(1u128, offer_pool + offer_amount))?;

    // calculate spread & commission
    let spread_amount: Uint128 = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool))
        .checked_sub(return_amount)
        .unwrap_or_else(|_| Uint128::zero());
    let commission_amount: Uint128 = return_amount * Decimal::from_str(&COMMISSION_RATE).unwrap();

    // commission will be absorbed to pool
    let return_amount: Uint128 = return_amount.checked_sub(commission_amount).unwrap();

    Ok((return_amount, spread_amount, commission_amount))
}

fn compute_offer_amount(
    offer_pool: Uint128,
    ask_pool: Uint128,
    ask_amount: Uint128,
) -> StdResult<(Uint128, Uint128, Uint128)> {
    // ask => offer
    // offer_amount = cp / (ask_pool - ask_amount / (1 - commission_rate)) - offer_pool
    let cp = Uint128::new(offer_pool.u128() * ask_pool.u128());
    let one_minus_commission =
        decimal_subtraction(Decimal::one(), Decimal::from_str(&COMMISSION_RATE).unwrap())?;

    let offer_amount: Uint128 = cp
        .multiply_ratio(
            1u128,
            ask_pool.checked_sub(ask_amount * reverse_decimal(one_minus_commission))?,
        )
        .checked_sub(offer_pool)?;

    let before_commission_deduction = ask_amount * reverse_decimal(one_minus_commission);
    let spread_amount = (offer_amount * Decimal::from_ratio(ask_pool, offer_pool))
        .checked_sub(before_commission_deduction)
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
) -> Result<(), ContractError> {
    if let (Some(max_spread), Some(belief_price)) = (max_spread, belief_price) {
        let expected_return = offer_amount * reverse_decimal(belief_price);
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
    slippage_tolerance: &Option<Decimal>,
    deposits: &[Uint128; 2],
    pools: &[Asset; 2],
) -> Result<(), ContractError> {
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
            return Err(ContractError::MaxSlippageAssertion {});
        }
    }

    Ok(())
}

pub fn update_k_last(deps: DepsMut, config: Config, x: Uint128, y: Uint128) -> StdResult<Config> {
    let config = Config {
        k_last: Uint128::new(x.u128() * y.u128()),
        ..config
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(config)
}

pub fn mint_fee(
    deps: Deps,
    config: Config,
    x: Uint128,
    y: Uint128,
    total_supply: Uint128,
) -> (Uint128, Option<SubMsg>) {
    let fee_address: Addr = deps
        .querier
        .query(&QueryRequest::Wasm(WasmQuery::Smart {
            contract_addr: config.factory_addr.to_string(),
            msg: to_binary(&FactoryQueryMsg::FeeAddress {}).unwrap(),
        }))
        .unwrap();

    if fee_address == Addr::unchecked("") {
        return (Uint128::zero(), None);
    }

    if config.k_last.is_zero() {
        return (Uint128::zero(), None);
    }

    let root_k = Uint128::new((x.u128() * y.u128()).integer_sqrt());
    let root_k_last = Uint128::new(config.k_last.u128().integer_sqrt());

    if root_k > root_k_last {
        let numerator = total_supply
            .checked_mul(root_k.checked_sub(root_k_last).unwrap())
            .unwrap();
        let denominator = root_k
            .checked_mul(Uint128::new(5))
            .unwrap()
            .checked_add(root_k_last)
            .unwrap();
        let liquidity = numerator.checked_div(denominator).unwrap();
        if !liquidity.is_zero() {
            return (
                liquidity,
                Some(SubMsg {
                    msg: WasmMsg::Execute {
                        contract_addr: config.pair_info.liquidity_token.to_string(),
                        msg: to_binary(&Cw20ExecuteMsg::Mint {
                            recipient: fee_address.to_string(),
                            amount: liquidity,
                        })
                        .unwrap(),
                        funds: vec![],
                    }
                    .into(),
                    id: 0,
                    gas_limit: None,
                    reply_on: ReplyOn::Never,
                }),
            );
        }
    }

    (Uint128::zero(), None)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
