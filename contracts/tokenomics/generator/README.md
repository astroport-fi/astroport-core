# Astroport Generator

The generator contract generates token rewards (ASTRO) based on locked LP token amount by liquidity pool providers. Also supports proxy staking via 3-d party contracts for getting dual rewards. Allowed reward proxies are managed via a whitelist. [Staking via proxy](https://miro.medium.com/max/1400/0*8hn2NSnZJZTa9YGV)

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with required contract addresses for depositing and reward distribution.

```json
{
  "owner": "terra...",
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

### `update_config`

Update current vesting contract. Only owner can execute it.

```json
{
  "update_config": {
    "vesting_contract": "terra..."
  }
}
```

### `add`

Adds support of a new LP with optional reward_proxy address.

```json
{
  "add": {
    "lp_token": "terra...",
    "alloc_point": "40",
    "reward_proxy": "terra..."
  }
}
```

### `set`

Update the given pool's ASTRO allocation point. Only owner can execute it.

```json
{
  "set": {
    "lp_token": "terra...",
    "alloc_point": "60"
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

#### `Deposit`

Deposits given lp amount and allocates ASTRO.
Execute this message by the LP token contract address from which you want to make a deposit.
```json
{
  "send": {
    "contract": <GeneratorContractAddress>,
    "amount": 999,
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In send.msg, you may decode this JSON string into base64 encoding.
```json
{
  "Deposit": {}
}
```

#### `DepositFor`

Deposits given lp amount and allocates ASTRO to beneficiary.
Execute this message by the LP token contract address from which you want to make a deposit.

```json
{
  "send": {
    "contract": <GeneratorContractAddress>,
    "amount": 999,
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In send.msg, you may decode this JSON string into base64 encoding.
```json
{
  "DepositFor": "terra..."
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
    "proxies": [
      "terra...",
      "terra..."
    ]
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

### `propose_new_owner`

Creates a request to change ownership. The validity period of the offer is set in the `expires_in` variable.

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

### `reward_info`

Returns reward information for the specified token.

```json
{
  "reward_info": {
    "lp_token": "terra..."
  }
}
```

Returns pool information for the specified token.

```json
{
  "pool_info": {
    "lp_token": "terra..."
  }
}
```

Returns the amount of ASTRO distributed at the future block and specified token.

```json
{
  "simulate_future_reward": {
    "lp_token": "terra...",
    "future_block": 999
  }
}
```
