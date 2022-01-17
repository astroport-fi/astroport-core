import { strictEqual } from "assert"
import {
    newClient,
    readArtifact,
    queryContract,
    executeContract, toEncodedBinary
} from './helpers.js'
import {Coin} from "@terra-money/terra.js";
import * as util from "util";

function provide_liquidity_msg_token_to_native(asset1: String, amount1: String, asset2: String, amount2: String) {
    let msg = {
        provide_liquidity: {
            assets: [
                {
                    info: {
                        token: {
                            contract_addr: asset1
                        }
                    },
                    amount: amount1,
                },
                {
                    info: {
                        native_token: {
                            denom: asset2
                        }
                    },
                    amount: amount2,
                },
            ]
        }
    }

    return msg
}

async function main() {
    const {terra, wallet} = newClient()
    const network = readArtifact(terra.config.chainID)
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    await executeContract(terra, wallet, network.bassetHubAddress, {
        "update_config": {
            "reward_contract": network.bassetRewardAddress,
        }
    })

    await executeContract(terra, wallet, network.tokenBLunaAddress, {
        "increase_allowance": {
            "spender": network.bassetHubAddress,
            "amount": "100000",
            "expires": {
                "never": {}
            }
        }
    })

    await executeContract(terra, wallet, network.tokenBLunaAddress, {
        "increase_allowance": {
            "spender": network.bassetRewardAddress,
            "amount": "100000",
            "expires": {
                "never": {}
            }
        }
    })

    let balance_rewarder = await queryContract(terra, network.tokenBLunaAddress, { balance: { address: network.bassetRewardAddress } })
    console.log("balance rewarder: ", balance_rewarder);

    let balance_hub = await queryContract(terra, network.tokenBLunaAddress, { balance: { address: network.bassetHubAddress } })
    console.log("balance rewarder: ", balance_hub);

    console.log(await queryContract(terra, network.bassetHubAddress, { config: {} }))

    console.log(await queryContract(terra, network.tokenBLunaAddress, { token_info: {} }))
    console.log(await queryContract(terra, network.tokenBLunaAddress, { minter: {} }))

    let balance2 = await queryContract(terra, network.tokenBLunaAddress, { balance: { address: wallet.key.accAddress } })
    strictEqual(balance2.balance, "1000000000000000")

    console.log('Setting allowance for contract')
    await executeContract(terra, wallet, network.tokenBLunaAddress, {
        "increase_allowance": {
            "spender": network.poolBlunaLuna,
            "amount": "100000",
            "expires": {
                "never": {}
            }
        }
    })

    let resp = await queryContract(terra, network.poolBlunaLuna, { pool: {} })
    console.log(util.inspect(resp, true, null))

    // provide liquidity
    let msg = provide_liquidity_msg_token_to_native(network.tokenBLunaAddress, "10", "uusd", "10");
    try {
        let resp = await executeContract(terra, wallet, network.poolBlunaLuna, msg, [new Coin("uusd", 10)])
        console.log(resp);
    } catch (e) {
       console.log(e)
    }

    // withdraw liquidity
    try {
        let resp = await executeContract(terra, wallet, network.lpTokenBlunaLuna,
            {"send": {
                    "contract": network.poolBlunaLuna,
                    "amount": "10",
                    "msg": toEncodedBinary({"withdraw_liquidity": {}})
                }
            })
        console.log(util.inspect(resp, true, null));
    } catch (e) {
        console.log(e)
    }
    console.log('FINISH')
}
main().catch(console.log)
