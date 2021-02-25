use cosmwasm_std::{Api, Decimal, Extern, Querier, StdResult, Storage, Uint128};
use terra_cosmwasm::TerraQuerier;

static DECIMAL_FRACTION: Uint128 = Uint128(1_000_000_000_000_000_000u128);

pub fn compute_tax<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
    amount: Uint128,
    denom: String,
) -> StdResult<Uint128> {
    if denom == "uluna" {
        return Ok(Uint128::zero());
    }

    let terra_querier = TerraQuerier::new(&deps.querier);
    let tax_rate: Decimal = (terra_querier.query_tax_rate()?).rate;
    let tax_cap: Uint128 = (terra_querier.query_tax_cap(denom.to_string())?).cap;
    Ok(std::cmp::min(
        (amount
            - amount.multiply_ratio(
                DECIMAL_FRACTION,
                DECIMAL_FRACTION * tax_rate + DECIMAL_FRACTION,
            ))?,
        tax_cap,
    ))
}
