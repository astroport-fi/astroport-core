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

### `post_initialize`

Token contract must execute it after creating.

```json
{
  "post_initialize": {}
}
```

### `enter`

Deposits token to get share token amount.

```json
{
  "enter": {
    "amount": "123"
  }
}
```

### `leave`

Unstakes share token to move back deposit token amount. Burns share.

```json
{
  "leave": {
    "share": "123"
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
