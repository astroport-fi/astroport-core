import 'dotenv/config'
import { strictEqual } from "assert"
import {
    Client,
    newClient,
    instantiateContract,
    queryContract,
    uploadContract, writeNetworkConfig, readNetworkConfig, executeContract,
} from './helpers.js'
import {configDefault} from "./deploy_configs.js";
import {join} from "path";
import {
    readdirSync,
} from 'fs'

async function uploadContracts(cl: Client) {
    const artifacts = readdirSync(process.env.ARTIFACTS_PATH!);
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    for(let i=0; i<artifacts.length; i++){
        if (artifacts[i].split('.').pop() == process.env.ARTIFACTS_EXTENSIONS!) {
            let codeID = await uploadContract(cl.terra, cl.wallet,
                join(process.env.ARTIFACTS_PATH!, artifacts[i])
            );
            console.log(`Contract: ${artifacts[i].split('.')[0]} was uploaded.\nStore code: ${codeID}`);
            networkConfig[`${artifacts[i].split('.')[0].split('_').pop()}`] = {}
            networkConfig[`${artifacts[i].split('.')[0].split('_').pop()}`][`ID`] = codeID;
        }
    }
    writeNetworkConfig(networkConfig, cl.terra.config.chainID)
    console.log('upload contracts ---> FINISH')
}

async function setupToken(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.token.Addr) {
        if (!cfg.tokenConfig.configInitMsg.initial_balances[0].address) {
            cfg.tokenConfig.configInitMsg.initial_balances[0].address = cl.wallet.key.accAddress
        }

        if (!cfg.tokenConfig.configInitMsg.mint.minter) {
            cfg.tokenConfig.configInitMsg.mint.minter = cl.wallet.key.accAddress
        }

        networkConfig.token.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.token.ID,
            cfg.tokenConfig.configInitMsg
        );

        let balance = await queryContract(cl.terra, networkConfig.token.Addr, {
            balance: {address: cl.wallet.key.accAddress}
        })

        // Validate token balance
        strictEqual(balance.balance, process.env.TOKEN_INITIAL_AMOUNT!)
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup token ---> FINISH')
    }
}

async function setupFactory(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.factory.Addr) {
        cfg.factoryConfig.configInitMsg.pair_configs[0].code_id = networkConfig.pair.ID
        cfg.factoryConfig.configInitMsg.pair_configs[1].code_id = networkConfig.stable.ID
        cfg.factoryConfig.configInitMsg.token_code_id = networkConfig.token.ID

        networkConfig.factory.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.factory.ID,
            cfg.factoryConfig.configInitMsg
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup factory ---> FINISH')
    }
}

async function setupRouter(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.router.Addr) {
        cfg.routerConfig.configInitMsg.astroport_factory = networkConfig.factory.Addr

        networkConfig.router.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.router.ID,
            cfg.routerConfig.configInitMsg
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup router ---> FINISH')
    }
}

async function setupVesting(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.vesting.Addr) {
        cfg.vestingConfig.configInitMsg.token_addr = networkConfig.token.Addr
        cfg.vestingConfig.configInitMsg.owner = cl.wallet.key.accAddress

        networkConfig.vesting.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.vesting.ID,
            cfg.vestingConfig.configInitMsg
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup vesting ---> FINISH')
    }
}

async function setupStaking(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.staking.Addr) {
        cfg.stakingConfig.configInitMsg.config.token_code_id = networkConfig.token.ID
        cfg.stakingConfig.configInitMsg.config.deposit_token_addr = process.env.TOKEN_DEPOSIT_ADDRESS! || networkConfig.token.Addr

        networkConfig.staking.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.staking.ID,
            cfg.stakingConfig.configInitMsg.config
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup staking ---> FINISH')
    }
}

async function setupMaker(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.maker.Addr) {
        cfg.makerConfig.configInitMsg.factory_contract = networkConfig.factory.Addr
        cfg.makerConfig.configInitMsg.staking_contract = networkConfig.staking.Addr
        cfg.makerConfig.configInitMsg.astro_token_contract = networkConfig.token.Addr

        networkConfig.maker.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.maker.ID,
            cfg.makerConfig.configInitMsg
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup maker ---> FINISH')
    }
}

async function setupGenerator(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    if (!networkConfig.generator.Addr) {
        cfg.generatorConfig.configInitMsg.config.astro_token = networkConfig.token.Addr
        cfg.generatorConfig.configInitMsg.config.vesting_contract = networkConfig.vesting.Addr

        networkConfig.generator.Addr = await instantiateContract(
            cl.terra,
            cl.wallet,
            networkConfig.generator.ID,
            cfg.generatorConfig.configInitMsg.config
        );
        writeNetworkConfig(networkConfig, cl.terra.config.chainID)
        console.log('setup generator ---> FINISH')
    }
}

async function settingTokensToVesting(cl: Client, cfg: Config) {
    const networkConfig = readNetworkConfig(cl.terra.config.chainID);

    const vestingAccounts = (
        cfg.registerVestingAccounts.register_vesting_accounts.vesting_accounts
    ).map((account: any) => ({
        ...account,
        address: networkConfig.generator.Addr,
    }));

    console.log('vestingAccounts:', JSON.stringify(vestingAccounts))
    // INCREASE ALLOWANCE
    let msg = { increase_allowance: { spender: networkConfig.vesting.Addr, amount: '63072000000000' } }
    let out = await executeContract(cl.terra, cl.wallet, networkConfig.token.Addr, msg)
    console.log(out.txhash)

    cfg.registerVestingAccounts.register_vesting_accounts.vesting_accounts = vestingAccounts
    const { registerVestingAccounts } = cfg;
    await executeContract(
        cl.terra,
        cl.wallet,
        networkConfig.vesting.Addr,
        registerVestingAccounts,
    )

    console.log('setting tokens to vesting ---> FINISH')
}

async function main() {
    const client = newClient();
    let config: Config = configDefault

    await uploadContracts(client);
    await setupToken(client, config);
    await setupFactory(client, config);
    await setupRouter(client, config);
    await setupVesting(client, config);
    await setupStaking(client, config);
    await setupMaker(client, config);
    await setupGenerator(client, config);
    await settingTokensToVesting(client, config);
}
main().catch(console.log)
