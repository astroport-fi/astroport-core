use crate::state::{CONFIG, USER_INFO};
use astroport::asset::AssetInfo;

use astroport::generator::{Config, MigrateMsg};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdError, StdResult, Uint128, Uint64};
use cw_storage_plus::Item;

/// This structure stores the core parameters for the Generator contract.
#[cw_serde]
pub struct ConfigV220 {
    /// Address allowed to change contract parameters
    pub owner: Addr,
    /// The Factory address
    pub factory: Addr,
    /// Contract address which can only set active generators and their alloc points
    pub generator_controller: Option<Addr>,
    /// The voting escrow contract address
    pub voting_escrow: Option<Addr>,
    /// [`AssetInfo`] of the ASTRO token
    pub astro_token: AssetInfo,
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

/// Stores the contract config(V2.2.0) at the given key
pub const CONFIG_V220: Item<ConfigV220> = Item::new("config");

/// Migrate config from V2.2.0
pub fn migrate_configs_from_v220(deps: &mut DepsMut, msg: &MigrateMsg) -> StdResult<()> {
    let cfg_220 = CONFIG_V220.load(deps.storage)?;

    let mut cfg = Config {
        owner: cfg_220.owner,
        factory: cfg_220.factory,
        generator_controller: cfg_220.generator_controller,
        voting_escrow: cfg_220.voting_escrow,
        voting_escrow_delegation: None,
        astro_token: cfg_220.astro_token,
        tokens_per_block: cfg_220.tokens_per_block,
        total_alloc_point: cfg_220.total_alloc_point,
        start_block: cfg_220.start_block,
        vesting_contract: cfg_220.vesting_contract,
        active_pools: cfg_220.active_pools,
        blocked_tokens_list: cfg_220.blocked_tokens_list,
        guardian: cfg_220.guardian,
        checkpoint_generator_limit: cfg_220.checkpoint_generator_limit,
    };

    if let Some(voting_escrow_delegation) = &msg.voting_escrow_delegation {
        cfg.voting_escrow_delegation = Some(deps.api.addr_validate(voting_escrow_delegation)?);
    }

    CONFIG.save(deps.storage, &cfg)
}

pub fn fix_neutron_users_reward_indexes(deps: &mut DepsMut) -> StdResult<()> {
    let pool1 =
        Addr::unchecked("neutron1sx99fxy4lqx0nv3ys86tkdrch82qygxyec5c8dxsk9raz4at5zpq48m66c");
    let pool2 =
        Addr::unchecked("neutron1jkcf80nd4pfc2krce3xk9m9y994pllq58avx89sfzqlalej4frus27ms3a");

    let depositor =
        Addr::unchecked("neutron1ryhxe5fzczelcfmrhmcw9x2jsqy677fw59fsctr09srk24lt93eszwlvyj");

    // We already know that the new user info structure is used and that the values of that type exist there
    USER_INFO.update::<_, StdError>(deps.storage, (&pool1, &depositor), |v| {
        let mut r = v.unwrap();
        r.reward_user_index += Decimal::raw(1960025734161847622);
        Ok(r)
    })?;
    USER_INFO.update::<_, StdError>(deps.storage, (&pool2, &depositor), |v| {
        let mut r = v.unwrap();
        r.reward_user_index += Decimal::raw(1301823709312052739);
        Ok(r)
    })?;

    Ok(())
}
