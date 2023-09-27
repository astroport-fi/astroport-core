use cosmwasm_std::{Addr, Deps, StdResult, Storage, Uint128};
use std::ops::Deref;

use crate::ContractError;
use astroport::common::OwnershipProposal;
use astroport::shared_multisig::{Config, MultisigRole, DEFAULT_WEIGHT};
use cw3::{Proposal, Vote, VoteInfo};
use cw_storage_plus::{Item, Map};

pub const CONFIG: Item<Config> = Item::new("config");
pub const PROPOSAL_COUNT: Item<u64> = Item::new("proposal_count");

pub const BALLOTS: Map<(u64, &MultisigRole), Vote> = Map::new("votes");
pub const PROPOSALS: Map<u64, Proposal> = Map::new("proposals");

/// Contains a proposal to change contract Manager One.
pub const MANAGER1_PROPOSAL: Item<OwnershipProposal> = Item::new("manager1_proposal");

/// Contains a proposal to change contract Manager Two.
pub const MANAGER2_PROPOSAL: Item<OwnershipProposal> = Item::new("manager2_proposal");

/// Key is reward token + manager
/// Values is amount of distributed rewards
pub const DISTRIBUTED_REWARDS: Map<(String, &MultisigRole), Uint128> =
    Map::new("distributed_rewards");

// settings for pagination
pub const MAX_LIMIT: u32 = 30;
pub const DEFAULT_LIMIT: u32 = 10;

pub fn next_id(store: &mut dyn Storage) -> StdResult<u64> {
    let id: u64 = PROPOSAL_COUNT.may_load(store)?.unwrap_or_default() + 1;
    PROPOSAL_COUNT.save(store, &id)?;
    Ok(id)
}

pub fn load_vote(deps: Deps, key: (u64, &MultisigRole)) -> StdResult<Option<VoteInfo>> {
    if let Some(vote) = BALLOTS.may_load(deps.storage, key)? {
        return Ok(Some(VoteInfo {
            proposal_id: key.0,
            voter: key.1.to_string(),
            vote,
            weight: DEFAULT_WEIGHT,
        }));
    }

    Ok(None)
}

pub fn released_rewards(
    store: &dyn Storage,
    denom: &String,
    role: &MultisigRole,
) -> Result<Uint128, ContractError> {
    Ok(
        if let Some(amount) = DISTRIBUTED_REWARDS.may_load(store, (denom.to_string(), role))? {
            amount
        } else {
            Uint128::zero()
        },
    )
}

pub(crate) fn update_distributed_rewards(
    store: &mut dyn Storage,
    denom: &String,
    amount: Uint128,
    total_amount: Uint128,
    sender: &Addr,
    cfg: &Config,
) -> Result<(), ContractError> {
    let released_manager1 = released_rewards(store.deref(), denom, &MultisigRole::Manager1)?;
    let released_manager2 = released_rewards(store.deref(), denom, &MultisigRole::Manager2)?;

    let sender_released = if sender == cfg.manager1 {
        released_manager1
    } else {
        released_manager2
    };

    let allowed_amount = (total_amount + released_manager1 + released_manager2)
        .checked_div(Uint128::new(2))?
        .checked_sub(sender_released)?;

    if amount > allowed_amount {
        return Err(ContractError::BalanceToSmall(
            sender.to_string(),
            allowed_amount.to_string(),
        ));
    }

    if sender == cfg.manager1 {
        DISTRIBUTED_REWARDS.save(
            store,
            (denom.to_string(), &MultisigRole::Manager1),
            &(sender_released + amount),
        )?;
    } else {
        DISTRIBUTED_REWARDS.save(
            store,
            (denom.to_string(), &MultisigRole::Manager2),
            &(sender_released + amount),
        )?;
    }

    Ok(())
}
