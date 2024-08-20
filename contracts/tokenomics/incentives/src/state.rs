use std::collections::{HashMap, HashSet};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal256, Env, Order, StdError, StdResult, Storage, Uint128, Uint256};
use cw_storage_plus::{Bound, Item, Map};
use itertools::Itertools;

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::common::OwnershipProposal;
use astroport::incentives::{Config, IncentivesSchedule};
use astroport::incentives::{PoolInfoResponse, RewardInfo, RewardType};
use astroport::incentives::{MAX_PAGE_LIMIT, MAX_REWARD_TOKENS};

use crate::error::ContractError;
use crate::traits::RewardInfoExt;
use crate::utils::asset_info_key;

/// General Incentives contract settings
pub const CONFIG: Item<Config> = Item::new("config");

/// Contains a proposal to change contract ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");
/// Pools which receive ASTRO emissions
pub const ACTIVE_POOLS: Item<Vec<(AssetInfo, Uint128)>> = Item::new("active_pools");
/// Prohibited tokens set. Key: binary representing [`AssetInfo`] converted with [`crate::utils::asset_info_key`].
pub const BLOCKED_TOKENS: Map<&[u8], ()> = Map::new("blocked_tokens");

/// Contains reward indexes for finished rewards. They are removed from [`PoolInfo`] and stored here.
/// Next time user claims rewards they will be able to claim outstanding rewards from this index.
/// key: (LP token asset, deregistration timestamp), value: array of tuples (reward token asset, reward index).
pub const FINISHED_REWARD_INDEXES: Map<(&AssetInfo, u64), Vec<(AssetInfo, Decimal256)>> =
    Map::new("fin_rew_inds");

/// key: lp_token (either cw20 or native), value: pool info
pub const POOLS: Map<&AssetInfo, PoolInfo> = Map::new("pools");
/// key: (lp_token, user_addr), value: user info
pub const USER_INFO: Map<(&AssetInfo, &Addr), UserInfo> = Map::new("user_info");
/// key: (LP token asset, reward token asset, schedule end point), value: reward per second
pub const EXTERNAL_REWARD_SCHEDULES: Map<(&AssetInfo, &AssetInfo, u64), Decimal256> =
    Map::new("reward_schedules");

/// Accumulates all orphaned rewards i.e. those which were added to a pool
/// but this pool never received any LP tokens deposits.
/// key: Key: binary representing [`AssetInfo`] converted with [`asset_info_key`],
/// value: total amount of orphaned tokens
pub const ORPHANED_REWARDS: Map<&[u8], Uint128> = Map::new("orphaned_rewards");

impl RewardInfoExt for RewardInfo {
    /// This function is tightly coupled with [`UserInfo`] structure. It iterates over all user's
    /// reward indexes and tries to find the one that matches current reward info. If found, it
    /// calculates the reward amount.
    /// Otherwise it assumes user never claimed this particular reward and their reward index is 0.
    /// Their position will be synced with pool indexes later on.
    fn calculate_reward(&self, user_info: &UserInfo) -> StdResult<Uint128> {
        let user_index_opt = user_info
            .last_rewards_index
            .iter()
            .find(|(reward_type, _)| reward_type.matches(&self.reward));

        // In case reward was moved into finished state and then is incentivized again
        // it might have self.reward_index == 0, but user index might be > 0 containing outstanding
        // rewards from past schedules.
        // Outstanding rewards from finished schedules are handled in claim_finished_rewards().
        // To account current active period properly we need to consider user index as 0.
        let user_amount = Uint256::from(user_info.amount);
        let u256_result = match user_index_opt {
            Some((_, user_reward_index)) if *user_reward_index > self.index => {
                self.index * user_amount
            }
            None => self.index * user_amount,
            Some((_, user_reward_index)) => (self.index - *user_reward_index) * user_amount,
        };

        Ok(u256_result.try_into()?)
    }
}

#[cw_serde]
#[derive(Default)]
pub struct PoolInfo {
    /// Total amount of LP tokens staked in this pool
    pub total_lp: Uint128,
    /// Vector containing reward info for each reward token
    pub rewards: Vec<RewardInfo>,
    /// Last time when reward indexes were updated
    pub last_update_ts: u64,
    /// Rewards to remove; In-memory hash map to avoid unnecessary state writes;
    /// Key: reward type, value: (reward index, orphaned rewards)
    /// NOTE: this is not part of serialized structure in state!
    #[serde(skip)]
    pub rewards_to_remove: HashMap<RewardType, (Decimal256, Decimal256)>,
}

