# Astroport Maker

The Maker contract collects part of Astroport's pair fees (according to the factory's `maker_fee`). The accrued fees are swapped to ASTRO and then send to stakers and governance (according to the `governance_percent`).

---

## InstantiateMsg

Initializes the contract with required addresses and the `governance_percent`.

```json
{
  "owner": "terra...",
  "astro_token_contract": "terra...",
  "factory_contract": "terra...",
  "staking_contract": "terra...",
  "governance_contract": "terra...",
  "governance_percent": 20,
  "max_spread": 23.3
}
```

## ExecuteMsg

### `collect`

Swaps accrued fee tokens to ASTRO.

```json
{
  "collect": {
    "pair_addresses": [
      "terra...",
      "terra..."
    ]
  }
}
```

### `update_config`

Updates the contract's general settings. All fields are optional.

```json
{
  "update_config": {
    "factory_contract": "terra...",
    "staking_contract": "terra...",
    "governance_contract": {
      "set": "terra..."
    },
    "governance_percent": "20",
    "max_spread": 23.3
  }
}
```

### `propose_new_owner`

Creates a proposal to change contract ownership. The proposal validity period is set in the `expires_in` variable.

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

Used to claim contract ownership, thus changing the contract's owner.

```json
{
  "claim_ownership": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns information about the Maker's configuration.

```json
{
  "config": {}
}
```

### `balances`

Returns token balances for each specified asset held by the Maker.

```json
{
  "balances": {
    "assets": [
      {
        "token": {
          "contract_addr": "terra..."
        }
      },
      {
        "native_token": {
          "denom": "uusd"
        }
      }
    ]
  }
}
```
