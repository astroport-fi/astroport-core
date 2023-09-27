# Astroport Shared Multisig

It is a multisig with two addresses created upon instantiation. Each address has its own role (manager1 or manager2), however, 
both have exactly the same permissions. Each role can propose a new address which can then claim that role.

## Instantiation

To create the multisig, you must pass in a set of address for each one to pass a proposal. To create a 2 multisig, 
pass 2 voters (manager1 and manager2).

```json
{
  "factory_addr": "wasm...",
  "max_voting_period": {
    "height": 123
  },
  "manager1": "wasm...",
  "manager2": "wasm...",
  "denom1": "wasm...",
  "denom2": "wasm...",
  "target_pool": "wasm..."
}
```

## ExecuteMsg

### `propose`

Example proposal

```json
{
  "propose": {
     "title": "Example proposal",
     "description": "Example proposal",
     "msgs": [
         {
           "wasm": {
               "execute": {
                   "contract_addr": "wasm...",
                   "msg": "<base64_encoded_json_string>",
                   "funds": []
               }
           }
         }
     ]
  }
}
```

### `vote`

Votes for a proposal with specified parameters

```json
{
  "vote": {
    "proposal_id": 123,
    "vote": {"yes": {}}
  }
}
```

### `execute`

Executes a proposal by ID

```json
{
  "execute": {
    "proposal_id": 123
  }
}
```

### `close`

Closes a proposal by ID

```json
{
  "execute": {
    "proposal_id": 123
  }
}
```

### `setup_max_voting_period`

Updates contract parameters

```json
{
  "setup_max_voting_period": {
    "max_voting_period": 123
  }
}
```

### `start_rage_quit`

Locks the contract and starts the migration from the target pool.

```json
{
  "start_rage_quit": {}
}
```

### `complete_target_pool_migration`

Completes the migration from the target pool.

```json
{
  "complete_target_pool_migration": {}
}
```

### `update_config`

Update configuration

```json
{
  "update_config": {
    "factory": "wasm...",
    "generator": "wasm..."
  }
}
```

### `transfer`

Transfer coins

```json
{
  "transfer": {
    "asset": {
        "native_token": {
          "denom": "uusd"
        }
    },
    "recipient": "wasm..."
  }
}
```

### `provide_liquidity`

Providing Liquidity With Slippage Tolerance

```json
{
  "provide_liquidity": {
    "pool": {
      "target": {}
    },
    "assets": [
      {
        "info": {
          "token": {
            "contract_addr": "wasm..."
          }
        },
        "amount": "1000000"
      },
      {
        "info": {
          "native_token": {
            "denom": "uusd"
          }
        },
        "amount": "1000000"
      }
    ],
    "slippage_tolerance": "0.01",
    "receiver": "wasm..."
  }
}
```

### `setup_pools`

```json
{
  "setup_pools": {
    "target_pool": "wasm...",
    "migration_pool": "wasm..."
  }
}
```

### `withdraw_target_pool_lp`

Withdraws LP tokens from the target pool. If `provide_params` is specified, liquidity will be introduced
into the migration pool in the same transaction.

```json
{
  "withdraw_target_pool_lp": {
    "withdraw_amount": "1234",
    "provide_params": {
      "slippage_tolerance": "0.01"
    }
  }
}
```

### `withdraw_rage_quit_lp`

Withdraws the LP tokens from the specified pool.

```json
{
  "withdraw_rage_quit_lp": {
    "pool": {
      "target": {}
    },
    "withdraw_amount": "1234"
  }
}
```

### `deposit_generator`

Stakes the target LP tokens in the Generator contract

```json
{
  "deposit_generator": {
    "amount": "1234"
  }
}
```

### `withdraw_generator`

Withdraw LP tokens from the Astroport generator.

```json
{
  "withdraw_generator": {
    "amount": "1234"
  }
}
```

### `claim_generator_rewards`

Update generator rewards and returns them to the Multisig.

```json
{
  "claim_generator_rewards": {}
}
```

### `propose_new_manager_1`

Creates an offer to change the contract manager. The validity period of the offer is set in the `expires_in` variable.
After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.

```json
{
  "propose_new_manager_1": {
    "new_manager": "wasm...",
    "expires_in": 1234567
  }
}
```

### `drop_manager_1_proposal`

Removes an existing offer to change the contract manager.

```json
{
  "drop_manager_1_proposal": {}
}
```

### `claim_manager_1`

Used to claim contract manager.

```json
{
  "claim_manager_1": {}
}
```

### `propose_new_manager_2`

Creates an offer to change the contract Manager2. The validity period of the offer is set in the `expires_in` variable.
After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.

```json
{
  "propose_new_manager_2": {
    "new_manager": "wasm...",
    "expires_in": 1234567
  }
}
```

### `drop_manager_2_proposal`

Removes an existing offer to change the contract Manager2.

```json
{
  "drop_manager_2_proposal": {}
}
```

### `claim_manager_2`

Used to claim contract Manager2.

```json
{
  "claim_manager_2": {}
}
```

## QueryMsg

### `config`

Returns the general config of the contract.

```json
{
  "config": {}
}
```

### `proposal`

Returns the information of the proposal

```json
{
  "proposal": { "proposal_id": 123 }
}
```

### `list_proposals`

Returns a list of proposals

```json
{
  "list_proposals": {}
}
```

### `reverse_proposals`

Returns the reversed list of proposals

```json
{
  "reverse_proposals": {}
}
```

### `vote`

Returns the vote (opinion as well as weight counted) as well as the address of the voter who submitted it

```json
{
  "vote": {
    "proposal_id": 123,
    "voter": "wasm..."
  }
}
```

### `list_votes`

Returns a list of votes (opinion as well as weight counted) as well as the addresses of the voters who submitted it

```json
{
  "list_votes": {
    "proposal_id": 123
  }
}
```