import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    toEncodedBinary,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from '@terra-money/terra.js';

const ARTIFACTS_PATH = '../artifacts'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    let network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.tokenAddress) {
        console.log(`Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`)
        return
    }

    await uploadAndInitVesting(terra, wallet)
    await uploadAndInitGenerator(terra, wallet)

    console.log('FINISH')
}

async function uploadAndInitVesting(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        console.log('Deploying Vesting...')
        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
            {
                token_addr: network.tokenAddress,
            },
        )
        network.vestingAddress = resp.shift()
        console.log(`Address Vesting Contract: ${network.vestingAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitGenerator(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        console.log('Deploying Generator...')

        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_generator.wasm'),
            {
                owner: wallet.key.accAddress,
                allowed_reward_proxies: [],
                astro_token: network.tokenAddress,
                start_block: '1',
                tokens_per_block: String(10000000),
                vesting_contract: network.vestingAddress,
            }
        )
        network.generatorAddress = resp.shift()
        console.log(`Address Generator Contract: ${network.generatorAddress}`)

        // Setup vesting for generator
        await setupVesting(terra, wallet, network)

        writeArtifact(network, terra.config.chainID)
    }
}

async function setupVesting(terra: LCDClient, wallet: any, network: any) {
    console.log('Setting Vesting...')

    let msg = {
        register_vesting_accounts: {
            vesting_accounts: [
                {
                    address: network.generatorAddress,
                    schedules: [
                        {
                            start_point: {
                                time: String(new Date(2021, 10, 6).getTime()),
                                amount: String("63072000000000")
                            }
                        }
                    ]
                }
            ]
        }
    }

    console.log('Register vesting accounts:', JSON.stringify(msg))

    await executeContract(terra, wallet, network.tokenAddress, {
        "send": {
            contract: network.vestingAddress,
            amount: String("63072000000000"),
            msg: toEncodedBinary(msg)
        }
    })
}

main().catch(console.log)
