use crate::error::ContractError;
use crate::state::{Config, BRIDGES, CONFIG, OWNERSHIP_PROPOSAL};
use std::cmp::min;

use crate::migration;
use crate::utils::{
    build_distribute_msg, build_swap_msg, validate_bridge, BRIDGES_INITIAL_DEPTH, BRIDGES_MAX_DEPTH,
};
use astroport::asset::{
    addr_validate_to_lower, token_asset, token_asset_info, Asset, AssetInfo, PairInfo,
};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::factory::UpdateAddr;
use astroport::maker::{
    AssetWithLimit, BalancesResponse, ConfigResponse, ExecuteMsg, InstantiateMsg, MigrateMsg,
    QueryMsg,
};
use astroport::pair::QueryMsg as PairQueryMsg;
use astroport::querier::query_pair_info;
use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, QueryRequest, Response, StdError, StdResult, SubMsg, Uint128, Uint64,
    WasmQuery,
};
use cw2::{get_contract_version, set_contract_version};
use std::collections::{HashMap, HashSet};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-maker";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Sets the default maximum spread in percentage
const DEFAULT_MAX_SPREAD: u64 = 5; // 5%

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let governance_contract = if let Some(governance_contract) = msg.governance_contract {
        Option::from(addr_validate_to_lower(deps.api, &governance_contract)?)
    } else {
        None
    };

    let governance_percent = if let Some(governance_percent) = msg.governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };
        governance_percent
    } else {
        Uint64::zero()
    };

    let max_spread = if let Some(max_spread) = msg.max_spread {
        if max_spread.gt(&Decimal::one()) {
            return Err(ContractError::IncorrectMaxSpread {});
        };

        max_spread
    } else {
        Decimal::percent(DEFAULT_MAX_SPREAD)
    };

    let cfg = Config {
        owner: addr_validate_to_lower(deps.api, &msg.owner)?,
        astro_token_contract: addr_validate_to_lower(deps.api, &msg.astro_token_contract)?,
        factory_contract: addr_validate_to_lower(deps.api, &msg.factory_contract)?,
        staking_contract: addr_validate_to_lower(deps.api, &msg.staking_contract)?,
        rewards_enabled: false,
        pre_upgrade_blocks: 0,
        last_distribution_block: 0,
        remainder_reward: Uint128::zero(),
        pre_upgrade_astro_amount: Uint128::zero(),
        governance_contract,
        governance_percent,
        max_spread,
    };

    CONFIG.save(deps.storage, &cfg)?;

    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Collect { assets }** Collects rewards from the pools, swaps to astro
/// token and distributes the rewards between staking and governance contracts
///
/// * **ExecuteMsg::UpdateConfig {
///             factory_contract,
///             staking_contract,
///             governance_contract,
///             governance_percent,
///             max_spread,
///         }** Updates general settings that contains in the [`Config`].
///
/// * **ExecuteMsg::UpdateBridges { add, remove }** Adds or removes bridge assets to swap rewards
///
/// * **ExecuteMsg::SwapBridgeAssets { assets }** Private method used by contract
/// to swap rewards using bridges and keep balances updated
///
/// * **ExecuteMsg::DistributeAstro {}** Private method used by contract to distribute ASTRO rewards
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Approves owner.
///
/// * **ExecuteMsg::EnableRewards** Enables collected rewards distribution
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Collect { assets } => collect(deps, env, assets),
        ExecuteMsg::UpdateConfig {
            factory_contract,
            staking_contract,
            governance_contract,
            governance_percent,
            max_spread,
        } => update_config(
            deps,
            info,
            factory_contract,
            staking_contract,
            governance_contract,
            governance_percent,
            max_spread,
        ),
        ExecuteMsg::UpdateBridges { add, remove } => update_bridges(deps, info, add, remove),
        ExecuteMsg::SwapBridgeAssets { assets, depth } => {
            swap_bridge_assets(deps, env, info, assets, depth)
        }
        ExecuteMsg::DistributeAstro {} => distribute_astro(deps, env, info),
        ExecuteMsg::ProposeNewOwner { owner, expires_in } => {
            let config: Config = CONFIG.load(deps.storage)?;

            propose_new_owner(
                deps,
                info,
                env,
                owner,
                expires_in,
                config.owner,
                OWNERSHIP_PROPOSAL,
            )
            .map_err(|e| e.into())
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

            drop_ownership_proposal(deps, info, config.owner, OWNERSHIP_PROPOSAL)
                .map_err(|e| e.into())
        }
        ExecuteMsg::ClaimOwnership {} => {
            claim_ownership(deps, info, env, OWNERSHIP_PROPOSAL, |deps, new_owner| {
                CONFIG.update::<_, StdError>(deps.storage, |mut v| {
                    v.owner = new_owner;
                    Ok(v)
                })?;

                Ok(())
            })
            .map_err(|e| e.into())
        }
        ExecuteMsg::EnableRewards { blocks } => {
            let mut config: Config = CONFIG.load(deps.storage)?;

            // permission check
            if info.sender != config.owner {
                return Err(ContractError::Unauthorized {});
            }

            // can be enabled only once
            if config.rewards_enabled {
                return Err(ContractError::RewardsAlreadyEnabled {});
            }

            if blocks == 0 {
                return Err(ContractError::Std(StdError::generic_err(
                    "Number of blocks should be > 0",
                )));
            }

            config.rewards_enabled = true;
            config.pre_upgrade_blocks = blocks;
            config.last_distribution_block = env.block.height;
            CONFIG.save(deps.storage, &config)?;
            Ok(Response::default())
        }
    }
}

