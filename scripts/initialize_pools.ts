import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract, Client, instantiateContract, queryContract,
} from './helpers.js'
import { configDefault } from './deploy_configs.js'
import { join } from 'path'
import { config } from 'dotenv'

const ARTIFACTS_PATH = '../artifacts'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)
    let deployConfig: Config = configDefault

    for (let i = 0; i < deployConfig.initialPools.length; i++) {
        let pool = deployConfig.initialPools[i]
        let pool_pair_key = pool.identifier + "PairAddress"
        let pool_lp_token_key = pool.identifier + "LpTokenAddress"

        // Create pool
        if (!network[pool_pair_key]) {
            console.log(`Creating pool ${pool.identifier}...`)
            let res = await executeContract(terra, wallet, network.factoryAddress, {
                create_pair: {
                    pair_type: pool.pairType,
                    asset_infos: pool.assetInfos,
                    init_params: pool.initParams
                }
            })
            network[pool_pair_key] = res.logs[0].eventsByType.from_contract.pair_contract_addr[0]

            let pool_info = await queryContract(terra, network[pool_pair_key], {
                pair: {}
            })
            network[pool_lp_token_key] = pool_info.liquidity_token

            console.log(`Pair successfully created! Address: ${network[pool_pair_key]}`)
            writeArtifact(network, terra.config.chainID)
        }

        // Deploy oracle
        let pool_oracle_key = pool.identifier + "OracleAddress"
        if (pool.initOracle && network[pool_pair_key] && !network[pool_oracle_key]) {
            console.log(`Deploying oracle for ${pool.identifier}...`)

            network[pool_oracle_key] = await deployContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_oracle.wasm'), {
                factory_contract: network.factoryAddress,
                asset_infos: pool.assetInfos
            })
            console.log(`Address of ${pool.identifier} oracle contract: ${network[pool_oracle_key]}`)
            writeArtifact(network, terra.config.chainID)
        }

        // Initialize generator
        if (network[pool_pair_key] && network[pool_lp_token_key] && pool.initGenerator) {
            let pool_generator_proxy_key = pool.identifier + "GeneratorProxyAddress"
            network[pool_generator_proxy_key] = undefined
            if (pool.initGenerator.generatorProxy) {
                // Deploy proxy contract
                console.log(`Deploying generator proxy for ${pool.identifier}...`)
                network[pool_generator_proxy_key] = await deployContract(terra, wallet, join(ARTIFACTS_PATH, pool.initGenerator.generatorProxy.artifactName), {
                    generator_contract_addr: network.generatorAddress,
                    pair_addr: network[pool_pair_key],
                    lp_token_addr: network[pool_lp_token_key],
                    reward_contract_addr: pool.initGenerator.generatorProxy.rewardContractAddr,
                    reward_token_addr: pool.initGenerator.generatorProxy.rewardTokenAddr
                })
                console.log(`Address of ${pool.identifier} generator proxy contract ${network[pool_generator_proxy_key]}`)

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

            // Add pool to generator
            console.log(`Adding ${pool.identifier} to generator...`)
            await executeContract(terra, wallet, network.generatorAddress, {
                add: {
                    alloc_point: String(pool.initGenerator.generatorAllocPoint),
                    reward_proxy: network[pool_generator_proxy_key],
                    lp_token: network[pool_lp_token_key]
                }
            })
        }
    }

    console.log('FINISH')
}

main().catch(console.log)
