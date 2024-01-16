#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, wasm_execute, Addr, DepsMut, Env, MessageInfo, Reply, ReplyOn,
    Response, StdError, StdResult, SubMsg, Uint128,
};
use cw20::{Cw20ExecuteMsg, Expiration};

use astroport::asset::{addr_opt_validate, Asset, AssetInfo, AssetInfoExt, PairInfo};
use astroport::factory::PairType;
use astroport::liquidity_manager::{Cw20HookMsg, ExecuteMsg, InstantiateMsg};
use astroport::pair::{
    Cw20HookMsg as PairCw20HookMsg, ExecuteMsg as PairExecuteMsg, QueryMsg as PairQueryMsg,
    QueryMsg,
};
use astroport::querier::query_supply;
use astroport_pair::contract::get_share_in_assets;

use crate::error::ContractError;
use crate::state::{ActionParams, Config, ReplyData, CONFIG, REPLY_DATA};
use crate::utils::{query_cw20_minter, query_lp_amount, xyk_provide_simulation};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    CONFIG.save(
        deps.storage,
        &Config {
            factory_addr: deps.api.addr_validate(&msg.astroport_factory)?,
        },
    )?;

    Ok(Response::default()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", "liquidity_manager"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::ProvideLiquidity {
            pair_addr,
            pair_msg: msg,
            min_lp_to_receive,
        } => {
            let pair_addr = deps.api.addr_validate(&pair_addr)?;
            provide_liquidity(deps, info, env, pair_addr, msg, min_lp_to_receive)
        }
        ExecuteMsg::Receive(cw20_msg) => match from_json(&cw20_msg.msg)? {
            Cw20HookMsg::WithdrawLiquidity {
                pair_msg: msg,
                min_assets_to_receive,
            } if matches!(&msg, PairCw20HookMsg::WithdrawLiquidity { .. }) => withdraw_liquidity(
                deps,
                info.sender,
                Addr::unchecked(cw20_msg.sender),
                cw20_msg.amount,
                msg,
                min_assets_to_receive,
            ),
            _ => Err(ContractError::UnsupportedCw20HookMsg {}),
        },
    }
}

const WITHDRAW_LIQUIDITY_REPLY_ID: u64 = 1;
const PROVIDE_LIQUIDITY_REPLY_ID: u64 = 2;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg.id {
        WITHDRAW_LIQUIDITY_REPLY_ID => match REPLY_DATA.load(deps.storage)? {
            ReplyData {
                receiver,
                params:
                    ActionParams::Withdraw {
                        pair_addr,
                        min_assets_to_receive,
                    },
            } => {
                let pair_info: PairInfo = deps
                    .querier
                    .query_wasm_smart(pair_addr, &QueryMsg::Pair {})?;
                let mut withdrawn_assets =
                    pair_info.query_pools(&deps.querier, env.contract.address)?;
                if withdrawn_assets[0].info.ne(&min_assets_to_receive[0].info) {
                    withdrawn_assets.swap(0, 1);
                }

                let messages = withdrawn_assets
                    .into_iter()
                    .zip(min_assets_to_receive.iter())
                    .map(|(withdrawn, min_to_receive)| {
                        if withdrawn.amount < min_to_receive.amount {
                            Err(ContractError::WithdrawSlippageViolation {
                                asset_name: withdrawn.info.to_string(),
                                received: withdrawn.amount,
                                expected: min_to_receive.amount,
                            })
                        } else {
                            Ok(withdrawn.into_msg(&receiver)?)
                        }
                    })
                    .collect::<Result<Vec<_>, ContractError>>()?;

                Ok(Response::new()
                    .add_messages(messages)
                    .add_attribute("liquidity_manager", "withdraw_check_passed"))
            }
            _ => Err(ContractError::InvalidReplyData {}),
        },
        PROVIDE_LIQUIDITY_REPLY_ID => match REPLY_DATA.load(deps.storage)? {
            ReplyData {
                receiver,
                params:
                    ActionParams::Provide {
                        lp_token_addr,
                        lp_amount_before,
                        min_lp_to_receive,
                        staked_in_generator,
                    },
            } => {
                let factory_addr = CONFIG.load(deps.storage)?.factory_addr;
                let lp_amount_after = query_lp_amount(
                    deps.querier,
                    lp_token_addr,
                    factory_addr,
                    staked_in_generator,
                    &receiver,
                )?;

                // allowing 1 to absorb rounding errors
                if lp_amount_after - lp_amount_before < min_lp_to_receive {
                    Err(ContractError::ProvideSlippageViolation(
                        lp_amount_after - lp_amount_before,
                        min_lp_to_receive,
                    ))
                } else {
                    Ok(Response::new().add_attribute("liquidity_manager", "provide_check_passed"))
                }
            }
            _ => Err(ContractError::InvalidReplyData {}),
        },
        _ => Err(StdError::generic_err(format!("Unsupported reply id {}", msg.id)).into()),
    }
}