impl PoolInfo {
    /// Loop over all rewards and update their indexes according to the amount of LP tokens staked and rewards per second.
    /// If multiple schedules for a specific reward passed since the last update, aggregate all rewards.
    /// Move to the next schedule if it's time to do so or remove reward from pool info if there are no more schedules left.
    pub fn update_rewards(
        &mut self,
        storage: &dyn Storage,
        env: &Env,
        lp_asset: &AssetInfo,
    ) -> StdResult<()> {
        let block_ts = env.block.time.seconds();
        let time_passed: Uint128 = block_ts.saturating_sub(self.last_update_ts).into();

        if time_passed.is_zero() {
            return Ok(());
        }

        for reward_info in self.rewards.iter_mut() {
            let mut collected_rewards = Decimal256::zero();
            let mut time_passed_inner = time_passed;

            // Whether we need to remove this reward from pool info. Only applicable for finished external rewards.
            let mut need_remove = false;

            if let RewardType::Ext {
                info,
                next_update_ts,
            } = &reward_info.reward
            {
                let mut next_update_ts = *next_update_ts;
                // Time to move to the next schedule?
                if next_update_ts <= block_ts {
                    // Schedule ended. Collect leftovers from the last update time
                    collected_rewards += reward_info.rps
                        * Decimal256::from_ratio(next_update_ts - self.last_update_ts, 1u8);

                    // Find which passed schedules should be processed (can be multiple ones)
                    let schedules = EXTERNAL_REWARD_SCHEDULES.prefix((lp_asset, info)).range(
                        storage,
                        Some(Bound::exclusive(next_update_ts)),
                        None,
                        Order::Ascending,
                    );

                    for period in schedules {
                        let (update_ts, period_reward_per_sec) = period?;
                        // We found a schedule which should be active atm
                        if update_ts > block_ts {
                            reward_info.rps = period_reward_per_sec;
                            reward_info.reward = RewardType::Ext {
                                info: info.clone(),
                                next_update_ts: update_ts,
                            };
                            time_passed_inner = (block_ts - next_update_ts).into();
                            next_update_ts = update_ts;
                            break;
                        }

                        // Process schedules one by one and collect rewards
                        collected_rewards += period_reward_per_sec
                            * Decimal256::from_ratio(update_ts - next_update_ts, 1u8);
                        next_update_ts = update_ts;
                    }

                    // Check there are neither active nor upcoming schedules left
                    if next_update_ts <= block_ts {
                        // Remove reward from pool info
                        need_remove = true;
                        reward_info.rps = Decimal256::zero();
                    }
                }
            }

            collected_rewards += reward_info.rps * Decimal256::from_ratio(time_passed_inner, 1u8);

            if self.total_lp.is_zero() {
                reward_info.orphaned += collected_rewards;
            } else {
                // Allowing the first depositor to claim orphaned rewards
                reward_info.index += (reward_info.orphaned + collected_rewards)
                    / Decimal256::from_ratio(self.total_lp, 1u8);
                reward_info.orphaned = Decimal256::zero();
            }

            if need_remove {
                self.rewards_to_remove.insert(
                    reward_info.reward.clone(),
                    (reward_info.index, reward_info.orphaned),
                );
            }
        }

        // Remove finished rewards. Only external rewards can be removed from PoolInfo.
        self.rewards
            .retain(|r| !self.rewards_to_remove.contains_key(&r.reward));

        self.last_update_ts = env.block.time.seconds();

        Ok(())
    }

    /// This function calculates all rewards for a specific user position.
    /// Converts them to [`Asset`]. Returns array of tuples (is_external_reward, Asset).
    pub fn calculate_rewards(&self, user_info: &mut UserInfo) -> StdResult<Vec<(bool, Asset)>> {
        self.rewards
            .iter()
            .map(|reward_info| {
                let amount = reward_info.calculate_reward(user_info)?;
                Ok((
                    reward_info.reward.is_external(),
                    reward_info.reward.asset_info().with_balance(amount),
                ))
            })
            .collect()
    }

