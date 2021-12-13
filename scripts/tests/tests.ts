import {strictEqual} from "assert"
import {Astroport} from "./lib.js";
import {
    newClient,
    readArtifact,
    queryContract, Client, toEncodedBinary, executeContract,
} from "../helpers.js"

async function main() {
    const { terra, wallet } = newClient()
    const network = readArtifact(terra.config.chainID)


    const astroport = new Astroport(terra, wallet);
    const staking = astroport.staking(network.stakingAddress);

    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, wallet.key.accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, wallet.key.accAddress);

    console.log(`ASTRO balance: ${astro_balance}`)
    console.log(`xASTRO balance: ${xastro_balance}`)

    const staking_amount = 100000;

    // 1. Deposit ASTRO to staking
    console.log(`Staking ${staking_amount} ASTRO`)
    await staking.stakeAstro(network.tokenAddress, staking_amount.toString())

    let new_astro_balance = await astroport.getTokenBalance(network.tokenAddress, wallet.key.accAddress);
    let new_xastro_balance = await astroport.getTokenBalance(network.xastroAddress, wallet.key.accAddress);

    console.log(`ASTRO balance: ${new_astro_balance}`)
    console.log(`xASTRO balance: ${new_xastro_balance}`)

    strictEqual(true, new_astro_balance < astro_balance);
    strictEqual(true, new_xastro_balance > xastro_balance);

    // TODO: Swap tokens in pool
    // TODO: Maker collect fees

    // 2. Unstake ASTRO
    console.log(`Unstaking ${xastro_balance} xASTRO`)
    await staking.unstakeAstro(network.xastroAddress, new_xastro_balance.toString())

    let final_astro_balance = await astroport.getTokenBalance(network.tokenAddress, wallet.key.accAddress);
    let final_xastro_balance = await astroport.getTokenBalance(network.xastroAddress, wallet.key.accAddress);

    console.log(`ASTRO balance: ${final_astro_balance}`)
    console.log(`xASTRO balance: ${final_xastro_balance}`)

    strictEqual(true, final_astro_balance >= astro_balance);
    strictEqual(final_xastro_balance, 0);
}

main().catch(console.log)