fn provide_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    pair_addr: Addr,
    exec_msg: PairExecuteMsg,
    min_lp_to_receive: Option<Uint128>,
) -> Result<Response, ContractError> {
    if let PairExecuteMsg::ProvideLiquidity {
        mut assets,
        slippage_tolerance,
        auto_stake,
        receiver,
    } = exec_msg
    {
        if assets.len() != 2 {
            return Err(ContractError::WrongPoolLength {});
        }

        let pair_info: PairInfo = deps
            .querier
            .query_wasm_smart(&pair_addr, &PairQueryMsg::Pair {})?;
        let mut funds = info.funds;
        let mut submessages = match &pair_info.pair_type {
            // Xyk pair has unfair minting policy; Returning excess assets back to user
            PairType::Xyk {} => {
                let pools = pair_info.query_pools(&deps.querier, &pair_addr)?;
                // Initial provide is always fair because initial LP dictates the price
                if !pools[0].amount.is_zero() && !pools[1].amount.is_zero() {
                    let predicted_lp_amount = xyk_provide_simulation(
                        deps.querier,
                        &pools,
                        &pair_info,
                        slippage_tolerance,
                        assets.clone(),
                    )?;

                    if pools[0].info.ne(&assets[0].info) {
                        assets.swap(0, 1);
                    }

                    // Add user's deposits
                    let pools = pools
                        .into_iter()
                        .zip(assets.iter())
                        .map(|(mut pool, asset)| {
                            pool.amount += asset.amount;
                            pool
                        })
                        .collect::<Vec<_>>();
                    let total_share = query_supply(&deps.querier, &pair_info.liquidity_token)?;
                    let share = get_share_in_assets(
                        &pools,
                        predicted_lp_amount,
                        total_share + predicted_lp_amount,
                    );

                    assets
                        .iter_mut()
                        .zip(share.iter())
                        .filter_map(|(asset, share_asset)| {
                            let maybe_repay = match &asset.info {
                                AssetInfo::Token { .. } => None,
                                AssetInfo::NativeToken { denom } => {
                                    let excess_coins =
                                        asset.amount.saturating_sub(share_asset.amount);

                                    // `xyk_provide_simulation` guarantees that native asset is present in funds
                                    funds
                                        .iter_mut()
                                        .find(|c| &c.denom == denom)
                                        .map(|c| {
                                            c.amount = share_asset.amount;
                                        })
                                        .unwrap();

                                    if !excess_coins.is_zero() {
                                        Some(
                                            asset
                                                .info
                                                .with_balance(excess_coins)
                                                .into_msg(&info.sender)
                                                .map(SubMsg::new),
                                        )
                                    } else {
                                        None
                                    }
                                }
                            };

                            asset.amount = share_asset.amount;

                            maybe_repay
                        })
                        .collect::<StdResult<_>>()?
                } else {
                    vec![]
                }
            }
            _ => vec![],
        };

        // pull cw20 tokens and increase allowance for pair contract
        let allowance_submessages = assets
            .iter()
            .filter_map(|asset| match &asset.info {
                AssetInfo::Token { contract_addr } if !asset.amount.is_zero() => {
                    let transfer_from_msg = wasm_execute(
                        contract_addr,
                        &Cw20ExecuteMsg::TransferFrom {
                            owner: info.sender.to_string(),
                            recipient: env.contract.address.to_string(),
                            amount: asset.amount,
                        },
                        vec![],
                    );
                    let increase_allowance_msg = wasm_execute(
                        contract_addr,
                        &Cw20ExecuteMsg::IncreaseAllowance {
                            spender: pair_addr.to_string(),
                            amount: asset.amount,
                            expires: Some(Expiration::AtHeight(env.block.height + 1)),
                        },
                        vec![],
                    );
                    Some([transfer_from_msg, increase_allowance_msg])
                }
                _ => None,
            })
            .flatten()
            .map(|v| v.map(SubMsg::new))
            .collect::<StdResult<Vec<_>>>()?;

        submessages.extend(allowance_submessages);

        let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or(info.sender);
        let tweaked_exec_msg = PairExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver: Some(receiver.to_string()),
        };
        let mut provide_msg = SubMsg::new(wasm_execute(&pair_addr, &tweaked_exec_msg, funds)?);

        if let Some(min_lp_to_receive) = min_lp_to_receive {
            let staked_in_generator = auto_stake.unwrap_or(false);
            let config = CONFIG.load(deps.storage)?;
            let lp_amount_before = query_lp_amount(
                deps.querier,
                pair_info.liquidity_token.to_string(),
                config.factory_addr,
                staked_in_generator,
                &receiver.to_string(),
            )?;

            REPLY_DATA.save(
                deps.storage,
                &ReplyData {
                    receiver: receiver.to_string(),
                    params: ActionParams::Provide {
                        lp_token_addr: pair_info.liquidity_token.to_string(),
                        lp_amount_before,
                        min_lp_to_receive,
                        staked_in_generator,
                    },
                },
            )?;
            provide_msg.reply_on = ReplyOn::Success;
            provide_msg.id = PROVIDE_LIQUIDITY_REPLY_ID;
        } else {
            // no need to reply as user decided not to enforce minimum LP token amount check
        }
        submessages.push(provide_msg);

        Ok(Response::new()
            .add_submessages(submessages)
            .add_attribute("contract", "liquidity_manager")
            .add_attribute("action", "provide_liquidity"))
    } else {
        Err(ContractError::UnsupportedExecuteMsg {})
    }
}

