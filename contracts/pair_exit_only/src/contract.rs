use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, coin, ensure, entry_point, from_json, to_json_binary, wasm_execute, Addr, Binary, Deps,
    DepsMut, Empty, Env, MessageInfo, Response, StdError, StdResult, Uint128,
};
use cw2::get_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg, TokenInfoResponse};
use cw_utils::must_pay;

use astroport::asset::{Asset, PairInfo};
use astroport::pair::{ExecuteMsg, PoolResponse, QueryMsg};
use astroport::token_factory::tf_burn_msg;

use crate::error::ContractError;
use crate::state::{is_native_lp, load_pair_info};

pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Pair contracts this code can take over. All of them keep `pair_info` in their `config` and
/// hold the pool's assets as plain balances.
const SUPPORTED: &[&str] = &[
    "astroport-pair-concentrated",
    "astroport-pair-concentrated-duality",
    "astroport-pair-transmuter",
];

/// Withdrawal through a CW20 LP token's `send`. Field names match the pairs' own hook.
#[cw_serde]
enum Cw20HookMsg {
    WithdrawLiquidity {
        #[serde(default)]
        assets: Vec<Asset>,
        #[serde(default)]
        min_assets_to_receive: Option<Vec<Asset>>,
    },
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> Result<Response, ContractError> {
    Err(ContractError::NotInstantiable {})
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let pair_info = load_pair_info(deps.storage)?;
    match msg {
        ExecuteMsg::WithdrawLiquidity {
            assets,
            min_assets_to_receive,
        } => {
            ensure!(is_native_lp(&pair_info), ContractError::ExitOnly {});
            let amount = must_pay(&info, &pair_info.liquidity_token)?;
            withdraw(
                deps.as_ref(),
                &env,
                &pair_info,
                info.sender,
                amount,
                assets,
                min_assets_to_receive,
            )
        }
        ExecuteMsg::Receive(Cw20ReceiveMsg {
            sender,
            amount,
            msg,
        }) => {
            ensure!(
                !is_native_lp(&pair_info) && info.sender == pair_info.liquidity_token,
                ContractError::Unauthorized {}
            );
            let Ok(Cw20HookMsg::WithdrawLiquidity {
                assets,
                min_assets_to_receive,
            }) = from_json(&msg)
            else {
                return Err(ContractError::ExitOnly {});
            };
            let sender = deps.api.addr_validate(&sender)?;
            withdraw(
                deps.as_ref(),
                &env,
                &pair_info,
                sender,
                amount,
                assets,
                min_assets_to_receive,
            )
        }
        _ => Err(ContractError::ExitOnly {}),
    }
}

/// LP tokens in circulation, including those being withdrawn in this call
fn total_share(deps: Deps, pair_info: &PairInfo) -> StdResult<Uint128> {
    if is_native_lp(pair_info) {
        Ok(deps
            .querier
            .query_supply(&pair_info.liquidity_token)?
            .amount)
    } else {
        Ok(deps
            .querier
            .query_wasm_smart::<TokenInfoResponse>(
                &pair_info.liquidity_token,
                &Cw20QueryMsg::TokenInfo {},
            )?
            .total_supply)
    }
}

/// The pool's assets, at the balances it holds
fn pool_assets(deps: Deps, env: &Env, pair_info: &PairInfo) -> StdResult<Vec<Asset>> {
    pair_info
        .asset_infos
        .iter()
        .map(|info| {
            Ok(Asset {
                info: info.clone(),
                amount: info.query_pool(&deps.querier, &env.contract.address)?,
            })
        })
        .collect()
}

/// `amount` LP tokens' share of every pool asset, rounded down
fn share(deps: Deps, env: &Env, pair_info: &PairInfo, amount: Uint128) -> StdResult<Vec<Asset>> {
    let total = total_share(deps, pair_info)?;
    ensure!(
        amount <= total,
        StdError::generic_err(format!(
            "{amount} is more than the {total} LP tokens issued"
        ))
    );
    let assets = pool_assets(deps, env, pair_info)?;
    if total.is_zero() {
        return Ok(assets
            .into_iter()
            .map(|a| Asset {
                info: a.info,
                amount: Uint128::zero(),
            })
            .collect());
    }
    Ok(assets
        .into_iter()
        .map(|a| Asset {
            amount: a.amount.multiply_ratio(amount, total),
            info: a.info,
        })
        .collect())
}

fn withdraw(
    deps: Deps,
    env: &Env,
    pair_info: &PairInfo,
    sender: Addr,
    amount: Uint128,
    assets: Vec<Asset>,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<Response, ContractError> {
    ensure!(assets.is_empty(), ContractError::ImbalancedWithdraw {});

    let refund = share(deps, env, pair_info, amount)?;
    // The LP tokens would be burned for nothing, e.g. before the pool is refilled
    ensure!(
        refund.iter().any(|a| !a.amount.is_zero()),
        ContractError::NothingToWithdraw {}
    );

    for min in min_assets_to_receive.unwrap_or_default() {
        let received = refund
            .iter()
            .find(|a| a.info == min.info)
            .ok_or_else(|| ContractError::InvalidAsset(min.info.to_string()))?
            .amount;
        ensure!(
            received >= min.amount,
            ContractError::WithdrawSlippageViolation {
                asset_name: min.info.to_string(),
                received,
                expected: min.amount,
            }
        );
    }

    let mut messages = refund
        .iter()
        .filter(|a| !a.amount.is_zero())
        .map(|a| a.clone().into_msg(&sender))
        .collect::<StdResult<Vec<_>>>()?;
    messages.push(if is_native_lp(pair_info) {
        tf_burn_msg(
            env.contract.address.to_string(),
            coin(amount.u128(), &pair_info.liquidity_token),
        )
    } else {
        wasm_execute(
            &pair_info.liquidity_token,
            &Cw20ExecuteMsg::Burn { amount },
            vec![],
        )?
        .into()
    });

    Ok(Response::new().add_messages(messages).add_attributes([
        attr("action", "withdraw_liquidity"),
        attr("sender", sender),
        attr("withdrawn_share", amount),
        attr(
            "refund_assets",
            refund
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", "),
        ),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let pair_info = load_pair_info(deps.storage)?;
    match msg {
        QueryMsg::Pair {} => to_json_binary(&pair_info),
        QueryMsg::Pool {} => to_json_binary(&PoolResponse {
            assets: pool_assets(deps, &env, &pair_info)?,
            total_share: total_share(deps, &pair_info)?,
        }),
        QueryMsg::Share { amount } => to_json_binary(&share(deps, &env, &pair_info, amount)?),
        QueryMsg::SimulateWithdraw { lp_amount } => {
            to_json_binary(&share(deps, &env, &pair_info, lp_amount)?)
        }
        _ => Err(StdError::generic_err(
            ContractError::ExitOnly {}.to_string(),
        )),
    }
}

#[cw_serde]
pub struct MigrateMsg {}

/// Takes over a pair, leaving its storage as it is. The pair's version stays in cw2, so it can
/// later be migrated back to a newer build of its own contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let version = get_contract_version(deps.storage)?;
    let refuse = |reason: &str| ContractError::MigrationError {
        contract: version.contract.clone(),
        version: version.version.clone(),
        reason: reason.to_string(),
    };

    ensure!(
        SUPPORTED.contains(&version.contract.as_str()),
        refuse("not a supported pair contract")
    );
    let pair_info = load_pair_info(deps.storage).map_err(|_| refuse("unreadable pair config"))?;
    ensure!(
        pair_info.contract_addr == env.contract.address,
        refuse("pair config is for another address")
    );
    // A pool already exit-only through pair_concentrated 4.3 withholds an asset; paying it out
    // here would undo that
    ensure!(
        deps.storage.get(b"exit_only").is_none(),
        refuse("already exit-only with a withheld asset")
    );
    // Liquidity placed on the Duality orderbook isn't in the pool's balance
    if let Some(raw) = deps.storage.get(b"orderbook_config") {
        #[derive(serde::Deserialize)]
        struct Orderbook {
            enabled: bool,
        }
        let orderbook: Orderbook =
            from_json(&raw).map_err(|_| refuse("unreadable orderbook config"))?;
        ensure!(
            !orderbook.enabled,
            refuse("orderbook integration is enabled")
        );
    }

    Ok(Response::new().add_attributes([
        attr("action", "migrate_to_exit_only"),
        attr("pair_contract", version.contract),
        attr("pair_version", version.version),
        attr("exit_only_version", CONTRACT_VERSION),
        attr("liquidity_token", pair_info.liquidity_token),
    ]))
}
