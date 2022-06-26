import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract, instantiateContract,
} from './helpers.js'
import { join } from 'path'
import {LCDClient} from '@terra-money/terra.js';

const ARTIFACTS_PATH = '../artifacts'

const STAKING_LABEL = "Astroport Staking"
const FACTORY_LABEL = "Astroport Factory"
const ROUTER_LABEL = "Astroport Router"
const MAKER_LABEL = "Astroport Maker"
const WHITELIST_LABEL = "Astroport Treasury"

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    let network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.tokenAddress) {
        console.log(`Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`)
        return
    }

    if (!network.multisigAddress) {
        console.log(`set the proper owner multisig for the contracts`)
        return
    }

    await uploadAndInitTreasury(terra, wallet)
    await uploadPairContracts(terra, wallet)
    await uploadAndInitStaking(terra, wallet)
    await uploadAndInitFactory(terra, wallet)
    await uploadAndInitRouter(terra, wallet)
    await uploadAndInitMaker(terra, wallet)

    // // Set new owner for admin
    // network = readArtifact(terra.config.chainID) // reload variables
    // console.log('Propose owner for factory. Onwership has to be claimed within 7 days')
    // await executeContract(terra, wallet, network.factoryAddress, {
    //     "propose_new_owner": {
    //         owner: network.multisigAddress,
    //         expires_in: 604800 // 7 days
    //     }
    // })


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
        console.log('Deploying Staking...')

        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
            {
                owner: network.multisigAddress,
                token_code_id: network.xastroTokenCodeID,
                deposit_token_addr:  network.tokenAddress,
                marketing: {
                    project: "Astroport",
                    description: "Astroport is a neutral marketplace where anyone, from anywhere in the galaxy, can dock to trade their wares.",
                    marketing: wallet.key.accAddress,
                    logo: {
                        url: "https://app.astroport.fi/tokens/xAstro.svg"
                    }
                }
            },
            STAKING_LABEL
        )

        let addresses = resp.shift()
        // @ts-ignore
        network.stakingAddress = addresses.shift();
        // @ts-ignore
        network.xastroAddress = addresses.shift();

        console.log(`Address Staking Contract: ${network.stakingAddress}`)
        console.log(`Address xASTRO Contract: ${network.xastroAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitFactory(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.factoryAddress) {
        console.log('Deploying Factory...')
        console.log(`CodeId Pair Contract: ${network.pairCodeID} CodeId Stable Pair Contract: ${network.pairStableCodeID}`)

        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
            {
                owner: wallet.key.accAddress, // We don't set multisig as owner, as we need to update maker address once it is deployed
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
                generator_address: undefined,
                fee_address: undefined,
                whitelist_code_id: network.whitelistCodeID
            },
            FACTORY_LABEL
        )
        // @ts-ignore
        network.factoryAddress = resp.shift().shift()
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
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_router.wasm'),
            {
                astroport_factory: network.factoryAddress,
            },
            ROUTER_LABEL
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
        console.log('Deploying Maker...')
        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'astroport_maker.wasm'),
            {
                owner: network.multisigAddress,
                factory_contract: String(network.factoryAddress),
                staking_contract: String(network.stakingAddress),
                astro_token_contract: String(network.tokenAddress),
                governance_contract: undefined,
                governance_percent: undefined,
                max_spread: "0.5"
            },
            MAKER_LABEL
        )
        // @ts-ignore
        network.makerAddress = resp.shift().shift()
        console.log(`Address Maker Contract: ${network.makerAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set maker address in factory
        console.log('Set maker and proper owner address in factory')
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
            network.multisigAddress,
            network.whitelistCodeID,
            {
                admins: [network.multisigAddress],
                mutable: true
            },
            WHITELIST_LABEL
            );
        // @ts-ignore
        network.whitelistAddress = resp.shift().shift()
        console.log(`Whitelist Contract Address: ${network.whitelistAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

main().catch(console.log)
