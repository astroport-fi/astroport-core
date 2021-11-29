import { strictEqual } from "assert"
import {
    newClient,
    readArtifact,
    queryContract,
} from "../helpers.js"

// Main
async function main() {
    const {terra, wallet} = newClient()
    const network = readArtifact(terra.config.chainID)
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    // deploying configs
    const VESTING_CONF = {
        owner: wallet.key.accAddress,
        token_addr: network.tokenAddress
    }

    // query vesting config
    let vestingResponse = await queryContract(terra, network.vestingAddress, { config: {} })
    console.log("vesting response: ", vestingResponse)
    strictEqual(VESTING_CONF.owner, vestingResponse.owner)
    strictEqual(VESTING_CONF.token_addr, vestingResponse.token_addr)

    console.log('FINISH')
}
main().catch(console.log)
