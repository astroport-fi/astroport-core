import { strictEqual } from "assert"
import {
    newClient,
    writeArtifact,
    readArtifact,
    instantiateContract,
    queryContract,
    uploadContract
} from './helpers.js'

const CW20_BINARY_PATH = process.env.CW20_BINARY_PATH || '../artifacts/astroport_token.wasm'

// Main
async function main() {
    const {terra, wallet} = newClient()
    const network = readArtifact(terra.config.chainID)
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    // Upload contract code
    network.tokenCodeID = await uploadContract(terra, wallet, CW20_BINARY_PATH!)
    console.log(`Token codeId: ${network.tokenCodeID}`)
    // Token info
    const TOKEN_NAME = "Astro"
    const TOKEN_SYMBOL = "ASTRO"
    const TOKEN_DECIMALS = 6
    // The minter address cannot be changed after the contract is instantiated
    const TOKEN_MINTER =  wallet.key.accAddress
    // The cap cannot be changed after the contract is instantiated
    const TOKEN_CAP = 1_000_000_000_000000
    // TODO check if we want initial balances in prod
    const TOKEN_INITIAL_AMOUNT = 1_000_000_000_000000
    const TOKEN_INITIAL_AMOUNT_ADDRESS = TOKEN_MINTER

    const TOKEN_INFO = {
        name: TOKEN_NAME,
        symbol: TOKEN_SYMBOL,
        decimals: TOKEN_DECIMALS,
        initial_balances: [
            {
                address: TOKEN_INITIAL_AMOUNT_ADDRESS,
                amount: String(TOKEN_INITIAL_AMOUNT)
            }
        ],
        mint: {
            minter: TOKEN_MINTER,
            cap: String(TOKEN_CAP)
        }
    }

    // Instantiate Astro token contract
    network.tokenAddress = await instantiateContract(terra, wallet, network.tokenCodeID, TOKEN_INFO)
    console.log("astro:", network.tokenAddress)
    console.log(await queryContract(terra, network.tokenAddress, { token_info: {} }))
    console.log(await queryContract(terra, network.tokenAddress, { minter: {} }))

    let balance = await queryContract(terra, network.tokenAddress, { balance: { address: TOKEN_INFO.initial_balances[0].address } })
    strictEqual(balance.balance, TOKEN_INFO.initial_balances[0].amount)

    writeArtifact(network, terra.config.chainID)
    console.log('FINISH')
}
main().catch(console.log)
