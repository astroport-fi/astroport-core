# Astroport Vesting

The vesting contract performs ASTRO token distribution (tokenomic based on it). The maximum supply of ASTRO tokens will be 1 billion. The token allocations for liquidity providers (LPs) is 60%. 20% of these tokens will be distributed during the first year and each next year will decrease by 20%.

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

```json
{
  "owner": "terra...",
  "token_addr": "terra..."
}
```

## ExecuteMsg

### `update_config`

Updates contract owner.

```json
{
  "update_config": {
    "owner": "terra..."
  }
}
```

### `register_vesting_accounts`

Registers account vesting schedules for future token distributions.

```json
{
  "register_vesting_accounts": {
    "vesting_accounts": [
      {
        "address": "terra...",
        "schedules": {
          "start_point": {
            "time": "123",
            "amount": "123"
          },
          "end_point": {
            "time": "123",
            "amount": "123"
          }
        }
      }
    ]
  }
}
```

### `claim`

Releases amount if claimed amount of tokens is available. Fields are optional.

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

Returns owner and token addresses.

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
    "limit": "123",
    "order_by": {
      "desc": {}
    }
  }
}
```