fn withdraw_liquidity(
    deps: DepsMut,
    lp_token_addr: Addr,
    receiver: Addr,
    amount: Uint128,
    inner_msg: PairCw20HookMsg,
    min_assets_to_receive: Vec<Asset>,
) -> Result<Response, ContractError> {
    let pair_addr = query_cw20_minter(deps.querier, lp_token_addr.clone())?;
    let pair_info: PairInfo = deps
        .querier
        .query_wasm_smart(&pair_addr, &QueryMsg::Pair {})?;

    if pair_info.asset_infos.len() != 2 {
        return Err(ContractError::WrongPoolLength {});
    }

    if pair_info.asset_infos.len() != min_assets_to_receive.len() {
        return Err(ContractError::WrongAssetLength {
            expected: pair_info.asset_infos.len(),
            actual: min_assets_to_receive.len(),
        });
    }
    // Check `min_assets_to_receive` belong to the pair
    for asset in &min_assets_to_receive {
        if !pair_info.asset_infos.contains(&asset.info) {
            return Err(ContractError::AssetNotInPair(asset.info.to_string()));
        }
    }

    let withdraw_msg = wasm_execute(
        lp_token_addr,
        &Cw20ExecuteMsg::Send {
            contract: pair_addr.to_string(),
            amount,
            msg: to_json_binary(&inner_msg)?,
        },
        vec![],
    )?;
    let msg_with_reply = SubMsg::reply_on_success(withdraw_msg, WITHDRAW_LIQUIDITY_REPLY_ID);

    REPLY_DATA.save(
        deps.storage,
        &ReplyData {
            receiver: receiver.to_string(),
            params: ActionParams::Withdraw {
                pair_addr,
                min_assets_to_receive,
            },
        },
    )?;

    Ok(Response::new()
        .add_submessage(msg_with_reply)
        .add_attribute("contract", "liquidity_manager")
        .add_attribute("action", "withdraw_liquidity"))
}
