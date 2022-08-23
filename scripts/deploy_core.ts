import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract, instantiateContract, queryContract, toEncodedBinary,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from '@terra-money/terra.js';
import {deployConfigs} from "./types.d/deploy_configs.js";

const ARTIFACTS_PATH = '../artifacts'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    let network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.tokenAddress) {
        throw new Error("Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...")
    }

    if (!network.multisigAddress) {
        throw new Error("Set the proper owner multisig for the contracts")
    }

    await uploadAndInitTreasury(terra, wallet)
    await uploadPairContracts(terra, wallet)
    await uploadAndInitStaking(terra, wallet)
    await uploadAndInitFactory(terra, wallet)
    await uploadAndInitRouter(terra, wallet)
    await uploadAndInitMaker(terra, wallet)

    await uploadAndInitVesting(terra, wallet)
    await uploadAndInitGenerator(terra, wallet)
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

async function uploadAndInitStaking(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.xastroTokenCodeID) {
        console.log('Register xASTRO token contract...')
        network.xastroTokenCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_xastro_token.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.stakingAddress) {
        deployConfigs.staking.initMsg.deposit_token_addr = network.tokenAddress
        deployConfigs.staking.initMsg.token_code_id = network.xastroTokenCodeID

        console.log('Deploying Staking...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.staking.admin,
            join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
            deployConfigs.staking.initMsg,
            deployConfigs.staking.label,
        )

        let addresses = resp.shift()
        // @ts-ignore
        network.stakingAddress = addresses.shift();
        // @ts-ignore
        network.xastroAddress = addresses.shift();

        console.log(`Staking Contract Address: ${network.stakingAddress}`)
        console.log(`xASTRO token Address: ${network.xastroAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitFactory(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.factoryAddress) {
        console.log('Deploying Factory...')
        console.log(`CodeId Pair Contract: ${network.pairCodeID} CodeId Stable Pair Contract: ${network.pairStableCodeID}`)

        deployConfigs.factory.initMsg.pair_configs[0].code_id = network.pairCodeID;
        deployConfigs.factory.initMsg.pair_configs[1].code_id = network.pairStableCodeID;
        deployConfigs.factory.initMsg.token_code_id = network.tokenCodeID;
        deployConfigs.factory.initMsg.whitelist_code_id = network.whitelistCodeID;

        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.factory.admin,
            join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
            deployConfigs.factory.initMsg,
            deployConfigs.factory.label
        )

        // @ts-ignore
        network.factoryAddress = resp.shift().shift()
        console.log(`Address Factory Contract: ${network.factoryAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set new owner for factory
        console.log('Propose owner for factory. Onwership has to be claimed within 7 days')
        await executeContract(terra, wallet, network.factoryAddress, {
            "propose_new_owner": deployConfigs.factory.proposeNewOwner
        })
    }
}

async function uploadAndInitRouter(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.routerAddress) {
        deployConfigs.router.initMsg.astroport_factory = network.factoryAddress

        console.log('Deploying Router...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.router.admin,
            join(ARTIFACTS_PATH, 'astroport_router.wasm'),
            deployConfigs.router.initMsg,
            deployConfigs.router.label
        )

        // @ts-ignore
        network.routerAddress = resp.shift().shift()
        console.log(`Address Router Contract: ${network.routerAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitMaker(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.makerAddress) {
        deployConfigs.maker.initMsg.factory_contract = network.factoryAddress
        deployConfigs.maker.initMsg.staking_contract = network.stakingAddress
        deployConfigs.maker.initMsg.astro_token_contract = network.tokenAddress

        console.log('Deploying Maker...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.maker.admin,
            join(ARTIFACTS_PATH, 'astroport_maker.wasm'),
            deployConfigs.maker.initMsg,
            deployConfigs.maker.label
        )

        // @ts-ignore
        network.makerAddress = resp.shift().shift()
        console.log(`Maker Contract Address: ${network.makerAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set maker address in factory
        console.log('Set the Maker and the proper owner address in the factory')
        await executeContract(terra, wallet, network.factoryAddress, {
            "update_config": {
                fee_address: network.makerAddress
            }
        })
    }
}

async function uploadAndInitTreasury(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.whitelistCodeID) {
        console.log('Register Treasury Contract...')
        network.whitelistCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_whitelist.wasm')!)
    }

    if (!network.whitelistAddress) {
        console.log('Instantiate the Treasury...')
        let resp = await instantiateContract(
            terra,
            wallet,
            deployConfigs.treasury.admin,
            network.whitelistCodeID,
            deployConfigs.treasury.initMsg,
            deployConfigs.treasury.label,
            );

        // @ts-ignore
        network.whitelistAddress = resp.shift().shift()
        console.log(`Whitelist Contract Address: ${network.whitelistAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitVesting(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        deployConfigs.vesting.initMsg.token_addr = network.tokenAddress

        console.log('Deploying Vesting...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.vesting.admin,
            join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
            deployConfigs.vesting.initMsg,
            deployConfigs.vesting.label
        )

        // @ts-ignore
        network.vestingAddress = resp.shift().shift()
        console.log(`Vesting Contract Address: ${network.vestingAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitGenerator(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        deployConfigs.generator.initMsg.astro_token = network.tokenAddress
        deployConfigs.generator.initMsg.vesting_contract = network.vestingAddress
        deployConfigs.generator.initMsg.factory = network.factoryAddress
        deployConfigs.generator.initMsg.whitelist_code_id = network.whitelistCodeID

        console.log('Deploying Generator...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.generator.admin,
            join(ARTIFACTS_PATH, 'astroport_generator.wasm'),
            deployConfigs.generator.initMsg,
            deployConfigs.generator.label
        )

        // @ts-ignore
        network.generatorAddress = resp.shift().shift()
        console.log(`Generator Contract Address: ${network.generatorAddress}`)

        writeArtifact(network, terra.config.chainID)

        // Set generator address in factory
        await executeContract(terra, wallet, network.factoryAddress, {
            update_config: {
                generator_address: network.generatorAddress,
            }
        })

        console.log(await queryContract(terra, network.factoryAddress, { config: {} }))
    }
}

async function registerGenerator(terra: LCDClient, wallet: any, lp_token: string, alloc_point: string) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        console.log(`Please deploy generator contract`)
        return
    }

    await executeContract(terra, wallet, network.generatorAddress, {
        setup_pools: {
            pools: [[lp_token, alloc_point]]
        }
    })
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
                                time: 1640865600,
                                amount: String("100") // 1% on total supply at once
                            },
                            end_point: {
                                time: 1672401600,
                                amount: String("10000")
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
            amount: String("10000"),
            msg: toEncodedBinary(msg)
        }
    })
}

