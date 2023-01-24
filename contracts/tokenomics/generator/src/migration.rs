use crate::state::{Config, CONFIG};
use astroport::asset::{token_asset_info, AssetInfo};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, DepsMut, StdError, StdResult, Uint128, Uint64};
use cw_storage_plus::Item;

/// This structure stores the core parameters for the Generator contract.
#[cw_serde]
pub struct ConfigV200 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// The voting escrow contract address
    pub voting_escrow: Option<Addr>,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_point: Uint128,
    /// The block number when the ASTRO distribution starts
    pub start_block: Uint64,
    /// The list of allowed proxy reward contracts
    pub allowed_reward_proxies: Vec<Addr>,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
    /// The list of active pools with allocation points
    pub active_pools: Vec<(Addr, Uint128)>,
    /// The list of blocked tokens
    pub blocked_tokens_list: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
    /// The amount of generators
    pub checkpoint_generator_limit: Option<u32>,
}

#[cw_serde]
pub struct ConfigV210 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// The voting escrow contract address
    pub voting_escrow: Option<Addr>,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// Total allocation points. Must be the sum of all allocation points in all active generators
    pub total_alloc_point: Uint128,
    /// The block number when the ASTRO distribution starts
    pub start_block: Uint64,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
    /// The list of active pools with allocation points
    pub active_pools: Vec<(Addr, Uint128)>,
    /// The list of blocked tokens
    pub blocked_tokens_list: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
    /// The amount of generators
    pub checkpoint_generator_limit: Option<u32>,
}

/// Stores the contract config(V2.0.0) at the given key
pub const CONFIG_V200: Item<ConfigV200> = Item::new("config");

/// Stores the contract config(V2.1.0) at the given key
pub const CONFIG_V210: Item<ConfigV210> = Item::new("config");

/// Migrate config to V2.0.0
pub fn migrate_configs_from_v200(deps: &mut DepsMut) -> Result<(), StdError> {
    let cfg_200 = CONFIG_V200.load(deps.storage)?;

    let cfg = Config {
        owner: cfg_200.owner,
        factory: cfg_200.factory,
        generator_controller: cfg_200.generator_controller,
        voting_escrow: cfg_200.voting_escrow,
        astro_token: token_asset_info(cfg_200.astro_token),
        tokens_per_block: cfg_200.tokens_per_block,
        total_alloc_point: cfg_200.total_alloc_point,
        start_block: cfg_200.start_block,
        vesting_contract: cfg_200.vesting_contract,
        active_pools: cfg_200.active_pools,
        guardian: cfg_200.guardian,
        blocked_tokens_list: cfg_200.blocked_tokens_list,
        checkpoint_generator_limit: cfg_200.checkpoint_generator_limit,
    };

    CONFIG.save(deps.storage, &cfg)?;
    Ok(())
}

/// Migrate config from V2.1.0
pub fn migrate_configs_from_v210(deps: &mut DepsMut) -> StdResult<()> {
    let cfg_210 = CONFIG_V210.load(deps.storage)?;
    let cfg = Config {
        owner: cfg_210.owner,
        factory: cfg_210.factory,
        generator_controller: cfg_210.generator_controller,
        voting_escrow: cfg_210.voting_escrow,
        astro_token: token_asset_info(cfg_210.astro_token),
        tokens_per_block: cfg_210.tokens_per_block,
        total_alloc_point: cfg_210.total_alloc_point,
        start_block: cfg_210.start_block,
        vesting_contract: cfg_210.vesting_contract,
        active_pools: cfg_210.active_pools,
        blocked_tokens_list: cfg_210.blocked_tokens_list,
        guardian: cfg_210.guardian,
        checkpoint_generator_limit: cfg_210.checkpoint_generator_limit,
    };

    CONFIG.save(deps.storage, &cfg)
}
