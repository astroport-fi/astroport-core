# Astroport Factory

The factory contract can perform creation of astroport pair contract and also be used as directory contract for all
pairs. Code ID for a pair type can be provided when instantiating a new pair, allowing for different pool mechanisms (
e.g. stableswap). Available pair types are managed via a whitelist.

README has updated with new messages (Astroport v1 messages follow)

---

## InstantiateMsg

```json
{
  "paid_code_id": "123",
  "token_code_id": "123",
  "fee_address": "terra...",
  "gov": "terra...",
  "init_hook": {
    "msg": "123",
    "contract_addr": "terra..."
  }
}
```

## ExecuteMsg

### `update_config`

```json
{
  "update_config": {
    "gov": "terra...",
    "owner": "terra...",
    "token_code_id": "123",
    "fee_address": "123"
  }
}
```

### `update_pair_config`

```json
{
  "update_pair_config": {
    "config": {
      "code_id": "123",
      "pair_type": {
        "xyk": {}
      },
      "total_fee_bps": "123",
      "maker_fee_bps": "123"
    }
  }
}
```

### `remove_pair_config`

```json
{
  "pair_type": {
    "stable": {}
  }
}
```

### `create_pair`

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
    ]
  },
  "init_hook": {
    "msg": "123",
    "contract_addr": "terra..."
  }
}
```

### `register`

```json
{
  "register": {
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

### `deregister`

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

## QueryMsg

### `config`

```json
{
  "config": {}
}
```

### `pair`

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

Register verified pair contract and token contract for pair contract creation. The sender will be the owner of the
factory contract.

```rust
{
/// Pair contract code ID, which is used to
pub pair_code_id: u64,
pub token_code_id: u64,
pub init_hook: Option<InitHook>,
}
```

### `pairs`

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
    "limit": "123"
  }
}
```

### `fee_info`

```json
{
  "pair_type": {
    "xyk": {}
  }
}
```

### UpdateConfig

```json
{
  "update_config": {
    "gov": "terra...",
    "owner": "terra...",
    "token_code_id": "123",
    "fee_address": "123"
  }
}
```

### Create Pair

When a user execute `CreatePair` operation, it creates `Pair` contract and `LP(liquidity provider)` token contract. It
also creates not fully initialized `PairInfo`,
> TODO discussion: which will be initialized with `Register` operation from the pair
contract's `InitHook`.

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
    ]
  },
  "init_hook": {
    "msg": "123",
    "contract_addr": "terra..."
  }
}
```

### Register

When a user executes `CreatePair` operation, it passes `InitHook` to `Pair` contract and `Pair` contract will invoke
passed `InitHook` registering created `Pair` contract to the factory. This operation is only allowed for a pair, which
is not fully initialized.

Once a `Pair` contract invokes it, the sender address is registered as `Pair` contract address for the given
asset_infos.

```json
{
  "register": {
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
