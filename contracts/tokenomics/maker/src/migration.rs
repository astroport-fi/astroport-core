use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, DepsMut, Uint128, Uint64};
use cw_storage_plus::Item;

use astroport::asset::AssetInfo;
use astroport::maker::{Config, MigrateMsg, SecondReceiverConfig};

use crate::error::ContractError;
use crate::state::CONFIG;
use crate::utils::{update_second_receiver_cfg, validate_cooldown};

pub(crate) fn migrate_from_v120_plus(deps: DepsMut, msg: MigrateMsg) -> Result<(), ContractError> {
    #[cw_serde]
    struct ConfigV130 {
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
        pub second_receiver_cfg: Option<SecondReceiverConfig>, // even tho versions < v1.3.0 don't have this field this is fully compatible for serde as this field is optional
    }
    let cfg_v130: ConfigV130 = Item::new("config").load(deps.storage)?;

    validate_cooldown(msg.collect_cooldown)?;

    let mut new_config = Config {
        owner: cfg_v130.owner,
        factory_contract: cfg_v130.factory_contract,
        staking_contract: cfg_v130.staking_contract,
        default_bridge: cfg_v130.default_bridge,
        governance_contract: cfg_v130.governance_contract,
        governance_percent: cfg_v130.governance_percent,
        astro_token: cfg_v130.astro_token,
        max_spread: cfg_v130.max_spread,
        rewards_enabled: cfg_v130.rewards_enabled,
        pre_upgrade_blocks: cfg_v130.pre_upgrade_blocks,
        last_distribution_block: cfg_v130.last_distribution_block,
        remainder_reward: cfg_v130.remainder_reward,
        pre_upgrade_astro_amount: cfg_v130.pre_upgrade_astro_amount,
        second_receiver_cfg: cfg_v130.second_receiver_cfg,
        collect_cooldown: msg.collect_cooldown,
    };

    update_second_receiver_cfg(deps.as_ref(), &mut new_config, &msg.second_receiver_params)?;

    Ok(CONFIG.save(deps.storage, &new_config)?)
}
