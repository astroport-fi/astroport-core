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

#### `Enter`

Deposits token to get share token amount.

Execute this message by the ASTRO token contract address from which you want to make a deposit.
```json
{
  "send": {
    "contract": <StakingContractAddress>,
    "amount": 999,
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In send.msg, you may decode this JSON string into base64 encoding.
```json
{
  "Enter": {}
}
```

#### `Leave`

Unstakes share token to move back deposit token amount. Burns share.

Execute this message by the xASTRO token contract address from which you want to move back deposit.
```json
{
  "send": {
    "contract": <StakingContractAddress>,
    "amount": 999,
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In send.msg, you may decode this JSON string into base64 encoding.
```json
{
  "Leave": {}
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
