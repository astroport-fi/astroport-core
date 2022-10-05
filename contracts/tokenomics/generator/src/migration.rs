use crate::state::{Config, CONFIG};
use astroport::asset::{addr_validate_to_lower, AssetInfo};

use astroport::generator::MigrateMsg;
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdError, StdResult, Storage, Uint128, Uint64};
use cw_storage_plus::{Item, Map};

/// This structure stores the parameters for a generator (in the upgraded version of the Generator contract).
#[cw_serde]
pub struct PoolInfoV100 {
    /// This is the share of ASTRO rewards that this generator receives every block
    pub alloc_point: Uint64,
    /// Accumulated amount of rewards per LP unit. Used for reward calculations
    pub last_reward_block: Uint64,
    /// This is the accrued amount of rewards up to the latest checkpoint
    pub accumulated_rewards_per_share: Decimal,
    /// The 3rd party proxy reward contract
    pub reward_proxy: Option<Addr>,
    /// This is the accrued amount of 3rd party rewards up to the latest checkpoint
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// This is the balance of 3rd party proxy rewards that the proxy had before a reward snapshot
    pub proxy_reward_balance_before_update: Uint128,
    /// The orphaned proxy rewards which are left behind by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
}

/// Stores the contract config(V1.0.0) at the given key
pub const POOL_INFOV100: Map<&Addr, PoolInfoV100> = Map::new("pool_info");

/// This structure stores the parameters for a generator (in the upgraded version v1.1.0 of the Generator contract).
#[cw_serde]
pub struct PoolInfoV110 {
    /// Allocation point is used to control reward distribution among the pools
    pub alloc_point: Uint64,
    /// Accumulated amount of reward per share unit. Used for reward calculations
    pub last_reward_block: Uint64,
    pub accumulated_rewards_per_share: Decimal,
    /// the reward proxy contract
    pub reward_proxy: Option<Addr>,
    pub accumulated_proxy_rewards_per_share: Decimal,
    /// for calculation of new proxy rewards
    pub proxy_reward_balance_before_update: Uint128,
    /// the orphan proxy rewards which are left by emergency withdrawals
    pub orphan_proxy_rewards: Uint128,
    /// The pool has assets giving additional rewards
    pub has_asset_rewards: bool,
}

/// Stores the contract config(V1.1.0) at the given key
pub const POOL_INFOV110: Map<&Addr, PoolInfoV110> = Map::new("pool_info");

/// This structure describes the main control config of generator.
#[cw_serde]
pub struct ConfigV100 {
    /// Contract address that used for controls settings
    pub owner: Addr,
    /// The ASTRO token address
    pub astro_token: Addr,
    /// Total amount of ASTRO rewards per block
    pub tokens_per_block: Uint128,
    /// The total allocation points. Must be the sum of all allocation points in all pools.
    pub total_alloc_point: Uint64,
    /// The block number when ASTRO mining starts.
    pub start_block: Uint64,
    /// The list of allowed reward proxy contracts
    pub allowed_reward_proxies: Vec<Addr>,
    /// The vesting contract from which rewards are distributed
    pub vesting_contract: Addr,
}

/// This structure stores the core parameters for the Generator contract.
#[cw_serde]
pub struct ConfigV120 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
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
    /// The blocked list of tokens
    pub blocked_list_tokens: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
}

/// Stores the contract config(V1.1.0) at the given key
pub const CONFIGV100: Item<ConfigV100> = Item::new("config");

/// Stores the contract config(V1.2.0) at the given key
pub const CONFIGV120: Item<ConfigV120> = Item::new("config");

