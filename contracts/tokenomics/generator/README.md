# Astroport Generator

The generator contract generates token rewards (ASTRO) based on locked LP token amount by liquidity pool providers. Also supports dual rewards feature and proxy staking via 3-d party contracts integration. Allowed reward proxies are managed via a whitelist.

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with required contract addresses for depositing and reward distribution.

```json
{
  "astro_token": "terra...",
  "tokens_per_block": "123",
  "start_block": "123",
  "allowed_reward_proxies": [
    "terra..."
  ],
  "vesting_contract": "terra..."
}
```

## ExecuteMsg

### `add`

Add support of a new LP with optional reward_proxy address.

```json
{
  "add": {
    "lp_token": "terra...",
    "alloc_point": "123",
    "with_update": "true",
    "reward_proxy": "terra..."
  }
}
```

### `set`

Updates pair average and cumulative prices.

```json
{
  "set": {
    "lp_token": "terra...",
    "alloc_point": "123",
    "with_update": "true",
    "reward_proxy": "terra..."
  }
}
```

### `add`

Updates pair average and cumulative prices.

```json
{
  "update": {}
}
```

### `add`

Updates pair average and cumulative prices.

```json
{
  "update": {}
}
```

### `add`

Updates pair average and cumulative prices.

```json
{
  "update": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `consult`

Multiplies a given amount and last average price in common.

```json
{
  "consult": {
    "token": {
      "info": {
        "token": {
          "contract_addr": "terra..."
        }
      }
    },
    "amount": "1000000"
  }
}
```
