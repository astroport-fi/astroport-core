# Astroport Generator Proxy for Mirror Protocol Staking Rewards

This generator proxy contract interacts with the MIR-UST staking contract. Stake and rewards based on locked LP token amount by liquidity pool providers (dual rewards feature). For a diagram of how dual reward proxies work, you can take a look [here](https://miro.medium.com/max/1400/0*8hn2NSnZJZTa9YGV).

---

## InstantiateMsg

Initializes the contract with required addresses (generator, LP token to stake etc).

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

Updates token proxy rewards.

```json
{
  "update_rewards": {}
}
```

### `send_rewards`

Sends token rewards amount for given address.

```json
{
  "send_rewards": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `withdraw`

Withdraws token rewards amount for given address.

```json
{
  "withdraw": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `emergency_withdraw`

Withdraws token rewards amount for given address.

```json
{
  "emergency_withdraw": {
    "account": "terra...",
    "amount": "123"
  }
}
```

### `callback`

Handles the callbacks messages of the contract.
In the current example used for transfer liquidity tokens after withdraw.

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

Returns the contract's configuration

```json
{
  "config": {}
}
```

### `deposit`

Returns deposited/staked token amount.

```json
{
  "deposit": {}
}
```

### `reward`

Gives token proxy reward amount.

```json
{
  "reward": {}
}
```

### `pending_token`

Gives token proxy reward pending amount.

```json
{
  "pending_token": {}
}
```

### `reward_info`

Returns the reward token contract address

```json
{
  "reward_info": {}
}
```