/// # Description
/// Collects astro tokens. Before that collects all assets and performs a swap operation for all
/// non-astro tokens into an astro token.
/// Returns an [`ContractError`] on failure, otherwise returns the [`Response`] object if the
/// operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **assets** is a vector that contains object of type [`AssetWithLimit`].
/// Sets the assets for which the collect operation will be performed.
fn collect(
    deps: DepsMut,
    env: Env,
    assets: Vec<AssetWithLimit>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    let astro = token_asset_info(cfg.astro_token_contract.clone());

    // check for duplicate asset
    let mut uniq = HashSet::new();
    if !assets
        .clone()
        .into_iter()
        .all(|a| uniq.insert(a.info.to_string()))
    {
        return Err(ContractError::DuplicatedAsset {});
    }

    // Swap all non-astro tokens
    let (mut response, bridge_assets) = swap_assets(
        deps.as_ref(),
        env.clone(),
        &cfg,
        assets.into_iter().filter(|a| a.info.ne(&astro)).collect(),
    )?;

    // If no messages - send astro directly
    if response.messages.is_empty() {
        let (mut distribute_msg, attributes) = distribute(deps, env, &mut cfg)?;
        if !distribute_msg.is_empty() {
            response.messages.append(&mut distribute_msg);
            response = response.add_attributes(attributes);
        }
    } else {
        response.messages.push(build_distribute_msg(
            env,
            bridge_assets,
            BRIDGES_INITIAL_DEPTH,
        )?);
    }

    Ok(response.add_attribute("action", "collect"))
}

/// ## Description
/// This enum describes available types for SwapTarget.
enum SwapTarget {
    Astro(SubMsg),
    Bridge { asset: AssetInfo, msg: SubMsg },
}

/// # Description
/// Swap all non-astro tokens to Astro. Returns an [`ContractError`] on failure, otherwise returns
/// the [`Response`] object if the operation was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **cfg** is the object of type [`Config`].
///
/// * **assets** is a vector that contains object of type [`AssetWithLimit`]. Sets the assets for
/// which the collect operation will be performed.
fn swap_assets(
    deps: Deps,
    env: Env,
    cfg: &Config,
    assets: Vec<AssetWithLimit>,
) -> Result<(Response, Vec<AssetInfo>), ContractError> {
    let mut response = Response::default();
    let mut bridge_assets = HashMap::new();

    for a in assets {
        // Get Balance
        let mut balance = a
            .info
            .query_pool(&deps.querier, env.contract.address.clone())?;
        if let Some(limit) = a.limit {
            if limit < balance && limit > Uint128::zero() {
                balance = limit;
            }
        }

        if !balance.is_zero() {
            let swap_msg = swap(deps, cfg, a.info, balance)?;

            match swap_msg {
                SwapTarget::Astro(msg) => {
                    response.messages.push(msg);
                }
                SwapTarget::Bridge { asset, msg } => {
                    response.messages.push(msg);
                    bridge_assets.insert(asset.to_string(), asset);
                }
            }
        }
    }

    Ok((response, bridge_assets.into_values().collect()))
}

