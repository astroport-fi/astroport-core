# Astroport Router

The Router Contract contains the logic to facilitate assets multi-hop swap operations via native & Astroport tokens.

Examples:
- KRT => UST => mABNB
- mABNB => UST => KRT

**On-chain swap & Astroport is supported.**

README has updated with new messages (Astroport v1 messages follow).

---

### Operations Assertion
The contract will check whether the resulting token is swapped into one token, check the swap amount is exceed minimum receive.

## InstantiateMsg

```json
{
  "astroport_factory": "terra..."
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

### `execute_swap_operation`

Swaps offer token to ask token. Msg is for internal use.

### Example

Swap UST => mABNB

```json
{
   "execute_swap_operation": {
     "operation": {
        "astro_swap": {
          "offer_asset_info": {
            "native_token": {
              "denom": "uusd"
            }
          },
          "ask_asset_info": {
            "token": {
              "contract_addr": "terra..."
            }
          }
        }
      },
     "to": "terra..."
   }
}
```

### `execute_swap_operations`

Performs multi-hop swap operations via native & Astroport tokens (swaps all offer tokens to ask token). Operations execute one-by-one and last one will return ask token.

### Example

Swap KRT => UST => mABNB

```json
{
  "execute_swap_operations": {
    "operations": [
      {
        "native_swap":{
          "offer_denom":"ukrw",
          "ask_denom":"uusd"
        }
      },
      {
        "astro_swap": {
          "offer_asset_info": {
            "native_token": {
              "denom": "uusd"
            }
          },
          "ask_asset_info": {
            "token": {
              "contract_addr": "terra..."
            }
          }
        }
      }
    ],
    "minimum_receive": "123",
    "to": "terra..."
  }
}
```

### `assert_minimum_receive`

Checks the swap amount is exceed minimum_receive. Msg is for internal use.

```json
{
  "assert_minimum_receive": {
    "asset_info": {
      "token": {
        "contract_addr": "terra..."
      }
    },
    "prev_balance": "123",
    "minimum_receive": "123",
    "receiver": "terra..."
  }
}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `config`

Returns factory contract address.

```json
{
  "config": {}
}
```

### `simulate_swap_operations`

Simulates multi-hop swap operations (execute_swap_operations), examples:

- KRT => UST => mABNB

```json
{
  "simulate_swap_operations" : {
    "offer_amount": "123",
    "operations": [
      {
        "native_swap": {
          "offer_denom": "ukrw",
          "ask_denom": "uusd"
        }
      },
      {
        "astro_swap": {
          "offer_asset_info": {
            "native_token": {
              "denom": "uusd"
            }
          },
          "ask_asset_info": {
            "token": {
              "contract_addr": "terra..."
            }
          }
        }
      }
    ]
  }
}
```

- mABNB => UST => KRT

```json
{
  "simulate_swap_operations" : {
    "offer_amount": "123",
    "operations": [
    {
      "native_swap": {
        "offer_denom": "uusd",
        "ask_denom": "ukrw"
      }
    },
    {
      "astro_swap": {
        "offer_asset_info": {
          "token": {
            "contract_addr": "terra..."
          }
        },
        "ask_asset_info": {
          "native_token": {
            "denom": "uusd"
          }
        }
      }
    }
  ]
  }
}
```
