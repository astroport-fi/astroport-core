# Astroport Router

The Router contract contains logic to facilitate multi-hop swaps for Terra native & Astroport tokens. Its interface can be found [here](../../packages/router/README.md)

---

### Operations Assertion

For every swap, the contract checks if the resulting token is the one that was asked for and whether the receiving amount exceeds the minimum to receive.
