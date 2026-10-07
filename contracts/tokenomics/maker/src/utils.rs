use std::collections::HashSet;
use std::str::FromStr;

use cosmwasm_std::{
    coins, to_json_binary, Addr, Api, Binary, CosmosMsg, Decimal, QuerierWrapper, StdResult,
    Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use astroport::asset::{Asset, AssetInfo};
use astroport::pair::MAX_ALLOWED_SLIPPAGE;
use astroport_router::msg::{
    Cw20HookMsg as RouterCw20HookMsg, ExecuteMsg as RouterExecuteMsg, QueryMsg as RouterQueryMsg,
    SimulateSwapOperationsResponse,
};

use crate::error::ContractError;
use crate::msg::{Leg, LegAction, SwapOperation, COOLDOWN_LIMITS, MAX_LEGS};

/// The most hops a route may have.
pub const MAX_ROUTE_HOPS: usize = 5;
/// Probe sizes tried in turn, as fractions of the amount swapped: the first whose output is at
/// least [`MIN_PROBE_OUT`] sets the route's rate. A smaller probe has less price impact of its
/// own; a larger one is needed for small amounts, where the integer rounding of a tiny output
/// would distort the rate.
const PROBE_DIVISORS: [u128; 4] = [1000, 100, 10, 1];
/// The least a probe may return for its rate to be used: its rounding is then under 0.1%.
const MIN_PROBE_OUT: u128 = 1_000;

pub fn op_assets(op: &SwapOperation) -> (&AssetInfo, &AssetInfo) {
    match op {
        SwapOperation::AstroSwap {
            offer_asset_info,
            ask_asset_info,
        }
        | SwapOperation::PoolSwap {
            offer_asset_info,
            ask_asset_info,
            ..
        } => (offer_asset_info, ask_asset_info),
    }
}

/// The asset a route ends in, or `start` for an empty route.
pub fn route_output(start: &AssetInfo, route: &[SwapOperation]) -> AssetInfo {
    route
        .last()
        .map(|op| op_assets(op).1.clone())
        .unwrap_or_else(|| start.clone())
}

/// Checks that `route` is a chain of hops starting at `from` and, if given, ending at `to`, and
/// that it doesn't visit the same asset twice.
pub fn validate_route(
    api: &dyn Api,
    route: &[SwapOperation],
    from: &AssetInfo,
    to: Option<&AssetInfo>,
) -> Result<(), ContractError> {
    let invalid = |reason: String| ContractError::InvalidRoute { reason };

    if route.is_empty() {
        return Err(invalid("must have at least one hop".to_string()));
    }
    if route.len() > MAX_ROUTE_HOPS {
        return Err(invalid(format!("can have at most {MAX_ROUTE_HOPS} hops")));
    }

    let mut current = from.clone();
    let mut visited = vec![from.to_string()];
    for op in route {
        let (offer, ask) = op_assets(op);
        offer.check(api)?;
        ask.check(api)?;

        if offer != &current {
            return Err(invalid(format!(
                "hop offers {offer} but the previous hop ends in {current}"
            )));
        }
        if offer == ask {
            return Err(invalid(format!("hop swaps {offer} into itself")));
        }
        if visited.contains(&ask.to_string()) {
            return Err(invalid(format!("visits {ask} twice")));
        }
        if let SwapOperation::PoolSwap { pool_addr, .. } = op {
            api.addr_validate(pool_addr)?;
        }

        visited.push(ask.to_string());
        current = ask.clone();
    }

    if let Some(to) = to {
        if &current != to {
            return Err(invalid(format!("ends in {current}, not {to}")));
        }
    }

    Ok(())
}

/// Checks the legs' shares, routes and targets. `maker` is the Maker's own address, which a leg
/// can't send to or deposit into.
pub fn validate_legs(
    api: &dyn Api,
    legs: &[Leg],
    base_asset: &AssetInfo,
    maker: &Addr,
) -> Result<(), ContractError> {
    if legs.is_empty() || legs.len() > MAX_LEGS {
        return Err(ContractError::InvalidLegCount { max: MAX_LEGS });
    }

    let mut total = Decimal::zero();
    for leg in legs {
        if leg.share.is_zero() {
            return Err(ContractError::InvalidLegShares {
                total: "a zero share".to_string(),
            });
        }
        total = total.checked_add(leg.share)?;

        if !leg.route.is_empty() {
            validate_route(api, &leg.route, base_asset, None)?;
        }

        let target = match &leg.action {
            LegAction::Send { recipient } => api.addr_validate(recipient)?,
            LegAction::Deposit { contract, .. } => api.addr_validate(contract)?,
        };
        if target == *maker {
            return Err(ContractError::InvalidLeg {
                reason: "can't target the Maker itself".to_string(),
            });
        }
    }

    if total != Decimal::one() {
        return Err(ContractError::InvalidLegShares {
            total: total.to_string(),
        });
    }

    Ok(())
}

/// Checks that no asset is listed twice.
pub fn ensure_unique<'a>(
    assets: impl IntoIterator<Item = &'a AssetInfo>,
) -> Result<(), ContractError> {
    let mut seen = HashSet::new();
    for asset in assets {
        if !seen.insert(asset.to_string()) {
            return Err(ContractError::DuplicateAsset {
                asset: asset.to_string(),
            });
        }
    }
    Ok(())
}

