use std::collections::HashSet;

use cosmwasm_std::{
    attr, ensure, ensure_eq, entry_point, to_json_binary, wasm_execute, Addr, Binary, Decimal,
    Deps, DepsMut, Env, MessageInfo, Order, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};

use crate::error::ContractError;
use crate::migration;
use crate::msg::{
    AssetWithLimit, BalancesResponse, Config, ExecuteMsg, InstantiateMsg, LegAction, MigrateMsg,
    QueryMsg, RouteResponse, SeizeConfig, SwapOperation,
};
use crate::state::{
    CONFIG, LAST_COLLECT_TS, LEG_SNAPSHOT, OWNERSHIP_PROPOSAL, ROUTES, SEIZE_CONFIG,
};
use crate::utils::{
    deposit_msg, ensure_unique, minimum_receive, route_output, swap_msg, validate_cooldown,
    validate_legs, validate_max_spread, validate_route,
};

/// Contract name that is used for migration.
pub(crate) const CONTRACT_NAME: &str = "astroport-maker";
/// Contract version that is used for migration.
pub(crate) const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Default max spread (as a percentage) for the Maker's swaps.
const DEFAULT_MAX_SPREAD: u64 = 5;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let config = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        router: deps.api.addr_validate(&msg.router)?,
        base_asset: msg.base_asset,
        max_spread: msg
            .max_spread
            .unwrap_or_else(|| Decimal::percent(DEFAULT_MAX_SPREAD)),
        collect_cooldown: msg.collect_cooldown,
        legs: msg.legs,
    };
    validate_config(deps.as_ref(), &config)?;
    CONFIG.save(deps.storage, &config)?;

    set_routes(
        deps.storage,
        deps.api,
        &config.base_asset,
        msg.routes,
        vec![],
    )?;

    LAST_COLLECT_TS.save(deps.storage, &env.block.time.seconds())?;
    SEIZE_CONFIG.save(
        deps.storage,
        &SeizeConfig {
            // set to an invalid address initially; the owner must set it explicitly
            receiver: Addr::unchecked(""),
            seizable_assets: vec![],
        },
    )?;

    Ok(Response::new().add_attributes([
        attr("action", "instantiate"),
        attr("owner", config.owner),
        attr("base_asset", config.base_asset.to_string()),
    ]))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Collect { assets } => collect(deps, env, assets),
        ExecuteMsg::Distribute {} => {
            ensure_self(&env, &info)?;
            distribute(deps, env)
        }
        ExecuteMsg::SnapshotLeg { leg } => {
            ensure_self(&env, &info)?;
            snapshot_leg(deps, env, leg)
        }
        ExecuteMsg::DepositLeg { leg } => {
            ensure_self(&env, &info)?;
            deposit_leg(deps, env, leg)
        }
        ExecuteMsg::UpdateConfig {
            router,
            base_asset,
            max_spread,
            collect_cooldown,
            legs,
        } => {
            let mut config = CONFIG.load(deps.storage)?;
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

            if let Some(router) = router {
                config.router = deps.api.addr_validate(&router)?;
            }
            if let Some(base_asset) = base_asset {
                config.base_asset = base_asset;
            }
            if let Some(max_spread) = max_spread {
                config.max_spread = max_spread;
            }
            if let Some(collect_cooldown) = collect_cooldown {
                config.collect_cooldown = Some(collect_cooldown);
            }
            if let Some(legs) = legs {
                config.legs = legs;
            }

            // checked as a whole, so a new base asset must come with legs that start from it
            validate_config(deps.as_ref(), &config)?;
            CONFIG.save(deps.storage, &config)?;

            Ok(Response::new().add_attribute("action", "update_config"))
        }
        ExecuteMsg::UpdateRoutes { add, remove } => {
            let config = CONFIG.load(deps.storage)?;
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

            set_routes(deps.storage, deps.api, &config.base_asset, add, remove)?;

            Ok(Response::new().add_attribute("action", "update_routes"))
        }
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(Into::into)
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(Into::into)
        }
        ExecuteMsg::Seize { assets } => seize(deps, env, assets),
        ExecuteMsg::UpdateSeizeConfig {
            receiver,
            seizable_assets,
        } => {
            let config = CONFIG.load(deps.storage)?;
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

            SEIZE_CONFIG.update::<_, StdError>(deps.storage, |mut seize_config| {
                if let Some(receiver) = receiver {
                    seize_config.receiver = deps.api.addr_validate(&receiver)?;
                }
                seize_config.seizable_assets = seizable_assets;
                Ok(seize_config)
            })?;

            Ok(Response::new().add_attribute("action", "update_seize_config"))
        }
    }
}

fn ensure_self(env: &Env, info: &MessageInfo) -> Result<(), ContractError> {
    ensure_eq!(
        info.sender,
        env.contract.address,
        ContractError::Unauthorized {}
    );
    Ok(())
}

