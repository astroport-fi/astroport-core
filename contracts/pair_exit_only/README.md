# Astroport Pair Exit-Only

Code an Astroport pair migrates to when it must only allow balanced withdrawals. It was written for the six Neutron
pools drained on 2026-09-22: their balances are zero while their LP supply is intact, so a dust deposit would mint
nearly all LP tokens and take the refill. Migrating them here before the refill closes that, and keeps swaps from
trading the refill at stale prices.

- **Withdrawals only.** An LP gets its share of every asset the pool holds, rounded down, and its LP tokens are burned.
  Native LP tokens are attached to `withdraw_liquidity {}`; CW20 LP tokens are sent with `{"withdraw_liquidity":{}}`.
  `min_assets_to_receive` is enforced. A withdrawal naming `assets` (one-sided) is refused, and so is one that would
  receive nothing, such as before the refill.
- **Everything else is refused:** deposits, swaps, config and ownership changes, and other CW20 hooks.
- **Queries:** `pair`, `pool`, `share` and `simulate_withdraw`. The others return an exit-only error.
- **No state is written.** The pair's storage, including its cw2 version, stays as it was, so migrating back to a
  build of its own contract that accepts that version reopens it. The current `pair_concentrated` and
  `pair_concentrated_duality` builds already accept 4.0.2 and 4.3.1; the 1.2.x pairs and the transmuter would need one.

## Migration

`{}`. Supported: `astroport-pair-concentrated` (1.x and 4.x), `astroport-pair-concentrated-duality` and
`astroport-pair-transmuter`, all of which keep `pair_info` in `config` and hold the pool as plain balances. Refused:

- other contracts;
- a config naming another address;
- a pool already exit-only through `pair_concentrated` 4.3 (it withholds an asset that this code would pay out);
- a duality pool with its orderbook enabled, since part of its liquidity would be on the orderbook.

The pools it's for (Neutron):

| Pool | Address | Contract | LP |
|---|---|---|---|
| dATOM/USDC | `neutron1nfns3ck2ykrs0fknckrzd9728cyf77devuzernhwcwrdxw7ssk2s3tjf8r` | pair-concentrated-duality 4.3.1 | native |
| NTRN/USDC | `neutron18c8qejysp4hgcfuxdpj4wf29mevzwllz5yh8uayjxamwtrs0n9fshq9vtv` | pair-concentrated-duality 4.3.1 | native |
| dATOM/ATOM | `neutron1yem82r0wf837lfkwvcu2zxlyds5qrzwkz8alvmg0apyrjthk64gqeq2e98` | pair-concentrated 4.0.2 | native |
| USDC/DYDX | `neutron1j4xpv03fw664mvntlhqnzp5hjqk2nfw00vrgx9qlq97rxc9fu3lqvmszl2` | pair-concentrated 1.2.13 | CW20 |
| wstETH/WETH | `neutron1yw0a7nxa8jdgzmdsme4gwxhj76n44z305qgwrzvlfavgna9epcys3k9m2f` | pair-concentrated 1.2.15 | CW20 |
| axlUSDC/USDC | `neutron1ns2tcunrlrk5yk62fpl74ycazanceyfmmq7dlj6sq8n0rnkuvk7szstkyx` | pair-transmuter 1.0.0 | CW20 |

## Caveats

- LP tokens staked in Incentives must be unstaked before withdrawing.
- A pool's minimum liquidity (LP held by the pool itself) and rounding dust stay in the pool.
- Pools tracking asset balances (`track_asset_balances`) don't record withdrawals made here; their own code records
  balances again from the next action after reopening.
