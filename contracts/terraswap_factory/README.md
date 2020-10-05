# TerraSwap Factory

The factory contract can perform creation of terraswap pair contract and also be used as directory contract for all pairs.

## InitMsg

```json
{
  "paid_code_id": "123",
  "token_code_id": "123",
  "init_hook": {
    "msg": "123",
    "contract_addr": "terra..."
  }
}
```

## HandleMsg

### `update_config`

```json
{
  "update_config": {
    "owner": "terra...",
    "token_id": "123",
    "pair_code_id": "123"
  }
}
```

### `create_pair`

```json
{
  "create_pair": {
    "pair_owner": "terra...",
    "commission_collector": "terra...",
    "lp_commission": "123.123",
    "owner_commission": "123.123",
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

Register verified pair contract and token contract for pair contract creation. The sender will be the owner of the factory contract.

```rust
{
    /// Pair contract code ID, which is used to
    pub pair_code_id: u64,
    pub token_code_id: u64,
    pub init_hook: Option<InitHook>,
}
```

### UpdateConfig

The factory contract owner can change relevant code IDs for future pair contract creation.

```json
{
    "update_config":
    {
        "owner": Option<HumanAddr>,
        "pair_code_id": Option<u64>,
        "token_code_id": Option<u64>,
    }
}
```

### Create Pair

When a user execute `CreatePair` operation, it creates `Pair` contract and `LP(liquidity provider)` token contract. It also creates not fully initialized `PairInfo`, which will be initialized with `Register` operation from the pair contract's `InitHook`.

```json
{
  "create_pair": {
    "pair_owner": "terra1~~",
    "commission_collect": "terra1~~",
    "lp_commission": "0.0025",
    "owner_commission": "0.0005",
    "asset_infos": [
      {
        "token": {
          "contract_addr": "terra1~~"
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

### Register

When a user executes `CreatePair` operation, it passes `InitHook` to `Pair` contract and `Pair` contract will invoke passed `InitHook` registering created `Pair` contract to the factory. This operation is only allowed for a pair, which is not fully initialized.

Once a `Pair` contract invokes it, the sender address is registered as `Pair` contract address for the given asset_infos.

```json
{ "register ":
    "asset_infos": [{
        "token": {
            "contract_addr": "terra1~~",
            }
        }, {
            "native_token": {
                "denom": "uusd",
            }
    }],
}
```