pub(crate) fn validate_config(deps: Deps, config: &Config) -> Result<(), ContractError> {
    config.base_asset.check(deps.api)?;
    validate_max_spread(config.max_spread)?;
    validate_cooldown(config.collect_cooldown)?;
    validate_legs(deps.api, &config.legs, &config.base_asset)
}

/// Removes `remove`, then sets each route in `add`, which must run from its asset to the base
/// asset.
pub(crate) fn set_routes(
    storage: &mut dyn cosmwasm_std::Storage,
    api: &dyn cosmwasm_std::Api,
    base_asset: &AssetInfo,
    add: Vec<(AssetInfo, Vec<SwapOperation>)>,
    remove: Vec<AssetInfo>,
) -> Result<(), ContractError> {
    ensure_unique(add.iter().map(|(asset, _)| asset))?;

    for asset in remove {
        ROUTES.remove(storage, asset.to_string());
    }

    for (asset, route) in add {
        asset.check(api)?;
        ensure!(
            &asset != base_asset,
            ContractError::InvalidRoute {
                reason: "can't start at the base asset, which isn't swapped".to_string()
            }
        );
        validate_route(api, &route, &asset, Some(base_asset))?;
        ROUTES.save(storage, asset.to_string(), &route)?;
    }

    Ok(())
}

/// Swaps the given fee tokens into the base asset, then splits the base asset across the legs.
fn collect(
    deps: DepsMut,
    env: Env,
    assets: Vec<AssetWithLimit>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    // allowing collect only once per cooldown period
    LAST_COLLECT_TS.update(deps.storage, |last_ts| match config.collect_cooldown {
        Some(cd_period) if env.block.time.seconds() < last_ts + cd_period => {
            Err(ContractError::Cooldown {
                next_collect_ts: last_ts + cd_period,
            })
        }
        _ => Ok(env.block.time.seconds()),
    })?;

    ensure_unique(assets.iter().map(|a| &a.info))?;

    let mut response = Response::new().add_attribute("action", "collect");

    for asset in assets {
        // the base asset isn't swapped, it's split across the legs below
        if asset.info == config.base_asset {
            continue;
        }

        let balance = asset
            .info
            .query_pool(&deps.querier, &env.contract.address)?;
        let amount = asset.limit.map_or(balance, |limit| limit.min(balance));
        if amount.is_zero() {
            continue;
        }

        let route = ROUTES
            .may_load(deps.storage, asset.info.to_string())?
            .ok_or_else(|| ContractError::NoRoute {
                asset: asset.info.to_string(),
            })?;
        // a route set before the base asset changed would end in the old one
        ensure!(
            route_output(&asset.info, &route) == config.base_asset,
            ContractError::InvalidRoute {
                reason: format!("from {} doesn't end in the base asset", asset.info)
            }
        );

        let minimum = minimum_receive(
            &deps.querier,
            config.router.as_str(),
            &route,
            amount,
            config.max_spread,
        )?;
        if minimum.is_zero() {
            response = response.add_attribute("skipped_dust", asset.info.to_string());
            continue;
        }

        response = response
            .add_message(swap_msg(
                config.router.as_str(),
                asset.info.with_balance(amount),
                route,
                minimum,
                None,
            )?)
            .add_attribute("collected", format!("{amount}{}", asset.info));
    }

    Ok(response.add_message(wasm_execute(
        env.contract.address,
        &ExecuteMsg::Distribute {},
        vec![],
    )?))
}

/// Splits the Maker's whole base asset balance across the legs by share. The last leg gets
/// what rounding leaves, so nothing is left behind.
fn distribute(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let balance = config
        .base_asset
        .query_pool(&deps.querier, &env.contract.address)?;

    let mut response = Response::new().add_attribute("action", "distribute");
    if balance.is_zero() {
        return Ok(response);
    }

    let mut assigned = Uint128::zero();
    let last = config.legs.len() - 1;

    for (index, leg) in config.legs.iter().enumerate() {
        let amount = if index == last {
            balance.checked_sub(assigned)?
        } else {
            balance * leg.share
        };
        assigned = assigned.checked_add(amount)?;
        if amount.is_zero() {
            continue;
        }

        let share = config.base_asset.with_balance(amount);
        response = response.add_attribute("leg", format!("{index}:{amount}"));

        if leg.route.is_empty() {
            response = response.add_message(match &leg.action {
                LegAction::Send { recipient } => share.into_msg(recipient)?,
                LegAction::Deposit { contract, msg } => deposit_msg(share, contract, msg.clone())?,
            });
            continue;
        }

        let minimum = minimum_receive(
            &deps.querier,
            config.router.as_str(),
            &leg.route,
            amount,
            config.max_spread,
        )?;
        if minimum.is_zero() {
            // dust stays in the Maker and is split again with the next collect
            continue;
        }

        match &leg.action {
            // the router delivers the output straight to the recipient
            LegAction::Send { recipient } => {
                response = response.add_message(swap_msg(
                    config.router.as_str(),
                    share,
                    leg.route.clone(),
                    minimum,
                    Some(recipient.clone()),
                )?);
            }
            // swap into the Maker, then deposit exactly what the swap produced
            LegAction::Deposit { .. } => {
                let leg_index = index as u32;
                response = response
                    .add_message(wasm_execute(
                        &env.contract.address,
                        &ExecuteMsg::SnapshotLeg { leg: leg_index },
                        vec![],
                    )?)
                    .add_message(swap_msg(
                        config.router.as_str(),
                        share,
                        leg.route.clone(),
                        minimum,
                        None,
                    )?)
                    .add_message(wasm_execute(
                        &env.contract.address,
                        &ExecuteMsg::DepositLeg { leg: leg_index },
                        vec![],
                    )?);
            }
        }
    }

    Ok(response)
}

