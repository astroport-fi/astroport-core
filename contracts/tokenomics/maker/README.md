# Astroport Maker

The Maker contract collects part of Astroport's pool fees (the factory's `maker_fee`). It swaps each fee token into a
single **base asset** through the router, then splits the base asset across a list of **legs**. Each leg takes a share,
may swap it along its own route, and then sends it to an address or deposits it into a contract.

Maker 2.x replaces 1.x's fixed splits (governance percent, second receiver, dev fund) and its bridges with legs and
routes. A 1.7.0 Maker is migrated in place; see [Migration](#migration).

---

## Concepts

### Base asset and routes

Every fee token needs a route: a list of router 2.x `SwapOperation`s (`astro_swap` or `pool_swap`) that starts at the
fee token and ends at the base asset. A route has between 1 and 5 hops, and each hop must offer what the previous hop
asked for. The base asset itself has no route; it's split across the legs as it is.

### Legs

```json
{
  "share": "0.5",
  "route": [],
  "action": { "send": { "recipient": "terra..." } }
}
```

- `share`: the part of the base asset balance this leg gets. There are 1 to 10 legs, every share is greater than zero and
  the shares add up to exactly 1. The last leg also gets what rounding leaves over.
- `route`: optional. If set, it starts at the base asset and the leg's share is swapped along it. It can end in any
  asset.
- `action`: what happens to the leg's output:
  - `send { recipient }`: transfers it to `recipient`. If the leg has a route, the router pays `recipient` directly.
  - `deposit { contract, msg }`: executes `contract` with `msg` and the output attached, as native funds or as a CW20
    `send` with `msg` as its hook. If the leg has a route, the Maker swaps first and deposits exactly what the swap
    produced.

For example, these legs burn half the fees and pay the other half to a DAO, with USDC as the base asset
(`eyJidXJuIjp7fX0=` is `{"burn":{}}`):

```json
[
  {
    "share": "0.5",
    "route": [
      {
        "astro_swap": {
          "offer_asset_info": { "native_token": { "denom": "usdc" } },
          "ask_asset_info": { "native_token": { "denom": "astro" } }
        }
      }
    ],
    "action": { "deposit": { "contract": "burner...", "msg": "eyJidXJuIjp7fX0=" } }
  },
  {
    "share": "0.5",
    "action": { "send": { "recipient": "dao..." } }
  }
]
```

### Price protection

Every swap goes through the router with a `minimum_receive`. The Maker prices the swap by simulating a probe of a
thousandth of the amount along the same route. It scales that rate up to the full amount and takes off `max_spread`:

```
minimum_receive = amount * (probe_out / probe) * (1 - max_spread)
```

A swap whose price impact is more than `max_spread` fails its minimum and reverts the whole collect. To collect a large
balance, use `limit` and collect it in parts.

Some fee amounts are too small to price, because their minimum comes out as zero. The Maker skips them, leaves them for a
later collect and reports them with a `skipped_dust` attribute. A leg share that is too small to swap is also skipped and
stays in the Maker for the next split.

`max_spread` is greater than zero and at most 50%. The default is 5%.

---

## InstantiateMsg

```json
{
  "owner": "terra...",
  "router": "terra...",
  "base_asset": { "native_token": { "denom": "usdc" } },
  "max_spread": "0.05",
  "collect_cooldown": 300,
  "legs": [],
  "routes": [
    [
      { "native_token": { "denom": "uluna" } },
      [
        {
          "astro_swap": {
            "offer_asset_info": { "native_token": { "denom": "uluna" } },
            "ask_asset_info": { "native_token": { "denom": "usdc" } }
          }
        }
      ]
    ]
  ]
}
```

`router` must be a router 2.x instance. `collect_cooldown` is optional; if set, it's between 30 and 600 seconds.
`routes` is optional.

## ExecuteMsg

### `collect`

Permissionless. Swaps the listed fee tokens into the base asset, then splits the Maker's whole base asset balance across
the legs:

- `limit` caps how much of a token is swapped. Without it, the whole balance is swapped.
- An empty list only splits the base asset the Maker already holds.
- The base asset can be listed, but it isn't swapped.
- If a cooldown is set, `collect` fails until the cooldown has passed since the last collect.

```json
{
  "collect": {
    "assets": [
      { "info": { "native_token": { "denom": "uluna" } }, "limit": "1000000" },
      { "info": { "token": { "contract_addr": "terra..." } } }
    ]
  }
}
```

### `update_config`

Owner only. Every field is optional.

- Legs are replaced as a whole and validated against the base asset, including a new one set in the same message.
- If the base asset changes, routes that end in the old one are refused at collect until they're replaced.

```json
{
  "update_config": {
    "router": "terra...",
    "base_asset": { "native_token": { "denom": "astro" } },
    "max_spread": "0.05",
    "collect_cooldown": 300,
    "legs": []
  }
}
```

### `update_routes`

Owner only. Adds or replaces routes, and removes the routes for the listed assets.

```json
{
  "update_routes": {
    "add": [
      [
        { "native_token": { "denom": "uluna" } },
        [
          {
            "astro_swap": {
              "offer_asset_info": { "native_token": { "denom": "uluna" } },
              "ask_asset_info": { "native_token": { "denom": "usdc" } }
            }
          }
        ]
      ]
    ],
    "remove": [{ "native_token": { "denom": "uatom" } }]
  }
}
```

### `seize`

Permissionless. Sends the listed assets to the seize receiver. Only assets in the seize config can be seized.

```json
{
  "seize": {
    "assets": [{ "info": { "native_token": { "denom": "uusdc" } }, "limit": "1000000" }]
  }
}
```

### `update_seize_config`

Owner only.

```json
{
  "update_seize_config": {
    "receiver": "terra...",
    "seizable_assets": [{ "native_token": { "denom": "uusdc" } }]
  }
}
```

### `propose_new_owner`

Creates a proposal to change contract ownership. The proposal expires after `expires_in` seconds.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing proposal to change contract ownership.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

The proposed owner claims ownership.

```json
{
  "claim_ownership": {}
}
```

### Internal messages

Only the Maker itself can call `distribute`, `snapshot_leg` and `deposit_leg`. `collect` uses them to split the base
asset and to deposit a swapping leg's output.

## QueryMsg

### `config`

```json
{ "config": {} }
```

### `routes`

Returns every fee token route.

```json
{ "routes": {} }
```

### `balances`

Returns the Maker's balances of the listed assets. Zero balances are left out.

```json
{ "balances": { "assets": [{ "native_token": { "denom": "uluna" } }] } }
```

### `query_seize_config`

```json
{ "query_seize_config": {} }
```

---

## Migration

Only a Maker at `astroport-maker` 1.7.0 can be migrated. The migration:

- keeps the owner, any pending ownership proposal, the seize config and the last collect time;
- keeps `max_spread` and `collect_cooldown` unless the message sets them;
- sets the router, base asset and legs from the message;
- removes the 1.x bridges and sets the routes from the message.

The old governance, second receiver and dev fund settings are dropped. Legs replace them.

```json
{
  "router": "terra...",
  "base_asset": { "native_token": { "denom": "usdc" } },
  "max_spread": null,
  "collect_cooldown": null,
  "legs": [],
  "routes": []
}
```

Fee tokens that aren't given a route stay in the Maker until the owner adds one with `update_routes`.