/// Validates the collector addresses and checks that none is listed twice.
pub fn validate_collectors(
    api: &dyn Api,
    collectors: &[String],
) -> Result<Vec<Addr>, ContractError> {
    let mut seen = HashSet::new();
    collectors
        .iter()
        .map(|collector| {
            let addr = api.addr_validate(collector)?;
            if !seen.insert(addr.clone()) {
                return Err(ContractError::DuplicateCollector {
                    address: addr.to_string(),
                });
            }
            Ok(addr)
        })
        .collect()
}

pub fn validate_max_spread(max_spread: Decimal) -> Result<(), ContractError> {
    if max_spread.is_zero() || max_spread > Decimal::from_str(MAX_ALLOWED_SLIPPAGE)? {
        return Err(ContractError::IncorrectMaxSpread {});
    }
    Ok(())
}

pub fn validate_cooldown(maybe_cooldown: Option<u64>) -> Result<(), ContractError> {
    if let Some(collect_cooldown) = maybe_cooldown {
        if !COOLDOWN_LIMITS.contains(&collect_cooldown) {
            return Err(ContractError::IncorrectCooldown {
                min: *COOLDOWN_LIMITS.start(),
                max: *COOLDOWN_LIMITS.end(),
            });
        }
    }
    Ok(())
}

/// `None` and `Some(0)` both mean no cooldown.
pub fn cooldown_or_none(cooldown: Option<u64>) -> Option<u64> {
    cooldown.filter(|cooldown| *cooldown > 0)
}

/// The least a swap of `amount` along `route` may return: its value at the route's rate for a
/// small probe (see [`PROBE_DIVISORS`]), less `max_spread`. A swap whose price impact is more
/// than `max_spread` fails its minimum and reverts.
///
/// Returns zero for dust that's too small to price, which the caller skips.
pub fn minimum_receive(
    querier: &QuerierWrapper,
    router: &str,
    route: &[SwapOperation],
    amount: Uint128,
    max_spread: Decimal,
) -> Result<Uint128, ContractError> {
    for divisor in PROBE_DIVISORS {
        let probe = amount.checked_div(Uint128::new(divisor))?;
        if probe.is_zero() {
            continue;
        }

        let probe_out: SimulateSwapOperationsResponse = querier.query_wasm_smart(
            router,
            &RouterQueryMsg::SimulateSwapOperations {
                offer_amount: probe,
                operations: route.to_vec(),
            },
        )?;
        if probe_out.amount.u128() < MIN_PROBE_OUT {
            continue;
        }

        let expected = amount.checked_multiply_ratio(probe_out.amount, probe)?;
        return Ok(expected * (Decimal::one() - max_spread));
    }

    Ok(Uint128::zero())
}

/// Swaps `offer` along `route` through the router, sending the output to `to` (the Maker if
/// not set).
pub fn swap_msg(
    router: &str,
    offer: Asset,
    route: Vec<SwapOperation>,
    minimum_receive: Uint128,
    to: Option<String>,
) -> StdResult<CosmosMsg> {
    match &offer.info {
        AssetInfo::NativeToken { denom } => Ok(WasmMsg::Execute {
            contract_addr: router.to_string(),
            msg: to_json_binary(&RouterExecuteMsg::ExecuteSwapOperations {
                operations: route,
                minimum_receive,
                to,
            })?,
            funds: coins(offer.amount.u128(), denom),
        }
        .into()),
        AssetInfo::Token { contract_addr } => Ok(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Send {
                contract: router.to_string(),
                amount: offer.amount,
                msg: to_json_binary(&RouterCw20HookMsg::ExecuteSwapOperations {
                    operations: route,
                    minimum_receive,
                    to,
                })?,
            })?,
            funds: vec![],
        }
        .into()),
    }
}

/// Executes `contract` with `msg` and `asset` attached: native funds, or a CW20 `send` with
/// `msg` as its hook.
pub fn deposit_msg(asset: Asset, contract: &str, msg: Binary) -> StdResult<CosmosMsg> {
    match &asset.info {
        AssetInfo::NativeToken { denom } => Ok(WasmMsg::Execute {
            contract_addr: contract.to_string(),
            msg,
            funds: coins(asset.amount.u128(), denom),
        }
        .into()),
        AssetInfo::Token { contract_addr } => Ok(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Send {
                contract: contract.to_string(),
                amount: asset.amount,
                msg,
            })?,
            funds: vec![],
        }
        .into()),
    }
}
