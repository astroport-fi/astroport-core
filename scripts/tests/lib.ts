import { Int } from "@terra-money/terra.js";
import {
    newClient,
    readArtifact,
    queryContract, Client, toEncodedBinary, executeContract,
} from "../helpers.js"

export class Astroport {
    terra: any;
    wallet: any;

    constructor(terra: any, wallet: any) {
        this.terra = terra
        this.wallet = wallet
    }

    async getTokenBalance(token: string, address: string) {
        let resp = await queryContract(this.terra, token, { balance: { address: address } })
        return parseInt(resp.balance)
    }

    staking(addr: string) {
        return new Staking(this.terra, this.wallet, addr);
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