# Astroport Oracle

The oracle contract can perform calculation pair assets average prices that consumed outside the astroport.

README has updated with new messages (Astroport v1 messages follow)

---

## ExecuteMsg

### `update`

Updates pair average and cumulative prices.

```json
{}
```

## QueryMsg

All query messages are described below. A custom struct is defined for each query response.

### `consult`

Multiplies a given amount and last average price in common.

```json
{
  "token":
    {
      "info": {
        "token": {
          "contract_addr": "terra..."
        }
      }
    },
  "amount": "1000000"
}
```
