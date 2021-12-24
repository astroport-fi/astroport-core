# Astroport Generator proxy to Mirror Protocol Rewards

The generator proxy contract interacts with staking contract MIR token. Stake and rewards based on locked LP token amount by liquidity pool providers (dual rewards feature). [Staking via proxy](https://miro.medium.com/max/1400/0*8hn2NSnZJZTa9YGV)

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Inits with required contract addresses for depositing and reward distribution.

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