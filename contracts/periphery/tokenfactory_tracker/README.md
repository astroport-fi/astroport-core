# Astroport TokenFactory Tracker

Tracks balances of TokenFactory token holders using timestamps

---

## InstantiateMsg

Initializes the contract with the TokenFactory denom to track as well as the
TokenFactory module address.

You can find the module address by using 

```shell
wasmd query auth module-account tokenfactory
```

Instantiate message

```json
{
  "tracked_denom": "factory/creator/denom",
  "tokenfactory_module_address": "wasm19ejy8n9qsectrf4semdp9cpknflld0j6el50hx"
}
```

Once the contract is instantiated it will only track the denom specified.
Attach this contract to TokenFactory (only admin can do this)

```shell
wasmd tx tokenfactory set-beforesend-hook factory/creator/denom wasm1trackingcontract
```

## ExecuteMsg

This contract has no executable messages


## QueryMsg

### `balance_at`

Query the balance of an address at a given timestamp in seconds.
If timestamp is not set, it will return the value at the current timestamp.

```json
{
  "balance_at": {
    "address": "wasm1...addr",
    "timestamp": "1698745413"
  }
}
```

### `total_supply_at`

Query the total supply at a given timestamp in seconds.
If timestamp is not set, it will return the value at the current timestamp.

```json
{
  "total_supply_at": {
    "timestamp": "1698745413"
  }
}
```

## MigrateMsg

This contract has no migrations