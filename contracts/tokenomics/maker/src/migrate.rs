#![cfg(not(tarpaulin_include))]

use crate::error::ContractError;
use crate::instantiate::{CONTRACT_NAME, CONTRACT_VERSION};
use crate::state::CONFIG;
use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory;
use astroport::maker::{Config, DevFundConfig};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, Decimal, DepsMut, Empty, Env, Response, StdError, StdResult};
use cw_storage_plus::{Item, Map};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OldDevFundConfig {
    /// The dev fund address
    pub address: String,
    /// The percentage of fees that go to the dev fund
    pub share: Decimal,
    /// Asset that devs want ASTRO to be swapped to
    pub asset_info: AssetInfo,
}

// Ignoring unnecessary fields
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct OldConfig {
    /// Address that's allowed to set contract parameters
    pub owner: Addr,
    /// The factory contract address
    pub factory_contract: Addr,
    /// The xASTRO staking contract address.
    pub staking_contract: Option<Addr>,
    /// The dev fund configuration
    pub dev_fund_conf: Option<OldDevFundConfig>,
    // /// Default bridge asset (Terra1 - LUNC, Terra2 - LUNA, etc.)
    // pub default_bridge: Option<AssetInfo>,
    /// The vxASTRO fee distributor contract address
    pub governance_contract: Option<Addr>,
    // /// The percentage of fees that go to the vxASTRO fee distributor
    // pub governance_percent: Uint64,
    /// The ASTRO token asset info
    pub astro_token: AssetInfo,
    /// The max spread allowed when swapping fee tokens to ASTRO
    pub max_spread: Decimal,
    // /// The flag which determines whether accrued ASTRO from fee swaps is being distributed or not
    // pub rewards_enabled: bool,
    // /// The number of blocks over which ASTRO that accrued pre-upgrade will be distributed
    // pub pre_upgrade_blocks: u64,
    // /// The last block until which pre-upgrade ASTRO will be distributed
    // pub last_distribution_block: u64,
    // /// The remainder of pre-upgrade ASTRO to distribute
    // pub remainder_reward: Uint128,
    // /// The amount of collected ASTRO before enabling rewards distribution
    // pub pre_upgrade_astro_amount: Uint128,
    // /// Parameters that describe the second receiver of fees
    // pub second_receiver_cfg: Option<SecondReceiverConfig>,
    /// If set defines the period when maker collect can be called
    pub collect_cooldown: Option<u64>,
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    let contract_version = cw2::get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        CONTRACT_NAME => match contract_version.version.as_ref() {
            "1.7.0" => {
                let old_config: OldConfig = Item::new("config").load(deps.storage)?;

                let astro_denom = match &old_config.astro_token {
                    AssetInfo::Token { .. } => {
                        Err(StdError::generic_err("ASTRO token must be native token"))
                    }
                    AssetInfo::NativeToken { denom } => Ok(denom.clone()),
                }?;

                let collector = match (
                    &old_config.governance_contract,
                    &old_config.staking_contract,
                ) {
                    (Some(gov_contract), None) => Ok(gov_contract.clone()),
                    (_, Some(staking)) => Ok(staking.clone()),
                    (None, None) => Err(StdError::generic_err(
                        "Both governance and staking contracts aren't set. Can't proceed with migration",
                    )),
                }?;

                let config = Config {
                    owner: old_config.owner,
                    factory_contract: old_config.factory_contract.clone(),
                    dev_fund_conf: old_config
                        .dev_fund_conf
                        .map(|old_dev_conf| -> StdResult<_> {
                            let pool_addr = deps
                                .querier
                                .query_wasm_smart::<PairInfo>(
                                    &old_config.factory_contract,
                                    &factory::QueryMsg::Pair {
                                        asset_infos: vec![
                                            old_dev_conf.asset_info.clone(),
                                            old_config.astro_token.clone(),
                                        ],
                                    },
                                )?
                                .contract_addr;

                            Ok(DevFundConfig {
                                address: old_dev_conf.address,
                                share: old_dev_conf.share,
                                asset_info: old_dev_conf.asset_info,
                                pool_addr,
                            })
                        })
                        .transpose()?,
                    astro_denom,
                    collector,
                    max_spread: old_config.max_spread,
                    collect_cooldown: old_config.collect_cooldown,
                };

                CONFIG.save(deps.storage, &config)?;

                Map::<String, AssetInfo>::new("bridges").clear(deps.storage);
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
