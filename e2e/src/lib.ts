import {
    Coin,
    Fee, Int,
    LCDClient,
    MnemonicKey,
    Msg,
    MsgExecuteContract,
    MsgInstantiateContract,
    MsgMigrateContract,
    MsgStoreCode,
    MsgTransfer
} from "@terra-money/feather.js";
import {LCDClientConfig} from "@terra-money/feather.js/dist/client/lcd/LCDClient";
import {Key} from "@terra-money/feather.js/dist/key";
import {Wallet} from "@terra-money/feather.js/dist/client/lcd/Wallet";
import {readFileSync, writeFileSync} from 'fs';
import {exec} from "child_process";

export const CONTRACTS = "./contracts"


const CHAINS: Record<string, LCDClientConfig> = {
    ["localneutron-1"]: {
        lcd: "http://localhost:31317",
        chainID: "localneutron-1",
        gasPrices: "0.01untrn",
        gasAdjustment: 2,
        prefix: "neutron"
    },
    ["localterra-1"]: {
        lcd: "http://localhost:1317",
        chainID: "localterra-1",
        gasPrices: "0.015uluna",
        gasAdjustment: 2,
        prefix: "terra"
    }
}

export type TestConfig = {
    astro_token: string, // on terra
    cw20_ics20: string,
    terra_channel?: string,
    neutron_channel?: string
    new_terra_channel?: string,
    new_neutron_channel?: string
    astro_ibc_denom?: string // on neutron
    astro_tf_denom?: string // on neutron
    astro_tf_ibc_denom?: string // on terra
    terra_converter?: string
    neutron_converter?: string
}

export const save_config = (config: TestConfig) => {
    writeFileSync("config.json", JSON.stringify(config, null, 2))
}

export const load_config = (): TestConfig => {
    return JSON.parse(readFileSync("config.json").toString())
}

interface LCD_Ext extends LCDClient {
    wallet(key: Key): Wallet;

    simulate(sender: string, chainId: string, messages: Msg[]): Promise<Fee>;
}

const simulate = async function (lcd: LCDClient, sender: string, chainId: string, messages: Msg[]): Promise<Fee> {
    const accountInfo = await lcd.auth.accountInfo(sender);

    return await lcd.tx.estimateFee(
        [{
            sequenceNumber: accountInfo.getSequenceNumber(),
            publicKey: accountInfo.getPublicKey(),
        }],
        {
            msgs: messages,
            chainID: chainId
        }
    );
};

const extendLCD = (lcd: LCDClient): LCD_Ext => {
    return {
        ...lcd,
        simulate: async (sender: string, chainId: string, messages: Msg[]) => {
            return simulate(lcd, sender, chainId, messages);
        },
        wallet: lcd.wallet,
    };
};

export const LCD = extendLCD(new LCDClient(CHAINS))

export const USER_MNEMONIC = "journey proud segment gorilla pencil common phone cloth undo walk civil add gate six measure often addict turn because wet bachelor mechanic ozone early"

export type Signer = { signer: Wallet, address: string, chain_id: string }

export const get_signers = (): Record<string, Signer> => {
    const terra_signer = LCD.wallet(new MnemonicKey({mnemonic: USER_MNEMONIC, coinType: 330}));
    const terra_signer_addr = terra_signer.key.accAddress("terra")

    const neutron_signer = LCD.wallet(new MnemonicKey({mnemonic: USER_MNEMONIC, coinType: 118}));
    const neutron_signer_addr = neutron_signer.key.accAddress("neutron")

    return {
        terra: {
            signer: terra_signer,
            address: terra_signer_addr,
            chain_id: "localterra-1"
        },
        neutron: {
            signer: neutron_signer,
            address: neutron_signer_addr,
            chain_id: "localneutron-1"
        }
    }
}

export const simulateAndBroadcast = async function (lcd: LCD_Ext, signer: Wallet, chainId: string, messages: Msg[]) {
    const chain_prefix = lcd.config[chainId].prefix
    const sender = signer.key.accAddress(chain_prefix)

    await lcd.simulate(sender, chainId, messages)
        .then(console.log)

    return await signer.createAndSignTx({msgs: messages, chainID: chainId})
        .then((tx) => lcd.tx.broadcastSync(tx, chainId))
        .then(async (result) => {
            while (true) {
                // query txhash
                const data = await lcd.tx.txInfo(result.txhash, chainId).catch(() => {
                });
                // if hash is onchain return data
                if (data) return data;
                // else wait 250ms and then repeat
                await new Promise((resolve) => setTimeout(resolve, 250));
            }
        })
}

