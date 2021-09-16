use cosmwasm_std::Uint128;

/// If the addition results in integer overflow, safely warps the number back to zero
pub fn warp_add(a: Uint128, b: Uint128) -> Uint128 {
    // The difference between a and Uint128::MAX, i.e. biggest number that can be added to
    // a without overflowing
    let diff = Uint128::MAX - a;

    if b <= diff {
        a + b
    } else {
        b - diff
    }
}
