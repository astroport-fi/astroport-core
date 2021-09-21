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
    deployConfig.pairConfig.code_id = pairCodeID

    let pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
    deployConfig.pairStableConfig.code_id = pairStableCodeID
    console.log("Code Pair Contract: " + pairCodeID + " Code Stable Pair Contract: " + pairStableCodeID)

    /*************************************** Deploy Factory Contract *****************************************/
    console.log("Deploying Factory...")
    const INIT_MSG = {
        pair_configs: [
            {
                code_id: pairCodeID,
                pair_type: { xyk: {}},
                total_fee_bps: 0,
                maker_fee_bps: 0
            },
            {
                code_id: pairStableCodeID,
                pair_type: {stable:{}},
                total_fee_bps: 0,
                maker_fee_bps: 0
            }
        ],
        token_code_id: Number(process.env.CW20_CODE_ID)
    }
     // const addressFactoryContract = await instantiateContract(terra, wallet, CodeID, INIT_MSG)
    // const addressFactoryContract = deployConfig.astroTokenContractAddress;
    const addressFactoryContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
        INIT_MSG
    )
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
    console.log("Address Router Contract: " + addressRouterContract)

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
            token_code_id: Number(process.env.CW20_CODE_ID),
            deposit_token_addr: deployConfig.astroTokenContractAddress,
        },
    )
    console.log("Address Staking Contract: " + addressStakingContract)


    /*************************************** Deploy Maker Contract *****************************************/
    console.log("Deploying Maker...")
    const addressMakerContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'maker.wasm'),
        {
            factory_contract: String(addressFactoryContract),
            staking_contract: String(addressStakingContract),
            astro_token_contract: String(deployConfig.astroTokenContractAddress),
        }
    )
    console.log("Address Gauge Contract: " + addressMakerContract)
    /*************************************** Deploy Gauge Contract *****************************************/
    console.log("Deploying Gauge...")
    const addressGaugeContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_gauge.wasm'),
        {
            token: deployConfig.astroTokenContractAddress,
            dev_addr: wallet.key.accAddress,
            tokens_per_block: String(10000000),
            start_block: 100000,
            bonus_end_block: 500000,
        }
    )
    console.log("Address Gauge Contract: " + addressGaugeContract)


    /*************************************** Setting tokens to Vesting Contract *****************************************/
    console.log("Setting Vesting...")
    await executeContract(
        terra,
        wallet,
        addressVestingContract,
        {register_vesting_accounts:{
            vesting_accounts: [
                {
                    address: addressGaugeContract,
                    schedules:[
                        [ String(new Date( 2022, 1, 1).getTime()), String(new Date( 2023, 1, 1).getTime()), String(1)],
                        [ String(new Date( 2022, 6, 1).getTime()), String(new Date( 2023, 1, 1).getTime()), String(1)],
                        [ String(new Date( 2023, 1, 1).getTime()), String(new Date( 2024, 1, 1).getTime()), String(1)],
                    ]
                }
            ]
        }
        })

    // const addressVestingContract = "terra1kyl8f2xkd63cga8szgkejdyvxay7mc7qpdc3c5"
    // const addressGaugeContract = "terra1qjrvlf27upqhqnrqmmu2y205ed2c3tc87dnku3"

    //console.log("Vesting accounts setup: ", await queryContract(terra, addressVestingContract, { "vesting_accounts": { } }))

    console.log("FINISH")
}

main().catch(console.log)