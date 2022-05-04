import {Astroport, Generator} from "./lib.js";
import {provideLiquidity} from "./test_router.js"
import {
    NativeAsset,
    newClient,
    readArtifact, TokenAsset,
} from "../helpers.js"

async function main() {
    const cl = newClient()
    const network = readArtifact(cl.terra.config.chainID)

    const astroport = new Astroport(cl.terra, cl.wallet);
    console.log(`chainID: ${cl.terra.config.chainID} wallet: ${cl.wallet.key.accAddress}`)

    // 1. Provide ASTRO-UST liquidity
    const liquidity_amount = 5000000;
    await provideLiquidity(network, astroport, cl.wallet.key.accAddress, network.poolAstroUst, [
        new NativeAsset('uusd', liquidity_amount.toString()),
        new TokenAsset(network.tokenAddress, liquidity_amount.toString())
    ])

    // 2. Provide LUNA-UST liquidity
    await provideLiquidity(network, astroport, cl.wallet.key.accAddress, network.poolLunaUst, [
        new NativeAsset('uluna', liquidity_amount.toString()),
        new NativeAsset('uusd', liquidity_amount.toString())
    ])

    // 3. Fetch the pool balances
    let lpTokenAstroUst = await astroport.getTokenBalance(network.lpTokenAstroUst, cl.wallet.key.accAddress);
    let lpTokenLunaUst = await astroport.getTokenBalance(network.lpTokenLunaUst, cl.wallet.key.accAddress);

    console.log(`AstroUst balance: ${lpTokenAstroUst}`)
    console.log(`LunaUst balance: ${lpTokenLunaUst}`)

    const generator = astroport.generator(network.generatorAddress);
    console.log("generator config: ", await generator.queryConfig());

    // 4. Register generators
    await generator.registerGenerator([
        [network.lpTokenAstroUst, "24528"],
        [network.lpTokenLunaUst, "24528"],
    ])

    // 4. Deposit to generator
    await generator.deposit(network.lpTokenAstroUst, "623775")
    await generator.deposit(network.lpTokenLunaUst, "10000000")

    // 5. Fetch the deposit balances
    console.log(`deposited: ${await generator.queryDeposit(network.lpTokenAstroUst, cl.wallet.key.accAddress)}`)
    console.log(`deposited: ${await generator.queryDeposit(network.lpTokenLunaUst, cl.wallet.key.accAddress)}`)

    // 6. Find checkpoint generators limit for user boost
    await findCheckpointGeneratorsLimit(generator, network)
}

async function findCheckpointGeneratorsLimit(generator: Generator, network: any) {
    let generators = []
    for(let i = 0; i < 40; i++) {
        generators.push(network.lpTokenAstroUst)
        generators.push(network.lpTokenLunaUst)
    }

    await generator.checkpointUserBoost(generators)

}

main().catch(console.log)
