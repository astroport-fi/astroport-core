import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract, Client,
} from './helpers.js'
import { configDefault } from './deploy_configs.js'
import { join } from 'path'

const ARTIFACTS_PATH = '../artifacts'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    let deployConfig: Config = configDefault
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.tokenAddress) {
        console.log(`Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`)
        return
    }

    /*************************************** Register Pairs Contract *****************************************/
    if (!network.pairCodeID) {
        console.log('Register Pair Contract...')
        network.pairCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair.wasm')!)
    }
    if (!network.pairStableCodeID) {
        console.log('Register Stable Pair Contract...')
        network.pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
    }

    /*************************************** Deploy Vesting Contract *****************************************/
    if (!network.vestingAddress) {
        console.log('Deploying Vesting...')
        network.vestingAddress = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
            {
                owner: wallet.key.accAddress,
                token_addr: network.tokenAddress,
            },
        )
        console.log(`Address Vesting Contract: ${network.vestingAddress}`)
    }

    /*************************************** Deploy Staking Contract *****************************************/
    if (!network.stakingAddress) {
        console.log('Deploying Staking...')
        deployConfig.stakingInitMsg.config.deposit_token_addr = network.tokenAddress
        deployConfig.stakingInitMsg.config.token_code_id = network.tokenCodeID
        network.stakingAddress = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
            deployConfig.stakingInitMsg.config
        )
        console.log(`Address Staking Contract: ${network.stakingAddress}`)
    }

    /*************************************** Deploy Generator Contract *****************************************/
    if (!network.generatorAddress) {
        console.log('Deploying Generator...')
        deployConfig.generatorInitMsg.config.astro_token = network.tokenAddress;
        deployConfig.generatorInitMsg.config.vesting_contract = network.vestingAddress;
        deployConfig.generatorInitMsg.config.owner = wallet.key.accAddress;
        network.generatorAddress = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_generator.wasm'),
            deployConfig.generatorInitMsg.config
        )
        console.log(`Address Generator Contract: ${network.generatorAddress}`)

        /*************************************** Setting tokens to Vesting Contract *****************************************/

        console.log('Increase allowance for vesting...')
        await executeContract(terra, wallet, network.tokenAddress, {
            "increase_allowance": {
                spender: network.vestingAddress,
                amount: String("63072000000000")
            }
        })

        console.log('Setting Vesting...')
        const vestingAccounts = (
            deployConfig.registerVestingAccounts.register_vesting_accounts.vesting_accounts
        ).map(account => ({
            ...account,
            address: network.generatorAddress,
        }));
        console.log('vestingAccounts:', JSON.stringify(vestingAccounts))

        deployConfig.registerVestingAccounts.register_vesting_accounts.vesting_accounts = vestingAccounts
        const { registerVestingAccounts } = deployConfig;
        await executeContract(
            terra,
            wallet,
            network.vestingAddress,
            registerVestingAccounts,
        )
    }

    /*************************************** Deploy Factory Contract *****************************************/
    if (!network.factoryAddress) {
        console.log('Deploying Factory...')
        deployConfig.factoryInitMsg.config.generator_address = network.generatorAddress
        deployConfig.factoryInitMsg.config.pair_configs[0].code_id = network.pairCodeID
        deployConfig.factoryInitMsg.config.pair_configs[1].code_id = network.pairStableCodeID
        deployConfig.factoryInitMsg.config.token_code_id = network.tokenCodeID
        console.log(`CodeId Pair Contract: ${network.pairCodeID} CodeId Stable Pair Contract: ${network.pairStableCodeID}`)
        deployConfig.factoryInitMsg.config.owner = wallet.key.accAddress
        network.factoryAddress = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
            deployConfig.factoryInitMsg.config
        )
        console.log(`Address Factory Contract: ${network.factoryAddress}`)
    }

    /*************************************** Deploy Router Contract *****************************************/
    if (!network.routerAddress) {
        console.log('Deploying Router...')
        network.routerAddress = await deployContract(
            terra,
            wallet,
            join(ARTIFACTS_PATH, 'astroport_router.wasm'),
            {
                astroport_factory: network.factoryAddress,
            },
        )
        console.log(`Address Router Contract: ${network.routerAddress}`)
    }

    /*************************************** Deploy Maker Contract *****************************************/
    if (!network.makerAddress) {
        console.log('Deploying Maker...')
        network.makerAddress = await deployContract(
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
        console.log(`Address Maker Contract: ${network.makerAddress}`)
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
