use astroport::asset::{validate_native_denom, AssetInfo, AssetInfoExt};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure, ensure_eq, to_json_string, Decimal, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, SubMsg,
};
use cw_utils::nonpayable;
use itertools::Itertools;
use std::collections::HashSet;

use astroport::maker::{
    AssetWithLimit, ExecuteMsg, PoolRoute, UpdateDevFundConfig, MAX_ALLOWED_SPREAD,
};

use crate::error::ContractError;
use crate::reply::POST_COLLECT_REPLY_ID;
use crate::state::{RouteStep, CONFIG, LAST_COLLECT_TS, OWNERSHIP_PROPOSAL, ROUTES, SEIZE_CONFIG};
use crate::utils::{asset_info_key, validate_cooldown, RoutesBuilder};

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    // All maker endpoints are non-payable
    nonpayable(&info)?;

    match msg {
        ExecuteMsg::Collect { assets } => collect(deps, env, assets),
        ExecuteMsg::UpdateConfig {
            astro_denom,
            collector,
            max_spread,
            collect_cooldown,
            dev_fund_config,
        } => update_config(
            deps,
            info,
            astro_denom,
            collector,
            max_spread,
            collect_cooldown,
            dev_fund_config,
        ),
        ExecuteMsg::SetPoolRoutes(routes) => set_pool_routes(deps, info, routes),
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
                CONFIG
                    .update::<_, StdError>(deps.storage, |mut v| {
                        v.owner = new_owner;
                        Ok(v)
                    })
                    .map(|_| ())
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

pub fn collect(
    deps: DepsMut,
    env: Env,
    assets: Vec<AssetWithLimit>,
) -> Result<Response, ContractError> {
    ensure!(!assets.is_empty(), ContractError::EmptyAssets {});

    let cfg = CONFIG.load(deps.storage)?;

    // Allowing collect only once per cooldown period
    LAST_COLLECT_TS.update(deps.storage, |last_ts| match cfg.collect_cooldown {
        Some(cd_period) if env.block.time.seconds() < last_ts + cd_period => {
            Err(ContractError::Cooldown {
                next_collect_ts: last_ts + cd_period,
            })
        }
        _ => Ok(env.block.time.seconds()),
    })?;

    let mut messages = vec![];
    let mut attrs = vec![attr("action", "collect")];

    let mut routes_builder = RoutesBuilder::default();
    for asset in assets {
        let balance = asset
            .info
            .query_pool(&deps.querier, &env.contract.address)
            .map(|balance| {
                asset
                    .limit
                    .map(|limit| asset.info.with_balance(limit.min(balance)))
                    .unwrap_or_else(|| asset.info.with_balance(balance))
            })?;

        // Skip silently if the balance is zero.
        // This allows our bot to operate normally without manual adjustments.
        if balance.amount.is_zero() {
            continue;
        }

        attrs.push(attr("collected_asset", &balance.to_string()));

        let built_routes =
            routes_builder.build_routes(deps.storage, &balance.info, &cfg.astro_denom)?;

        attrs.push(attr("route_taken", built_routes.route_taken));

        let swap_msg = MsgSwapExactAmountIn {
            sender: env.contract.address.to_string(),
            routes: built_routes.routes,
            token_in: Some(coin(balance.amount.u128(), balance.denom.clone()).into()),
            token_out_min_amount: min_out_amount.to_string(),
        };
        messages.push(SubMsg::new(swap_msg));
    }

    messages
        .last_mut()
        .map(|msg| SubMsg::reply_on_success(msg, POST_COLLECT_REPLY_ID))
        .ok_or(ContractError::NothingToCollect {})?;

    Ok(Response::new()
        .add_submessages(messages)
        .add_attributes(attrs))
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    astro_denom: Option<String>,
    collector: Option<String>,
    max_spread: Option<Decimal>,
    collect_cooldown: Option<u64>,
    dev_fund_conf: Option<Box<UpdateDevFundConfig>>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});

    let mut attrs = vec![];

    if let Some(astro_denom) = astro_denom {
        validate_native_denom(&astro_denom)?;
        attrs.push(attr("new_astro_denom", &astro_denom));
        config.astro_denom = astro_denom;
    }

    if let Some(collector) = collector {
        config.collector = deps.api.addr_validate(&collector)?;
        attrs.push(attr("new_collector", &collector));
    }

    if let Some(max_spread) = max_spread {
        ensure!(
            max_spread <= MAX_ALLOWED_SPREAD,
            ContractError::MaxSpreadTooHigh {}
        );
        attrs.push(attr("new_max_spread", max_spread.to_string()));
        config.max_spread = max_spread;
    }

    if let Some(collect_cooldown_val) = collect_cooldown {
        validate_cooldown(collect_cooldown)?;
        attrs.push(attr(
            "new_collect_cooldown",
            collect_cooldown_val.to_string(),
        ));
        config.collect_cooldown = Some(collect_cooldown_val);
    }

    if let Some(dev_fund_config) = dev_fund_conf {
        config.dev_fund_conf = dev_fund_config.set;

        if let Some(dev_fund_conf) = config.dev_fund_conf.as_ref() {
            deps.api.addr_validate(&dev_fund_conf.address)?;
            ensure!(
                dev_fund_conf.share > Decimal::zero() && dev_fund_conf.share <= Decimal::one(),
                StdError::generic_err("Dev fund share must be > 0 and <= 1")
            );
            // Ensure we can swap ASTRO into dev fund asset
            get_pool(
                &deps.querier,
                &config.factory_contract,
                &config.astro_token,
                &dev_fund_conf.asset_info,
            )?;
            attrs.push(attr(
                "new_dev_fund_settings",
                to_json_string(dev_fund_conf)?,
            ));
        }
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attrs))
}