async function setupPools(terra: LCDClient, wallet: any) {
    // setup pools
    // await registerGenerator(terra, wallet, "terra17n5sunn88hpy965mzvt3079fqx3rttnplg779g", "28303")
    // await registerGenerator(terra, wallet, "terra1m24f7k4g66gnh9f7uncp32p722v0kyt3q4l3u5", "24528")
    // await registerGenerator(terra, wallet, "terra1htw7hm40ch0hacm8qpgd24sus4h0tq3hsseatl", "47169")


    // TESTNET
    // await registerGenerator(terra, wallet, "terra1cs66g290h4x0pf6scmwm8904yc75l3m7z0lzjr", "28303")
    // await registerGenerator(terra, wallet, "terra1q8aycvr54jarwhqvlr54jr2zqttctnefw7x37q", "24528")
    // await registerGenerator(terra, wallet, "terra1jzutwpneltsys6t96vkdr2zrc6cg0ke4e6y3s0", "47169")

    // await setupVesting(terra, wallet, network)

    // Set new owner for generator
    // network = readArtifact(terra.config.chainID) // reload variables
    // console.log('Propose owner for generator. Onwership has to be claimed within 7 days')
    // await executeContract(terra, wallet, network.generatorAddress, {
    //     "propose_new_owner": {
    //         owner: network.multisigAddress,
    //         expires_in: 604800 // 7 days
    //     }
    // })
}
main().catch(console.log)