    /// Set astro per second for this pool according to alloc points and general astro per second value
    pub fn set_astro_rewards(&mut self, config: &Config, alloc_points: Uint128) {
        if let Some(astro_reward_info) = self.rewards.iter_mut().find(|r| !r.reward.is_external()) {
            astro_reward_info.rps = Decimal256::from_ratio(
                config.astro_per_second * alloc_points,
                config.total_alloc_points,
            );
        } else {
            self.rewards.push(RewardInfo {
                reward: RewardType::Int(config.astro_token.clone()),
                rps: Decimal256::from_ratio(
                    config.astro_per_second * alloc_points,
                    config.total_alloc_points,
                ),
                index: Default::default(),
                orphaned: Default::default(),
            });
        }
    }

    /// Check whether this pools receiving ASTRO emissions
    pub fn is_active_pool(&self) -> bool {
        self.rewards
            .iter()
            .any(|r| !r.reward.is_external() && !r.rps.is_zero())
    }

    /// This function disables ASTRO rewards in a specific pool.
    /// We must keep ASTRO schedule even tho reward per second becomes zero
    /// because users still should be able to claim outstanding rewards according to indexes.
    pub fn disable_astro_rewards(&mut self) {
        if let Some(astro_reward_info) = self.rewards.iter_mut().find(|r| !r.reward.is_external()) {
            astro_reward_info.rps = Decimal256::zero();
        }
    }

    /// Add external reward to a pool. If reward already exists, update its schedule.
    /// Complexity O(n + m) = O(m) where n - number of rewards in pool (constant),
    /// m - number of schedules that new schedule intersects.
    /// The idea is to walk through all schedules and increase them by new reward per second.  
    ///
    /// ## Algorithm description
    /// New schedule (start_x, end_x, rps_x).  
    /// Schedule always takes effect from the current block.  
    /// rps - rewards per second  
    ///
    /// There are several possible cases.
    /// 1. This is a new reward (i.e. no entries matching this reward in PoolInfo.rewards)
    ///     - Add External(end_x, rps_x) to PoolInfo.rewards array
    /// 2. Reward in PoolInfo contains schedule (start_s, rps_s). start_s is point when the next schedule should be picked up.
    ///     - Add rps_x to current active rps_s;
    ///     - Fetch all schedules from EXTERNAL_REWARD_SCHEDULES (array of pairs (end_s, rps_s)) where end_s > start_x;
    ///     - If end_s >= end_x then new schedule is fully covered by the first one. Set point (end_x, rps_s + rps_x);
    ///     - Otherwise loop over all schedules and update them until end_s >= end_x or until all schedules passed.
    pub fn incentivize(
        &mut self,
        storage: &mut dyn Storage,
        lp_asset: &AssetInfo,
        schedule: &IncentivesSchedule,
        astro_token: &AssetInfo,
    ) -> Result<(), ContractError> {
        let ext_rewards_len = self
            .rewards
            .iter()
            .filter(|r| r.reward.is_external())
            .count();

        let maybe_active_schedule = self.rewards.iter_mut().find(
            |r| matches!(&r.reward, RewardType::Ext { info, .. } if info == &schedule.reward_info),
        );

        // Check that we don't exceed the maximum number of reward tokens per pool.
        // Allowing ASTRO reward to exceed this limit
        if ext_rewards_len == MAX_REWARD_TOKENS as usize
            && maybe_active_schedule.is_none()
            && schedule.reward_info.ne(astro_token)
        {
            return Err(ContractError::TooManyRewardTokens {
                lp_token: lp_asset.to_string(),
            });
        }

        if let Some(active_schedule) = maybe_active_schedule {
            let next_update_ts = match &active_schedule.reward {
                RewardType::Ext { next_update_ts, .. } => *next_update_ts,
                RewardType::Int(_) => {
                    unreachable!("Only external rewards can be extended")
                }
            };

            let mut to_save = vec![];

            if next_update_ts >= schedule.end_ts {
                // Newly added schedule is fully covered by the first schedule.
                // Set a new break in schedule only if its end is greater
                if next_update_ts > schedule.end_ts {
                    to_save.push((next_update_ts, active_schedule.rps));
                }

                active_schedule.reward = RewardType::Ext {
                    info: schedule.reward_info.clone(),
                    next_update_ts: schedule.end_ts,
                };
            } else {
                // Create iterator starting from schedule.start_ts till the end
                let mut overlapping_schedules = EXTERNAL_REWARD_SCHEDULES
                    .prefix((lp_asset, &schedule.reward_info))
                    .range(
                        storage,
                        Some(Bound::exclusive(schedule.next_epoch_start_ts)),
                        None,
                        Order::Ascending,
                    );

                // Add rps to next overlapping schedules.
                loop {
                    if let Some((end_ts, rps_state)) = overlapping_schedules.next().transpose()? {
                        if end_ts >= schedule.end_ts {
                            to_save.push((schedule.end_ts, rps_state + schedule.rps));
                            break;
                        } else {
                            to_save.push((end_ts, rps_state + schedule.rps));
                        }
                    } else {
                        to_save.push((schedule.end_ts, schedule.rps));
                        break;
                    }
                }
            };

            // Update state
            for (update_ts, rps) in to_save {
                EXTERNAL_REWARD_SCHEDULES.save(
                    storage,
                    (lp_asset, &schedule.reward_info, update_ts),
                    &rps,
                )?;
            }

            // New schedule anyway hits an active one
            active_schedule.rps += schedule.rps;
        } else {
            self.rewards.push(RewardInfo {
                reward: RewardType::Ext {
                    info: schedule.reward_info.clone(),
                    next_update_ts: schedule.end_ts,
                },
                rps: schedule.rps,
                index: Default::default(),
                orphaned: Default::default(),
            });
        }

        Ok(())
    }

