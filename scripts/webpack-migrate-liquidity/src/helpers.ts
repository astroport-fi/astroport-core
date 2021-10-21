import {
    Coins,
    isTxError,
    LCDClient,
    LocalTerra,
    MnemonicKey,
    Msg,
    MsgExecuteContract,
    StdTx,
    Wallet
} from '@terra-money/terra.js';
import { CustomError } from 'ts-custom-error'

export interface Client {
    wallet: Wallet
    terra: LCDClient | LocalTerra
}

export function newClient(url?: string, chainID?: string, mnemonic?: string): Client {
    const client = <Client>{}

    client.terra = new LCDClient({
        URL: url! || String(process.env.LCD_CLIENT_URL),
        chainID: chainID! || String(process.env.CHAIN_ID)
    });
    client.wallet = recover(client.terra, mnemonic! || String(process.env.WALLET));

    return client
}

// Tequila lcd is load balanced, so txs can't be sent too fast, otherwise account sequence queries
// may resolve an older state depending on which lcd you end up with. Generally 1000 ms is is enough
// for all nodes to sync up.
let TIMEOUT = 1000

export async function sleep(timeout: number) {
    await new Promise(resolve => setTimeout(resolve, timeout))
}

export class TransactionError extends CustomError {
    public constructor(
        public code: number,
        public codespace: string | undefined,
        public rawLog: string,
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
        throw new TransactionError(result.code, result.codespace, result.raw_log)
    }
    return result
}

export async function executeContract(terra: LCDClient, wallet: Wallet, contractAddress: string, msg: object, coins?: Coins.Input) {
    const executeMsg = new MsgExecuteContract(wallet.key.accAddress, contractAddress, msg, coins);
    return await performTransaction(terra, wallet, executeMsg);
}

export async function queryContract(terra: LCDClient, contractAddress: string, query: object): Promise<any> {
    return await terra.wasm.contractQuery(contractAddress, query)
}

export function recover(terra: LCDClient, mnemonic: string) {
    const mk = new MnemonicKey({ mnemonic: mnemonic });
    return terra.wallet(mk);
}
