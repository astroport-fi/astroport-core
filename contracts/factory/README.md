# Astroport Factory

The factory contract can perform creation of astroport pair contract and used as directory contract for all pairs. Available pair types are stable and xyk only.

README has updated with new messages (Astroport v1 messages follow).

---

## InstantiateMsg

Code ID for a pair type is provided when instantiating a new pair. So, you don’t have to execute pair contract additionally.

```json
{
  "token_code_id": 123,
  "fee_address": "terra...",
  "owner": "terra...",
  "generator_address": "terra...",
  "pair_configs": [{
      "code_id": 123,
      "pair_type": {
        "xyk": {}
      },
      "total_fee_bps": 100,
      "maker_fee_bps": 10,
      "is_disabled": false
    }
  ]
}
```

## ExecuteMsg

### `update_config`

Updates relevant code IDs.

```json
{
  "update_config": {
    "token_code_id": 123,
    "fee_address": "terra...",
    "generator_address": "terra..."
  }
}
```

### `update_pair_config`

Updating code id and fees for specified pair type or disable pair configs. All fields are optional.

```json
{
  "update_pair_config": {
    "config": {
      "code_id": 123,
      "pair_type": {
        "xyk": {}
      },
      "total_fee_bps": 100,
      "maker_fee_bps": 10,
      "is_disabled": false
    }
  }
}
```

### `create_pair`

Anyone can execute it to create swap pair. When a user executes `CreatePair` operation, it creates `Pair` contract and `LP(liquidity provider)` token contract. It also creates not fully initialized `PairInfo`. Pair `contract_address` for the given asset_infos will be initialized with reply, which is only allowed for a pair, which is not fully initialized.

```json
{
  "create_pair": {
    "pair_type": {
      "xyk": {}
    },
    "asset_infos": [
      {
        "token": {
          "contract_addr": "terra..."
        }
      },
      {
        "native_token": {
          "denom": "uusd"
        }
      }
    ],
    "init_params": "<base64_encoded_json_string: Optional binary serialised parameters for custom pool types>"
  }
}
```

### `deregister`

Deregisters already registered pair (deletes pair).

```json
{
  "deregister": {
    "asset_infos": [
      {
        "token": {
          "contract_address": "terra..."
        }
      },
      {
        "native_token": {
          "denom": "uusd"
        }
      }
    ]
  }
}
```

### `propose_new_owner`

Creates an offer for a new owner. The validity period of the offer is set in the `expires_in` variable.

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

### `config`

Returns general settings of the factory

```json
{
  "config": {}
}
```

### `pair`

Gives info for specified assets pair.

```json
{
  "pair": {
    "asset_infos": [
      {
        "token": {
          "contract_address": "terra..."
        }
      },
      {
        "native_token": {
          "denom": "uusd"
        }
      }
    ]
  }
}
```

### `pairs`

Gives paginated pair infos using specified start_after and limit. Given fields are optional.

```json
{
  "pairs": {
    "start_after": [
      {
        "token": {
          "contract_address": "terra..."
        }
      },
      {
        "native_token": {
          "denom": "uusd"
        }
      }
    ],
    "limit": 10
  }
}
```

### `fee_info`

Gives fees for specified pair type.

```json
{
  "pair_type": {
    "xyk": {}
  }
}
```