/// # Description
/// Performs the swap operation to astro token. Returns an [`ContractError`] on failure,
/// otherwise returns the vector that contains the objects of type [`SubMsg`] if the operation
/// was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **cfg** is the object of type [`Config`].
///
/// * **from_token** is the object of type [`AssetInfo`].
///
/// * **amount_in** is the object of type [`Uint128`].
fn swap(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let astro = token_asset_info(cfg.astro_token_contract.clone());

    // 1. check direct pair with ASTRO
    let direct_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), astro],
    );

    if direct_pool.is_ok() {
        let msg = build_swap_msg(deps, cfg, direct_pool.unwrap(), from_token, amount_in)?;
        return Ok(SwapTarget::Astro(msg));
    }

    // 2. check if bridge token exists
    let bridge_token = BRIDGES
        .load(deps.storage, from_token.to_string())
        .map_err(|_| ContractError::CannotSwap(from_token.clone()))?;

    let bridge_pool = query_pair_info(
        &deps.querier,
        cfg.factory_contract.clone(),
        &[from_token.clone(), bridge_token.clone()],
    )
    .map_err(|_| ContractError::InvalidBridgeNoPool(from_token.clone(), bridge_token.clone()))?;

    let msg = build_swap_msg(deps, cfg, bridge_pool, from_token, amount_in)?;

    Ok(SwapTarget::Bridge {
        asset: bridge_token,
        msg,
    })
}

/// ## Description
/// Swaps collected rewards using bridge assets. Returns an [`ContractError`] on failure
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **assets** is an vector field of type [`AssetWithLimit`].
///
/// * **depth** is the object of type [`u64`]. Sets the exchange depth.
///
/// ##Executor
/// Only maker contract itself can execute it
fn swap_bridge_assets(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    assets: Vec<AssetInfo>,
    depth: u64,
) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    if assets.is_empty() {
        return Ok(Response::default());
    }

    if depth >= BRIDGES_MAX_DEPTH {
        return Err(ContractError::MaxBridgeDepth(depth));
    }

    let cfg = CONFIG.load(deps.storage)?;

    let bridges = assets
        .into_iter()
        .map(|a| AssetWithLimit {
            info: a,
            limit: None,
        })
        .collect();

    let (response, bridge_assets) = swap_assets(deps.as_ref(), env.clone(), &cfg, bridges)?;

    // there always should be some messages, if there are none - something went wrong
    if response.messages.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Empty swap messages",
        )));
    }

    Ok(response
        .add_submessage(build_distribute_msg(env, bridge_assets, depth + 1)?)
        .add_attribute("action", "swap_bridge_assets"))
}

/// ## Description
/// Distributes ASTRO rewards. Returns an [`ContractError`] on failure
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// ##Executor
/// Only maker contract itself can execute it
fn distribute_astro(deps: DepsMut, env: Env, info: MessageInfo) -> Result<Response, ContractError> {
    if info.sender != env.contract.address {
        return Err(ContractError::Unauthorized {});
    }

    let mut cfg = CONFIG.load(deps.storage)?;
    let (distribute_msg, attributes) = distribute(deps, env, &mut cfg)?;
    if distribute_msg.is_empty() {
        return Ok(Response::default());
    }

    Ok(Response::default()
        .add_submessages(distribute_msg)
        .add_attributes(attributes))
}

type DistributeMsgParts = (Vec<SubMsg>, Vec<(String, String)>);

