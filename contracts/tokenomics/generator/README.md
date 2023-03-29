# Astroport Generator

The Generator contract allocates token rewards (ASTRO) for various LP tokens and distributes them pro-rata to LP stakers. The Generator supports proxy staking via 3rd party contracts that offer a second reward besides ASTRO token emissions.

---

## InstantiateMsg

Initializes the contract with required addresses and contracts used for reward distributions.

```json
{
  "owner": "terra...",
  "astro_token": "terra...",
  "tokens_per_block": "123",
  "start_block": "123",
  "vesting_contract": "terra..."
}
```

## ExecuteMsg

### `update_config`

Update the vesting contract address, generator controller contract address or generator guardian address.
Only the contract owner can execute this.

```json
{
  "update_config": {
    "vesting_contract": "terra...",
    "generator_controller": "terra...",
    "guardian": "terra...",
    "voting_escrow": "terra...",
    "generator_limit": 20
  }
}
```

### `setup_pools`

Set up a new list of pools with allocation points.

```json
{
  "setup_pools": {
    "pools" : [
      [
        "terra...",
        "60"
      ],
      [
        "terra...",
        "40"
      ]
    ]
  }
}
```

### `update_pool`

Update has_asset_rewards parameter for the given pool.

```json
{
  "update_pool": {
    "lp_token": "terra...",
    "has_asset_rewards": true
  }
}
```

### `claim_rewards`

Update rewards and return it to user.

```json
{
  "claim_rewards": {
      "lp_tokens": ["terra...", "terra...", "terra..."]
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

### `deposit`

Stakes LP tokens in a specific generator (inside the Generator contract).
In order to stake in the Generator contract, you should execute this message inside the contract of the LP token you want to stake.

```json
{
  "send": {
    "contract": <GeneratorContractAddress>,
    "amount": "999",
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

Inside `send.msg`, you may encode this JSON string into base64 encoding:

```json
{
  "deposit": {}
}
```

### `depositFor`

Stakes LP tokens in the Generator on behalf of another address.
In order to stake in the Generator contract, you should execute this message inside the LP token you want to stake.

```json
{
  "send": {
    "contract": <GeneratorContractAddress>,
    "amount": "999",
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In `send.msg`, you may encode this JSON string into base64 encoding:

```json
{
  "deposit_for": "terra..."
}
```

### `withdraw`

Unstakes LP tokens from the Generator contract and claims outstanding token emissions.

```json
{
  "withdraw": {
    "lp_token": "terra...",
    "amount": "123"
  }
}
```

### `emergency_withdraw`

Unstakes LP tokens without caring about rewards. To be used only in emergencies such as a critical bug found in the Generator contract.

```json
{
  "emergency_withdraw": {
    "lp_token": "terra..."
  }
}
```

### `send_orphan_reward`

Sends orphaned rewards (left behind by emergency withdraws) to another address. Only the contract owner can transfer orphan rewards.

```json
{
  "send_orphan_reward": {
    "recipient": "terra...",
    "lp_token": "terra..."
  }
}
```

### `set_tokens_per_block`

Sets the total amount of ASTRO distributed per block among all active generators. Only the owner can execute this.

```json
{
  "set_tokens_per_block": {
    "amount": "123"
  }
}
```

### `propose_new_owner`

Creates a request to change contract ownership. The validity period of the offer is set by the `expires_in` variable. Only the current owner can execute this.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes the existing offer to change contract ownership. Only the contract owner can execute this.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used by the newly proposed contract owner to claim contract ownership.

```json
{
  "claim_ownership": {}
}
```

### `move_to_proxy`

Change the current dual rewards proxy for a specific LP token. Only the contract owner can execute this.

```json
{
  "move_to_proxy": {
    "lp_token": "terra...",
    "proxy": "terra..."
  }
}
```

### `update_tokens_blockedlist`

Add or remove tokens to and from the tokens blocked list.
Only the owner contract or generator guardian can execute this.

```json
{
  "update_tokens_blockedlist": {
    "add": ["terra...", "terra..."],
    "remove": ["terra...", "terra...", "terra..."]
  }
}
```

### `deactivate_pool`

Sets the allocation point to zero for specified pool. Only the factory contract can execute this.

```json
{
  "deactivate_pool": {
    "lp_token": "terra..."
  }
}
```

### `deactivate_pools`

Sets the allocation point to zero for each pool by the pair type.

```json
{
  "deactivate_pool": {
    "pair_types": [{"xyk": {}}, {"stable":  {}}]
  }
}
```

### `checkpoint_user_boost`

Updates emissions boost for specified generators

```json
{
  "checkpoint_user_boost": {
    "generators": ["terra...", "terra..."],
    "user": "terra..."
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `pool_length`

Returns the total amount of generators that have been created until now.

```json
{
  "pool_length": {}
}
```

### `deposit`

Returns the amount of a specific LP token that a user currently has staked in the Generator.

```json
{
  "deposit": {
    "lp_token": "terra...",
    "user": "terra..."
  }
}
```

### `pending_token`

Returns the amount of pending ASTRO and 3rd party token rewards that can be claimed by a user that staked a specific LP token.

```json
{
  "pending_token": {
    "lp_token": "terra...",
    "user": "terra..."
  }
}
```

### `config`

Returns the main Generator contract configuration.

```json
{
  "config": {}
}
```

### `orphan_proxy_rewards`

Returns the amount of orphaned proxy rewards left behind by emergency withdrawals.

```json
{
  "orphan_proxy_rewards": {
    "lp_token": "terra..."
  }
}
```

### `reward_info`

Returns information about token emissions for the specified LP token.

```json
{
  "reward_info": {
    "lp_token": "terra..."
  }
}
```

### `pool_info`

Returns pool information for the specified LP token.

```json
{
  "pool_info": {
    "lp_token": "terra..."
  }
}
```

### `simulate_future_reward`

Returns the amount of ASTRO that will be distributed up to a future block and for a specific LP token.

```json
{
  "simulate_future_reward": {
    "lp_token": "terra...",
    "future_block": 999
  }
}
```

### `list_of_stakers`

Returns a list of stakers that currently have funds in a specific generator.

```json
{
  "list_of_stakers": {
    "lp_token": "terra...",
    "start_after": "terra...",
    "limit": 5
  }
}
```

### `blocked_tokens_list`

Returns the list of blocked tokens

```json
{
  "blocked_tokens_list": {}
}
```

### `active_pool_length`

Returns the total amount of active generators.

```json
{
  "active_pool_length": {}
}
```

### `user_virtual_amount`

Returns the current virtual amount in a specific generator

```json
{
  "user_virtual_amount": {
    "lp_token": "terra...",
    "user": "terra..."
  }
}
```

### `total_virtual_amount`

Returns the total virtual supply of generator

```json
{
  "total_virtual_amount": {
    "lp_token": "terra..."
  }
}
```

### `reward_proxies_list`

Returns a list of reward proxy contracts which have been ever used

```json
{
  "reward_proxies_list": {}
}
```
