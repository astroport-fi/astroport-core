# Astroport Generator Proxy Rewards Template

This generator proxy contract allows an external staking contract to be connected to the Generator. It gives Generator stakers the ability to claim both ASTRO emissions as well as 3rd party tokens at the same time. This is referred to as "dual rewards" in Astroport.

## Be sure that all the template's TODOs get properly changed!

---

## InstantiateMsg

Initializes the proxy contract with required addresses (generator, LP token to stake etc).

```json
{
  "generator_contract_addr": "terra...",
  "pair_addr": "terra...",
  "lp_token_addr": "terra...",
  "reward_contract_addr": "terra...",
  "reward_token_addr": "terra..."
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

### `update_rewards`

Updates 3rd party token proxy rewards and withdraws rewards from the 3rd party staking contract.

```json
{
  "update_rewards": {}
}
```

### `send_rewards`

Sends accrued token rewards to a specific account.

```json
{
  "send_rewards": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `withdraw`

Withdraws LP tokens alongside any outstanding token rewards and sends them to the specified address.

```json
{
  "withdraw": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `emergency_withdraw`

Unstake LP tokens without caring about accrued rewards.

```json
{
  "emergency_withdraw": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `callback`

Handles callback mesasges.

One example is for transferring LP tokens after a withdrawal from the 3rd party staking contract.

```json
{
  "callback": {
    "transfer_lp_tokens_after_withdraw": {
      "account": "terra...",
      "prev_lp_balance": "1234"
    }
  }
}

```
## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns the contract's configuration.

```json
{
  "config": {}
}
```

### `deposit`

Returns the deposited/staked token amount for a specific account.

```json
{
  "deposit": {}
}
```

### `reward`

Returns the total amount of 3rd party rewards.

```json
{
  "reward": {}
}
```

### `pending_token`

Returns the total amount of pending rewards for all stakers.

```json
{
  "pending_token": {}
}
```

### `reward_info`

Returns the reward (3rd party) token contract address.

```json
{
  "reward_info": {}
}
```