pub fn set_pool_routes(
    deps: DepsMut,
    info: MessageInfo,
    routes: Vec<PoolRoute>,
) -> Result<Response, ContractError> {
    ensure!(!routes.is_empty(), ContractError::EmptyRoutes {});
    ensure!(
        routes.iter().map(|r| &r.asset_in).all_unique(),
        ContractError::DuplicatedRoutes {}
    );

    let config = CONFIG.load(deps.storage)?;
    ensure!(info.sender == config.owner, ContractError::Unauthorized {});

    let mut attrs = vec![attr("action", "set_pool_routes")];

    let mut routes_builder = RoutesBuilder::default();

    for route in &routes {
        ensure!(
            route.asset_in != AssetInfo::native(&config.astro_denom),
            ContractError::AstroInRoute {
                route: route.clone()
            }
        );

        // Sanity checks via osmosis pool manager
        let pm_quierier = PoolmanagerQuerier::new(&deps.querier);
        let pool_denoms = pm_quierier
            .total_pool_liquidity(route.pool_id)?
            .liquidity
            .into_iter()
            .map(|coin| coin.denom)
            .collect_vec();

        ensure!(
            pool_denoms.contains(&route.denom_in),
            ContractError::InvalidPoolDenom {
                pool_addr: route.pool_addr.clone(),
                asset: route.asset_in.to_string()
            }
        );
        ensure!(
            pool_denoms.contains(&route.denom_out),
            ContractError::InvalidPoolDenom {
                pool_addr: route.pool_addr.clone(),
                asset: route.asset_in.to_string()
            }
        );

        let route_key = asset_info_key(&route.asset_in);
        if ROUTES.has(deps.storage, &route_key) {
            attrs.push(attr("updated_route", &route.asset_in));
        }

        let route_step = RouteStep {
            asset_out: route.asset_out.clone(),
            pool_addr: route.pool_addr.clone(),
        };

        // If route exists then this iteration updates the route.
        ROUTES.save(deps.storage, &route_key, &route_step)?;

        routes_builder
            .routes_cache
            .insert(route.asset_in.clone(), route_step);
    }

    // Check all updated routes end up in ASTRO. It also checks for possible loops.
    routes.iter().try_for_each(|route| {
        routes_builder
            .build_routes(deps.storage, &route.asset_in, &config.astro_denom)
            .map(|_| ())
    })?;

    Ok(Response::new().add_attributes(attrs))
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

            // Filter assets with empty balances
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

#[cfg(test)]
mod unit_tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, Addr};
    use cw_utils::PaymentError;

    use astroport::maker::{Config, COOLDOWN_LIMITS};

    use super::*;

    #[test]
    fn collect_basic_tests() {
        let mut deps = mock_dependencies();

        let assets = vec![];
        let err = collect(deps.as_mut(), mock_env(), assets).unwrap_err();
        assert_eq!(err, ContractError::EmptyAssets {});

        let mut env = mock_env();
        let config = Config {
            owner: Addr::unchecked("owner"),
            astro_denom: "astro".to_string(),
            collector: Addr::unchecked("satellite"),
            max_spread: Default::default(),
            collect_cooldown: Some(60),
            dev_fund_conf: None,
        };
        CONFIG.save(deps.as_mut().storage, &config).unwrap();
        LAST_COLLECT_TS
            .save(deps.as_mut().storage, &env.block.time.seconds())
            .unwrap();
        let assets = vec![CoinWithLimit {
            denom: "uusd".to_string(),
            amount: None,
        }];
        let err = collect(deps.as_mut(), env.clone(), assets.clone()).unwrap_err();
        assert_eq!(
            err,
            ContractError::Cooldown {
                next_collect_ts: env.block.time.seconds() + config.collect_cooldown.unwrap(),
            }
        );

        env.block.time = env
            .block
            .time
            .plus_seconds(config.collect_cooldown.unwrap());
        let err = collect(deps.as_mut(), env.clone(), assets).unwrap_err();
        assert_eq!(err, ContractError::NothingToCollect {});
    }

    #[test]
    fn update_config_basic_tests() {
        let mut deps = mock_dependencies();
        let config = Config {
            owner: Addr::unchecked("owner"),
            astro_denom: "astro".to_string(),
            collector: Addr::unchecked("satellite"),
            max_spread: Default::default(),
            collect_cooldown: Some(60),
            dev_fund_conf: None,
        };
        CONFIG.save(deps.as_mut().storage, &config).unwrap();

        let err = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("random", &[]),
            ExecuteMsg::UpdateConfig {
                astro_denom: None,
                collector: None,
                max_spread: None,
                collect_cooldown: None,
                dev_fund_config: None,
            },
        )
        .unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let err = update_config(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            Some("1a".to_string()),
            None,
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err("Invalid denom length [3,128]: 1a"))
        );

        let err = update_config(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            None,
            Some("s".to_string()),
            None,
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(
            err,
            ContractError::Std(StdError::generic_err("Invalid input: human address too short for this mock implementation (must be >= 3)."))
        );

        let err = update_config(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            None,
            None,
            Some(Decimal::percent(99)),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::MaxSpreadTooHigh {});

        let err = update_config(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            None,
            None,
            None,
            Some(COOLDOWN_LIMITS.end() + 1),
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::IncorrectCooldown { min: 30, max: 600 });

        update_config(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            Some("new_astro".to_string()),
            Some("new_collector".to_string()),
            Some(Decimal::percent(10)),
            Some(*COOLDOWN_LIMITS.start()),
            None,
        )
        .unwrap();
    }

    #[test]
    fn set_routes_basic_tests() {
        let mut deps = mock_dependencies();
        let config = Config {
            owner: Addr::unchecked("owner"),
            astro_denom: "astro".to_string(),
            collector: Addr::unchecked("satellite"),
            max_spread: Default::default(),
            collect_cooldown: Some(60),
            dev_fund_conf: None,
        };
        CONFIG.save(deps.as_mut().storage, &config).unwrap();

        let routes = vec![PoolRoute {
            denom_in: "uatom".to_string(),
            denom_out: "utest".to_string(),
            pool_id: 1,
        }];
        let err =
            set_pool_routes(deps.as_mut(), mock_info("random", &[]), routes.clone()).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});

        let routes = vec![
            PoolRoute {
                denom_in: "uatom".to_string(),
                denom_out: "utest".to_string(),
                pool_id: 1,
            },
            PoolRoute {
                denom_in: "uatom".to_string(),
                denom_out: "ucoin".to_string(),
                pool_id: 2,
            },
        ];
        let err = set_pool_routes(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            routes.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::DuplicatedRoutes {});

        let wrong_route = PoolRoute {
            denom_in: "astro".to_string(),
            denom_out: "utest".to_string(),
            pool_id: 1,
        };
        let routes = vec![wrong_route.clone()];
        let err = set_pool_routes(
            deps.as_mut(),
            mock_info(config.owner.as_str(), &[]),
            routes.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::AstroInRoute { route: wrong_route });
    }

    #[test]
    fn test_nonpayable() {
        let mut deps = mock_dependencies();

        let res = execute(
            deps.as_mut(),
            mock_env(),
            mock_info("test", &coins(1, "uosmo")),
            ExecuteMsg::Collect { assets: vec![] },
        )
        .unwrap_err();
        assert_eq!(
            res,
            ContractError::PaymentError(PaymentError::NonPayable {})
        );
    }
}
