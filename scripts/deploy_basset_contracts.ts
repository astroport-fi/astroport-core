import { strictEqual } from "assert"
import {
    newClient,
    writeArtifact,
    readArtifact,
    queryContract,
    uploadContract, performTransaction, deployContract
} from './helpers.js'
import {Coin, LCDClient, MsgInstantiateContract} from "@terra-money/terra.js";
import {join} from "path";

const ARTIFACTS_PATH = '../../anchor-bAsset-contracts/artifacts'

const TOKEN_INITIAL_AMOUNT = String(1_000_000_000_000000)

async function deploy_basset_registry(terra: LCDClient, wallet: any) {
    const network = readArtifact(terra.config.chainID)

    if (!network.bassetRegisterAddress) {
        console.log('Deploying basset register contract...')
        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'anchor_airdrop_registry.wasm'),
            {
                hub_contract: network.bassetHubAddress,
                reward_contract: network.bassetRewardAddress,
            },
        )

        network.bassetRegisterCodeID = resp.contract_codeID
        network.bassetRegisterAddress = resp.reps.shift()
        console.log(`Address basset register contract: ${network.bassetRegisterAddress}`)
        writeArtifact(network, terra.config.chainID)
        console.log('FINISH')
    }
}

async function deploy_basset_reward(terra: LCDClient, wallet: any) {
    const network = readArtifact(terra.config.chainID)

    if (!network.bassetRewardAddress) {
        console.log('Deploying basset reward contract...')
        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'anchor_basset_reward.wasm'),
            {
                reward_denom: "uusd",
                hub_contract: network.bassetHubAddress,
            },
        )

        network.bassetRewardCodeID = resp.contract_codeID
        network.bassetRewardAddress = resp.reps.shift()
        console.log(`Address basset register contract: ${network.bassetRewardAddress}`)
        writeArtifact(network, terra.config.chainID)
        console.log('FINISH')
    }
}

async function deploy_basset_hub(terra: LCDClient, wallet: any) {
    const network = readArtifact(terra.config.chainID)

    if (!network.bassetHubAddress) {
        network.bassetHubCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'anchor_basset_hub.wasm')!)
        console.log(`bassetHubCodeID: ${network.bassetHubCodeID}`)

        const HUB_INFO = {
            epoch_period: 30,
            underlying_coin_denom: "uluna",
            unbonding_period: 210,
            peg_recovery_fee: "0",
            er_threshold: "1",
            reward_denom: "uusd",
            validator: "terravaloper1dcegyrekltswvyy0xy69ydgxn9x8x32zdy3ua5"
        }

        const instantiateMsg = new MsgInstantiateContract(wallet.key.accAddress, network.multisigAddress,
            network.bassetHubCodeID, HUB_INFO, [new Coin("uluna", 1000)]);
        let result = await performTransaction(terra, wallet, instantiateMsg)

        let resp = result.logs[0].events.filter(element => element.type == 'instantiate_contract');
        network.bassetHubAddress = resp[0].attributes.filter(element => element.key == 'contract_address' ).map(x => x.value).shift();

        console.log("basset hub:", network.bassetHubAddress)
        writeArtifact(network, terra.config.chainID)
        console.log('FINISH')
    }
}

async function deploys_basset_token(terra: LCDClient, wallet: any) {
    const network = readArtifact(terra.config.chainID)

    if (!network.tokenBLunaAddress) {
        console.log('Deploying basset token contract...')
        const TOKEN_INFO = {
            name: "BLUNA",
            symbol: "BLUNA",
            decimals: 6,
            initial_balances: [
                {
                    address: wallet.key.accAddress,
                    amount: TOKEN_INITIAL_AMOUNT
                }
            ],
            hub_contract: network.bassetHubAddress
        }

        let resp = await deployContract(
            terra,
            wallet,
            network.multisigAddress,
            join(ARTIFACTS_PATH, 'anchor_basset_token.wasm'),
            TOKEN_INFO,
        )
        console.log(`Address basset token contract: ${network.tokenBLunaAddress}`)
        console.log(`Token codeId: ${network.tokenBLunaCodeID}`)

        network.tokenBLunaCodeID = resp.contract_codeID
        network.tokenBLunaAddress = resp.reps.shift()
        writeArtifact(network, terra.config.chainID)

        let balance = await queryContract(terra, network.tokenBLunaAddress, {balance: {address: TOKEN_INFO.initial_balances[0].address}})
        strictEqual(balance.balance, TOKEN_INFO.initial_balances[0].amount)

        writeArtifact(network, terra.config.chainID)
        console.log('FINISH')
    }
}

async function main() {
    const {terra, wallet} = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    await deploy_basset_hub(terra, wallet);
    await deploys_basset_token(terra, wallet);
    await deploy_basset_reward(terra, wallet);
    await deploy_basset_registry(terra, wallet);
}
main().catch(console.log)
