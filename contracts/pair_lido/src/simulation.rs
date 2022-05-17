// SPDX-License-Identifier: GPL-3.0-only
// Copyright Lido

use crate::math::{decimal_division, decimal_division_in_256, decimal_multiplication_in_256};
use crate::queries::{
    query_current_batch, query_hub_params, query_hub_state, query_total_tokens_issued,
};
use crate::state::Config;
use cosmwasm_std::{Decimal, Deps, StdResult, Uint128};
use std::ops::Mul;

/// ## Description
/// Returns how much bluna user will get for provided stluna amount
/// ## Params
/// * **deps** is the object of type [`Deps`],
///
/// * **config** is the object of type [`Config`],
///
/// * **stluna_amount** is the object of type [`Uint128`]
pub fn convert_stluna_to_bluna(
    deps: Deps,
    config: Config,
    stluna_amount: Uint128,
) -> StdResult<Uint128> {
    let state = query_hub_state(deps, config.hub_addr.clone())?;
    let params = query_hub_params(deps, config.hub_addr.clone())?;
    let current_batch = query_current_batch(deps, config.hub_addr)?;
    let total_bluna_supply = query_total_tokens_issued(deps, config.bluna_addr)?;

    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let denom_equiv = state.stluna_exchange_rate.mul(stluna_amount);

    let bluna_to_mint = decimal_division(denom_equiv, state.bluna_exchange_rate);
    let requested_bluna_with_fee = current_batch.requested_bluna_with_fee;

    let mut bluna_mint_amount_with_fee = bluna_to_mint;
    if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_to_mint * recovery_fee;
        let required_peg_fee = (total_bluna_supply + bluna_to_mint + requested_bluna_with_fee)
            - (state.total_bond_bluna_amount + denom_equiv);
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_mint_amount_with_fee = bluna_to_mint.checked_sub(peg_fee)?;
    }

    Ok(bluna_mint_amount_with_fee)
}

/// ## Description
/// Returns how much stluna user have to provide to get **bluna_amount**
/// ## Params
/// * **deps** is the object of type [`Deps`],
///
/// * **config** is the object of type [`Config`],
///
/// * **stluna_amount** is the object of type [`Uint128`]
pub fn get_required_stluna(
    deps: Deps,
    config: Config,
    asked_bluna_amount: Uint128,
) -> StdResult<Uint128> {
    let state = query_hub_state(deps, config.hub_addr.clone())?;
    let params = query_hub_params(deps, config.hub_addr.clone())?;
    let current_batch = query_current_batch(deps, config.hub_addr)?;
    let total_bluna_supply = query_total_tokens_issued(deps, config.bluna_addr)?;

    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let requested_bluna_with_fee = current_batch.requested_bluna_with_fee;

    // just a reversed calculations from the function above
    let denom_equiv: Uint128 = if state.bluna_exchange_rate < threshold {
        let denom_equiv_with_applied_required_fee = asked_bluna_amount
            + (total_bluna_supply + requested_bluna_with_fee)
            - (state.total_bond_bluna_amount);

        let denom_equiv_with_applied_max_peg_fee =
            decimal_division_in_256(state.bluna_exchange_rate, Decimal::one() - recovery_fee)
                * asked_bluna_amount;

        Uint128::min(
            denom_equiv_with_applied_max_peg_fee,
            denom_equiv_with_applied_required_fee,
        )
    } else {
        state.bluna_exchange_rate * asked_bluna_amount
    };

    let stluna_amount = decimal_division(denom_equiv, state.stluna_exchange_rate);

    Ok(stluna_amount)
}

/// ## Description
/// Returns how much stluna user will get for provided bluna amount
/// ## Params
/// * **deps** is the object of type [`Deps`],
///
/// * **config** is the object of type [`Config`],
///
/// * **bluna_amount** is the object of type [`Uint128`]
pub fn convert_bluna_to_stluna(
    deps: Deps,
    config: Config,
    bluna_amount: Uint128,
) -> StdResult<Uint128> {
    let state = query_hub_state(deps, config.hub_addr.clone())?;
    let params = query_hub_params(deps, config.hub_addr.clone())?;
    let current_batch = query_current_batch(deps, config.hub_addr.clone())?;
    let total_bluna_supply = query_total_tokens_issued(deps, config.bluna_addr)?;

    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    // Apply peg recovery fee
    let bluna_amount_with_fee: Uint128 = if state.bluna_exchange_rate < threshold {
        let max_peg_fee = bluna_amount * recovery_fee;
        let required_peg_fee = (total_bluna_supply + current_batch.requested_bluna_with_fee)
            .checked_sub(state.total_bond_bluna_amount)?;
        let peg_fee = Uint128::min(max_peg_fee, required_peg_fee);
        bluna_amount.checked_sub(peg_fee)?
    } else {
        bluna_amount
    };

    let denom_equiv = state.bluna_exchange_rate.mul(bluna_amount_with_fee);

    let stluna_to_mint = decimal_division(denom_equiv, state.stluna_exchange_rate);

    Ok(stluna_to_mint)
}

/// ## Description
/// Returns how much bluna user have to provide to get **stluna_amount**
/// ## Params
/// * **deps** is the object of type [`Deps`],
///
/// * **config** is the object of type [`Config`],
///
/// * **stluna_amount** is the object of type [`Uint128`]
pub fn get_required_bluna(
    deps: Deps,
    config: Config,
    asked_stluna_amount: Uint128,
) -> StdResult<Uint128> {
    let state = query_hub_state(deps, config.hub_addr.clone())?;
    let params = query_hub_params(deps, config.hub_addr.clone())?;
    let current_batch = query_current_batch(deps, config.hub_addr)?;
    let total_bluna_supply = query_total_tokens_issued(deps, config.bluna_addr)?;

    let threshold = params.er_threshold;
    let recovery_fee = params.peg_recovery_fee;

    let offer_bluna =
        decimal_division_in_256(state.stluna_exchange_rate, state.bluna_exchange_rate)
            .mul(asked_stluna_amount);

    let mut offer_bluna_with_fee = offer_bluna;

    // just a reversed calculations from the function above
    if state.bluna_exchange_rate < threshold {
        let offer_bluna_with_max_peg_fee = decimal_division_in_256(
            state.stluna_exchange_rate,
            decimal_multiplication_in_256(state.bluna_exchange_rate, Decimal::one() - recovery_fee),
        )
        .mul(asked_stluna_amount);

        let required_peg_fee = (total_bluna_supply + current_batch.requested_bluna_with_fee)
            .checked_sub(state.total_bond_bluna_amount)?;

        let bluna_amount = decimal_multiplication_in_256(
            decimal_division_in_256(state.stluna_exchange_rate, state.bluna_exchange_rate),
            state.stluna_exchange_rate,
        )
        .mul(asked_stluna_amount);

        let offer_bluna_with_required_peg_fee = bluna_amount + required_peg_fee;

        offer_bluna_with_fee = Uint128::min(
            offer_bluna_with_max_peg_fee,
            offer_bluna_with_required_peg_fee,
        );
    }

    Ok(offer_bluna_with_fee)
}
