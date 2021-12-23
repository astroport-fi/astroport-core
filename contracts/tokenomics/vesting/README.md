# Astroport Vesting

The vesting contract performs ASTRO token distribution (tokenomic based on it). The maximum supply of ASTRO tokens will be 1 billion. The token allocations for liquidity providers (LPs) is 60%. 20% of these tokens will be distributed during the first year and each next year will decrease by 20%.

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

```json
{
  "token_addr": "terra..."
}
```


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

#### `RegisterVestingAccounts`

Registers account vesting schedules for future token distributions.

Execute this message by the ASTRO token contract address for future token distributions.
```json
{
  "send": {
    "contract": <VestingContractAddress>,
    "amount": 999,
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In send.msg, you may decode this JSON string into base64 encoding.
```json
{
  "RegisterVestingAccounts": {
    "vesting_accounts": [
      {
        "address": "terra...",
        "schedules": {
          "start_point": {
            "time": "1634125119000000000",
            "amount": "123"
          },
          "end_point": {
            "time": "1664125119000000000",
            "amount": "123"
          }
        }
      }
    ]
  }
}
```

### `claim`

Claims the amount from Vesting for transfer to the recipient. Fields are optional.

```json
{
  "claim": {
    "recipient": "terra...",
    "amount": "123"
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the vesting token contract address

```json
{
  "config": {}
}
```

### `vesting_account`

Gives vesting schedules for specified account.

```json
{
  "vesting_account": {
    "address": "terra..."
  }
}
```

### `vesting_accounts`

Gives paginated vesting schedules using specified parameters. Given fields are optional.

```json
{
  "vesting_accounts": {
    "start_after": "terra...",
    "limit": 10,
    "order_by": {
      "desc": {}
    }
  }
}
```

### `available amount`

Returns the available amount for specified account.

```json
{
  "available_amount": {
    "address": "terra..."
  }
}
```