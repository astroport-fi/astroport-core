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
import {chainConfigs} from "./types.d/chain_configs.js";

async function uploadAndInitOracle(terra: LCDClient, wallet: any, pair: Pair, network: any, pool_pair_key: string) {
    let pool_oracle_key = "oracle" + pair.identifier

    if (pair.initOracle && network[pool_pair_key] && !network[pool_oracle_key]) {
        chainConfigs.oracle.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.oracle.initMsg.factory_contract ||= network.factoryAddress
        chainConfigs.oracle.initMsg.asset_infos ||= pair.assetInfos

        console.log(`Deploying oracle for ${pair.identifier}...`)
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.oracle.admin,
            join(ARTIFACTS_PATH, 'astroport_oracle.wasm'),
            chainConfigs.oracle.initMsg,
            chainConfigs.oracle.label)

        // @ts-ignore
        network[pool_oracle_key] = resp.shift().shift();
        console.log(`Address of ${pair.identifier} oracle contract: ${network[pool_oracle_key]}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitGeneratorProxy(terra: LCDClient, wallet: any, pair: Pair, network: any, pool_pair_key: string, pool_lp_token_key: string) {
    if (network[pool_pair_key] && network[pool_lp_token_key] && pair.initGenerator) {
        let pool_generator_proxy_key = "generatorProxy" + pair.identifier

        if (pair.initGenerator.generatorProxy) {
            chainConfigs.generatorProxy.admin ||= chainConfigs.generalInfo.multisig
            chainConfigs.generatorProxy.initMsg.generator_contract_addr ||= network.generatorAddress
            chainConfigs.generatorProxy.initMsg.pair_addr ||= network[pool_pair_key]
            chainConfigs.generatorProxy.initMsg.lp_token_addr ||= network[pool_lp_token_key]
            chainConfigs.generatorProxy.initMsg.reward_contract_addr ||= pair.initGenerator.generatorProxy.rewardContractAddr
            chainConfigs.generatorProxy.initMsg.reward_token_addr ||= pair.initGenerator.generatorProxy.rewardTokenAddr

            // Deploy proxy contract
            console.log(`Deploying generator proxy for ${pair.identifier}...`)
            let resp = await deployContract(
                terra,
                wallet,
                chainConfigs.generatorProxy.admin,
                join(ARTIFACTS_PATH, pair.initGenerator.generatorProxy.artifactName),
                chainConfigs.generatorProxy.initMsg,
                chainConfigs.generatorProxy.label)

            // @ts-ignore
            network[pool_generator_proxy_key] = resp.shift().shift();
            console.log(`Address of ${pair.identifier} generator proxy contract ${network[pool_generator_proxy_key]}`)
            writeArtifact(network, terra.config.chainID)

            // Set generator proxy as allowed
            let config = await queryContract(terra, network.generatorAddress, {
                config: {}
            })
            let new_allowed_proxies: string[] = config.allowed_reward_proxies
            new_allowed_proxies.push(network[pool_generator_proxy_key])
            console.log(`Set the proxy as allowed in generator... Allowed proxies with new one: ${new_allowed_proxies}`)
            await executeContract(terra, wallet, network.generatorAddress, {
                set_allowed_reward_proxies: {
                    proxies: new_allowed_proxies
                }
            })
        }
    }
}

async function createPools(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)
    let pairs = chainConfigs.createPairs.pairs;
    let pools: string[][] = [];

    for (let i = 0; i < pairs.length; i++) {
        let pair = pairs[i]
        let pool_pair_key = "pool" + pair.identifier
        let pool_lp_token_key = "lpToken" + pair.identifier

        // Create pool
        if (!network[pool_pair_key]) {
            console.log(`Creating pool ${pair.identifier}...`)
            let initParams = pair.initParams;
            if (initParams) {
                initParams = toEncodedBinary(initParams)
            }

            let res = await executeContract(terra, wallet, network.factoryAddress, {
                create_pair: {
                    pair_type: pair.pairType,
                    asset_infos: pair.assetInfos,
                    init_params: initParams
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

            if (pair.initGenerator) {
                pools.push([pool_info.liquidity_token, pair.initGenerator.generatorAllocPoint])
            }
        }

        // Deploy oracle
        await uploadAndInitOracle(terra, wallet, pair, network, pool_pair_key)

        // Initialize generator proxy
        await uploadAndInitGeneratorProxy(terra, wallet, pair, network, pool_pair_key, pool_lp_token_key)
    }

    await setupPools(terra, wallet, pools)
}

async function setupPools(terra: LCDClient, wallet: any, pools: string[][]) {
    const network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        throw new Error("Please deploy the generator contract")
    }

    if (pools.length > 0) {
        let active_pool_length = await queryContract(terra, network.generatorAddress, { active_pool_length: {}})
        if (active_pool_length == 0) {
            console.log("Setup pools for the generator...")
            await executeContract(terra, wallet, network.generatorAddress, {
                setup_pools: {
                    pools: pools
                }
            })
        } else {
            console.log("You are cannot setup new pools because the generator has %s active pools already.", active_pool_length)
        }
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
