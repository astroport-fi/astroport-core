use crate::error::ContractError;
use crate::state::{BRIDGES, CONFIG, OWNERSHIP_PROPOSAL};
use std::cmp::min;

use crate::migration::{migrate_from_v1, migrate_from_v120};

use crate::utils::{
    build_distribute_msg, build_send_msg, build_swap_msg, try_build_swap_msg,
    update_second_receiver_cfg, validate_bridge, BRIDGES_EXECUTION_MAX_DEPTH,
    BRIDGES_INITIAL_DEPTH,
};
use astroport::asset::{addr_opt_validate, Asset, AssetInfo};
use astroport::common::{claim_ownership, drop_ownership_proposal, propose_new_owner};
use astroport::factory::UpdateAddr;
use astroport::maker::{
    AssetWithLimit, BalancesResponse, Config, ConfigResponse, ExecuteMsg, InstantiateMsg,
    MigrateMsg, QueryMsg, SecondReceiverConfig, SecondReceiverParams,
};
use astroport::pair::MAX_ALLOWED_SLIPPAGE;
use cosmwasm_std::{
    attr, entry_point, to_binary, Addr, Attribute, Binary, Decimal, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, SubMsg, Uint128, Uint64,
};
use cw2::{get_contract_version, set_contract_version};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-maker";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Sets the default maximum spread (as a percentage) used when swapping fee tokens to ASTRO.
const DEFAULT_MAX_SPREAD: u64 = 5; // 5%

/// Creates a new contract with the specified parameters in [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let governance_contract = addr_opt_validate(deps.api, &msg.governance_contract)?;

    let governance_percent = if let Some(governance_percent) = msg.governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };
        governance_percent
    } else {
        Uint64::zero()
    };

    if msg.staking_contract.is_none() && governance_percent != Uint64::new(100) {
        return Err(ContractError::GovernancePercentMustBe100 {});
    }

    let max_spread = if let Some(max_spread) = msg.max_spread {
        if max_spread.is_zero() || max_spread.gt(&Decimal::from_str(MAX_ALLOWED_SLIPPAGE)?) {
            return Err(ContractError::IncorrectMaxSpread {});
        };

        max_spread
    } else {
        Decimal::percent(DEFAULT_MAX_SPREAD)
    };

    msg.astro_token.check(deps.api)?;

    if let Some(default_bridge) = &msg.default_bridge {
        default_bridge.check(deps.api)?
    }

    let mut cfg = Config {
        owner: deps.api.addr_validate(&msg.owner)?,
        default_bridge: msg.default_bridge,
        astro_token: msg.astro_token,
        factory_contract: deps.api.addr_validate(&msg.factory_contract)?,
        staking_contract: addr_opt_validate(deps.api, &msg.staking_contract)?,
        rewards_enabled: false,
        pre_upgrade_blocks: 0,
        last_distribution_block: 0,
        remainder_reward: Uint128::zero(),
        pre_upgrade_astro_amount: Uint128::zero(),
        governance_contract,
        governance_percent,
        max_spread,
        second_receiver_cfg: None,
    };

    update_second_receiver_cfg(deps.as_ref(), &mut cfg, &msg.second_receiver_params)?;

    if cfg.staking_contract.is_none() && cfg.governance_contract.is_none() {
        return Err(
            StdError::generic_err("Either staking or governance contract must be set").into(),
        );
    }

    CONFIG.save(deps.storage, &cfg)?;

    let (second_fee_receiver, second_receiver_cut) = if let Some(SecondReceiverConfig {
        second_fee_receiver,
        second_receiver_cut,
    }) = cfg.second_receiver_cfg
    {
        (
            second_fee_receiver.to_string(),
            second_receiver_cut.to_string(),
        )
    } else {
        (String::from("none"), String::from("0"))
    };

    Ok(Response::default().add_attributes([
        attr("owner", msg.owner),
        attr(
            "default_bridge",
            cfg.default_bridge
                .map(|v| v.to_string())
                .unwrap_or_else(|| String::from("none")),
        ),
        attr("astro_token", cfg.astro_token.to_string()),
        attr("factory_contract", msg.factory_contract),
        attr(
            "staking_contract",
            msg.staking_contract.unwrap_or_else(|| String::from("none")),
        ),
        attr(
            "governance_contract",
            msg.governance_contract
                .unwrap_or_else(|| String::from("none")),
        ),
        attr("governance_percent", governance_percent),
        attr("max_spread", max_spread.to_string()),
        attr("second_fee_receiver", second_fee_receiver),
        attr("second_receiver_cut", second_receiver_cut),
    ]))
}

