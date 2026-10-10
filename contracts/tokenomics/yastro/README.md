# Astroport yASTRO

Stake ASTRO for yASTRO, a transferable CW20 that earns a share of protocol fees. On Terra the Maker deposits USDC.inj
into this contract every week, and it streams to yASTRO holders over the following week.

The contract is both the staking contract and the yASTRO token. Every balance change (stake, unstake, transfer, send,
`transfer_from`, `send_from`) first settles the rewards of both sides at their old balances, so rewards follow whoever
holds the yASTRO, second by second, wherever the token moves: wallets, DAOs, pools.

- **Stake:** attach ASTRO to `stake` and receive the same amount of yASTRO.
- **Unstake:** `unstake` burns yASTRO and starts the unbonding period (7 days at launch, at most 28). Unbonding ASTRO
  earns nothing. `withdraw` then sends every matured unbonding. An address can have up to 30 open unbondings.
- **Rewards stream over epochs.** Epochs are Astroport's weeks, starting Monday 00:00 UTC. Everything deposited with
  `deposit_rewards` during an epoch is paid out evenly, per second, over the next epoch, to yASTRO holders pro-rata to
  their balance. Holding yASTRO for a minute earns a minute of rewards, so there's nothing to gain by buying just before
  a deposit. The Maker deposits on Sunday night, so its fees start streaming at Monday 00:00. Anything streamed while
  nobody holds yASTRO is streamed again the next epoch.
- **Reward denoms:** only the ones the owner allows (at most 5, ever). ASTRO can't be a reward, so staked ASTRO and
  rewards never mix.
- **Claim:** rewards are never pushed. `claim` sends the caller's accrued rewards; `pending_rewards` shows them (for the
  app's rewards center). Fractions of a unit are kept until they add up.
- **Contracts that can't claim** (DAO treasuries, vaults): anyone can call `claim_for`, which sends an address's
  rewards to that same address.
- **Pools:** a pool holding yASTRO earns like any holder. Anyone can call `forward_pool_rewards` for any contract that
  answers Astroport's `pair {}` query with yASTRO among its assets. It sends the pool's rewards to Astroport Incentives as
  a schedule on the pool's LP token (from now until the next Monday plus a week, the shortest Incentives allows), and
  LPs who stake their LP tokens in Incentives earn them there. LPs who don't stake in Incentives get none of it.
  - It needs someone to have staked the LP token in Incentives (otherwise the first staker would take everything), and
    yASTRO on Incentives' fee exemption list. No funds are attached.
  - A reward too small for Incentives' minimum of 1 unit per second (about 0.6–1.2 USDC per call), or one Incentives
    rejects, stays with the pool for the next call.
  - `claim_for` refuses pools.

The contract always holds at least the staked plus unbonding ASTRO (`staking_state`), and pays rewards only from what was
deposited. Rounding leaves dust in the contract, never a shortfall.

**Owner powers:** the reward denoms, the unbonding period (up to 28 days, for new unbondings), and the Incentives
address. The Incentives address decides where pool rewards are forwarded, so it's as trusted as the owner. The owner
can't move staked ASTRO or anyone's rewards. The wasm admin can migrate the contract.

**Integrations:** removing the Maker's reward denom makes the Maker's deposit leg fail, and with it the whole
distribution; change the Maker's legs first. yASTRO is a CW20, so it can't be bridged over IBC without forfeiting its
rewards to the bridge escrow.

---

## InstantiateMsg

```json
{
  "owner": "terra1...",
  "astro_denom": "ibc/8D8A7F7253615E5F76CB6252A1E1BD921D5EDB7BBAAF8913FB1C77FF125D9995",
  "unbonding_period": 604800,
  "reward_denoms": ["ibc/E8481AD838C31D4FC12A504B10F9B4E2F830F8818D2735C2FFC707579B5FA60B"],
  "incentives": "terra1...",
  "name": "Staked ASTRO",
  "symbol": "yASTRO",
  "decimals": 6,
  "marketing": {
    "project": "https://astroport.fi",
    "description": "Stake ASTRO, earn protocol fees",
    "marketing": "terra1...",
    "logo": { "url": "https://..." }
  }
}
```

## ExecuteMsg

| Message | Who | What |
|---|---|---|
| `stake { recipient? }` | anyone, with ASTRO | mints yASTRO 1:1 |
| `unstake { amount }` | holder | burns yASTRO, starts unbonding |
| `withdraw {}` | holder | sends matured unbondings |
| `deposit_rewards {}` | anyone, with reward denoms | streams them to holders over the next epoch |
| `claim { recipient? }` | holder | sends accrued rewards |
| `claim_for { address }` | anyone | sends `address`'s rewards to `address` (not pools) |
| `forward_pool_rewards { pool }` | anyone | a pool's rewards to its LP stakers in Incentives |
| `transfer`, `send`, `transfer_from`, `send_from`, `increase_allowance`, `decrease_allowance` | holder | CW20 |
| `update_marketing`, `upload_logo` | marketing address | CW20 marketing |
| `update_config { add_reward_denoms?, remove_reward_denoms?, unbonding_period?, incentives? }` | owner | removing a denom stops deposits; what's queued still streams and owed rewards stay claimable |
| `propose_new_owner`, `drop_ownership_proposal`, `claim_ownership` | owner | two-step ownership transfer |

## QueryMsg

`config`, `pending_rewards { address }`, `unbondings { address }`, `reward_state` (per denom: index, `rate` per second
this epoch, `epoch_end`, `queued` for next epoch, `total_deposited`), `staking_state`, plus the CW20 queries `balance`,
`token_info`, `minter` (always empty), `allowance`, `all_allowances`, `all_spender_allowances`, `all_accounts`,
`marketing_info` and `download_logo`.

APR for a reward denom is `rate * 31_536_000 / total_staked`, in reward units per staked ASTRO unit.
