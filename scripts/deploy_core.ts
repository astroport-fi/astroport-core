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
import {strictEqual} from "assert";

const ARTIFACTS_PATH = '../artifacts'
const SECONDS_DIVIDER: number = 60 * 60 * 24 // min, hour, day

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    if (!deployConfigs.multisig.address) {
        throw new Error("Set the proper owner multisig for the contracts")
    }

    await uploadAndInitToken(terra, wallet)
    await uploadAndInitTreasury(terra, wallet)
    await uploadPairContracts(terra, wallet)
    await uploadAndInitStaking(terra, wallet)
    await uploadAndInitFactory(terra, wallet)
    await uploadAndInitRouter(terra, wallet)
    await uploadAndInitMaker(terra, wallet)

    await uploadAndInitVesting(terra, wallet)
    await uploadAndInitGenerator(terra, wallet)
    // await setupPools(terra, wallet)
    await setupVestingAccounts(terra, wallet)
    console.log('FINISH')
}

async function uploadAndInitToken(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.tokenCodeID) {
        network.tokenCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_token.wasm')!)
        console.log(`Token codeId: ${network.tokenCodeID}`)
    }

    if (!network.tokenAddress) {
        deployConfigs.token.admin ||= wallet.key.accAddress
        deployConfigs.token.initMsg.marketing.marketing ||= wallet.key.accAddress

        for (let i=0; i<deployConfigs.token.initMsg.initial_balances.length; i++) {
            deployConfigs.token.initMsg.initial_balances[i].address ||= wallet.key.accAddress
        }

        console.log('Deploying Token...')
        let resp = await deployContract(
            terra,
            wallet,
            deployConfigs.token.admin,
            join(ARTIFACTS_PATH, 'astroport_token.wasm'),
            deployConfigs.token.initMsg,
            deployConfigs.token.label,
        )

        // @ts-ignore
        network.tokenAddress = resp.shift().shift()
        console.log("astro:", network.tokenAddress)
        console.log(await queryContract(terra, network.tokenAddress, { token_info: {} }))
        console.log(await queryContract(terra, network.tokenAddress, { minter: {} }))

        for (let i=0; i<deployConfigs.token.initMsg.initial_balances.length; i++) {
            let balance = await queryContract(terra, network.tokenAddress, { balance: { address: deployConfigs.token.initMsg.initial_balances[i].address } })
            strictEqual(balance.balance, deployConfigs.token.initMsg.initial_balances[i].amount)
        }

        writeArtifact(network, terra.config.chainID)
    }
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
        deployConfigs.staking.initMsg.deposit_token_addr ||= network.tokenAddress
        deployConfigs.staking.initMsg.token_code_id ||= network.xastroTokenCodeID
        deployConfigs.staking.admin ||= deployConfigs.multisig.address

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

        for (let i=0; i<deployConfigs.factory.initMsg.pair_configs.length; i++) {
            if (!deployConfigs.factory.initMsg.pair_configs[i].code_id) {
                if ( JSON.stringify(deployConfigs.factory.initMsg.pair_configs[i].pair_type) === JSON.stringify({ xyk: {}}) ) {
                    deployConfigs.factory.initMsg.pair_configs[i].code_id ||= network.pairCodeID;
                }

                if (JSON.stringify(deployConfigs.factory.initMsg.pair_configs[i].pair_type) === JSON.stringify({ stable: {}}) ) {
                    deployConfigs.factory.initMsg.pair_configs[i].code_id ||= network.pairStableCodeID;
                }
            }
        }

        deployConfigs.factory.initMsg.token_code_id ||= network.tokenCodeID;
        deployConfigs.factory.initMsg.whitelist_code_id ||= network.whitelistCodeID;
        deployConfigs.factory.initMsg.owner ||= wallet.key.accAddress;
        deployConfigs.factory.admin ||= deployConfigs.multisig.address;

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
        if (deployConfigs.factory.change_owner) {
            console.log('Propose owner for factory. Ownership has to be claimed within %s days',
                Number(deployConfigs.factory.proposeNewOwner.expires_in) / SECONDS_DIVIDER)
            await executeContract(terra, wallet, network.factoryAddress, {
                "propose_new_owner": deployConfigs.factory.proposeNewOwner
            })
        }
    }
}

async function uploadAndInitRouter(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.routerAddress) {
        deployConfigs.router.initMsg.astroport_factory ||= network.factoryAddress
        deployConfigs.router.admin ||= deployConfigs.multisig.address;

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
        deployConfigs.maker.initMsg.factory_contract ||= network.factoryAddress;
        deployConfigs.maker.initMsg.staking_contract ||= network.stakingAddress;
        deployConfigs.maker.initMsg.astro_token_contract ||= network.tokenAddress;
        deployConfigs.maker.admin ||= deployConfigs.multisig.address;

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

    if (!network.treasuryAddress) {
        deployConfigs.treasury.admin ||= deployConfigs.multisig.address;

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
        network.treasuryAddress = resp.shift().shift()
        console.log(`Treasury Contract Address: ${network.treasuryAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitVesting(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        deployConfigs.vesting.initMsg.token_addr ||= network.tokenAddress;
        deployConfigs.vesting.initMsg.owner ||= wallet.key.accAddress;
        deployConfigs.vesting.admin ||= deployConfigs.multisig.address;

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
        deployConfigs.generator.initMsg.astro_token ||= network.tokenAddress;
        deployConfigs.generator.initMsg.vesting_contract ||= network.vestingAddress;
        deployConfigs.generator.initMsg.factory ||= network.factoryAddress;
        deployConfigs.generator.initMsg.whitelist_code_id ||= network.whitelistCodeID;
        deployConfigs.generator.initMsg.owner ||= wallet.key.accAddress;
        deployConfigs.generator.admin ||=  deployConfigs.multisig.address;

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

async function setupVestingAccounts(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        throw new Error("Please deploy the vesting contract")
    }

    console.log('Register vesting accounts:', JSON.stringify(deployConfigs.vesting.registration.msg))
    await executeContract(terra, wallet, network.tokenAddress, {
        "send": {
            contract: network.vestingAddress,
            amount: deployConfigs.vesting.registration.amount,
            msg: toEncodedBinary(deployConfigs.vesting.registration.msg)
        }
    })
}

async function setupPools(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        throw new Error("Please deploy the generator contract")
    }

    await executeContract(terra, wallet, network.generatorAddress, {
        setup_pools: {
            pools: deployConfigs.generator.registration.pools
        }
    })

    // Set new owner for generator
    if (deployConfigs.generator.change_owner) {
        console.log('Propose owner for generator. Ownership has to be claimed within %s days',
            Number(deployConfigs.generator.propose_new_owner.expires_in) / SECONDS_DIVIDER)
        await executeContract(terra, wallet, network.generatorAddress, {
            "propose_new_owner": deployConfigs.generator.propose_new_owner
        })
    }
}

await main()
