use cosmwasm_std::{StdResult, Uint128};

use crate::state::UserInfo;

/// This trait is meant to extend [`astroport::incentives::RewardInfo`].
pub trait RewardInfoExt {
    fn calculate_reward(&self, user_info: &UserInfo) -> StdResult<Uint128>;
}
