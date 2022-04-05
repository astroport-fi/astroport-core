import {strictEqual} from "assert"
import {Astroport} from "./lib.js";
import {
    NativeAsset,
    newClient,
    readArtifact,
    TokenAsset,
} from "../helpers.js"


async function main() {
    const { terra, wallet } = newClient()
    const network = readArtifact(terra.config.chainID)

    const astroport = new Astroport(terra, wallet);
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    // 1. Provide liquidity
    await provideLiquidity(network, astroport, wallet.key.accAddress)

    // 2. Stake ASTRO
    await stake(network, astroport, wallet.key.accAddress)

    // 3. Swap tokens in pool
    await swap(network, astroport, wallet.key.accAddress)

    // 4. Collect Maker fees
    await collectFees(network, astroport, wallet.key.accAddress)

    // 5. Withdraw liquidity
    await withdrawLiquidity(network, astroport, wallet.key.accAddress)

    // 6. Unstake ASTRO
    await unstake(network, astroport, wallet.key.accAddress)
}

async function provideLiquidity(network: any, astroport: Astroport, accAddress: string) {
    const liquidity_amount = 100000000;
    const pool_uust_astro = astroport.pair(network.poolAstroUst);

    // Provide liquidity in order to swap
    await pool_uust_astro.provideLiquidity(new NativeAsset('uusd', liquidity_amount.toString()), new TokenAsset(network.tokenAddress, liquidity_amount.toString()))

    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    console.log(`ASTRO balance: ${astro_balance}`)
    console.log(`xASTRO balance: ${xastro_balance}`)
}

async function withdrawLiquidity(network: any, astroport: Astroport, accAddress: string) {
    const pool_uust_astro = astroport.pair(network.poolAstroUst);

    let pair_info = await pool_uust_astro.queryPair();
    let lp_token_amount = await astroport.getTokenBalance(pair_info.liquidity_token, accAddress);

    // Withdraw liquidity
    await pool_uust_astro.withdrawLiquidity(pair_info.liquidity_token, lp_token_amount.toString());

    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    console.log(`ASTRO balance: ${astro_balance}`)
    console.log(`xASTRO balance: ${xastro_balance}`)
}

async function stake(network: any, astroport: Astroport, accAddress: string) {
    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    const staking = astroport.staking(network.stakingAddress);
    const staking_amount = 100000;

    console.log(`Staking ${staking_amount} ASTRO`)
    await staking.stakeAstro(network.tokenAddress, staking_amount.toString())

    let new_astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let new_xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    console.log(`ASTRO balance: ${new_astro_balance}`)
    console.log(`xASTRO balance: ${new_xastro_balance}`)

    strictEqual(true, new_astro_balance < astro_balance);
    strictEqual(true, new_xastro_balance > xastro_balance);
}

async function unstake(network: any, astroport: Astroport, accAddress: string) {
    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    const staking = astroport.staking(network.stakingAddress);

    console.log(`Unstaking ${xastro_balance} xASTRO`)
    await staking.unstakeAstro(network.xastroAddress, xastro_balance.toString())

    let final_astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let final_xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    console.log(`ASTRO balance: ${final_astro_balance}`)
    console.log(`xASTRO balance: ${final_xastro_balance}`)

    strictEqual(true, final_astro_balance >= astro_balance);
    strictEqual(final_xastro_balance, 0);
}

async function swap(network: any, astroport: Astroport, accAddress: string) {
    const pool_uust_astro = astroport.pair(network.poolAstroUst);
    const factory = astroport.factory(network.factoryAddress);
    const swap_amount = 10000;

    let pair_info = await pool_uust_astro.queryPair();

    let astro_balance = await astroport.getTokenBalance(network.tokenAddress, accAddress);
    let xastro_balance = await astroport.getTokenBalance(network.xastroAddress, accAddress);

    console.log(`ASTRO balance: ${astro_balance}`)
    console.log(`xASTRO balance: ${xastro_balance}`)

    let fee_info = await factory.queryFeeInfo('xyk');
    strictEqual(true,  fee_info.fee_address != null, "fee address is not set")
    strictEqual(true,  fee_info.total_fee_bps > 0, "total_fee_bps address is not set")
    strictEqual(true,  fee_info.maker_fee_bps > 0, "maker_fee_bps address is not set")

    console.log('swap some tokens back and forth to accumulate commission')
    for (let index = 0; index < 5; index++) {
        console.log("swap astro to uusd")
        await pool_uust_astro.swapCW20(network.tokenAddress, swap_amount.toString())

        console.log("swap uusd to astro")
        await pool_uust_astro.swapNative(new NativeAsset('uusd', swap_amount.toString()))

        let lp_token_amount = await astroport.getTokenBalance(pair_info.liquidity_token, accAddress);
        let share_info = await pool_uust_astro.queryShare(lp_token_amount.toString());
        console.log(share_info)
    }
}

async function collectFees(network: any, astroport: Astroport, accAddress: string) {
    const maker = astroport.maker(network.makerAddress);

    let maker_cfg = await maker.queryConfig();
    strictEqual(maker_cfg.astro_token_contract, network.tokenAddress)
    strictEqual(maker_cfg.staking_contract, network.stakingAddress)

    let balances = await maker.queryBalances([new TokenAsset(network.tokenAddress, '0')]);
    strictEqual(true, balances.length > 0, "maker balances are empty. no fees are collected")

    console.log(balances)

    let resp = await maker.collect([network.poolAstroUst])
    console.log(resp)
}

main().catch(console.log)