fn leg_output(config: &Config, leg: u32) -> Result<AssetInfo, ContractError> {
    let leg = config
        .legs
        .get(leg as usize)
        .ok_or_else(|| StdError::generic_err(format!("No leg {leg}")))?;
    Ok(route_output(&config.base_asset, &leg.route))
}

fn snapshot_leg(deps: DepsMut, env: Env, leg: u32) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let balance = leg_output(&config, leg)?.query_pool(&deps.querier, &env.contract.address)?;
    LEG_SNAPSHOT.save(deps.storage, &balance)?;

    Ok(Response::new())
}

fn deposit_leg(deps: DepsMut, env: Env, leg: u32) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let output = leg_output(&config, leg)?;

    let before = LEG_SNAPSHOT.load(deps.storage)?;
    LEG_SNAPSHOT.remove(deps.storage);

    let produced = output
        .query_pool(&deps.querier, &env.contract.address)?
        .checked_sub(before)?;
    if produced.is_zero() {
        return Ok(Response::new());
    }

    let LegAction::Deposit { contract, msg } = &config.legs[leg as usize].action else {
        return Err(StdError::generic_err(format!("Leg {leg} doesn't deposit")).into());
    };

    Ok(Response::new()
        .add_message(deposit_msg(
            output.with_balance(produced),
            contract,
            msg.clone(),
        )?)
        .add_attribute("deposited", format!("{leg}:{produced}{output}")))
}

fn seize(deps: DepsMut, env: Env, assets: Vec<AssetWithLimit>) -> Result<Response, ContractError> {
    ensure!(
        !assets.is_empty(),
        StdError::generic_err("assets vector is empty")
    );

    let conf = SEIZE_CONFIG.load(deps.storage)?;

    ensure!(
        !conf.seizable_assets.is_empty(),
        StdError::generic_err("No seizable assets found")
    );

    let input_set = assets
        .iter()
        .map(|a| a.info.to_string())
        .collect::<HashSet<_>>();
    let seizable_set = conf
        .seizable_assets
        .iter()
        .map(|a| a.to_string())
        .collect::<HashSet<_>>();

    ensure!(
        input_set.is_subset(&seizable_set),
        StdError::generic_err("Input vector contains assets that are not seizable")
    );

    let send_msgs = assets
        .into_iter()
        .filter_map(|asset| {
            let balance = asset
                .info
                .query_pool(&deps.querier, &env.contract.address)
                .ok()?;

            let limit = asset
                .limit
                .map(|limit| limit.min(balance))
                .unwrap_or(balance);

            // filter assets with empty balances
            if limit.is_zero() {
                None
            } else {
                Some(asset.info.with_balance(limit).into_msg(&conf.receiver))
            }
        })
        .collect::<StdResult<Vec<_>>>()?;

    Ok(Response::new()
        .add_messages(send_msgs)
        .add_attribute("action", "seize"))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::Routes {} => to_json_binary(
            &ROUTES
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| {
                    let (_, operations) = item?;
                    let asset = operations
                        .first()
                        .map(|op| crate::utils::op_assets(op).0.clone())
                        .ok_or_else(|| StdError::generic_err("Empty route"))?;
                    Ok(RouteResponse { asset, operations })
                })
                .collect::<StdResult<Vec<_>>>()?,
        ),
        QueryMsg::Balances { assets } => {
            let balances = assets
                .into_iter()
                .map(|info| {
                    let amount = info.query_pool(&deps.querier, &env.contract.address)?;
                    Ok(Asset { info, amount })
                })
                .collect::<StdResult<Vec<_>>>()?
                .into_iter()
                .filter(|asset| !asset.amount.is_zero())
                .collect();
            to_json_binary(&BalancesResponse { balances })
        }
        QueryMsg::QuerySeizeConfig {} => to_json_binary(&SEIZE_CONFIG.load(deps.storage)?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    migration::migrate(deps, env, msg)
}
