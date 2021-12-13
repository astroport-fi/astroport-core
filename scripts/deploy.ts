import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract,
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

    await uploadPairContracts(terra, wallet)
    await uploadAndInitVesting(terra, wallet)
    await uploadAndInitStaking(terra, wallet)
    await uploadAndInitGenerator(terra, wallet)
    await uploadAndInitFactory(terra, wallet)
    await uploadAndInitRouter(terra, wallet)
    await uploadAndInitMaker(terra, wallet)

    console.log('FINISH')
}

async function uploadPairContracts(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.pairCodeID) {
        console.log('Register Pair Contract...')
        network.pairCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.pairStableCodeID) {
        console.log('Register Stable Pair Contract...')
        network.pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitVesting(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        console.log('Deploying Vesting...')
        let resp = await deployContract(
            terra,
            wallet,
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

async function uploadAndInitStaking(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.stakingAddress) {
        console.log('Deploying Staking...')

        let resp = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
            {
                token_code_id: network.tokenCodeID,
                deposit_token_addr:  network.tokenAddress,
            }
        )

        network.stakingAddress = resp.shift()
        network.xastroAddress = resp.shift();

        console.log(`Address Staking Contract: ${network.stakingAddress}`)
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

async function uploadAndInitFactory(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.factoryAddress) {
        console.log('Deploying Factory...')
        console.log(`CodeId Pair Contract: ${network.pairCodeID} CodeId Stable Pair Contract: ${network.pairStableCodeID}`)

        let resp = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
            {
                owner: wallet.key.accAddress,
                pair_configs: [
                    {
                        code_id: network.pairCodeID,
                        pair_type: { xyk: {} },
                        total_fee_bps: 30, // 0.3% xyk
                        maker_fee_bps: 3333 // 1/3rd of xyk fees go to maker
                    },
                    {
                        code_id: network.pairStableCodeID,
                        pair_type: { stable: {} },
                        total_fee_bps: 5, // 0.05% stableswap
                        maker_fee_bps: 5000 // 50% of stableswap fees go to the Maker
                    }
                ],
                token_code_id: network.tokenCodeID,
                generator_address: network.generatorAddress,
                fee_address: undefined,
            }
        )
        network.factoryAddress = resp.shift()
        console.log(`Address Factory Contract: ${network.factoryAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitRouter(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.routerAddress) {
        console.log('Deploying Router...')
        let resp = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_router.wasm'),
            {
                astroport_factory: network.factoryAddress,
            },
        )
        network.routerAddress = resp.shift()
        console.log(`Address Router Contract: ${network.routerAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitMaker(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.makerAddress) {
        console.log('Deploying Maker...')
        let resp = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_maker.wasm'),
            {
                owner: wallet.key.accAddress,
                factory_contract: String(network.factoryAddress),
                staking_contract: String(network.stakingAddress),
                astro_token_contract: String(network.tokenAddress),
            }
        )
        network.makerAddress = resp.shift()
        console.log(`Address Maker Contract: ${network.makerAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set maker address in factory
        console.log('Set maker address in factory')
        await executeContract(terra, wallet, network.factoryAddress, {
            "update_config": {
                fee_address: network.makerAddress
            }
        })
    }
}

main().catch(console.log)
