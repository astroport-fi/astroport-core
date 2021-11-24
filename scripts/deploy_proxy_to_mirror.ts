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

    if (!network.mirror_staking && network.mirAddress) {
        console.log('Deploying mirror staking contract...')
        network.mirror_staking = await deployContract(terra, wallet, join(ARTIFACTS_PATH, 'mirror_staking.wasm'), {
            base_denom: "uusd",
            mint_contract: "terra1x46rqay4d3cssq8gxxvqz8xt6nwlz4td20k38v", // mock value
            mirror_token: network.mirAddress,
            oracle_contract: "terra1x46rqay4d3cssq8gxxvqz8xt6nwlz4td20k38v", // mock value
            owner: wallet.key.accAddress,
            premium_min_update_interval: 0,
            short_reward_contract: "terra1x46rqay4d3cssq8gxxvqz8xt6nwlz4td20k38v", // mock value
            terraswap_factory: "terra1x46rqay4d3cssq8gxxvqz8xt6nwlz4td20k38v", // mock value
        })
        console.log(`Address of the deployed contract ${network.mirror_staking}`)
    }

    if (network.mir_ust_pair && network.mirror_staking && !network.mir_ust_pair_registered_in_mirror_staking) {
        let pool_info = await queryContract(terra, network.mir_ust_pair, {
            "pair": {}
        })

        console.log('Registering MIR-UST pair in mirror staking contract...')
        await executeContract(terra, wallet, network.mirror_staking, {
            register_asset: {
                asset_token: network.mir_ust_pair,
                staking_token: pool_info.liquidity_token
            }
        })
        network.mir_ust_pair_registered_in_mirror_staking = true
        console.log('Registered successfully')
    }

    if (!network.proxy_to_mirror && network.generatorAddress && network.mir_ust_pair && network.mirror_staking && network.mirAddress) {
        let pool_info = await queryContract(terra, network.mir_ust_pair, {
            "pair": {}
        })

        console.log('Deploying generator proxy to mirror...')
        network.proxy_to_mirror = await deployContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_generator_proxy_to_mirror.wasm'), {
            generator_contract_addr: network.generatorAddress,
            pair_addr: network.mir_ust_pair,
            lp_token_addr: pool_info.liquidity_token,
            reward_contract_addr: network.mirror_staking,
            reward_token_addr: network.mirAddress
        })
        console.log(`Address of the deployed contract ${network.proxy_to_mirror}`)
    }

    if (!network.proxy_set_as_allowed_in_generator && network.generatorAddress && network.proxy_to_mirror) {
        let config = await queryContract(terra, network.generatorAddress, {
            config: {}
        })
        let new_allowed_proxies: Array<String> = config.allowed_reward_proxies
        new_allowed_proxies.push(network.proxy_to_mirror as String)
        console.log(`Set the proxy as allowed in generator... Allowed proxies with new one: ${new_allowed_proxies}`)
        await executeContract(terra, wallet, network.generatorAddress, {
            set_allowed_reward_proxies: {
                proxies: new_allowed_proxies
            }
        })
        console.log(`Test step 1`)

        network.proxy_set_as_allowed_in_generator = true
        console.log('Set successfully')

    }

    if (!network.proxy_to_mirror_registered_in_generator && network.generatorAddress && network.proxy_to_mirror && network.mir_ust_pair) {
        let pool_info = await queryContract(terra, network.mir_ust_pair, {
            "pair": {}
        })

        console.log('Registering proxy to mirror in generator...')
        await executeContract(terra, wallet, network.generatorAddress, {
            add: {
                alloc_point: String(100),
                reward_proxy: network.proxy_to_mirror,
                lp_token: pool_info.liquidity_token
            }
        })

        network.proxy_to_mirror_registered_in_generator = true
        console.log('Registered successfully')
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