/// Exposes execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Collect { assets }** Swaps collected fee tokens to ASTRO
/// and distributes the ASTRO between xASTRO and vxASTRO stakers.
///
/// * **ExecuteMsg::UpdateConfig {
///             factory_contract,
///             staking_contract,
///             governance_contract,
///             governance_percent,
///             max_spread,
///             second_receiver_params,
///         }** Updates general contract settings stores in the [`Config`].
///
/// * **ExecuteMsg::UpdateBridges { add, remove }** Adds or removes bridge assets used to swap fee tokens to ASTRO.
///
/// * **ExecuteMsg::SwapBridgeAssets { assets }** Swap fee tokens (through bridges) to ASTRO.
///
/// * **ExecuteMsg::DistributeAstro {}** Private method used by the contract to distribute ASTRO rewards.
///
/// * **ExecuteMsg::ProposeNewOwner { owner, expires_in }** Creates a new request to change contract ownership.
///
/// * **ExecuteMsg::DropOwnershipProposal {}** Removes a request to change contract ownership.
///
/// * **ExecuteMsg::ClaimOwnership {}** Claims contract ownership.
///
/// * **ExecuteMsg::EnableRewards** Enables collected ASTRO (pre Maker upgrade) to be distributed to xASTRO stakers.
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
            basic_asset,
            max_spread,
            second_receiver_params,
        } => update_config(
            deps,
            info,
            factory_contract,
            staking_contract,
            governance_contract,
            governance_percent,
            basic_asset,
            max_spread,
            second_receiver_params,
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
            .map_err(Into::into)
        }
        ExecuteMsg::DropOwnershipProposal {} => {
            let config: Config = CONFIG.load(deps.storage)?;

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
        ExecuteMsg::EnableRewards { blocks } => {
            let mut config: Config = CONFIG.load(deps.storage)?;

            // Permission check
            if info.sender != config.owner {
                return Err(ContractError::Unauthorized {});
            }

            // Can be enabled only once
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

            Ok(Response::default().add_attribute("action", "enable_rewards"))
        }
    }
}

