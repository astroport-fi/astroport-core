# Astroport Base Stableswap Pair

The stableswap pool uses the 4A(Rx+Ry) + D formula, resulting in a constant price ∆x / ∆y = 1. More details around how the pool functions can be found [here](https://docs.astroport.fi/astroport/astroport/astro-pools/stableswap-invariant-pools). Its interface can be found [here](../../packages/pair_stable/README.md)

---

### Liquidity Providers

A user can provide liquidity to a constant product pool by calling `provide_liquidity`. Users can also withdraw liquidity by calling `withdraw_liquidity`.

Whenever liquidity is deposited into a pool, special tokens known as "liquidity tokens" are minted to the provider’s address, in proportion to how much liquidity they contributed to the pool. These tokens are a representation of a liquidity provider’s contribution to a pool. Whenever a trade occurs, the `lp_commission` is distributed pro-rata to all LPs in the pool at the moment of the trade. To receive the underlying liquidity back plus accrued LP fees, LPs must burn their liquidity tokens.

When providing liquidity from a smart contract, the most important thing to keep in mind is that the amount of tokens deposited into a pool and the amount of tokens withdrawn later from the pool will most likely not be the same (even if stableswap encourages a constant 1:1 ratio between all assets in the pool).

As an example, let's say the global ratio between two tokens x:y is 1.01:1 (1 x = 0.99 y), but the current ratio between the tokens in an Astroport pair is 1:1.01 (1 x = 1.01 y). Let's also say that someone may decide to LP in the x:y Astroport pool at the current 1:1.01 ratio. As the Astroport pool gets arbitraged to the global ratio, the amount of x & y tokens that the LP can withdraw changes because the total amounts of x & y tokens in the pool also change.

> Note that before executing the `provide_liqudity` operation, a user must allow the pool contract to take tokens from their wallet

### Slippage Tolerance for Providing Liquidity

If a user specifies a slippage tolerance when they provide liquidity in a constant product pool, the pool contract makes sure that the transaction goes through only if the pool price does not change more than tolerance.

As an example, let's say someone LPs in a pool and specifies a 1% slippage tolerance. The user LPs 200 UST and 200 `ASSET`. With a 1% slippage tolerance, `amountUSTMin` (the minimum amount of UST to LP) should be set to 198 UST, and `amountASSETMin` (the minimum amount of `ASSET` to LP) should be set to .99 `ASSET`. This means that, in a worst case scenario, liquidity will be added at a pool rate of 198 `ASSET`/1 UST or 202.02 UST/1 `ASSET` (200 UST + .99 `ASSET`). If the contract cannot add liquidity within these bounds (because the pool ratio changed more than the tolerance), the transaction will revert.

## Traders

### Slippage Tolerance for Swaps

Astroport has two options to protect traders against slippage during swaps:

1. Providing `max_spread`
The spread is calculated as the difference between the ask amount (using the constant pool price) before and after the swap operation. Once `max_spread` is set, it will be compared against the actual swap spread. In case the swap spread exceeds the provided max limit, the swap will fail.

Note that the spread is calculated before commission deduction in order to properly represent the pool's ratio change.

2. Providing `max_spread` + `belief_price`
If `belief_price` is provided in combination with `max_spread`, the pool will check the difference between the return amount (using `belief_price`) and the real pool price.

Please note that Astroport has the default value for the spread set to 0.5% and the max allowed spread set to 50%.