/// # Description
/// Performs the distribute of astro token. Returns an [`ContractError`] on failure,
/// otherwise returns the vector that contains the objects of type [`SubMsg`] if the operation
/// was successful.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **cfg** is the object of type [`Config`].
fn distribute(
    deps: DepsMut,
    env: Env,
    cfg: &mut Config,
) -> Result<DistributeMsgParts, ContractError> {
    let mut result = vec![];
    let mut attributes = vec![];

    let astro = token_asset_info(cfg.astro_token_contract.clone());

    let mut amount = astro.query_pool(&deps.querier, env.contract.address.clone())?;
    if amount.is_zero() {
        return Ok((result, attributes));
    }
    let mut pure_astro_reward = amount;
    let mut current_preupgrade_distribution = Uint128::zero();

    if !cfg.rewards_enabled {
        cfg.pre_upgrade_astro_amount = amount;
        cfg.remainder_reward = amount;
        CONFIG.save(deps.storage, cfg)?;
        return Ok((result, attributes));
    } else if !cfg.remainder_reward.is_zero() {
        let blocks_passed = env.block.height - cfg.last_distribution_block;
        if blocks_passed == 0 {
            return Ok((result, attributes));
        }
        let mut remainder_reward = cfg.remainder_reward;
        let astro_distribution_portion =
            cfg.pre_upgrade_astro_amount / Uint128::from(cfg.pre_upgrade_blocks);
        current_preupgrade_distribution = min(
            Uint128::from(blocks_passed) * astro_distribution_portion,
            remainder_reward,
        );

        // subtract undistributed remainder reward
        amount -= remainder_reward;
        pure_astro_reward = amount;

        // add a portion of reward
        amount += current_preupgrade_distribution;

        remainder_reward -= current_preupgrade_distribution;

        // reduce the number of pre-upgrade astro amount
        cfg.remainder_reward = remainder_reward;
        cfg.last_distribution_block = env.block.height;
        CONFIG.save(deps.storage, cfg)?;
    }

    let governance_amount = if let Some(governance_contract) = cfg.governance_contract.clone() {
        let amount =
            amount.multiply_ratio(Uint128::from(cfg.governance_percent), Uint128::new(100));
        if amount.u128() > 0 {
            let to_governance_asset = token_asset(cfg.astro_token_contract.clone(), amount);
            result.push(SubMsg::new(
                to_governance_asset.into_msg(&deps.querier, governance_contract)?,
            ))
        }
        amount
    } else {
        Uint128::zero()
    };

    let to_staking_asset =
        token_asset(cfg.astro_token_contract.clone(), amount - governance_amount);

    attributes.push(("action".to_string(), "distribute_astro".to_string()));
    attributes.push((
        "astro_distribution".to_string(),
        pure_astro_reward.to_string(),
    ));
    if !current_preupgrade_distribution.is_zero() {
        attributes.push((
            "preupgrade_astro_distribution".to_string(),
            current_preupgrade_distribution.to_string(),
        ));
    }

    result.push(SubMsg::new(
        to_staking_asset.into_msg(&deps.querier, cfg.staking_contract.clone())?,
    ));
    Ok((result, attributes))
}

/// ## Description
/// Updates general settings. Returns an [`ContractError`] on failure or the following [`Config`]
/// data will be updated if successful.
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **factory_contract** is an [`Option`] field of type [`String`].
///
/// * **staking_contract** is an [`Option`] field of type [`String`].
///
/// * **governance_contract** is an [`Option`] field of type [`UpdateAddr`].
///
/// * **governance_percent** is an [`Option`] field of type [`Uint64`].
///
/// * **max_spread** is an [`Option`] field of type [`Decimal`].
///
/// ##Executor
/// Only owner can execute it
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    factory_contract: Option<String>,
    staking_contract: Option<String>,
    governance_contract: Option<UpdateAddr>,
    governance_percent: Option<Uint64>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "set_config")];

    let mut config = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(factory_contract) = factory_contract {
        config.factory_contract = addr_validate_to_lower(deps.api, &factory_contract)?;
        attributes.push(Attribute::new("factory_contract", &factory_contract));
    };

    if let Some(staking_contract) = staking_contract {
        config.staking_contract = addr_validate_to_lower(deps.api, &staking_contract)?;
        attributes.push(Attribute::new("staking_contract", &staking_contract));
    };

    if let Some(action) = governance_contract {
        match action {
            UpdateAddr::Set(gov) => {
                config.governance_contract = Option::from(addr_validate_to_lower(deps.api, &gov)?);
                attributes.push(Attribute::new("governance_contract", &gov));
            }
            UpdateAddr::Remove {} => {
                config.governance_contract = None;
            }
        }
    }

    if let Some(governance_percent) = governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };

        config.governance_percent = governance_percent;
        attributes.push(Attribute::new("governance_percent", governance_percent));
    };

    if let Some(max_spread) = max_spread {
        if max_spread.gt(&Decimal::one()) {
            return Err(ContractError::IncorrectMaxSpread {});
        };

        config.max_spread = max_spread;
        attributes.push(Attribute::new("max_spread", max_spread.to_string()));
    };

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// ## Description
/// Adds or removes bridges to swap rewards to ASTRO. Returns an [`ContractError`] on failure
///
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **add** is an [`Option`] field of type [`Vec<(AssetInfo, AssetInfo)>`].
///
/// * **remove** is an [`Option`] field of type [`Vec<AssetInfo>`].
///
/// ##Executor
/// Only owner can execute it
fn update_bridges(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<(AssetInfo, AssetInfo)>>,
    remove: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    // remove old bridges
    if let Some(remove_bridges) = remove {
        for asset in remove_bridges {
            BRIDGES.remove(deps.storage, asset.to_string());
        }
    }

    // add new bridges
    let astro = token_asset_info(cfg.astro_token_contract.clone());
    if let Some(add_bridges) = add {
        for (asset, bridge) in add_bridges {
            if asset.equal(&bridge) {
                return Err(ContractError::InvalidBridge(asset, bridge));
            }

            // Check that bridge token can be swapped to ASTRO
            validate_bridge(
                deps.as_ref(),
                &cfg,
                asset.clone(),
                bridge.clone(),
                astro.clone(),
                BRIDGES_INITIAL_DEPTH,
            )?;

            BRIDGES.save(deps.storage, asset.to_string(), &bridge)?;
        }
    }

    Ok(Response::default().add_attribute("action", "update_bridges"))
}

