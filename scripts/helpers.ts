import 'dotenv/config'
import {
    Coin,
    Coins,
    isTxError,
    LCDClient,
    LocalTerra,
    MnemonicKey,
    Msg,
    MsgExecuteContract,
    MsgInstantiateContract,
    MsgMigrateContract,
    MsgStoreCode,
    MsgUpdateContractAdmin,
    StdTx,
    Wallet
} from '@terra-money/terra.js';
import {
    readFileSync,
    writeFileSync,
} from 'fs'
import path from 'path'
import { CustomError } from 'ts-custom-error'

export const ARTIFACTS_PATH = '../artifacts'

export function readArtifact(name: string = 'artifact') {
    try {
        const data = readFileSync(path.join(ARTIFACTS_PATH, `${name}.json`), 'utf8')
        return JSON.parse(data)
    } catch (e) {
        return {}
    }
}

export interface Client {
    wallet: Wallet
    terra: LCDClient | LocalTerra
}

export function newClient(): Client {
    const client = <Client>{}
    if (process.env.WALLET) {
        client.terra = new LCDClient({
            URL: String(process.env.LCD_CLIENT_URL),
            chainID: String(process.env.CHAIN_ID)
        })
        client.wallet = recover(client.terra, process.env.WALLET)
    } else {
        client.terra = new LocalTerra()
        client.wallet = (client.terra as LocalTerra).wallets.test1
    }
    return client
}

export function writeArtifact(data: object, name: string = 'artifact') {
    writeFileSync(path.join(ARTIFACTS_PATH, `${name}.json`), JSON.stringify(data, null, 2))
}

// Tequila lcd is load balanced, so txs can't be sent too fast, otherwise account sequence queries
// may resolve an older state depending on which lcd you end up with. Generally 1000 ms is enough
// for all nodes to sync up.
let TIMEOUT = 1000

export function setTimeoutDuration(t: number) {
    TIMEOUT = t
}

export function getTimeoutDuration() {
    return TIMEOUT
}

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

export async function uploadContract(terra: LCDClient, wallet: Wallet, filepath: string) {
    const contract = readFileSync(filepath, 'base64');
    const uploadMsg = new MsgStoreCode(wallet.key.accAddress, contract);
    let result = await performTransaction(terra, wallet, uploadMsg);
    return Number(result.logs[0].eventsByType.store_code.code_id[0]) // code_id
}

export async function instantiateContract(terra: LCDClient, wallet: Wallet, admin_address: string, codeId: number, msg: object) {
    const instantiateMsg = new MsgInstantiateContract(wallet.key.accAddress, admin_address, codeId, msg, undefined);
    let result = await performTransaction(terra, wallet, instantiateMsg)
    return result.logs[0].events[0].attributes.filter(element => element.key == 'contract_address' ).map(x => x.value);
}

export async function executeContract(terra: LCDClient, wallet: Wallet, contractAddress: string, msg: object, coins?: Coins.Input) {
    const executeMsg = new MsgExecuteContract(wallet.key.accAddress, contractAddress, msg, coins);
    return await performTransaction(terra, wallet, executeMsg);
}

export async function queryContract(terra: LCDClient, contractAddress: string, query: object): Promise<any> {
    return await terra.wasm.contractQuery(contractAddress, query)
}

export async function deployContract(terra: LCDClient, wallet: Wallet, admin_address: string, filepath: string, initMsg: object) {
    const codeId = await uploadContract(terra, wallet, filepath);
    return await instantiateContract(terra, wallet, admin_address, codeId, initMsg);
}

export async function migrate(terra: LCDClient, wallet: Wallet, contractAddress: string, newCodeId: number, msg: object) {
    const migrateMsg = new MsgMigrateContract(wallet.key.accAddress, contractAddress, newCodeId, msg);
    return await performTransaction(terra, wallet, migrateMsg);
}

export function recover(terra: LCDClient, mnemonic: string) {
    const mk = new MnemonicKey({ mnemonic: mnemonic });
    return terra.wallet(mk);
}

export async function update_contract_admin(
  terra: LCDClient,
  wallet: Wallet,
  contract_address: string,
  admin_address: string
) {
    let msg = new MsgUpdateContractAdmin(
        wallet.key.accAddress,
        admin_address,
        contract_address
    );

    return await performTransaction(terra, wallet, msg);
}

export function initialize(terra: LCDClient) {
    const mk = new MnemonicKey();

    console.log(`Account Address: ${mk.accAddress}`);
    console.log(`MnemonicKey: ${mk.mnemonic}`);

    return terra.wallet(mk);
}

export function toEncodedBinary(object: any) {
    return Buffer.from(JSON.stringify(object)).toString('base64');
}

export class NativeAsset {
    denom: string;
    amount?: string

    constructor(denom: string, amount?: string) {
        this.denom = denom
        this.amount = amount
    }

    getInfo() {
        return {
            "native_token": {
                "denom": this.denom,
            }
        }
    }

    withAmount() {
        return {
            "info": this.getInfo(),
            "amount": this.amount
        }
    }

    getDenom() {
        return this.denom
    }

    toCoin() {
        return new Coin(this.denom, this.amount || "0")
    }
}

export class TokenAsset {
    addr: string;
    amount?: string

    constructor(addr: string, amount?: string) {
        this.addr = addr
        this.amount = amount
    }

    getInfo() {
        return {
            "token": {
                "contract_addr": this.addr
            }
        }
    }

    withAmount() {
        return {
            "info": this.getInfo(),
            "amount": this.amount
        }
    }

    toCoin() {
        return null
    }

    getDenom() {
        return this.addr
    }
}

export class NativeSwap {
    offer_denom: string;
    ask_denom: string;

    constructor(offer_denom: string, ask_denom: string) {
        this.offer_denom = offer_denom
        this.ask_denom = ask_denom
    }

    getInfo() {
        return {
            "native_swap": {
                "offer_denom": this.offer_denom,
                "ask_denom": this.ask_denom
            }
        }
    }
}

export class AstroSwap {
    offer_asset_info: TokenAsset|NativeAsset;
    ask_asset_info: TokenAsset|NativeAsset;

    constructor(offer_asset_info: TokenAsset|NativeAsset, ask_asset_info: TokenAsset|NativeAsset) {
        this.offer_asset_info = offer_asset_info
        this.ask_asset_info = ask_asset_info
    }

    getInfo() {
        return {
            "astro_swap": {
                "offer_asset_info": this.offer_asset_info.getInfo(),
                "ask_asset_info": this.ask_asset_info.getInfo(),
            }
        }
    }
}
