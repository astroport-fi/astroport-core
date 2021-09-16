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

const ARTIFACTS_PATH = "../artifacts/"

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
    // deployConfig.pairConfig.code_id = pairCodeID
    //
    let pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
    // deployConfig.pairStableConfig.code_id = pairStableCodeID
    console.log("Code Pair Contract: " + pairCodeID + " Code Stable Pair Contract: " + pairStableCodeID)

    /*************************************** Deploy Factory Contract *****************************************/
    // console.log("Deploying Factory...")
    // let CodeID = await uploadContract(terra, wallet, "../artifacts/astroport_factory.wasm"!)
    // console.log(CodeID)
    // const INIT_MSG = {
    //     pair_configs: [
    //         {
    //             code_id: String(pairCodeID),
    //             pair_type: {xyk:{}},
    //             total_fee_bps: String(0),
    //             maker_fee_bps: String(0),
    //         },
    //         {
    //             code_id: String(pairStableCodeID),
    //             pair_type: {stable:{}},
    //             total_fee_bps: String(0),
    //             maker_fee_bps: String(0),
    //         }
    //     ],
    //     token_code_id: String(process.env.CW20_CODE_ID),
    //     init_hook: undefined,
    //     fee_address: undefined,
    // }
    // const addressFactoryContract = await instantiateContract(terra, wallet, CodeID, INIT_MSG)
    const addressFactoryContract = deployConfig.astroTokenContractAddress;
    // const addressFactoryContract = await deployContract(
    //     terra,
    //     wallet,
    //     join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
    //     {
    //         pair_configs: [{
    //             code_id: String(pairCodeID),
    //             pair_type: { xyk: {} },
    //             total_fee_bps: String(0),
    //             maker_fee_bps: String(0),
    //         } ],
    //         token_code_id: String(process.env.CW20_CODE_ID),
    //         fee_address: undefined,
    //         init_hook: undefined,
    //     },
    // )
    console.log("Address Factory Contract: " + addressFactoryContract)

    // /*************************************** Deploy Router Contract *****************************************/
    console.log("Deploying Router...")
    const addressRouterContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_router.wasm'),
        {
            astroport_factory: addressFactoryContract,
        },
    )
    console.log("Address Vesting Contract: " + addressRouterContract)
    /*************************************** Deploy Vesting Contract *****************************************/
    console.log("Deploying Vesting...")
    const addressVestingContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
        {
            owner: wallet.key.accAddress,
            token_addr: deployConfig.astroTokenContractAddress,
            genesis_time: String(Date.now())
        },
    )
    console.log("Address Vesting Contract: " + addressVestingContract)

    /*************************************** Deploy Staking Contract *****************************************/
    console.log("Deploying Staking...")
    const addressStakingContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
        {
            token_code_id: String(process.env.CW20_CODE_ID),
            deposit_token_addr: deployConfig.astroTokenContractAddress,
        },
    )
    console.log("Address Staking Contract: " + addressStakingContract)


    /*************************************** Deploy Gauge Contract *****************************************/
    console.log("Deploying Gauge...")
    const addressGaugeContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_gauge.wasm'),
        {
            token: deployConfig.astroTokenContractAddress,
            dev_addr: wallet.key.accAddress,
            tokens_per_block: String(100),
            start_block: String(100000),
            bonus_end_block: String(500000),
        }
    )
    console.log("Address Gauge Contract: " + addressGaugeContract)

    console.log("FINISH")
}

main().catch(console.log)