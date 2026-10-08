#!/usr/bin/env python3
"""Builds the payout Merkle tree from a JSON list of {"address", "amount"} and writes the root and
each entry's proof. Same hashing as src/merkle.rs:

    leaf = sha256(0x00 || "{address}:{amount}")
    node = sha256(0x01 || min(a, b) || max(a, b))

Leaves are ordered by leaf hash. An odd node at the end of a level moves up unchanged.

    python3 merkle.py list.json tree.json
"""
import hashlib
import json
import sys


def leaf_hash(address, amount):
    return hashlib.sha256(b"\x00" + f"{address}:{int(amount)}".encode()).digest()


def node_hash(a, b):
    lo, hi = sorted((a, b))
    return hashlib.sha256(b"\x01" + lo + hi).digest()


def build(entries):
    addresses = [e["address"] for e in entries]
    if len(set(addresses)) != len(addresses):
        sys.exit("duplicate address in the list")
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
    root, payments = build(entries)
    total = sum(int(e["amount"]) for e in entries)
    json.dump({"merkle_root": root, "total": str(total), "count": len(payments), "payments": payments},
              open(sys.argv[2], "w"), indent=1)
    print(f"root {root}  entries {len(payments)}  total {total}")


if __name__ == "__main__":
    main()
