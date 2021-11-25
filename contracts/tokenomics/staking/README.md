# Astroport Staking

The staking contract provide staking ASTRO tokens.

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

```json
{
  "token_code_id": 123,
  "deposit_token_addr": "terra..."
}
```

## ExecuteMsg

### `receive`

CW20 receive msg.

```json
{
  "receive": {
    "sender": "terra...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns deposit and share token addresses.

```json
{
  "config": {}
}
```

## Cw20HookMsg

### `Enter`

Deposits token to get share token amount.
Must be sent from token contract.

```json
{
  "send": {
    "contract": HumanAddr,
    "amount": Uint128,
    "msg": Binary{
      "Enter": {}
    })
  }
}
```

### `Leave`

Unstakes share token to move back deposit token amount. Burns share.
Must be sent from token contract.
```json
{
  "send": {
    "contract": HumanAddr,
    "amount": Uint128,
    "msg": {
      "Leave": {}
    }
  }
}
```
