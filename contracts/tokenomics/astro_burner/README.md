# Astroport ASTRO Burner

The burner removes a tokenfactory denom from supply. It burns its whole balance of the denom with tokenfactory
`MsgBurn`, so the burner must be the denom's admin.

Anyone can trigger a burn. Funds can reach the burner in two ways:

- They are attached to `burn`. The Maker does this with a `deposit` leg whose `msg` is `{"burn":{}}`.
- They are sent to the burner directly, with a later `burn` call from anyone.

---

## InstantiateMsg

```json
{
  "denom": "factory/neutron1.../astro"
}
```

## ExecuteMsg

### `burn`

Burns the burner's whole balance of the denom, including funds attached to the call. If the call attaches any other
denom, it fails. With nothing to burn, it does nothing.

```json
{
  "burn": {}
}
```

## QueryMsg

### `config`

```json
{ "config": {} }
```

### `total_burned`

Returns the total amount this contract has burned.

```json
{ "total_burned": {} }
```
