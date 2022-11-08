import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    uploadContract, queryContract,
} from './helpers.js'
import { join } from 'path'
import { chainConfigs } from "./types.d/chain_configs.js";
import { strictEqual } from "assert";

const ARTIFACTS_PATH = '../artifacts'

// This script is mainly used for test purposes to deploy a token for further pool deployment

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    if (!chainConfigs.generalInfo.multisig) {
        throw new Error("Set the proper owner multisig for the contracts")
    }

    let network = readArtifact(terra.config.chainID)

    if (!network.tokenCodeID) {
        network.tokenCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_token.wasm')!)
        writeArtifact(network, terra.config.chainID)
        console.log(`Token codeId: ${network.tokenCodeID}`)
    }

    let msg = {
        admin: chainConfigs.generalInfo.multisig,
        initMsg: {
            name: "Test 1",
            symbol: "TEST-T",
            decimals: 6,
            initial_balances: [
                {
                    address: chainConfigs.generalInfo.multisig,
                    amount: "1000000000000000"
                }
            ],
            mint: {
                minter: chainConfigs.generalInfo.multisig
            }
        },
        label: "Test token"
    };

    console.log(`Deploying Token ${msg.initMsg.symbol}...`)
    let resp = await deployContract(
        terra,
        wallet,
        wallet.key.accAddress,
        join(ARTIFACTS_PATH, 'astroport_token.wasm'),
        msg.initMsg,
        msg.label,
    )

    // @ts-ignore
    let tokenAddress: string = resp.shift().shift()
    console.log("Token address:", tokenAddress)
    console.log(await queryContract(terra, tokenAddress, { token_info: {} }))
    console.log(await queryContract(terra, tokenAddress, { minter: {} }))

    for (let i = 0; i < msg.initMsg.initial_balances.length; i++) {
        let balance = await queryContract(terra, tokenAddress, { balance: { address: msg.initMsg.initial_balances[i].address } })
        strictEqual(balance.balance, msg.initMsg.initial_balances[i].amount)
    }

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}

await main()
