import {
    newClient,
    readArtifact,
    queryContract,
    Client,
    toEncodedBinary,
    executeContract,
    NativeAsset,
    TokenAsset,
    NativeSwap,
    AstroSwap,
    performTransaction
} from "../helpers.js"
import {LCDClient, Coin, MsgExecuteContract, Numeric, Coins} from '@terra-money/terra.js';
import util from 'util';

export class Astroport {
    terra: LCDClient;
    wallet: any;

    constructor(terra: any, wallet: any) {
        this.terra = terra
        this.wallet = wallet
    }

    async getNativeBalance(address: string, denom: string) {
        let balances = await this.terra.bank.balance(address)
        return balances.get(denom)
    }

    async getTokenBalance(token: string, address: string) {
        let resp = await queryContract(this.terra, token, { balance: { address: address } })
        return parseInt(resp.balance)
    }

    staking(addr: string) {
        return new Staking(this.terra, this.wallet, addr);
    }

    generator(addr: string) {
        return new Generator(this.terra, this.wallet, addr);
    }

    pair(addr: string) {
        return new Pair(this.terra, this.wallet, addr);
    }

    maker(addr: string) {
        return new Maker(this.terra, this.wallet, addr);
    }

    factory(addr: string) {
        return new Factory(this.terra, this.wallet, addr);
    }

    router(addr: string) {
        return new Router(this.terra, this.wallet, addr);
    }
}

class Pair {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async queryPool() {
        return await queryContract(this.terra, this.addr, {pool: {}})
    }

    async queryPair() {
        return await queryContract(this.terra, this.addr, {pair: {}})
    }

    async queryShare(amount: string) {
        return await queryContract(this.terra, this.addr, {share: {amount}})
    }

    async swapNative(offer_asset: NativeAsset) {
        await executeContract(this.terra, this.wallet, this.addr, {
            swap: {
                offer_asset: offer_asset.withAmount()
            }
        }, [offer_asset.toCoin()])
    }

    async swapCW20(token_addr: string, amount: string) {
        let msg = Buffer.from(JSON.stringify({swap: {}})).toString("base64");

        await executeContract(this.terra, this.wallet, token_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }

    async provideLiquidity(a1: NativeAsset | TokenAsset, a2: NativeAsset | TokenAsset) {
        let msg = {
            "provide_liquidity": {
                "assets": [a1.withAmount(), a2.withAmount()],
            }
        }

        let coins = [];
        let assets = [a1, a2]
        for (const key in assets) {
            const asset = assets[key];

            // send tokens
            if (asset instanceof NativeAsset) {
                coins.push(asset.toCoin())
            }

            // set allowance
            if (asset instanceof TokenAsset) {
                console.log('Setting allowance for contract')
                await executeContract(this.terra, this.wallet, asset.addr, {
                    "increase_allowance": {
                        "spender": this.addr,
                        "amount": asset.amount,
                        "expires": {
                            "never": {}
                        }
                    }
                })
            }
        }

        await executeContract(this.terra, this.wallet, this.addr, msg, coins)
    }

    async withdrawLiquidity(lp_addr: string, amount: string) {
        let msg = Buffer.from(JSON.stringify({withdraw_liquidity: {}})).toString("base64");

        await executeContract(this.terra, this.wallet, lp_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }
}

class Staking {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async stakeAstro(astro_addr: string, amount: string) {
        let msg = Buffer.from(JSON.stringify({enter: {}})).toString("base64");

        await executeContract(this.terra, this.wallet, astro_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }

    async unstakeAstro(xastro_addr: string, amount: string) {
        let msg = Buffer.from(JSON.stringify({leave: {}})).toString("base64");

        await executeContract(this.terra, this.wallet, xastro_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }
}

class Maker {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async queryConfig() {
        return await queryContract(this.terra, this.addr, {config: {}})
    }

    async queryBalances(asset_infos: (TokenAsset|NativeAsset)[]) {
        let resp = await queryContract(this.terra, this.addr, {balances: {assets: asset_infos.map(x => x.getInfo())}});
        return resp.balances;
    }

    async collect(pair_addresses: string[]) {
        return await executeContract(this.terra, this.wallet, this.addr, {
            collect: {
                pair_addresses,
            }
        })
    }
}

class Factory {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async queryFeeInfo(pair_type: string) {
        var pt: any = {};
        pt[pair_type] = {};

        let resp = await queryContract(this.terra, this.addr, {fee_info: {pair_type: pt}});
        return resp
    }
}

export class Router {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async queryConfig() {
        return await queryContract(this.terra, this.addr, {config: {}})
    }

    async assertMinimumReceive(asset_info: TokenAsset | NativeAsset, prev_balance: string, minimum_receive: string, receiver: string) {
        return await executeContract(this.terra, this.wallet, this.addr, {
            "assert_minimum_receive": {
                "asset_info": asset_info.getInfo(),
                "minimum_receive": minimum_receive,
                "prev_balance": prev_balance,
                "receiver": receiver
            }
        });
    }

    async swapOperationsCW20(token_addr: string, amount: string, minimum_receive: string, operations: (NativeSwap|AstroSwap)[], to?: string) {
        let msg = Buffer.from(JSON.stringify({
            execute_swap_operations: {
                    operations: operations.map(value => value.getInfo()),
                    minimum_receive: minimum_receive,
                    to: to
            }})).toString("base64");

        return await executeContract(this.terra, this.wallet, token_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }

    async swapOperations(operations: (NativeSwap | AstroSwap)[], coins: Coin, minimum_receive?: string, to?: string) {
        return await executeContract(this.terra, this.wallet, this.addr, {
            "execute_swap_operations": {
                "operations": operations.map(value => value.getInfo()),
                "minimum_receive": minimum_receive,
                "to": to
            }
        },  [coins]);
    }
}

class Generator {
    terra: any;
    wallet: any;
    addr: string;

    constructor(terra: any, wallet: any, addr:string) {
        this.terra = terra
        this.wallet = wallet
        this.addr = addr;
    }

    async deposit(lp_addr: string, amount: string) {
        let msg = Buffer.from(JSON.stringify({deposit: {}})).toString("base64");

        await executeContract(this.terra, this.wallet, lp_addr, {
            send: {
                contract: this.addr,
                amount,
                msg
            }
        })
    }

    async withdraw(lp_addr: string, amount: string) {
        await executeContract(this.terra, this.wallet, this.addr, {
            withdraw: {
                lp_token: lp_addr,
                amount: amount,
            }
        })
    }

    async queryDeposit(lp_token: string, user: string) {
        return await queryContract(this.terra, this.addr, {
            deposit: {
                "lp_token": lp_token,
                "user": user,
            }
        })
    }
}