# Astroport xASTRO Staking

This staking contract allows ASTRO holders to stake their tokens in exchange for xASTRO. The amount of ASTRO they can claim later increases as accrued fees in the Maker contract get swapped to ASTRO which is then sent to stakers.

---

## InstantiateMsg

Initializes the contract with the token code ID used by ASTRO and the ASTRO token address.

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

Deposits ASTRO in the xASTRO staking contract.

Execute this message by calling the ASTRO token contract and use a message like this:
```json
{
  "send": {
    "contract": <StakingContractAddress>,
    "amount": "999",
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In `send.msg`, you may encode this JSON string into base64 encoding:
```json
{
  "enter": {}
}
```

#### `leave`

Burns xASTRO and unstakes underlying ASTRO (initial staked amount + accrued ASTRO since staking).

Execute this message by calling the xASTRO token contract and use a message like this:
```json
{
  "send": {
    "contract": <StakingContractAddress>,
    "amount": "999",
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In `send.msg` you may encode this JSON string into base64 encoding:
```json
{
  "leave": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the ASTRO and xASTRO addresses.

```json
{
  "config": {}
}
```

### `get_total_shares`

Returns the total amount of xASTRO tokens.

```json
{
  "get_total_shares": {}
}
```

### `get_total_deposit`

Returns the total amount of ASTRO deposits in the staking contract.

```json
{
  "get_total_deposit": {}
}
```
