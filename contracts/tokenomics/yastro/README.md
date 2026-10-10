# Astroport yASTRO

Stake ASTRO for yASTRO, a transferable CW20 that earns a share of protocol fees. On Terra the Maker deposits USDC.inj
into this contract, and every yASTRO holder can claim their share.

The contract is both the staking contract and the yASTRO token. Every balance change (stake, unstake, transfer, send,
`transfer_from`, `send_from`) first settles the rewards of both sides at their old balances. Rewards therefore always
follow whoever held the yASTRO when each deposit arrived, wherever the token moves: wallets, DAOs, pools.

- **Stake:** attach ASTRO to `stake` and receive the same amount of yASTRO.
- **Unstake:** `unstake` burns yASTRO and starts the unbonding period (7 days at launch). Unbonding ASTRO earns nothing.
  `withdraw` then sends every matured unbonding. An address can have up to 30 open unbondings.
- **Rewards:** `deposit_rewards` splits the attached reward tokens across current holders, pro-rata to their yASTRO.
  Only the reward denoms the owner allows are accepted. ASTRO can't be a reward, so staked ASTRO and rewards never mix.
  Deposits made while nobody holds yASTRO are added to the next deposit.
- **Claim:** rewards are never pushed. `claim` sends the caller's accrued rewards, and `pending_rewards` shows them
  (for the app's rewards center).
- **Pools:** a pool holding yASTRO earns like any other holder, but can't claim for itself. For pools the owner
  registers, anyone can call `forward_pool_rewards` to move the pool's rewards into Astroport Incentives as a one-epoch
  schedule for that pool's LP token (the shortest Incentives allows: until the next Monday plus one week). LPs who stake
  their LP tokens in Incentives then claim them there. Incentives charges a fee the first time a pool gets a new reward
  token (and after any gap), unless this contract is on its fee exemption list; otherwise the caller attaches the fee.

The contract always holds at least the staked plus unbonding ASTRO (`staking_state`), and pays rewards only from what was
deposited. Rounding leaves dust in the contract, never a shortfall.

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
  "decimals": 6
}
```

## ExecuteMsg

| Message | Who | What |
|---|---|---|
| `stake { recipient? }` | anyone, with ASTRO | mints yASTRO 1:1 |
| `unstake { amount }` | holder | burns yASTRO, starts unbonding |
| `withdraw {}` | holder | sends matured unbondings |
| `deposit_rewards {}` | anyone, with reward denoms | splits the rewards across holders |
| `claim { recipient? }` | holder | sends accrued rewards |
| `forward_pool_rewards { pool }` | anyone | registered pool's rewards to Incentives |
| `transfer`, `send`, `transfer_from`, `send_from`, `increase_allowance`, `decrease_allowance` | holder | CW20 |
| `update_config { add_reward_denoms?, remove_reward_denoms?, unbonding_period?, incentives? }` | owner | removing a denom stops deposits; owed rewards stay claimable. A new unbonding period applies to new unbondings |
| `set_pool { pool, lp_token? }` | owner | registers a pool, or unregisters it without `lp_token` |
| `propose_new_owner`, `drop_ownership_proposal`, `claim_ownership` | owner | two-step ownership transfer |

## QueryMsg

`config`, `pending_rewards { address }`, `unbondings { address }`, `reward_state`, `staking_state`, `pools`, plus the
CW20 queries `balance`, `token_info`, `minter` (always empty), `allowance`, `all_allowances`, `all_spender_allowances`,
`all_accounts` and `marketing_info`.
