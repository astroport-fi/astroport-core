# Astroport xASTRO Payout

Pays out a fixed list of `(address, amount)` entries, once per address. It was built to compensate xASTRO holders after
the September 2026 staking incident: the list is every pre-attack xASTRO holder at the pre-attack ASTRO rate, plus
refunds of ASTRO deposited after the attack.

The contract stores only the list's Merkle root. Anyone can push payments with their proofs, and each payment always goes
to the address in its leaf, so it doesn't matter who submits it. A bot pays everyone in batches, and holders can also
submit their own entry.

- An address is paid at most once. Already-paid entries in a batch are skipped, so retrying a batch is safe.
- An invalid proof fails the whole batch.
- The contract never pays out more than `total`.
- The owner can `close` the payout at any time, which sends the remaining balance to a recipient and stops all payments.
- The asset is configurable (native or cw20), so the same code serves Neutron and Terra.

## The tree

`merkle.py` builds the root and proofs from a JSON list of `{"address", "amount"}`:

```
python3 merkle.py list.json tree.json
```

```
leaf = sha256(0x00 || "{address}:{amount}")
node = sha256(0x01 || min(a, b) || max(a, b))
```

Leaves are ordered by hash. An odd node at the end of a level moves up unchanged. The prefixes keep a leaf from passing
for an inner node, and the sorted pair means a proof is just the list of sibling hashes. Anyone can rebuild the root from
the published list and check it against `config`.

---

## InstantiateMsg

```json
{
  "owner": "neutron1...",
  "asset_info": { "native_token": { "denom": "factory/neutron1.../astro" } },
  "merkle_root": "9546d979...",
  "total": "327331114071527"
}
```

## ExecuteMsg

### `pay`

Anyone can call it. Funds can't be attached.

```json
{
  "pay": {
    "payments": [
      { "address": "neutron1...", "amount": "1000000", "proof": ["ab12...", "cd34..."] }
    ]
  }
}
```

### `close`

Owner only. Ends the payout and sends the remaining balance to `recipient`.

```json
{ "close": { "recipient": "neutron1..." } }
```

### `propose_new_owner`, `drop_ownership_proposal`, `claim_ownership`

The usual two-step ownership transfer.

## QueryMsg

### `config`

```json
{ "config": {} }
```

### `status`

Returns `paid_total`, `paid_count` and `closed`.

```json
{ "status": {} }
```

### `paid`

Returns the amount paid to an address, or `null`.

```json
{ "paid": { "address": "neutron1..." } }
```

### `paid_list`

Paid addresses in address order.

```json
{ "paid_list": { "start_after": "neutron1...", "limit": 50 } }
```