    /// Deregister specific reward from pool. Calculate accrued rewards at this point. Calculate remaining rewards
    /// (with those which didn't start yet) and remove upcoming schedules.
    /// Complexity is either O(1) or O(m) depending on bypass_upcoming_schedules toggle,
    /// where m - number of upcoming schedules.
    pub fn deregister_reward(
        &mut self,
        storage: &mut dyn Storage,
        lp_asset: &AssetInfo,
        reward_asset: &AssetInfo,
        bypass_upcoming_schedules: bool,
    ) -> Result<Uint128, ContractError> {
        let (pos, reward_info) = self
            .rewards
            .iter()
            .find_position(|reward| matches!(&reward.reward, RewardType::Ext { info, .. } if info == reward_asset))
            .ok_or_else(|| ContractError::RewardNotFound { pool: lp_asset.to_string(), reward: reward_asset.to_string() })?;
        self.rewards_to_remove.insert(
            reward_info.reward.clone(),
            (reward_info.index, reward_info.orphaned),
        );
        let reward_info = self.rewards.remove(pos);

        let next_update_ts = match &reward_info.reward {
            RewardType::Ext { next_update_ts, .. } => *next_update_ts,
            RewardType::Int(_) => unreachable!("Only external rewards can be deregistered"),
        };

        // Assume update_rewards() was called before
        let mut remaining = reward_info.rps
            * Decimal256::from_ratio(next_update_ts.saturating_sub(self.last_update_ts), 1u8);

        // Remove active schedule from state
        EXTERNAL_REWARD_SCHEDULES.remove(storage, (lp_asset, reward_asset, next_update_ts));

        // If there is too much spam in the state, we can bypass upcoming schedules
        if !bypass_upcoming_schedules {
            let schedules = EXTERNAL_REWARD_SCHEDULES
                .prefix((lp_asset, reward_asset))
                .range(
                    storage,
                    Some(Bound::exclusive(next_update_ts)),
                    None,
                    Order::Ascending,
                )
                .collect::<StdResult<Vec<_>>>()?;

            // Collect future rewards and remove future schedules from state
            let mut prev_time = next_update_ts;
            schedules
                .into_iter()
                .for_each(|(update_ts, period_reward_per_sec)| {
                    if update_ts > next_update_ts {
                        remaining += period_reward_per_sec
                            * Decimal256::from_ratio(update_ts - prev_time, 1u8);
                        prev_time = update_ts;
                    }

                    EXTERNAL_REWARD_SCHEDULES.remove(storage, (lp_asset, reward_asset, update_ts));
                })
        }

        // Take orphaned rewards as well
        remaining += reward_info.orphaned;

        Ok(remaining.to_uint_floor().try_into()?)
    }

    pub fn load(storage: &dyn Storage, lp_token: &AssetInfo) -> StdResult<Self> {
        POOLS.load(storage, lp_token)
    }

    pub fn may_load(storage: &dyn Storage, lp_token: &AssetInfo) -> StdResult<Option<Self>> {
        POOLS.may_load(storage, lp_token)
    }

