import {
    LCDClient,
    LocalTerra,
    Wallet
} from "@terra-money/terra.js"
import 'dotenv/config'
import {
    recover,
    setTimeoutDuration,
    migrate,
    uploadContract
} from "./helpers.js"
import { join } from "path"

import {testnet, bombay, local} from './migrate_configs.js';

const ARTIFACTS_PATH = "../artifacts/"

async function main() {
    let terra: LCDClient | LocalTerra
    let wallet: Wallet
    let deployConfig: Migrate = testnet
    let contractAddress = String(process.env.CONTRACT_ADDRESS)

    if (process.env.NETWORK === "testnet" || process.env.NETWORK === "bombay") {
        terra = new LCDClient({
            URL: String( process.env.LCD_CLIENT_URL),
            chainID: String( process.env.CHAIN_ID)
        })
        wallet = recover(terra, process.env.WALLET!)
        if (process.env.NETWORK === "bombay"){
            deployConfig = bombay
        }
    } else {
        setTimeoutDuration(0)
        terra = new LocalTerra()
        wallet = (terra as LocalTerra).wallets.test1
        deployConfig = local
    }
    for (const el of deployConfig.contracts) {
        if ( el.migrate ) {
            console.log("uploading...");
            const newCodeId = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, el.filepath)!);

            console.log('migrating...');
            const migrateResult = await migrate(terra, wallet, el.address, newCodeId);

            console.log("migration complete: ");
            console.log(migrateResult);
        } else {
            console.log( `contract ${el.filepath} skip migrate` )
        }
    }
    console.log("OK")
}
main().catch(console.log)
