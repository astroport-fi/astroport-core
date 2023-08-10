# Astroport Liquidity Manager

The Astroport Liquidity Manager is a contract that allows users to provide and withdraw liquidity from the 
Astroport xyk and stable pools with additional slippage limit enforcement. This contract is meant to be non-upgradable and
standalone. It depends only on the actual Astroport factory address. Liquidity Manager also exposes provide/withdraw simulation queries
for xyk and stable pools.

---

## InstantiateMsg

Initializes the contract with the Astroport factory contract address.

```json
{
  "astroport_factory": "wasm1..."
}
```

## ExecuteMsg

### `receive`

CW20 receive msg. Handles only withdraw messages which should come from Astroport LP tokens.

```json
{
  "receive": {
    "sender": "wasm...",
    "amount": "123",
    "msg": "<base64_encoded_json_string>"
  }
}
```

where <base64_encoded_json_string> is a base64 encoded json string of the following format:

```json
{
  "withdraw_liquidity": {
    "pair_msg": {
      "withdraw_liquidity": {}
    },
    "min_assets_to_receive": [
      {
        "info": {
          "native_token": {
            "denom": "uusd"
          }
        },
        "amount": "100000"
      },
      {
        "info": {
          "token": {
            "contract_addr": "wasm1...cw20address"
          }
        },
        "amount": "100000"
      }
    ]
  }
}
```

`min_assets_to_receive` enforces after-withdraw check that the user receives at least the specified amount of assets.

### `provide_liquidity`

Provides liquidity through Liquidity Manager with slippage limit enforcement. Handles XYK pair imbalanced provide and 
returns excess assets to the user.

```json
{
  "provide_liquidity": {
    "pair_addr": "wasm1...",
    "pair_msg": {
      "provide_liquidity": {
        "assets": [
          {
            "info": {
              "native_token": {
                "denom": "uusd"
              }
            },
            "amount": "100000"
          },
          {
            "info": {
              "token": {
                "contract_addr": "wasm1...cw20address"
              }
            },
            "amount": "100000"
          }
        ],
        "slippage_tolerance": "0.02",
        "auto_stake": true,
        "receiver": "wasm1...addr"
      }
    },
    "min_lp_to_receive": "1000"
  }
}
```

`pair_msg` is equal to original Astroport provide message for all pools. `min_lp_to_receive` enforces after-provide check that the user receives at least the specified amount of LP tokens.

## QueryMsg

### `simulate`

Simulates liquidity provide or withdraw.

Provide simulation example: 

```json
{
  "simulate_provide": {
    "pair_addr": "wasm1...addr",
    "pair_msg": {
      "provide_liquidity": {
        "assets": [
          {
            "info": {
              "native_token": {
                "denom": "uusd"
              }
            },
            "amount": "100000"
          },
          {
            "info": {
              "token": {
                "contract_addr": "wasm1...cw20address"
              }
            },
            "amount": "100000"
          }
        ],
        "slippage_tolerance": "0.02",
        "auto_stake": true,
        "receiver": "wasm1...addr"
      }
    }
  }
}
```

Withdraw simulation example:

```json
{
  "simulate_withdraw": {
    "pair_addr": "wasm1...addr",
    "lp_tokens": "1000"
  }
}
```
