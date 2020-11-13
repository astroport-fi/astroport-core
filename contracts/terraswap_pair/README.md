# TerraSwap Pair

## Handlers

### Initialize

This is mainly used from terraswap factory contract to create new terraswap pair. It initialize all swap created parameters which can be updated later with owner key.

It creates liquidity token contract as init response, and execute init hook to register created liquidity token contract to self.

```rust
{
    /// Asset infos
    pub asset_infos: [AssetInfo; 2],
    /// Token code ID for liqudity token creation
    pub token_code_id: u64,
    /// Hook for post initalization
    pub init_hook: Option<InitHook>,
}
```

### Liquidity Provider

The contract has two types of pool, the one is collateral and the other is asset pool. A user can provide liquidity to each pool by sending `provide_liquidity` msgs and also can withdraw with `withdraw_liquidity` msgs.

Whenever liquidity is deposited into a pool, special tokens known as liquidity tokens are minted to the provider’s address, in proportion to how much liquidity they contributed to the pool. These tokens are a representation of a liquidity provider’s contribution to a pool. Whenever a trade occurs, the `lp_commission%` of fee is distributed pro-rata to all LPs in the pool at the moment of the trade. To receive the underlying liquidity back, plus commission fees that were accrued while their liquidity was locked, LPs must burn their liquidity tokens.

When providing liquidity from a smart contract, the most important thing to keep in mind is that tokens deposited into a pool at any rate other than the current oracle price ratio are vulnerable to being arbitraged. As an example, if the ratio of x:y in a pair is 10:2 (i.e. the price is 5), and someone naively adds liquidity at 5:2 (a price of 2.5), the contract will simply accept all tokens (changing the price to 3.75 and opening up the market to arbitrage), but only issue pool tokens entitling the sender to the amount of assets sent at the proper ratio, in this case 5:1. To avoid donating to arbitrageurs, it is imperative to add liquidity at the current price. Luckily, it’s easy to ensure that this condition is met!

> Note before executing the `provide_liqudity` operation, a user must allow the contract to use the liquidity amount of asset in the token contract.

#### Slipage Tolerance

If a user specify the slipage tolerance at provide liquidity msg, the contract restricts the operation when the exchange rate is dropped more than the tolerance.

So, at a 1% tolerance level, if a user sends a transaction with 200 UST and 1 ASSET, amountUSTMin should be set to e.g. 198 UST, and amountASSETMin should be set to .99 ASSET. This means that, at worst, liquidity will be added at a rate between 198 ASSET/1 UST and 202.02 UST/1 ASSET (200 UST/.99 ASSET).

#### Request Format

- Provide Liquidity

  1. Without Slippage Tolerance

  ```json
  {
    "provide_liquidity": {
      "assets": [
        {
          "info": {
            "token": {
              "contract_addr": "terra~~"
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
      ]
    }
  }
  ```

  2. With Slippage Tolerance

  ```json
  {
    "provide_liquidity": {
      "assets": [
        {
          "info": {
            "token": {
              "contract_addr": "terra~~"
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
      ]
    },
    "slippage_tolerance": "0.01"
  }
  ```

- Withdraw Liquidity (must be sent to liquidity token contract)
  ```json
  {
    "withdraw_liquidity": {}
  }
  ```

### Swap

Any user can swap an asset by sending `swap` or invoking `send` msg to token contract with `swap` hook message.

- Native Token => Token

  ```json
  {
      "swap": {
          "offer_asset": {
              "info": {
                  "native_token": {
                      "denom": String
                  }
              },
              "amount": Uint128
          },
          "belief_price": Option<Decimal>,
          "max_spread": Option<Decimal>,
          "to": Option<HumanAddr>
      }
  }
  ```

- Token => Native Token

  **Must be sent to token contract**

  ```json
  {
      "send": {
          "contract": HumanAddr,
          "amount": Uint128,
          "msg": Binary({
              "swap": {
                  "belief_price": Option<Decimal>,
                  "max_spread": Option<Decimal>,
                  "to": Option<HumanAddr>
              }
          })
      }
  }
  ```

#### Swap Spread

The spread is determined with following uniswap mechanism:

```rust
// -max_minus_spread < spread < max_spread
// minus_spread means discount rate.
// Ensure `asset pool * collateral pool = constant product`
let cp = Uint128(offer_pool.u128() * ask_pool.u128());
let return_amount = offer_amount * exchange_rate;
let return_amount = (ask_pool - cp.multiply_ratio(1u128, offer_pool + offer_amount))?;


// calculate spread & commission
let spread_amount: Uint128 =
    (offer_amount * Decimal::from_ratio(ask_pool, offer_pool) - return_amount)?;
let lp_commission: Uint128 = return_amount * config.lp_commission;
let owner_commission: Uint128 = return_amount * config.owner_commission;

// commission will be absorbed to pool
let return_amount: Uint128 =
    (return_amount - (lp_commission + owner_commission)).unwrap();
```

#### Commission

The `lp_commission` remains in the swap pool, which is fixed to `0.3%`, causing a permanent increase in the constant product K. The value of this permanently increased pool goes to all LPs.

