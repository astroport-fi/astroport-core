# Astroport Generator

The generator contract generates token rewards (ASTRO) based on locked LP token amount by liquidity pool providers. Also supports proxy staking via 3-d party contracts for getting dual rewards. Allowed reward proxies are managed via a whitelist. [Staking via proxy](https://miro.medium.com/max/1400/0*8hn2NSnZJZTa9YGV)

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

Adds support of a new LP with optional reward_proxy address. `with_update` for updating reward variables for all pools.

```json
{
  "add": {
    "lp_token": "terra...",
    "alloc_point": "40",
    "with_update": true,
    "reward_proxy": "terra..."
  }
}
```

### `set`

Updates LP token allocation point. `with_update` for updating pool reward only.

```json
{
  "set": {
    "lp_token": "terra...",
    "alloc_point": "60",
    "with_update": true
  }
}
```

### `mass_update_pools`

Updates reward variables for all pools.

```json
{
  "mass_update_pools": {}
}
```

### `update_pool`

Updates reward variables of the given pool to be up-to-date.

```json
{
  "update_pool": {
      "lp_token": "terra..."
  }
}
```

### `deposit`

Deposits given lp amount and allocates ASTRO.

```json
{
  "deposit": {
    "lp_token": "terra...",
    "amount": "123"
  }
}
```

### `withdraw`

Withdraws given lp amount and rewards.

```json
{
  "withdraw": {
    "lp_token": "terra...",
    "amount": "123"
  }
}
```

### `emergency_withdraw`

Withdraws deposited lp without caring about rewards. Use emergency only.

```json
{
  "emergency_withdraw": {
    "lp_token": "terra..."
  }
}
```

### `set_allowed_reward_proxies`

Updates allowed proxies whitelist for 3-d party staking.

```json
{
  "set_allowed_reward_proxies": {
    "lp_token": "terra...",
    "amount": "123"
  }
}
```

### `send_orphan_reward`

Orphan rewards accumulate after emergency withdraws. Owner can send orphan rewards to recipient.

```json
{
  "send_orphan_reward": {
    "recipient": "terra...",
    "lp_token": "terra..."
  }
}
```

### `set_tokens_per_block`

Sets reward amount that will be generated per block.

```json
{
  "set_tokens_per_block": {
    "amount": "123"
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `pool_length`

Returns pools count.

```json
{
  "pool_length": {}
}
```

### `deposit`

Returns deposited lp token amount by user.

```json
{
  "deposit": {
    "lp_token": "terra...",
    "user": "terra..."
  }
}
```

### `pending_token`

Gives pending ASTRO and proxy amounts.

```json
{
  "pending_token": {
    "lp_token": "terra...",
    "user": "terra..."
  }
}
```

### `config`

```json
{
  "config": {}
}
```

### `orphan_proxy_rewards`

Returns orphan rewards amount.

```json
{
  "orphan_proxy_rewards": {
    "lp_token": "terra..."
  }
}
```
