# Astroport Pair Exit-Only

Code an Astroport pair migrates to when it must only allow balanced withdrawals. It was written for the six Neutron
pools drained on 2026-09-22: their balances are zero while their LP supply is intact, so a dust deposit would mint
nearly all LP tokens and take the refill. Migrating them here before the refill closes that, keeps swaps from
trading the refill at stale prices, and holds withdrawals until the refill is complete.

- **Withdrawals only.** An LP gets its share of every asset the pool holds, rounded down, and its LP tokens are burned.
  Native LP tokens are attached to `withdraw_liquidity {}`; CW20 LP tokens are sent with `{"withdraw_liquidity":{}}`.
  `min_assets_to_receive` is enforced. A withdrawal naming `assets` (one-sided) is refused, and so is one that would
  receive nothing.
- **Withdrawals open when the refill is in.** Each pool has a gate, set by the migration:
  - `max_total_share`, the LP supply when the pool was drained. Withdrawals stay closed while the supply is above it,
    since LP minted after the drain would take a share of the refill.
  - `min_balances`, what the refill brings per asset. Withdrawals open once every listed asset backs each LP token at
    least that much (`balance / supply >= min / max_total_share`). Before that, a withdrawal against a partial refill
    or a donation would burn the LP tokens and give up their share of what's still to come. Withdrawals keep each
    asset's balance per LP token where it was (rounding leaves a little more), so once open a pool stays open.
- **Everything else is refused:** deposits, swaps, config and ownership changes, and other CW20 hooks.
- **Queries:** `pair`, `pool`, `share` and `simulate_withdraw` (which fails, saying why, while withdrawals are
  closed). The others return an exit-only error.
- **Only the gate is written** (key `exit_only_gate`). The pair's other storage, including its cw2 version, stays as it
  was, so a build of its own contract that accepts that version can take it back. Today only the duality pools have
  one (`pair_concentrated_duality` 4.3.3 takes 4.3.1 with `{"migrate":{}}`). `pair_concentrated` doesn't accept
  4.0.2, nothing accepts the 1.2.x pairs, and the transmuter only accepts 1.1.x. Reopening also needs the pool's
  price state reconciled with the refill: it is a separate, reviewed migration.

## Migration

```json
{
  "max_total_share": "1348944812977",
  "min_balances": [
    { "info": { "native_token": { "denom": "ibc/B559…" } }, "amount": "1300448683196" },
    { "info": { "native_token": { "denom": "factory/…/udatom" } }, "amount": "849315856190" }
  ]
}
```

Supported: `astroport-pair-concentrated` (1.x and 4.x), `astroport-pair-concentrated-duality` and
`astroport-pair-transmuter`, all of which keep `pair_info` in `config` and hold the pool as plain balances. Refused:

- other contracts;
- a config naming another address;
- a pool already exit-only through `pair_concentrated` 4.3 (it withholds an asset that this code would pay out);
- a duality pool with its orderbook enabled, orders still placed, a delayed trade, or no orderbook state, since part of
  its liquidity could be on the orderbook;
- a gate with a zero `max_total_share`, or `min_balances` naming another asset or one asset twice.

The migration freezes the pool even if its LP supply is already above `max_total_share`; withdrawals then stay closed.
Migrating here again replaces the gate.

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
- Anything sent to a pool after its withdrawals open goes to the LP tokens still outstanding: a later recovery doesn't
  reach LPs who have already withdrawn.
- If a refill comes in below `min_balances`, the pool stays closed until it's migrated again with a lower gate.
- Tokens that aren't one of the pool's assets, and LP tokens sent to the pool directly, stay in the pool.
- A pool's minimum liquidity (LP held by the pool itself) and rounding dust stay in the pool.
- Pools tracking asset balances (`track_asset_balances`) don't record withdrawals made here; their own code records
  balances again from the next action after reopening.
