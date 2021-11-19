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

    if (!network.ancAddress) {
        console.log("Instantiate ANC token")
        const TOKEN_ANC_INFO = {
            name: "ANC",
            symbol: "ANC",
            decimals: 6,
            initial_balances: [
                {
                    address: wallet.key.accAddress,
                    amount: "1000000000000000"
                }
            ]
        }
        network.ancAddress = await instantiateContract(terra, wallet, network.tokenCodeID, TOKEN_ANC_INFO)
    }

    if (!network.anc_ust_pair) {
        console.log("Create ANC-UST pool")
        let resp = await executeContract(terra, wallet, network.factoryAddress, {
            "create_pair": {
                asset_infos: [
                    {
                        "token": {
                            contract_addr: network.ancAddress
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
        network.anc_ust_pair = resp.logs[0].eventsByType.from_contract.pair_contract_addr[0]
        console.log(`pair_contract_addr: ${network.anc_ust_pair}`)
    }

    if (!network.psiAddress) {
        console.log("Instantiate PSI token")
        const TOKEN_PSI_INFO = {
            name: "PSI",
            symbol: "PSI",
            decimals: 6,
            initial_balances: [
                {
                    address: wallet.key.accAddress,
                    amount: "1000000000000000"
                }
            ]
        }
        network.psiAddress = await instantiateContract(terra, wallet, network.tokenCodeID, TOKEN_PSI_INFO)
    }

    if (!network.psi_ust_pair) {
        console.log("Create PSI-UST pool")

        let resp = await executeContract(terra, wallet, network.factoryAddress, {
            "create_pair": {
                asset_infos: [
                    {
                        "token": {
                            contract_addr: network.psiAddress
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
        network.psi_ust_pair = resp.logs[0].eventsByType.from_contract.pair_contract_addr[0]
        console.log(`pair_contract_addr: ${network.psi_ust_pair}`)
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

main().catch(console.log)