export const storeCode = async function (lcd: LCD_Ext, signer: Wallet, chainId: string, wasm_path: string) {
    const chain_prefix = lcd.config[chainId].prefix
    const sender = signer.key.accAddress(chain_prefix)
    const data = readFileSync(wasm_path, 'base64');
    return simulateAndBroadcast(lcd, signer, chainId, [new MsgStoreCode(sender, data)])
        .then((txResp) => parseInt(txResp!.logs![0].eventsByType.store_code.code_id[0]))
}

export const initContract = async function (lcd: LCD_Ext, signer: Wallet, chainId: string, code_id: number, init_msg: any) {
    const chain_prefix = lcd.config[chainId].prefix
    const sender = signer.key.accAddress(chain_prefix)
    const initMsg = new MsgInstantiateContract(
        sender,
        sender,
        code_id,
        init_msg,
        [],
        "label"
    );

    return await simulateAndBroadcast(lcd, signer, chainId, [initMsg])
        .then((resp: any) => resp.logs[0].eventsByType.instantiate._contract_address[0] as string)
}

export const execContract = async function (lcd: LCD_Ext, signer: Wallet, chainId: string, contract: string, msg: any, funds?: Coin[]) {
    const chain_prefix = lcd.config[chainId].prefix
    const sender = signer.key.accAddress(chain_prefix)
    const execMsg = new MsgExecuteContract(
        sender,
        contract,
        msg,
        funds
    );

    return await simulateAndBroadcast(lcd, signer, chainId, [execMsg])
}

export const migrateContract = async function (
    lcd: LCD_Ext,
    signer: Wallet,
    chainId: string,
    contract: string,
    code_id: number,
    msg: any
) {
    const chain_prefix = lcd.config[chainId].prefix
    const sender = signer.key.accAddress(chain_prefix)
    const initMsg = new MsgMigrateContract(
        sender,
        contract,
        code_id,
        msg,
    );

    return await simulateAndBroadcast(lcd, signer, chainId, [initMsg])
}

export const toBase64 = (object: any) => {
    return Buffer.from(JSON.stringify(object)).toString('base64');
}

export const execShellCommand = (cmd: string) => {
    return new Promise((resolve, _) => {
        exec(cmd, (error, stdout, stderr) => {
            if (error) {
                console.error(error);
                throw error;
            }
            resolve(stdout ? stdout : stderr);
        });
    });
}

export const ibcTransferOldAstro = async (
    signer: Signer,
    config: TestConfig,
    amount: number,
    receiver: string,
    timeout_sec?: number
) => {
    switch (signer.chain_id) {
        case "localterra-1":
            const inner_msg = {
                send: {
                    contract: config.cw20_ics20,
                    amount: amount.toString(),
                    msg: toBase64({
                        channel: config.terra_channel,
                        remote_address: receiver,
                    })
                }
            }
            const terra_msg = new MsgExecuteContract(signer.address, config.astro_token, inner_msg)
            return simulateAndBroadcast(LCD, signer.signer, signer.chain_id, [terra_msg])
        case "localneutron-1":
            return ibcIcs20Transfer(signer, new Coin(config.astro_ibc_denom!, amount), config.neutron_channel!, receiver, timeout_sec)
        default:
            throw new Error(`Unsupported chainId ${signer.chain_id}`)
    }
}

export const ibcIcs20Transfer = async (
    signer: Signer,
    coin: Coin,
    channel: string,
    receiver: string,
    timeout_sec?: number
) => {
    const msg = new MsgTransfer(
        "transfer",
        channel,
        coin,
        signer.address,
        receiver,
        undefined,
        Date.now() * 1000000 + 1000000000 * (timeout_sec || 10),
        undefined,
    )
    return simulateAndBroadcast(LCD, signer.signer, signer.chain_id, [msg])
}

export const getCw20Balance = async (config: TestConfig, address: string) => {
    const msg = {
        balance: {
            address: address
        }
    }
    return LCD.wasm.contractQuery(config.astro_token, msg).then((resp: any) => {
        return parseInt(resp.balance)
    })
}

export const getNativeBalance = async (address: string, denom: string) => {
    return LCD.bank.balance(address)
        .then(([coins, ]) => coins.get(denom)?.amount.toNumber() || 0);
}