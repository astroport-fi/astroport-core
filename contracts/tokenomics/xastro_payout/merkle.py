#!/usr/bin/env python3
"""Builds the payout Merkle tree from a JSON list of {"address", "amount"} and writes the root and
each entry's proof. Same hashing as src/merkle.rs:

    leaf = sha256(0x00 || "{address}:{amount}")
    node = sha256(0x01 || min(a, b) || max(a, b))

Leaves are ordered by leaf hash. An odd node at the end of a level moves up unchanged.

    python3 merkle.py list.json tree.json [hrp]

Every address must be lowercase bech32 (with the given prefix, if any) and every amount positive:
the contract validates addresses in their one canonical form and can't send zero.
"""
import hashlib
import json
import sys


def leaf_hash(address, amount):
    return hashlib.sha256(b"\x00" + f"{address}:{int(amount)}".encode()).digest()


def node_hash(a, b):
    lo, hi = sorted((a, b))
    return hashlib.sha256(b"\x01" + lo + hi).digest()


BECH32 = "qpzry9x8gf2tvdw0s3jn54khce6mua7l"


def bech32_ok(address, hrp=None):
    if address != address.lower() or "1" not in address:
        return False
    prefix, data = address.rsplit("1", 1)
    if (hrp and prefix != hrp) or len(data) < 7 or any(c not in BECH32 for c in data):
        return False
    gen = [0x3B6A57B2, 0x26508E6D, 0x1EA119FA, 0x3D4233DD, 0x2A1462B3]
    chk = 1
    for v in [ord(c) >> 5 for c in prefix] + [0] + [ord(c) & 31 for c in prefix] + [BECH32.index(c) for c in data]:
        top = chk >> 25
        chk = (chk & 0x1FFFFFF) << 5 ^ v
        for i in range(5):
            chk ^= gen[i] if (top >> i) & 1 else 0
    return chk == 1


def validate(entries, hrp=None):
    seen = set()
    for e in entries:
        a, n = e["address"], int(e["amount"])
        if not bech32_ok(a, hrp):
            sys.exit(f"not a lowercase bech32{' ' + hrp if hrp else ''} address: {a}")
        if n <= 0 or str(n) != str(e["amount"]).strip():
            sys.exit(f"amount must be a positive integer: {a} {e['amount']}")
        if a.lower() in seen:
            sys.exit(f"duplicate address in the list: {a}")
        seen.add(a.lower())


def build(entries):
    leaves = sorted((leaf_hash(e["address"], e["amount"]), e) for e in entries)
    levels = [[h for h, _ in leaves]]
    while len(levels[-1]) > 1:
        cur = levels[-1]
        levels.append([node_hash(cur[i], cur[i + 1]) if i + 1 < len(cur) else cur[i] for i in range(0, len(cur), 2)])
    out = []
    for idx, (_, e) in enumerate(leaves):
        proof, i = [], idx
        for level in levels[:-1]:
            sibling = i ^ 1
            if sibling < len(level):
                proof.append(level[sibling].hex())
            i //= 2
        out.append({"address": e["address"], "amount": str(int(e["amount"])), "proof": proof})
    return levels[-1][0].hex(), out


def main():
    entries = json.load(open(sys.argv[1]))
    validate(entries, sys.argv[3] if len(sys.argv) > 3 else None)
    root, payments = build(entries)
    total = sum(int(e["amount"]) for e in entries)
    json.dump({"merkle_root": root, "total": str(total), "count": len(payments), "payments": payments},
              open(sys.argv[2], "w"), indent=1)
    print(f"root {root}  entries {len(payments)}  total {total}")


if __name__ == "__main__":
    main()