    /// Reflect changes to pool info in state. Save finished rewards indexes from in-memory hash map.
    /// If reward schedule has orphaned rewards accumulate them in ORPHANED_REWARDS.
    /// This function consumes self just to make sure it becomes unusable after calling save().
    pub fn save(self, storage: &mut dyn Storage, lp_token: &AssetInfo) -> StdResult<()> {
        if !self.rewards_to_remove.is_empty() {
            self.rewards_to_remove
                .iter()
                .map(|(reward, index)| (reward.asset_info().clone(), *index))
                .group_by(|(_, (_, orphaned_amount))| orphaned_amount.is_zero())
                .into_iter()
                .try_for_each(|(is_zero, group)| {
                    if is_zero {
                        let finished_indexes = group
                            .map(|(reward_asset_info, (index, _))| (reward_asset_info, index))
                            .collect_vec();
                        FINISHED_REWARD_INDEXES.save(
                            storage,
                            (lp_token, self.last_update_ts),
                            &finished_indexes,
                        )
                    } else {
                        // Processing finished schedules with orphaned rewards
                        for (reward, (_, orphaned_amount)) in group {
                            ORPHANED_REWARDS.update::<_, StdError>(
                                storage,
                                &asset_info_key(&reward),
                                |amount| {
                                    Ok(amount.unwrap_or_default()
                                        + Uint128::try_from(orphaned_amount.to_uint_floor())?)
                                },
                            )?;
                        }

                        Ok(())
                    }
                })?;
        }

        POOLS.save(storage, lp_token, &self)
    }

    pub fn into_response(self) -> PoolInfoResponse {
        PoolInfoResponse {
            total_lp: self.total_lp,
            rewards: self.rewards,
            last_update_ts: self.last_update_ts,
        }
    }
}

/// List all stakers of a specific pool.
pub fn list_pool_stakers(
    storage: &dyn Storage,
    lp_token: &AssetInfo,
    start_after: Option<Addr>,
    limit: Option<u8>,
) -> StdResult<Vec<(Addr, Uint128)>> {
    let start = start_after.as_ref().map(Bound::exclusive);
    let limit = limit.unwrap_or(MAX_PAGE_LIMIT).max(MAX_PAGE_LIMIT);
    USER_INFO
        .prefix(lp_token)
        .range(storage, start, None, Order::Ascending)
        .take(limit as usize)
        .map(|item| item.map(|(user, user_info)| (user, user_info.amount)))
        .collect()
}

/// This structure is for internal use only.
/// Used to add/subtract LP tokens from user position and pool.
pub enum Op<T> {
    Add(T),
    Sub(T),
    Noop,
}

#[cw_serde]
/// This structure stores user position in a specific pool.
pub struct UserInfo {
    /// Amount of LP tokens staked
    pub amount: Uint128,
    /// Last rewards indexes per reward token
    pub last_rewards_index: Vec<(RewardType, Decimal256)>,
    /// The last time user claimed rewards
    pub last_claim_time: u64,
}

impl UserInfo {
    /// Create empty user position with last claim time set to current block time.
    pub fn new(env: &Env) -> Self {
        Self {
            amount: Uint128::zero(),
            last_rewards_index: vec![],
            last_claim_time: env.block.time.seconds(),
        }
    }

    /// Loads user position from state. If position doesn't exist returns an error.
    /// Can be used in context where position must exist.
    pub fn load_position(
        storage: &dyn Storage,
        user: &Addr,
        lp_token: &AssetInfo,
    ) -> Result<Self, ContractError> {
        Self::may_load_position(storage, user, lp_token)?.ok_or_else(|| {
            ContractError::PositionDoesntExist {
                user: user.to_string(),
                lp_token: lp_token.to_string(),
            }
        })
    }

    /// Tries to load user position from state. If position doesn't exist returns None.
    /// Can be used in context where position may or may not exist. For example, in deposit context.
    pub fn may_load_position(
        storage: &dyn Storage,
        user: &Addr,
        lp_token: &AssetInfo,
    ) -> StdResult<Option<Self>> {
        USER_INFO.may_load(storage, (lp_token, user))
    }

