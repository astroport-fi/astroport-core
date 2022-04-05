# Astroport Factory

The factory contract can create new Astroport pair contracts (and associated LP token contracts) and it is used as a directory for all pairs. The default pair types are constant product and stableswap but governance may decide to add custom pools that can have any implementation.

---

## InstantiateMsg

The instantiation message takes in the token code ID for the token type supported on Astroport. It also takes in the `fee_address` that collects fees for governance, the contract `owner`, the Generator contract address and the initial pair types available to create.

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

Updates contract variables, namely the code ID of the token implementation used in Astroport, the address that receives governance fees and the Generator contract address.

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

This function can be used to:

- Update the code ID used to instantiate new pairs of a specific type
- Change the fee structure for a pair
- Disable the pair type so no other pairs can be instantiated

Note that all fields are optional.

The fee structure for a pair is set up as follows:

- `total_fee_bps` is the total amount of fees (in bps) that are charged on each swap
- `maker_fee_bps` is the percentage of fees out of `total_fee_bps` that is sent to governance. 100% is 10,000

As an example, let's say a pool charged 30bps (`total_fee_bps` is 30) and we want 1/3r of the fees to go to governance. In this case, `maker_fee_bps` should be 3333 because 3333 / 10,000 * 30 / 100 = 0.1%

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

Anyone can execute this function to create an Astroport pair. `CreatePair` creates both a `Pair` contract and a `LP(liquidity provider)` token contract. The account that instantiates the pair must specify the pair type they want as well as the assets for which the pool is created.

Custom pool types may also need extra parameters which can be packed in `init_params`.

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

Deregisters an already registered pair. This allows someone else to create a new pair (of any type) for the tokens that don't have a registered pair anymore. This is how pairs can be "upgraded".

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

Creates an offer to change the contract ownership. The validity period of the offer is set in the `expires_in` variable. After `expires_in` seconds pass, the proposal expires and cannot be accepted anymore.

```json
{
  "propose_new_owner": {
    "owner": "terra...",
    "expires_in": 1234567
  }
}
```

### `drop_ownership_proposal`

Removes an existing offer to change the contract owner.

```json
{
  "drop_ownership_proposal": {}
}
```

### `claim_ownership`

Used to claim contract ownership.

```json
{
  "claim_ownership": {}
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns general factory parameters (owner, token code ID, pair type configurations).

```json
{
  "config": {}
}
```

### `pair`

Returns information about a specific pair.

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

Returns information about multiple pairs (the result is paginated). The function starts returning pair information starting after the pair  `start_after`. The function returns maximum `limit` pairs.

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

Returns the fee information for a specific pair type (`total_fee_bps` and `maker_fee_bps`).

```json
{
  "pair_type": {
    "xyk": {}
  }
}
```

### `blacklisted_pair_types`

Returns a vector that contains blacklisted pair types.

```json
{
  "blacklisted_pair_types": {}
}
```
