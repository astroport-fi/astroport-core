import 'dotenv/config'
import {
    deployContract,
    executeContract,
    recover,
    setTimeoutDuration,
    uploadContract,
} from "./helpers.js"
import { LCDClient, LocalTerra, Wallet } from "@terra-money/terra.js"
import { testnet, bombay, local } from './deploy_configs.js';
import { join } from "path"

const ARTIFACTS_PATH = "../artifacts/"

async function main() {
    let terra: LCDClient | LocalTerra
    let wallet: Wallet
    let deployConfig: Config

    if (process.env.NETWORK === "testnet") {
        terra = new LCDClient({
            URL: String(process.env.LCD_CLIENT_URL),
            chainID: String(process.env.CHAIN_ID)
        })
        wallet = recover(terra, process.env.WALLET!)
        deployConfig = testnet

    } else if (process.env.NETWORK === "bombay") {
        terra = new LCDClient({
            URL: String(process.env.LCD_CLIENT_URL),
            chainID: String(process.env.CHAIN_ID)
        })
        wallet = recover(terra, process.env.WALLET!)
        deployConfig = bombay
    } else {
        console.log("NETWORK:" + process.env.NETWORK)
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
    deployConfig.factoryInitMsg.config.pair_configs[0].code_id = pairCodeID

    let pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
    deployConfig.factoryInitMsg.config.pair_configs[1].code_id = pairStableCodeID
    console.log("CodeIs Pair Contract: " + pairCodeID + " CodeId Stable Pair Contract: " + pairStableCodeID)

    /*************************************** Deploy Factory Contract *****************************************/
    console.log("Deploying Factory...")
    deployConfig.factoryInitMsg.config.gov = wallet.key.accAddress
    const addressFactoryContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
        deployConfig.factoryInitMsg.config
    )
    console.log("Address Factory Contract: " + addressFactoryContract)

    /*************************************** Deploy Router Contract *****************************************/
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
        },
    )
    console.log("Address Vesting Contract: " + addressVestingContract)
    /*************************************** Deploy Staking Contract *****************************************/
    console.log("Deploying Staking...")
    const addressStakingContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
        deployConfig.stakingInitMsg.config
    )
    console.log("Address Staking Contract: " + addressStakingContract)
    /*************************************** Deploy Maker Contract *****************************************/
    console.log("Deploying Maker...")
    const addressMakerContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_maker.wasm'),
        {
            factory_contract: String(addressFactoryContract),
            staking_contract: String(addressStakingContract),
            astro_token_contract: String(deployConfig.astroTokenContractAddress),
        }
    )
    console.log("Address Maker Contract: " + addressMakerContract)
    /*************************************** Deploy Generator Contract *****************************************/
    console.log("Deploying Generator...")
    deployConfig.generatorInitMsg.config.astro_token = deployConfig.astroTokenContractAddress;
    deployConfig.generatorInitMsg.config.vesting_contract = addressVestingContract;
    const addressGeneratorContract = await deployContract(
        terra,
        wallet,
        join(ARTIFACTS_PATH, 'astroport_generator.wasm'),
        deployConfig.generatorInitMsg.config
    )
    console.log("Address Generator Contract: " + addressGeneratorContract)

    /*************************************** Setting tokens to Vesting Contract *****************************************/
    console.log("Setting Vesting...")
    const vestingAccounts = (
        deployConfig.registerVestingAccounts.register_vesting_accounts.vesting_accounts
    ).map(account => ({
        ...account,
        address: addressGeneratorContract,
    }));
    console.log('vestingAccounts:', JSON.stringify(vestingAccounts))
    // INCREASE ALLOWANCE
    let out: any, msg: any
    msg = { increase_allowance: { spender: addressVestingContract, amount: '63072000000000' } }
    console.log('execute', deployConfig.astroTokenContractAddress, JSON.stringify(msg))
    out = await executeContract(terra, wallet, deployConfig.astroTokenContractAddress, msg)
    console.log(out.txhash)

    deployConfig.registerVestingAccounts.register_vesting_accounts.vesting_accounts = vestingAccounts
    const { registerVestingAccounts } = deployConfig;
    await executeContract(
        terra,
        wallet,
        addressVestingContract,
        registerVestingAccounts,
    )
    console.log("FINISH")
}

main().catch(console.log)
