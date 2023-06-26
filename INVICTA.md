1. temporarily remove pair_stable_bluna contract from workspace due to obsolete: https://github.com/Anchor-Protocol/anchor-bAsset-contracts
2. remove MIR related contracts
3. can no longer claim bAsset rewards (contracts/pair_stable_luna). Remove `anchor-basset = {git = "https://github.com/Anchor-Protocol/anchor-bAsset-contracts.git", tag = "v0.2.1", package = "basset"}`
4. Keep `[AssetInfo; 2]` in factor asset_infos instead of `Vec<AssetInfo>` (in new astroport)
5. Need to replace pair_stable_bluna/tests/integration.rs from using cw-multi-test to classic-test-tube
6. Change calculation of unsafe operation from (also to conform with cosmwasm-std Decimal256 instead of old terra big number Decimal256)

    `Uint128 * Decimal::from(Decimal256::one() / Decimal256::from(belief_price))` 

    to 

    `Uint128 * Uint128::try_from((Decimal256::one() / Decimal256::from(belief_price)).to_uint_floor()).unwrap()`
7. All rounding from Decimal to Uint will be floor (Decimal -> Uint128, Decimal256 -> Uint256)