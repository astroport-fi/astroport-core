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

    if (!network.tokenAddress) {
        console.log(`Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`)
        return
    }

    console.log("create astro-ust pair")
    let resp = await executeContract(terra, wallet, network.factoryAddress, {
        "create_pair": {
            asset_infos: [
                {
                    "token": {
                        contract_addr: network.tokenAddress
                    }
                },
                {
                    "native_token": {
                        denom: "uusd"
                    }
                }
            ]

        }
    })
    console.log("pair successfully created!")
    network.astro_ust_pair = resp.logs[0].eventsByType.from_contract.pair_contract_addr[0]
    console.log(`pair_contract_addr: ${network.astro_ust_pair}`)

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
