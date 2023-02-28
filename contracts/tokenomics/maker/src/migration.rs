use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, StdResult, Uint128, Uint64};
use cw_storage_plus::Item;

use astroport::asset::token_asset_info;
use astroport::maker::MigrateMsg;

use crate::state::{Config, CONFIG};

pub(crate) fn migrate_to_v110(deps: DepsMut, msg: MigrateMsg) -> StdResult<()> {
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

    let new_config = Config {
        owner: old_config.owner,
        factory_contract: old_config.factory_contract,
        staking_contract: Some(old_config.staking_contract),
        default_bridge: msg.default_bridge,
        governance_contract: old_config.governance_contract,
        governance_percent: old_config.governance_percent,
        astro_token: token_asset_info(old_config.astro_token_contract),
        max_spread: old_config.max_spread,
        rewards_enabled: old_config.rewards_enabled,
        pre_upgrade_blocks: old_config.pre_upgrade_blocks,
        last_distribution_block: old_config.last_distribution_block,
        remainder_reward: old_config.remainder_reward,
        pre_upgrade_astro_amount: old_config.pre_upgrade_astro_amount,
    };

    CONFIG.save(deps.storage, &new_config)
}