/// Swaps fee tokens to ASTRO and distribute the resulting ASTRO to xASTRO and vxASTRO stakers.
///
/// * **assets** array with fee tokens being swapped to ASTRO.
fn collect(
    deps: DepsMut,
    env: Env,
    assets: Vec<AssetWithLimit>,
) -> Result<Response, ContractError> {
    let mut cfg = CONFIG.load(deps.storage)?;

    let astro = cfg.astro_token.clone();

    // Check for duplicate assets
    let mut uniq = HashSet::new();
    if !assets
        .clone()
        .into_iter()
        .all(|a| uniq.insert(a.info.to_string()))
    {
        return Err(ContractError::DuplicatedAsset {});
    }

    // Swap all non ASTRO tokens
    let (mut response, bridge_assets) = swap_assets(
        deps.as_ref(),
        &env.contract.address,
        &cfg,
        assets.into_iter().filter(|a| a.info.ne(&astro)).collect(),
        true,
    )?;

    // If no swap messages - send ASTRO directly to x/vxASTRO stakers
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

/// This enum describes available token types that can be used as a SwapTarget.
enum SwapTarget {
    Astro(SubMsg),
    Bridge { asset: AssetInfo, msg: SubMsg },
}

/// Swap all non ASTRO tokens to ASTRO.
///
/// * **contract_addr** maker contract address.
///
/// * **assets** array with assets to swap to ASTRO.
///
/// * **with_validation** whether the swap operation should be validated or not.
fn swap_assets(
    deps: Deps,
    contract_addr: &Addr,
    cfg: &Config,
    assets: Vec<AssetWithLimit>,
    with_validation: bool,
) -> Result<(Response, Vec<AssetInfo>), ContractError> {
    let mut response = Response::default();
    let mut bridge_assets = HashMap::new();

    for a in assets {
        // Get balance
        let mut balance = a.info.query_pool(&deps.querier, contract_addr)?;
        if let Some(limit) = a.limit {
            if limit < balance && limit > Uint128::zero() {
                balance = limit;
            }
        }

        if !balance.is_zero() {
            let swap_msg = if with_validation {
                swap(deps, cfg, a.info, balance)?
            } else {
                swap_no_validate(deps, cfg, a.info, balance)?
            };

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

/// Checks if all required pools and bridges exists and performs a swap operation to ASTRO.
///
/// * **from_token** token to swap to ASTRO.
///
/// * **amount_in** amount of tokens to swap.
fn swap(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    // 1. Check if bridge tokens exist
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(bridge_token) = bridge_token {
        let bridge_pool = validate_bridge(
            deps,
            &cfg.factory_contract,
            &from_token,
            &bridge_token,
            &cfg.astro_token,
            BRIDGES_INITIAL_DEPTH,
        )?;

        let msg = build_swap_msg(
            cfg.max_spread,
            &bridge_pool,
            &from_token,
            Some(&bridge_token),
            amount_in,
        )?;
        return Ok(SwapTarget::Bridge {
            asset: bridge_token,
            msg,
        });
    }

    // 2. Check for a pair with a default bridge
    if let Some(default_bridge) = &cfg.default_bridge {
        if from_token.ne(default_bridge) {
            let swap_to_default =
                try_build_swap_msg(&deps.querier, cfg, &from_token, default_bridge, amount_in);
            if let Ok(msg) = swap_to_default {
                return Ok(SwapTarget::Bridge {
                    asset: default_bridge.clone(),
                    msg,
                });
            }
        }
    }

    // 3. Check for a direct pair with ASTRO
    let swap_to_astro =
        try_build_swap_msg(&deps.querier, cfg, &from_token, &cfg.astro_token, amount_in);
    if let Ok(msg) = swap_to_astro {
        return Ok(SwapTarget::Astro(msg));
    }

    Err(ContractError::CannotSwap(from_token))
}

/// Performs a swap operation to ASTRO without additional checks.
/// was successful.
///
/// * **from_token** token to swap to ASTRO.
///
/// * **amount_in** amount of tokens to swap.
fn swap_no_validate(
    deps: Deps,
    cfg: &Config,
    from_token: AssetInfo,
    amount_in: Uint128,
) -> Result<SwapTarget, ContractError> {
    let astro = cfg.astro_token.clone();

    // Check if next level bridge exists
    let bridge_token = BRIDGES.load(deps.storage, from_token.to_string());
    if let Ok(asset) = bridge_token {
        let msg = try_build_swap_msg(&deps.querier, cfg, &from_token, &asset, amount_in)?;
        return Ok(SwapTarget::Bridge { asset, msg });
    }

    // Check for a direct swap to ASTRO
    let swap_to_astro = try_build_swap_msg(&deps.querier, cfg, &from_token, &astro, amount_in);
    if let Ok(msg) = swap_to_astro {
        return Ok(SwapTarget::Astro(msg));
    }

    Err(ContractError::CannotSwap(from_token))
}

/// Swaps collected fees using bridge assets.
///
/// * **assets** array with fee tokens to swap as well as amount of tokens to swap.
///
/// * **depth** maximum route length used to swap a fee token.
///
/// ## Executor
/// Only the Maker contract itself can execute this.
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

    // Check that the contract doesn't call itself endlessly
    if depth >= BRIDGES_EXECUTION_MAX_DEPTH {
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

    let (response, bridge_assets) =
        swap_assets(deps.as_ref(), &env.contract.address, &cfg, bridges, false)?;

    // There should always be some messages, if there are none - something went wrong
    if response.messages.is_empty() {
        return Err(ContractError::Std(StdError::generic_err(
            "Empty swap messages",
        )));
    }

    Ok(response
        .add_submessage(build_distribute_msg(env, bridge_assets, depth + 1)?)
        .add_attribute("action", "swap_bridge_assets"))
}

/// Distributes ASTRO rewards to x/vxASTRO holders.
///
/// ## Executor
/// Only the Maker contract itself can execute this.
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

type DistributeMsgParts = (Vec<SubMsg>, Vec<Attribute>);

/// Private function that performs the ASTRO token distribution to x/vxASTRO.
fn distribute(
    deps: DepsMut,
    env: Env,
    cfg: &mut Config,
) -> Result<DistributeMsgParts, ContractError> {
    let mut result = vec![];
    let mut attributes = vec![];

    let mut amount = cfg
        .astro_token
        .query_pool(&deps.querier, &env.contract.address)?;
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
        let astro_distribution_portion = cfg
            .pre_upgrade_astro_amount
            .checked_div(Uint128::from(cfg.pre_upgrade_blocks))?;

        current_preupgrade_distribution = min(
            Uint128::from(blocks_passed).checked_mul(astro_distribution_portion)?,
            remainder_reward,
        );

        // Subtract undistributed rewards
        amount = amount.checked_sub(remainder_reward)?;
        pure_astro_reward = amount;

        // Add the amount of pre Maker upgrade accrued ASTRO from fee token swaps
        amount = amount.checked_add(current_preupgrade_distribution)?;

        remainder_reward = remainder_reward.checked_sub(current_preupgrade_distribution)?;

        // Reduce the amount of pre-upgrade ASTRO that has to be distributed
        cfg.remainder_reward = remainder_reward;
        cfg.last_distribution_block = env.block.height;
        CONFIG.save(deps.storage, cfg)?;
    }

    let second_receiver_amount = if let Some(second_receiver_cfg) = &cfg.second_receiver_cfg {
        let amount = amount.multiply_ratio(
            Uint128::from(second_receiver_cfg.second_receiver_cut),
            Uint128::new(100),
        );

        if !amount.is_zero() {
            let asset = Asset {
                info: cfg.astro_token.clone(),
                amount,
            };

            result.push(SubMsg::new(
                asset.into_msg(second_receiver_cfg.second_fee_receiver.to_string())?,
            ))
        }

        amount
    } else {
        Uint128::zero()
    };

    let governance_amount = if let Some(governance_contract) = &cfg.governance_contract {
        let amount = amount
            .checked_sub(second_receiver_amount)?
            .multiply_ratio(Uint128::from(cfg.governance_percent), Uint128::new(100));

        if !amount.is_zero() {
            result.push(SubMsg::new(build_send_msg(
                &Asset {
                    info: cfg.astro_token.clone(),
                    amount,
                },
                governance_contract.to_string(),
                None,
            )?))
        }

        amount
    } else {
        Uint128::zero()
    };

    if let Some(staking_contract) = &cfg.staking_contract {
        let amount = amount.checked_sub(governance_amount + second_receiver_amount)?;
        if !amount.is_zero() {
            let to_staking_asset = Asset {
                info: cfg.astro_token.clone(),
                amount,
            };
            result.push(SubMsg::new(to_staking_asset.into_msg(staking_contract)?));
        }
    }

    attributes = vec![
        attr("action", "distribute_astro"),
        attr("astro_distribution", pure_astro_reward),
    ];
    if !current_preupgrade_distribution.is_zero() {
        attributes.push(attr(
            "preupgrade_astro_distribution",
            current_preupgrade_distribution,
        ));
    }

    Ok((result, attributes))
}

/// Updates general contract parameters.
///
/// * **factory_contract** address of the factory contract.
///
/// * **staking_contract** address of the xASTRO staking contract.
///
/// * **governance_contract** address of the vxASTRO fee distributor contract.
///
/// * **governance_percent** percentage of ASTRO that goes to the vxASTRO fee distributor.
///
/// * **default_bridge_opt** default bridge asset used for intermediate swaps to ASTRO.
///
/// * **max_spread** max spread used when swapping fee tokens to ASTRO.
///
/// * **second_receiver_params** describes the second receiver of fees
///
/// ## Executor
/// Only the owner can execute this.
#[allow(clippy::too_many_arguments)]
fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    factory_contract: Option<String>,
    staking_contract: Option<String>,
    governance_contract: Option<UpdateAddr>,
    governance_percent: Option<Uint64>,
    default_bridge_opt: Option<AssetInfo>,
    max_spread: Option<Decimal>,
    second_receiver_params: Option<SecondReceiverParams>,
) -> Result<Response, ContractError> {
    let mut attributes = vec![attr("action", "set_config")];

    let mut config = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(factory_contract) = factory_contract {
        config.factory_contract = deps.api.addr_validate(&factory_contract)?;
        attributes.push(attr("factory_contract", &factory_contract));
    };

    if let Some(staking_contract) = staking_contract {
        config.staking_contract = Some(deps.api.addr_validate(&staking_contract)?);
        attributes.push(attr("staking_contract", &staking_contract));
    };

    if let Some(action) = governance_contract {
        match action {
            UpdateAddr::Set(gov) => {
                config.governance_contract = Some(deps.api.addr_validate(&gov)?);
                attributes.push(attr("governance_contract", &gov));
            }
            UpdateAddr::Remove {} => {
                if config.staking_contract.is_none() {
                    return Err(StdError::generic_err(
                        "Cannot remove governance contract if staking contract is not set",
                    )
                    .into());
                }
                attributes.push(attr("governance_contract", "removed"));
                config.governance_contract = None;
            }
        }
    }

    if let Some(governance_percent) = governance_percent {
        if governance_percent > Uint64::new(100) {
            return Err(ContractError::IncorrectGovernancePercent {});
        };
        if config.staking_contract.is_none() && governance_percent != Uint64::new(100) {
            return Err(ContractError::GovernancePercentMustBe100 {});
        }

        config.governance_percent = governance_percent;
        attributes.push(attr("governance_percent", governance_percent));
    };

    if let Some(default_bridge) = &default_bridge_opt {
        default_bridge.check(deps.api)?;
        attributes.push(attr("default_bridge", default_bridge.to_string()));
        config.default_bridge = default_bridge_opt;
    }

    if let Some(max_spread) = max_spread {
        if max_spread.is_zero() || max_spread > Decimal::from_str(MAX_ALLOWED_SLIPPAGE)? {
            return Err(ContractError::IncorrectMaxSpread {});
        };

        config.max_spread = max_spread;
        attributes.push(attr("max_spread", max_spread.to_string()));
    };

    update_second_receiver_cfg(deps.as_ref(), &mut config, &second_receiver_params)?;

    if let Some(second_receiver_params) = second_receiver_params {
        attributes.push(attr(
            "second_fee_receiver",
            second_receiver_params.second_fee_receiver,
        ));
        attributes.push(attr(
            "second_receiver_cut",
            second_receiver_params.second_receiver_cut,
        ));
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new().add_attributes(attributes))
}

/// Adds or removes bridge tokens used to swap fee tokens to ASTRO.
///
/// * **add** array of bridge tokens added to swap fee tokens with.
///
/// * **remove** array of bridge tokens removed from being used to swap certain fee tokens.
///
/// ## Executor
/// Only the owner can execute this.
fn update_bridges(
    deps: DepsMut,
    info: MessageInfo,
    add: Option<Vec<(AssetInfo, AssetInfo)>>,
    remove: Option<Vec<AssetInfo>>,
) -> Result<Response, ContractError> {
    let cfg = CONFIG.load(deps.storage)?;

    // Permission check
    if info.sender != cfg.owner {
        return Err(ContractError::Unauthorized {});
    }

    // Remove old bridges
    if let Some(remove_bridges) = remove {
        for asset in remove_bridges {
            BRIDGES.remove(
                deps.storage,
                deps.api.addr_validate(&asset.to_string())?.to_string(),
            );
        }
    }

    // Add new bridges
    let astro = cfg.astro_token.clone();
    if let Some(add_bridges) = add {
        for (asset, bridge) in add_bridges {
            if asset.equal(&bridge) {
                return Err(ContractError::InvalidBridge(asset, bridge));
            }

            // Check that bridge tokens can be swapped to ASTRO
            validate_bridge(
                deps.as_ref(),
                &cfg.factory_contract,
                &asset,
                &bridge,
                &astro,
                BRIDGES_INITIAL_DEPTH,
            )?;

            BRIDGES.save(deps.storage, asset.to_string(), &bridge)?;
        }
    }

    Ok(Response::default().add_attribute("action", "update_bridges"))
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns the Maker contract configuration using a [`ConfigResponse`] object.
///
/// * **QueryMsg::Balances { assets }** Returns the balances of certain fee tokens accrued by the Maker
/// using a [`ConfigResponse`] object.
///
/// * **QueryMsg::Bridges {}** Returns the bridges used for swapping fee tokens
/// using a vector of [`(String, String)`] denoting Asset -> Bridge connections.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_get_config(deps)?),
        QueryMsg::Balances { assets } => to_binary(&query_get_balances(deps, env, assets)?),
        QueryMsg::Bridges {} => to_binary(&query_bridges(deps)?),
    }
}

/// Returns information about the Maker configuration using a [`ConfigResponse`] object.
fn query_get_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner,
        factory_contract: config.factory_contract,
        staking_contract: config.staking_contract,
        governance_contract: config.governance_contract,
        governance_percent: config.governance_percent,
        astro_token: config.astro_token,
        max_spread: config.max_spread,
        remainder_reward: config.remainder_reward,
        pre_upgrade_astro_amount: config.pre_upgrade_astro_amount,
        default_bridge: config.default_bridge,
        second_receiver_cfg: config.second_receiver_cfg,
    })
}

/// Returns Maker's fee token balances for specific tokens using a [`BalancesResponse`] object.
///
/// * **assets** array with assets for which we query the Maker's balances.
fn query_get_balances(deps: Deps, env: Env, assets: Vec<AssetInfo>) -> StdResult<BalancesResponse> {
    let mut resp = BalancesResponse { balances: vec![] };

    for a in assets {
        // Get balance
        let balance = a.query_pool(&deps.querier, &env.contract.address)?;
        if !balance.is_zero() {
            resp.balances.push(Asset {
                info: a,
                amount: balance,
            })
        }
    }

    Ok(resp)
}

/// Returns bridge tokens used for swapping fee tokens to ASTRO.
fn query_bridges(deps: Deps) -> StdResult<Vec<(String, String)>> {
    BRIDGES
        .range(deps.storage, None, None, Order::Ascending)
        .map(|bridge| {
            let (bridge, asset) = bridge?;
            Ok((bridge, asset.to_string()))
        })
        .collect()
}

/// Manages contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-maker" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" | "1.1.0" => {
                migrate_from_v1(deps.branch(), &msg)?;
            }
            "1.2.0" => migrate_from_v120(deps.branch(), msg)?,
            "1.3.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
