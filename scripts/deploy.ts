import 'dotenv/config'
import {
    deployContract,
    executeContract,
    instantiateContract,
    queryContract,
    recover,
    setTimeoutDuration,
    uploadContract,
} from "./helpers.js"
import { LCDClient, LocalTerra, Wallet } from "@terra-money/terra.js"
import {testnet, local} from './deploy_configs.js';
import { join } from "path"

const ARTIFACTS_PATH = "../artifacts"

async function main() {
    let terra: LCDClient | LocalTerra
    let wallet: Wallet
    let deployConfig: Config

    if (process.env.NETWORK === "testnet") {
        terra = new LCDClient({
            URL: 'https://tequila-lcd.terra.dev',
            chainID: 'tequila-0004'
        })
        wallet = recover(terra, process.env.TEST_MAIN!)
        deployConfig = testnet
    } else {
        console.log("NETWORK:" +process.env.NETWORK)
        terra = new LocalTerra()
        wallet = (terra as LocalTerra).wallets.test1
        setTimeoutDuration(0)
        deployConfig = local
    }
    console.log(`Wallet address from seed: ${wallet.key.accAddress}`)

    if (!deployConfig.astroTokenContractAddress) {
        console.log(`Please deploy the CW20-base ASTRO token, and then set this address in the deploy config before running this script...`)
        return
    }

    /*************************************** Register Pairs Contract *****************************************/
    console.log("Register Pairs Contract...")
    let pairCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair.wasm')!)
    deployConfig.pairConfig.code_id = pairCodeID;

    let pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair.wasm')!)
    deployConfig.pairStableConfig.code_id = pairStableCodeID;
    console.log("Code Pair Contract: " + pairCodeID + " Code Stable Pair Contract: " + pairStableCodeID)

    /*************************************** Deploy Factory Contract *****************************************/
    console.log("Deploying Factory...")
    const addressFactoryContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
        {
            "pair_configs": [deployConfig.pairConfig, deployConfig.pairStableConfig],
            "token_code_id": process.env.CW20_CODE_ID,
        },
    )
    console.log("Address Vesting Contract: " + addressFactoryContract)


    /*************************************** Deploy Vesting Contract *****************************************/
    // console.log("Deploying Vesting...")
    // const addressVestingContract = await deployContract(
    //     terra,
    //     wallet,
    //     join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
    //     {
    //         "owner": wallet.key.accAddress,
    //         "token_addr": deployConfig.astroTokenContractAddress,
    //         "genesis_time": Date.now()
    //     },
    // )
    // console.log("Address Vesting Contract: " + addressVestingContract)

    console.log("FINISH")
}

main().catch(console.log)