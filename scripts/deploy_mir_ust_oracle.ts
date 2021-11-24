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

    if (!network.mir_ust_pair) {
        console.log(`Please deploy mir_ust_pair first, or set this address before running this script...`)
        return
    }


    if (!network.mir_ust_oracle) {
        console.log("Deploying MIR-UST oracle...")
        network.mir_ust_oracle = await deployContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_oracle.wasm'), {
            factory_contract: network.factoryAddress,
            asset_infos: [
                {
                    "token": {
                        contract_addr: network.mirAddress
                    }
                },
                {
                    "native_token": {
                        denom: "uusd"
                    }
                }
            ]
        })
        console.log(`Address of the deployed contract: ${network.mir_ust_oracle}`)
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