    /// Reset user index for all finished rewards.
    /// This function is called after processing finished schedules and before processing active
    /// schedules for a specific user.
    /// The idea is as follows:
    /// - get all finished rewards from FINISHED_REWARDS_INDEXES which finished after last time when user claimed rewards
    /// - merge them with rewards_to_remove
    /// - iterate over all finished rewards and set user index to 0.
    pub fn reset_user_index(
        &mut self,
        storage: &dyn Storage,
        lp_token: &AssetInfo,
        pool_info: &PoolInfo,
    ) -> StdResult<()> {
        let mut finished: HashSet<_> = FINISHED_REWARD_INDEXES
            .prefix(lp_token)
            .range(
                storage,
                Some(Bound::exclusive(self.last_claim_time)),
                None,
                Order::Ascending,
            )
            .map(|res| res.map(|(_, indexes)| indexes))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .map(|(reward_asset, _)| reward_asset)
            .collect();

        finished.extend(
            pool_info
                .rewards_to_remove
                .keys()
                .map(|reward| reward.asset_info().clone()),
        );

        for (reward, index) in self.last_rewards_index.iter_mut() {
            if reward.is_external() && finished.contains(reward.asset_info()) {
                *index = Decimal256::zero();
            }
        }

        Ok(())
    }

    /// This function calculates all outstanding rewards from finished schedules for a specific user position.
    /// The idea is as follows:
    /// - get all finished rewards from FINISHED_REWARDS_INDEXES which were deregistered after last claim time
    /// - merge them with rewards_to_remove
    /// - iterate over all user indexes and find differences. If user doesn't have index for deregistered reward then
    /// they never claimed it and their index defaults to 0.
    pub fn claim_finished_rewards(
        &self,
        storage: &dyn Storage,
        lp_token: &AssetInfo,
        pool_info: &PoolInfo,
    ) -> StdResult<Vec<Asset>> {
        let finished_iter = FINISHED_REWARD_INDEXES
            .prefix(lp_token)
            .range(
                storage,
                Some(Bound::exclusive(self.last_claim_time)),
                None,
                Order::Ascending,
            )
            .map(|res| res.map(|(_, indexes)| indexes))
            .collect::<StdResult<Vec<_>>>()?
            .into_iter()
            .flatten();

        let to_remove_iter = pool_info
            .rewards_to_remove
            .iter()
            .map(|(reward, (index, _))| (reward.asset_info().clone(), *index));

        let lp_tokens_amount = Uint256::from(self.amount);

        finished_iter
            .chain(to_remove_iter)
            .into_group_map_by(|(reward_info, _)| reward_info.clone())
            .into_values()
            .flat_map(|indexes_group| {
                indexes_group
                    .into_iter()
                    .enumerate()
                    .map(|(i, (reward_info, finished_index))| {
                        // User could have claimed this reward from the first schedule before it was finished
                        let amount = if i == 0 {
                            let user_reward_index = self
                                .last_rewards_index
                                .iter()
                                .filter(|(reward_type, _)| reward_type.is_external())
                                .find_map(|(reward_type, index)| {
                                    if reward_type.asset_info() == &reward_info {
                                        Some(*index)
                                    } else {
                                        None
                                    }
                                })
                                .unwrap_or_default();

                            (finished_index - user_reward_index) * lp_tokens_amount
                        } else {
                            // Subsequent finished schedules consider user never claimed rewards
                            // thus their index was 0
                            finished_index * lp_tokens_amount
                        };

                        Ok(reward_info.with_balance(Uint128::try_from(amount)?))
                    })
            })
            .collect()
    }

    /// Add/remove LP tokens from user position and pool info.
    /// Sync reward indexes and set last claim time.
    pub fn update_and_sync_position(&mut self, operation: Op<Uint128>, pool_info: &mut PoolInfo) {
        match operation {
            Op::Add(amount) => {
                self.amount += amount;
                pool_info.total_lp += amount;
            }
            Op::Sub(amount) => {
                self.amount -= amount;
                pool_info.total_lp -= amount;
            }
            Op::Noop => {}
        }

        self.last_rewards_index = pool_info
            .rewards
            .iter()
            .map(|reward_info| (reward_info.reward.clone(), reward_info.index))
            .collect();
        self.last_claim_time = pool_info.last_update_ts;
    }

    /// Save user position to state.
    /// This function consumes self just to make sure it becomes unusable after calling save().
    pub fn save(
        self,
        storage: &mut dyn Storage,
        user: &Addr,
        lp_token: &AssetInfo,
    ) -> StdResult<()> {
        USER_INFO.save(storage, (lp_token, user), &self)
    }

    /// Remove user position from state.
    pub fn remove(self, storage: &mut dyn Storage, user: &Addr, lp_token: &AssetInfo) {
        USER_INFO.remove(storage, (lp_token, user))
    }
}
