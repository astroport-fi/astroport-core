# Astroport Router

The Router contract performs multi-hop swaps across Astroport pools. Each hop is either a pool the factory knows for an
asset pair (`astro_swap`) or a pool given by address (`pool_swap`), so pools that aren't registered in the factory, such as
a transmuter instantiated directly, can be part of a route.

Router 2.x is deployed as its own instance. Router 1.x instances keep their old API and can't be migrated to 2.x.

---

### Price protection

Pools' own spread checks are disabled on every hop, including single-hop swaps. The only price guarantee is
`minimum_receive`: after the last hop, the router checks that the receiver got at least that amount of the ask asset and
reverts the whole transaction otherwise. It is required and must be greater than zero.

The bound is only as good as the number you send. Set `minimum_receive` from a price you trust (for example your own quote
minus slippage), not from a hop's own simulation. `minimum_receive: "1"` is accepted but protects almost nothing.

Nested routes are refused: a route can't be started while another one is in progress (`RouteInProgress`).

### Choosing pools

`pool_swap` accepts any address. The router checks that the pool's `pair {}` query reports that address and both assets, which
catches mistyped routes, but it can't prove a contract is a genuine Astroport pool. Only build `pool_swap` routes from pools
you trust.

Each hop offers the router's whole balance of the offer asset. The router holds nothing between transactions, so this is only
the route's own funds plus anything sent to the router by mistake.

## InstantiateMsg

Initializes the contract with the Astroport factory contract address, used to resolve `astro_swap` hops.

```json
{
  "astroport_factory": "terra..."
}
```

## ExecuteMsg

### `receive`

CW20 receive msg. The embedded message is `execute_swap_operations` below, used when the first offer asset is a CW20 token.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

### `execute_swap_operations`

Swaps the sent amount along the route. Hops execute one by one and the last one sends the ask asset to `to` (the sender if
omitted). Anyone can call it.

The response data contains the total `return_amount` (see `SwapResponseData`). It is only set when the route starts with a
native asset; a CW20 `send` resets the response data.

#### Example

Swap LUNA => USDC.n through a factory pool, then USDC.n => USDC.inj through a standalone transmuter:

```json
{
  "execute_swap_operations": {
    "operations": [
      {
        "astro_swap": {
          "offer_asset_info": { "native_token": { "denom": "uluna" } },
          "ask_asset_info": { "native_token": { "denom": "ibc/USDC_N..." } }
        }
      },
      {
        "pool_swap": {
          "pool_addr": "terra1...transmuter",
          "offer_asset_info": { "native_token": { "denom": "ibc/USDC_N..." } },
          "ask_asset_info": { "native_token": { "denom": "ibc/USDC_INJ..." } }
        }
      }
    ],
    "minimum_receive": "123",
    "to": "terra..."
  }
}
```

### `execute_swap_operation`

Executes a single hop. Only the router itself can call it.

## QueryMsg

### `config`

Returns the general configuration for the router contract.

```json
{
  "config": {}
}
```

### `simulate_swap_operations`

Simulates a route for an offer amount and returns the amount received. Takes the same operations as
`execute_swap_operations`.

```json
{
  "simulate_swap_operations": {
    "offer_amount": "123",
    "operations": [
      {
        "astro_swap": {
          "offer_asset_info": { "native_token": { "denom": "uluna" } },
          "ask_asset_info": { "native_token": { "denom": "ibc/USDC_N..." } }
        }
      },
      {
        "pool_swap": {
          "pool_addr": "terra1...transmuter",
          "offer_asset_info": { "native_token": { "denom": "ibc/USDC_N..." } },
          "ask_asset_info": { "native_token": { "denom": "ibc/USDC_INJ..." } }
        }
      }
    ]
  }
}
```

### `reverse_simulate_swap_operations`

Simulates a route backwards: returns the offer amount needed to receive `ask_amount`.

```json
{
  "reverse_simulate_swap_operations": {
    "ask_amount": "123",
    "operations": [
      {
        "astro_swap": {
          "offer_asset_info": { "native_token": { "denom": "uluna" } },
          "ask_asset_info": { "native_token": { "denom": "ibc/USDC_N..." } }
        }
      }
    ]
  }
}
```
