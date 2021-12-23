# Astroport Maker

The maker contract collects pair assets per pool (following to factory's `maker_fee`) tries to swap it to ASTRO and sends to staking and governance (following to `governance_percent`).

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with required contract addresses and `governance_percent`.

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

Collects astro tokens from the given pairs

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

Updates general settings. All fields are optional.

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

Creates an offer for a new owner. The validity period of the offer is set in the `expires_in` variable.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing offer for the new owner.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used to claim(approve) new owner proposal, thus changing contract's owner.

```json
{
  "claim_ownership": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns information about the maker configs.

```json
{
  "config": {}
}
```

### `balances`

Returns the balance for each asset in the specified input parameters

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