/// Migrate config to V1.2.0
pub fn migrate_configs_to_v120(
    deps: &mut DepsMut,
    pools: Vec<(Addr, Uint64)>,
    msg: &MigrateMsg,
) -> Result<(), StdError> {
    let cfg_100 = CONFIGV100.load(deps.storage)?;
    let pools = pools
        .into_iter()
        .map(|(addr, apoints)| (addr, apoints.into()))
        .collect();

    let mut cfg = ConfigV120 {
        owner: cfg_100.owner,
        factory: addr_validate_to_lower(deps.api, &msg.factory.clone().unwrap())?,
        generator_controller: None,
        astro_token: cfg_100.astro_token,
        tokens_per_block: cfg_100.tokens_per_block,
        total_alloc_point: cfg_100.total_alloc_point.into(),
        start_block: cfg_100.start_block,
        allowed_reward_proxies: cfg_100.allowed_reward_proxies,
        vesting_contract: cfg_100.vesting_contract,
        active_pools: pools,
        blocked_list_tokens: vec![],
        guardian: None,
    };

    if let Some(generator_controller) = &msg.generator_controller {
        cfg.generator_controller = Some(addr_validate_to_lower(deps.api, generator_controller)?);
    }

    if let Some(blocked_list_tokens) = &msg.blocked_list_tokens {
        cfg.blocked_list_tokens = blocked_list_tokens.to_owned();
    }

    if let Some(guardian) = &msg.guardian {
        cfg.guardian = Some(addr_validate_to_lower(deps.api, guardian)?);
    }

    CONFIGV120.save(deps.storage, &cfg)?;

    Ok(())
}

/// Migrate config to V1.3.0
pub fn migrate_configs_to_v130(storage: &mut dyn Storage) -> StdResult<()> {
    let cfg_120 = CONFIGV120.load(storage)?;
    let cfg = ConfigV130 {
        owner: cfg_120.owner,
        factory: cfg_120.factory,
        generator_controller: cfg_120.generator_controller,
        astro_token: cfg_120.astro_token,
        tokens_per_block: cfg_120.tokens_per_block,
        total_alloc_point: cfg_120.total_alloc_point,
        start_block: cfg_120.start_block,
        allowed_reward_proxies: cfg_120.allowed_reward_proxies,
        vesting_contract: cfg_120.vesting_contract,
        active_pools: cfg_120.active_pools,
        blocked_tokens_list: cfg_120.blocked_list_tokens, // renamed this field
        guardian: cfg_120.guardian,
    };

    CONFIGV130.save(storage, &cfg)
}

/// This structure describes the main control config of generator.
#[cw_serde]
pub struct ConfigV130 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
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
    /// The blocked list of tokens
    pub blocked_tokens_list: Vec<AssetInfo>,
    /// The guardian address which can add or remove tokens from blacklist
    pub guardian: Option<Addr>,
}

/// Stores the contract config(V1.3.0) at the given key
pub const CONFIGV130: Item<ConfigV130> = Item::new("config");

/// Migrate config to V2.0.0
pub fn migrate_configs_to_v200(deps: &mut DepsMut, msg: &MigrateMsg) -> Result<(), StdError> {
    let cfg_130 = CONFIGV130.load(deps.storage)?;

    let mut cfg = Config {
        owner: cfg_130.owner,
        factory: cfg_130.factory,
        generator_controller: cfg_130.generator_controller,
        voting_escrow: None,
        astro_token: cfg_130.astro_token,
        tokens_per_block: cfg_130.tokens_per_block,
        total_alloc_point: cfg_130.total_alloc_point,
        start_block: cfg_130.start_block,
        allowed_reward_proxies: cfg_130.allowed_reward_proxies,
        vesting_contract: cfg_130.vesting_contract,
        active_pools: cfg_130.active_pools,
        guardian: cfg_130.guardian,
        blocked_tokens_list: cfg_130.blocked_tokens_list,
        checkpoint_generator_limit: None,
    };

    if let Some(voting_escrow) = &msg.voting_escrow {
        cfg.voting_escrow = Some(addr_validate_to_lower(deps.api, voting_escrow)?);
    }

    if let Some(generator_limit) = msg.generator_limit {
        cfg.checkpoint_generator_limit = Some(generator_limit);
    }

    CONFIG.save(deps.storage, &cfg)?;

    Ok(())
}
