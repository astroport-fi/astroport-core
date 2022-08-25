import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    queryContract,
    toEncodedBinary, ARTIFACTS_PATH,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from "@terra-money/terra.js";
import {deployConfigs} from "./types.d/deploy_configs.js";

async function uploadAndInitOracle(terra: LCDClient, wallet: any, pair: Pair, network: any, pool_pair_key: string) {
    let pool_oracle_key = "oracle" + pair.identifier

    if (pair.initOracle && network[pool_pair_key] && !network[pool_oracle_key]) {
        console.log(`Deploying oracle for ${pair.identifier}...`)

        let resp = await deployContract(terra, wallet, network.multisigAddress, join(ARTIFACTS_PATH, 'astroport_oracle.wasm'), {
            factory_contract: network.factoryAddress,
            asset_infos: pair.assetInfos
        }, "Astroport Oracle")

        // @ts-ignore
        network[pool_oracle_key] = resp.shift().shift();

        console.log(`Address of ${pair.identifier} oracle contract: ${network[pool_oracle_key]}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitGeneratorProxy(terra: LCDClient, wallet: any, pair: Pair, network: any, pool_pair_key: string, pool_lp_token_key: string) {
    if (network[pool_pair_key] && network[pool_lp_token_key] && pair.initGenerator) {
        let pool_generator_proxy_key = "generatorProxy" + pair.identifier
        network[pool_generator_proxy_key] = undefined

        if (pair.initGenerator.generatorProxy) {
            // Deploy proxy contract
            console.log(`Deploying generator proxy for ${pair.identifier}...`)
            let resp = await deployContract(terra, wallet, network.multisigAddress, join(ARTIFACTS_PATH, pair.initGenerator.generatorProxy.artifactName), {
                generator_contract_addr: network.generatorAddress,
                pair_addr: network[pool_pair_key],
                lp_token_addr: network[pool_lp_token_key],
                reward_contract_addr: pair.initGenerator.generatorProxy.rewardContractAddr,
                reward_token_addr: pair.initGenerator.generatorProxy.rewardTokenAddr
            }, "Astroport Generator Proxy")

            // @ts-ignore
            network[pool_generator_proxy_key] = resp.shift().shift();
            console.log(`Address of ${pair.identifier} generator proxy contract ${network[pool_generator_proxy_key]}`)

            // Set generator proxy as allowed
            let config = await queryContract(terra, network.generatorAddress, {
                config: {}
            })
            let new_allowed_proxies: Array<String> = config.allowed_reward_proxies
            new_allowed_proxies.push(network[pool_generator_proxy_key] as String)
            console.log(`Set the proxy as allowed in generator... Allowed proxies with new one: ${new_allowed_proxies}`)
            await executeContract(terra, wallet, network.generatorAddress, {
                set_allowed_reward_proxies: {
                    proxies: new_allowed_proxies
                }
            })

        }

        // To add the pool to the generator we need to set all active pools
        // console.log(`Adding ${pair.identifier} to generator...`)
        // await executeContract(terra, wallet, network.generatorAddress, {
        //     setup_pools: {
        //         pools: [[network[pool_lp_token_key], String(pair.initGenerator.generatorAllocPoint)]]
        //     }
        // })
    }
}

async function createPools(terra: LCDClient, wallet: any) {
    const network = readArtifact(terra.config.chainID)
    let pairs = deployConfigs.createPairs.pairs;

    for (let i = 0; i < pairs.length; i++) {
        let pair = pairs[i]
        let pool_pair_key = "pool" + pair.identifier
        let pool_lp_token_key = "lpToken" + pair.identifier

        // Create pool
        if (!network[pool_pair_key]) {
            console.log(`Creating pool ${pair.identifier}...`)
            if (pair.initParams) {
                pair.initParams = toEncodedBinary(pair.initParams)
            }

            let res = await executeContract(terra, wallet, network.factoryAddress, {
                create_pair: {
                    pair_type: pair.pairType,
                    asset_infos: pair.assetInfos,
                    init_params: pair.initParams
                }
            })

            network[pool_pair_key] = res.logs[0].eventsByType.wasm.pair_contract_addr[0]
            let pool_info = await queryContract(terra, network[pool_pair_key], {
                pair: {}
            })

            // write liquidity token
            network[pool_lp_token_key] = pool_info.liquidity_token

            console.log(`Pair successfully created! Address: ${network[pool_pair_key]}`)
            writeArtifact(network, terra.config.chainID)
        }

        // Deploy oracle
        // await uploadAndInitOracle(terra, wallet, pair, network, pool_pair_key)

        // Initialize generator proxy
        // await uploadAndInitGeneratorProxy(terra, wallet, pair, network, pool_pair_key, pool_lp_token_key)
    }
}

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.tokenAddress) {
        throw new Error("Token address is not set, create ASTRO token first")
    }

    if (!network.factoryAddress) {
        throw new Error("Factory address is not set, deploy factory first")
    }

    await createPools(terra, wallet)
    console.log('FINISH')
}

await main()
