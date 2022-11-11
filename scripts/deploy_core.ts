import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    deployContract,
    executeContract,
    uploadContract, instantiateContract, queryContract, toEncodedBinary,
} from './helpers.js'
import { join } from 'path'
import { LCDClient } from '@terra-money/terra.js';
import { chainConfigs } from "./types.d/chain_configs.js";
import { strictEqual } from "assert";

const ARTIFACTS_PATH = '../artifacts'
const SECONDS_IN_DAY: number = 60 * 60 * 24 // min, hour, day

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)

    if (!chainConfigs.generalInfo.multisig) {
        throw new Error("Set the proper owner multisig for the contracts")
    }

    await uploadAndInitToken(terra, wallet)
    await uploadAndInitTreasury(terra, wallet)
    await uploadPairContracts(terra, wallet)
    await uploadAndInitStaking(terra, wallet)
    await uploadAndInitFactory(terra, wallet)
    await uploadAndInitRouter(terra, wallet)
    await uploadAndInitMaker(terra, wallet)

    await uploadAndInitVesting(terra, wallet)
    await uploadAndInitGenerator(terra, wallet)
    await setupVestingAccounts(terra, wallet)
}

async function uploadAndInitToken(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.tokenCodeID) {
        network.tokenCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_token.wasm')!)
        writeArtifact(network, terra.config.chainID)
        console.log(`Token codeId: ${network.tokenCodeID}`)
    }

    if (!network.tokenAddress) {
        chainConfigs.token.admin ||= chainConfigs.generalInfo.multisig
        chainConfigs.token.initMsg.marketing.marketing ||= chainConfigs.generalInfo.multisig

        for (let i = 0; i < chainConfigs.token.initMsg.initial_balances.length; i++) {
            chainConfigs.token.initMsg.initial_balances[i].address ||= chainConfigs.generalInfo.multisig
        }

        console.log('Deploying Token...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.token.admin,
            join(ARTIFACTS_PATH, 'astroport_token.wasm'),
            chainConfigs.token.initMsg,
            chainConfigs.token.label,
        )

        // @ts-ignore
        network.tokenAddress = resp.shift().shift()
        console.log("astro:", network.tokenAddress)
        console.log(await queryContract(terra, network.tokenAddress, { token_info: {} }))
        console.log(await queryContract(terra, network.tokenAddress, { minter: {} }))

        for (let i = 0; i < chainConfigs.token.initMsg.initial_balances.length; i++) {
            let balance = await queryContract(terra, network.tokenAddress, { balance: { address: chainConfigs.token.initMsg.initial_balances[i].address } })
            strictEqual(balance.balance, chainConfigs.token.initMsg.initial_balances[i].amount)
        }

        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadPairContracts(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.pairCodeID) {
        console.log('Register Pair Contract...')
        network.pairCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.pairStableCodeID) {
        console.log('Register Stable Pair Contract...')
        network.pairStableCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_pair_stable.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitStaking(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.xastroTokenCodeID) {
        console.log('Register xASTRO token contract...')
        network.xastroTokenCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_xastro_token.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.stakingAddress) {
        chainConfigs.staking.initMsg.deposit_token_addr ||= network.tokenAddress
        chainConfigs.staking.initMsg.token_code_id ||= network.xastroTokenCodeID
        chainConfigs.staking.initMsg.marketing.marketing ||= chainConfigs.generalInfo.multisig
        chainConfigs.staking.initMsg.owner ||= chainConfigs.generalInfo.multisig
        chainConfigs.staking.admin ||= chainConfigs.generalInfo.multisig

        console.log('Deploying Staking...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.staking.admin,
            join(ARTIFACTS_PATH, 'astroport_staking.wasm'),
            chainConfigs.staking.initMsg,
            chainConfigs.staking.label,
        )

        let addresses = resp.shift()
        // @ts-ignore
        network.stakingAddress = addresses.shift();
        // @ts-ignore
        network.xastroAddress = addresses.shift();

        console.log(`Staking Contract Address: ${network.stakingAddress}`)
        console.log(`xASTRO token Address: ${network.xastroAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitFactory(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.factoryAddress) {
        console.log('Deploying Factory...')
        console.log(`CodeId Pair Contract: ${network.pairCodeID}`)
        console.log(`CodeId Stable Pair Contract: ${network.pairStableCodeID}`)

        for (let i = 0; i < chainConfigs.factory.initMsg.pair_configs.length; i++) {
            if (!chainConfigs.factory.initMsg.pair_configs[i].code_id) {
                if (JSON.stringify(chainConfigs.factory.initMsg.pair_configs[i].pair_type) === JSON.stringify({ xyk: {} })) {
                    chainConfigs.factory.initMsg.pair_configs[i].code_id ||= network.pairCodeID;
                }

                if (JSON.stringify(chainConfigs.factory.initMsg.pair_configs[i].pair_type) === JSON.stringify({ stable: {} })) {
                    chainConfigs.factory.initMsg.pair_configs[i].code_id ||= network.pairStableCodeID;
                }
            }
        }

        chainConfigs.factory.initMsg.token_code_id ||= network.tokenCodeID;
        chainConfigs.factory.initMsg.whitelist_code_id ||= network.whitelistCodeID;
        chainConfigs.factory.initMsg.owner ||= wallet.key.accAddress;
        chainConfigs.factory.admin ||= chainConfigs.generalInfo.multisig;

        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.factory.admin,
            join(ARTIFACTS_PATH, 'astroport_factory.wasm'),
            chainConfigs.factory.initMsg,
            chainConfigs.factory.label
        )

        // @ts-ignore
        network.factoryAddress = resp.shift().shift()
        console.log(`Address Factory Contract: ${network.factoryAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set new owner for factory
        if (chainConfigs.factory.change_owner) {
            console.log('Propose owner for factory. Ownership has to be claimed within %s days',
                Number(chainConfigs.factory.proposeNewOwner.expires_in) / SECONDS_IN_DAY)
            await executeContract(terra, wallet, network.factoryAddress, {
                "propose_new_owner": chainConfigs.factory.proposeNewOwner
            })
        }
    }
}

async function uploadAndInitRouter(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.routerAddress) {
        chainConfigs.router.initMsg.astroport_factory ||= network.factoryAddress
        chainConfigs.router.admin ||= chainConfigs.generalInfo.multisig;

        console.log('Deploying Router...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.router.admin,
            join(ARTIFACTS_PATH, 'astroport_router.wasm'),
            chainConfigs.router.initMsg,
            chainConfigs.router.label
        )

        // @ts-ignore
        network.routerAddress = resp.shift().shift()
        console.log(`Address Router Contract: ${network.routerAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitMaker(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.makerAddress) {
        chainConfigs.maker.initMsg.owner ||= chainConfigs.generalInfo.multisig;
        chainConfigs.maker.initMsg.factory_contract ||= network.factoryAddress;
        chainConfigs.maker.initMsg.staking_contract ||= network.stakingAddress;
        chainConfigs.maker.initMsg.astro_token ||= {
            token: {
                contract_addr: network.tokenAddress
            }
        };
        chainConfigs.maker.admin ||= chainConfigs.generalInfo.multisig;

        console.log('Deploying Maker...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.maker.admin,
            join(ARTIFACTS_PATH, 'astroport_maker.wasm'),
            chainConfigs.maker.initMsg,
            chainConfigs.maker.label
        )

        // @ts-ignore
        network.makerAddress = resp.shift().shift()
        console.log(`Maker Contract Address: ${network.makerAddress}`)
        writeArtifact(network, terra.config.chainID)

        // Set maker address in factory
        console.log('Set the Maker and the proper owner address in the factory')
        await executeContract(terra, wallet, network.factoryAddress, {
            "update_config": {
                fee_address: network.makerAddress
            }
        })
    }
}

async function uploadAndInitTreasury(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.whitelistCodeID) {
        console.log('Register Treasury Contract...')
        network.whitelistCodeID = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, 'astroport_whitelist.wasm')!)
        writeArtifact(network, terra.config.chainID)
    }

    if (!network.treasuryAddress) {
        chainConfigs.treasury.admin ||= chainConfigs.generalInfo.multisig;
        chainConfigs.treasury.initMsg.admins[0] ||= chainConfigs.generalInfo.multisig;

        console.log('Instantiate the Treasury...')
        let resp = await instantiateContract(
            terra,
            wallet,
            chainConfigs.treasury.admin,
            network.whitelistCodeID,
            chainConfigs.treasury.initMsg,
            chainConfigs.treasury.label,
        );

        // @ts-ignore
        network.treasuryAddress = resp.shift().shift()
        console.log(`Treasury Contract Address: ${network.treasuryAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitVesting(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        chainConfigs.vesting.initMsg.vesting_token ||= { token: { contract_addr: network.tokenAddress } };
        chainConfigs.vesting.initMsg.owner ||= chainConfigs.generalInfo.multisig;
        chainConfigs.vesting.admin ||= chainConfigs.generalInfo.multisig;

        console.log('Deploying Vesting...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.vesting.admin,
            join(ARTIFACTS_PATH, 'astroport_vesting.wasm'),
            chainConfigs.vesting.initMsg,
            chainConfigs.vesting.label
        )

        // @ts-ignore
        network.vestingAddress = resp.shift().shift()
        console.log(`Vesting Contract Address: ${network.vestingAddress}`)
        writeArtifact(network, terra.config.chainID)
    }
}

async function uploadAndInitGenerator(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.generatorAddress) {
        chainConfigs.generator.initMsg.astro_token ||= { token: { contract_addr: network.tokenAddress } };
        chainConfigs.generator.initMsg.vesting_contract ||= network.vestingAddress;
        chainConfigs.generator.initMsg.factory ||= network.factoryAddress;
        chainConfigs.generator.initMsg.whitelist_code_id ||= network.whitelistCodeID;
        chainConfigs.generator.initMsg.owner ||= wallet.key.accAddress;
        chainConfigs.generator.admin ||= chainConfigs.generalInfo.multisig;

        console.log('Deploying Generator...')
        let resp = await deployContract(
            terra,
            wallet,
            chainConfigs.generator.admin,
            join(ARTIFACTS_PATH, 'astroport_generator.wasm'),
            chainConfigs.generator.initMsg,
            chainConfigs.generator.label
        )

        // @ts-ignore
        network.generatorAddress = resp.shift().shift()
        console.log(`Generator Contract Address: ${network.generatorAddress}`)

        writeArtifact(network, terra.config.chainID)

        // Set generator address in factory
        await executeContract(terra, wallet, network.factoryAddress, {
            update_config: {
                generator_address: network.generatorAddress,
            }
        })

        // Set new owner for generator
        if (chainConfigs.generator.change_owner) {
            console.log('Propose owner for generator. Ownership has to be claimed within %s days',
                Number(chainConfigs.generator.proposeNewOwner.expires_in) / SECONDS_IN_DAY)
            await executeContract(terra, wallet, network.generatorAddress, {
                "propose_new_owner": chainConfigs.generator.proposeNewOwner
            })
        }

        console.log(await queryContract(terra, network.factoryAddress, { config: {} }))
    }
}

async function setupVestingAccounts(terra: LCDClient, wallet: any) {
    let network = readArtifact(terra.config.chainID)

    if (!network.vestingAddress) {
        throw new Error("Please deploy the vesting contract")
    }

    if (!network.vestingAccountsRegistered) {
        chainConfigs.vesting.registration.msg.register_vesting_accounts.vesting_accounts[0].address = network.generatorAddress;
        console.log('Register vesting accounts:', JSON.stringify(chainConfigs.vesting.registration.msg))
        await executeContract(terra, wallet, network.tokenAddress, {
            "send": {
                contract: network.vestingAddress,
                amount: chainConfigs.vesting.registration.amount,
                msg: toEncodedBinary(chainConfigs.vesting.registration.msg)
            }
        })
        network.vestingAccountsRegistered = true
        writeArtifact(network, terra.config.chainID)
    }

}

await main()