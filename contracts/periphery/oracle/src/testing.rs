use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::Uint128;
use std::ops::Mul;

#[test]
fn decimal_overflow() {
    let price_cumulative_current = Uint128::from(100u128);
    let price_cumulative_last = Uint128::from(192738282u128);
    let time_elapsed: u64 = 86400;
    let amount = Uint128::from(1000u128);
    let price_average = Decimal256::from_ratio(
        if price_cumulative_current > price_cumulative_last {
            Uint256::from(price_cumulative_current) - Uint256::from(price_cumulative_last)
        } else {
            Uint256::from(price_cumulative_current)
                + Uint256::from(Uint128::MAX - price_cumulative_last)
        },
        time_elapsed,
    );
    println!("{}", price_average.to_string());
    println!(
        "{}",
        Uint128::from(price_average.mul(Uint256::from(amount))).to_string()
    );
}