/// # Description
/// Describes all query messages.
/// # Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns information about the maker configs
/// in a [`ConfigResponse`] object.
///
/// * **QueryMsg::Balances { assets }** Returns the balance for each asset
/// in the [`ConfigResponse`] object.
///
/// * **QueryMsg::Bridges {}** Returns the bridges used for swapping fees for each asset
/// in vector of [`(String, String)`] denoting Asset -> Bridge Asset.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_get_config(deps)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
        QueryMsg::Bridges {} => to_binary(&query_bridges(deps, env)?),
    }
}

/// ## Description
/// Returns information about the maker configs in a [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
fn query_get_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner,
        factory_contract: config.factory_contract,
        staking_contract: config.staking_contract,
        governance_contract: config.governance_contract,
        governance_percent: config.governance_percent,
        astro_token_contract: config.astro_token_contract,
        max_spread: config.max_spread,
        remainder_reward: config.remainder_reward,
        pre_upgrade_astro_amount: config.pre_upgrade_astro_amount,
    })
}

/// ## Description
/// Returns the balance for each asset in the [`ConfigResponse`] object.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **assets** is a vector that contains object of type [`AssetInfo`].
fn query_get_balances(deps: Deps, env: Env, assets: Vec<AssetInfo>) -> StdResult<BalancesResponse> {
    let mut resp = BalancesResponse { balances: vec![] };

    for a in assets {
        // Get Balance
        let balance = a.query_pool(&deps.querier, env.contract.address.clone())?;
        if !balance.is_zero() {
            resp.balances.push(Asset {
                info: a,
                amount: balance,
            })
        }
    }

    Ok(resp)
}

/// ## Description
/// Returns bridges for swapping rewards set in maker contract
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
fn query_bridges(deps: Deps, _env: Env) -> StdResult<Vec<(String, String)>> {
    let bridges = BRIDGES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|item| {
            let (asset, bridge) = item.unwrap();
            (String::from_utf8(asset).unwrap(), bridge.to_string())
        })
        .collect::<Vec<(String, String)>>();

    Ok(bridges)
}

/// ## Description
/// Returns the asset information for the specified pair.
///
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **contract_addr** is the object of type [`Addr`]. Sets the pair contract address.
pub fn query_pair(deps: Deps, contract_addr: Addr) -> StdResult<[AssetInfo; 2]> {
    let res: PairInfo = deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
        contract_addr: String::from(contract_addr),
        msg: to_binary(&PairQueryMsg::Pair {})?,
    }))?;

    Ok(res.asset_infos)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-maker" => match contract_version.version.as_ref() {
            "1.0.0" => {
                let config_v100 = migration::CONFIGV100.load(deps.storage)?;

                let new_config = Config {
                    owner: config_v100.owner,
                    factory_contract: config_v100.factory_contract,
                    staking_contract: config_v100.staking_contract,
                    governance_contract: config_v100.governance_contract,
                    governance_percent: config_v100.governance_percent,
                    astro_token_contract: config_v100.astro_token_contract,
                    max_spread: config_v100.max_spread,
                    rewards_enabled: false,
                    pre_upgrade_blocks: 0,
                    last_distribution_block: 0,
                    remainder_reward: Uint128::zero(),
                    pre_upgrade_astro_amount: Uint128::zero(),
                };

                CONFIG.save(deps.storage, &new_config)?;
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
