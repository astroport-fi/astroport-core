use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdResult, Uint128, Uint64};
use cw_storage_plus::Item;

use crate::state::CONFIG;
use crate::utils::update_second_receiver_cfg;
use astroport::asset::{token_asset_info, AssetInfo};
use astroport::maker::{Config, MigrateMsg};

pub(crate) fn migrate_from_v1(deps: DepsMut, msg: &MigrateMsg) -> StdResult<()> {
    #[cw_serde]
    struct OldConfig {
        pub owner: Addr,
        pub factory_contract: Addr,
        pub staking_contract: Addr,
        pub governance_contract: Option<Addr>,
        pub governance_percent: Uint64,
        pub astro_token_contract: Addr,
        pub max_spread: Decimal,
        pub rewards_enabled: bool,
        pub pre_upgrade_blocks: u64,
        pub last_distribution_block: u64,
        pub remainder_reward: Uint128,
        pub pre_upgrade_astro_amount: Uint128,
    }
    let old_config: OldConfig = Item::new("config").load(deps.storage)?;

    if let Some(default_bridge) = &msg.default_bridge {
        default_bridge.check(deps.api)?
    }

    let mut new_config = Config {
        owner: old_config.owner,
        factory_contract: old_config.factory_contract,
        staking_contract: Some(old_config.staking_contract),
        default_bridge: msg.default_bridge.clone(),
        governance_contract: old_config.governance_contract,
        governance_percent: old_config.governance_percent,
        astro_token: token_asset_info(old_config.astro_token_contract),
        max_spread: old_config.max_spread,
        rewards_enabled: old_config.rewards_enabled,
        pre_upgrade_blocks: old_config.pre_upgrade_blocks,
        last_distribution_block: old_config.last_distribution_block,
        remainder_reward: old_config.remainder_reward,
        pre_upgrade_astro_amount: old_config.pre_upgrade_astro_amount,
        second_receiver_cfg: None,
    };

    update_second_receiver_cfg(deps.as_ref(), &mut new_config, &msg.second_receiver_params)?;

    CONFIG.save(deps.storage, &new_config)
}

pub(crate) fn migrate_from_v120(deps: DepsMut, msg: MigrateMsg) -> StdResult<()> {
    #[cw_serde]
    struct ConfigV120 {
        pub owner: Addr,
        pub factory_contract: Addr,
        pub staking_contract: Option<Addr>,
        pub default_bridge: Option<AssetInfo>,
        pub governance_contract: Option<Addr>,
        pub governance_percent: Uint64,
        pub astro_token: AssetInfo,
        pub max_spread: Decimal,
        pub rewards_enabled: bool,
        pub pre_upgrade_blocks: u64,
        pub last_distribution_block: u64,
        pub remainder_reward: Uint128,
        pub pre_upgrade_astro_amount: Uint128,
    }
    let cfg_v120: ConfigV120 = Item::new("config").load(deps.storage)?;

    let mut new_config = Config {
        owner: cfg_v120.owner,
        factory_contract: cfg_v120.factory_contract,
        staking_contract: cfg_v120.staking_contract,
        default_bridge: cfg_v120.default_bridge,
        governance_contract: cfg_v120.governance_contract,
        governance_percent: cfg_v120.governance_percent,
        astro_token: cfg_v120.astro_token,
        max_spread: cfg_v120.max_spread,
        rewards_enabled: cfg_v120.rewards_enabled,
        pre_upgrade_blocks: cfg_v120.pre_upgrade_blocks,
        last_distribution_block: cfg_v120.last_distribution_block,
        remainder_reward: cfg_v120.remainder_reward,
        pre_upgrade_astro_amount: cfg_v120.pre_upgrade_astro_amount,
        second_receiver_cfg: None,
    };

    update_second_receiver_cfg(deps.as_ref(), &mut new_config, &msg.second_receiver_params)?;

    CONFIG.save(deps.storage, &new_config)
}
