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

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (!network.mirAddress) {
        console.log("Instantiate MIR token")
        const TOKEN_MIR_INFO = {
            name: "MIR",
            symbol: "MIR",
            decimals: 6,
            initial_balances: [
                {
                    address: wallet.key.accAddress,
                    amount: "1000000000000000"
                }
            ]
        }
        network.mirAddress = await instantiateContract(terra, wallet, network.tokenCodeID, TOKEN_MIR_INFO)
    }

    if (!network.mir_ust_pair) {
        console.log("Create MIR-UST pool")
        let resp = await executeContract(terra, wallet, network.factoryAddress, {
            "create_pair": {
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

            }
        })
        console.log("pair successfully created!")
        network.mir_ust_pair = resp.logs[0].eventsByType.from_contract.pair_contract_addr[0]
        console.log(`pair_contract_addr: ${network.mir_ust_pair}`)
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
