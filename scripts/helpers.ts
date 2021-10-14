import 'dotenv/config'
import {
    Coins,
    isTxError,
    LCDClient,
    MnemonicKey,
    Msg,
    MsgExecuteContract,
    MsgInstantiateContract,
    MsgMigrateContract,
    MsgStoreCode,
    StdTx,
    Wallet
} from '@terra-money/terra.js';
import {
    readFileSync,
    writeFileSync,
} from 'fs'
import path from 'path'
import { CustomError } from 'ts-custom-error'

const TIMEOUT = 1000

export interface Client {
    wallet: Wallet
    terra: LCDClient
}

export function newClient(): Client {
    const client = <Client>{}
    client.terra = new LCDClient({
        URL: String(process.env.NODE),
        chainID: String(process.env.CHAIN_ID)
    })
    client.wallet = recover(client.terra, String(process.env.WALLET_MNEMONIC!))
    return client
}

export function readNetworkConfig(name: string = 'artifact') {
    try {
        const data = readFileSync(path.join(process.env.OUTPUT_CONTRACTS_INFO!, `${name}.json`), 'utf8')
        return JSON.parse(data)
    } catch (e) {
        return {}
    }
}

export function writeNetworkConfig(data: object, name: string = 'artifact') {
    writeFileSync(path.join(process.env.OUTPUT_CONTRACTS_INFO!, `${name}.json`), JSON.stringify(data, null, 2))
}

export async function sleep(timeout: number) {
    await new Promise(resolve => setTimeout(resolve, timeout))
}

export class TransactionError extends CustomError {
    public constructor(
        public code: number,
    ) {
        super("transaction failed")
    }
}

export async function createTransaction(wallet: Wallet, msg: Msg) {
    return await wallet.createTx({ msgs: [msg]})
}

export async function broadcastTransaction(terra: LCDClient, signedTx: StdTx) {
    const result = await terra.tx.broadcast(signedTx)
    await sleep(TIMEOUT)
    return result
}

export async function performTransaction(terra: LCDClient, wallet: Wallet, msg: Msg) {
    const tx = await createTransaction(wallet, msg)
    const signedTx = await wallet.key.signTx(tx)
    const result = await broadcastTransaction(terra, signedTx)
    if (isTxError(result)) {
        throw new TransactionError(result.code)
    }
    return result
}

export async function uploadContract(terra: LCDClient, wallet: Wallet, filepath: string) {
    const contract = readFileSync(filepath, 'base64');
    const uploadMsg = new MsgStoreCode(wallet.key.accAddress, contract);
    let result = await performTransaction(terra, wallet, uploadMsg);
    return Number(result.logs[0].eventsByType.store_code.code_id[0]) // code_id
}

export async function instantiateContract(terra: LCDClient, wallet: Wallet, codeId: number, msg: object) {
    const instantiateMsg = new MsgInstantiateContract(wallet.key.accAddress, wallet.key.accAddress, codeId, msg, undefined);
    let result = await performTransaction(terra, wallet, instantiateMsg)
    const attributes = result.logs[0].events[0].attributes
    return attributes[attributes.length - 1].value // contract address
}

export async function executeContract(terra: LCDClient, wallet: Wallet, contractAddress: string, msg: object, coins?: Coins.Input) {
    const executeMsg = new MsgExecuteContract(wallet.key.accAddress, contractAddress, msg, coins);
    return await performTransaction(terra, wallet, executeMsg);
}

export async function queryContract(terra: LCDClient, contractAddress: string, query: object): Promise<any> {
    return await terra.wasm.contractQuery(contractAddress, query)
}

export async function deployContract(terra: LCDClient, wallet: Wallet, filepath: string, initMsg: object) {
    const codeId = await uploadContract(terra, wallet, filepath);
    return await instantiateContract(terra, wallet, codeId, initMsg);
}

export async function migrate(terra: LCDClient, wallet: Wallet, contractAddress: string, newCodeId: number) {
    const migrateMsg = new MsgMigrateContract(wallet.key.accAddress, contractAddress, newCodeId, {});
    return await performTransaction(terra, wallet, migrateMsg);
}

export function recover(terra: LCDClient, mnemonic: string) {
    const mk = new MnemonicKey({ mnemonic: mnemonic });
    return terra.wallet(mk);
}