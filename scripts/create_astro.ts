import {
    isTxError,
    LCDClient,
    LocalTerra,
    MsgExecuteContract,
    MsgSend,
    //MsgUpdateContractOwner,
    StdTx,
    Wallet
} from "@terra-money/terra.js"
import { CLIKey } from "@terra-money/terra.js/dist/key/CLIKey.js"
import { strictEqual } from "assert"
import { execSync } from "child_process"
import { unlinkSync, writeFileSync } from "fs"
import 'dotenv/config'
import {
    createTransaction,
    instantiateContract,
    performTransaction,
    queryContract,
    recover,
    setTimeoutDuration,
    uploadContract
} from "./helpers.js"
import {bombay, testnet} from "./deploy_configs";

// Required environment variables:

// All:
const MULTISIG_ADDRESS = process.env.MULTISIG_ADDRESS!
// Name of the multisig in terracli
const MULTISIG_NAME = process.env.MULTISIG_NAME!
// Names of the multisig keys in terracli
const MULTISIG_KEYS = process.env.MULTISIG_KEYS!.split(",")
const MULTISIG_THRESHOLD = parseInt(process.env.MULTISIG_THRESHOLD!)
const CW20_BINARY_PATH = process.env.CW20_BINARY_PATH

// Testnet:
const CHAIN_ID = process.env.CHAIN_ID
const LCD_CLIENT_URL = process.env.LCD_CLIENT_URL
const CW20_CODE_ID = process.env.CW20_CODE_ID
// LocalTerra:




// Main
async function main() {
    let terra: LCDClient | LocalTerra
    let wallet: Wallet
    let cw20CodeID: number

    if (process.env.NETWORK === "testnet" || process.env.NETWORK === "bombay") {
        terra = new LCDClient({
            URL: String(process.env.LCD_CLIENT_URL),
            chainID: String(process.env.CHAIN_ID)
        })
        wallet = recover(terra, process.env.WALLET!)
    } else{
        setTimeoutDuration(0)
        terra = new LocalTerra()
        wallet = (terra as LocalTerra).wallets.test1
    }
    // Upload contract code
    cw20CodeID = await uploadContract(terra, wallet, CW20_BINARY_PATH!)
    console.log(cw20CodeID)
    //const multisig = new Wallet(terra, new CLIKey({ keyName: MULTISIG_NAME }))

    // Token info
    const TOKEN_NAME = "Astro"
    const TOKEN_SYMBOL = "ASTRO"
    const TOKEN_DECIMALS = 6
    // The minter address cannot be changed after the contract is instantiated
    const TOKEN_MINTER =  wallet.key.accAddress
    // The cap cannot be changed after the contract is instantiated
    const TOKEN_CAP = 1_000_000_000_000000
    // TODO check if we want initial balances in prod
    const TOKEN_INITIAL_AMOUNT = 1_000_000_000000
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
    const astroAddress = await instantiateContract(terra, wallet, cw20CodeID, TOKEN_INFO)
    console.log("astro:", astroAddress)
    console.log(await queryContract(terra, astroAddress, { token_info: {} }))
    console.log(await queryContract(terra, astroAddress, { minter: {} }))

    let balance = await queryContract(terra, astroAddress, { balance: { address: TOKEN_INFO.initial_balances[0].address } })
    strictEqual(balance.balance, TOKEN_INFO.initial_balances[0].amount)

    console.log("OK")
}
main().catch(console.log)