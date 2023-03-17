# Astroport native coins wrapper contract

This contract allows you to wrap native coins into Cw20 tokens.

---

## InstantiateMsg

Initializes the contract with the token code identifier that will be used to create a Cw20 token for wrapping native coins.

```json
{
  "denom": "denom",
  "token_code_id": 123,
  "token_decimals": 6
}
```

## ExecuteMsg

### `wrap`

Wraps the amount of specified native coin and issues cw20 tokens instead.
You should send the amount of the native coin through the `funds` array.

```json
{
  "wrap": {}
}
```

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

#### `Unwrap`

Receives Cw20 wrapped tokens and returns unwrapped native coins.

Execute this message by calling the CW20 native wrapped token contract and use a message like this:
```json
{
  "send": {
    "contract": <NativeWrapperContractAddress>,
    "amount": "999",
    "msg": "base64-encodedStringOfWithdrawMsg"
  }
}
```

In `send.msg`, you may encode this JSON string into base64 encoding:
```json
{
  "unwrap": {}
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
