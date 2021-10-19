import {Client, executeContract, newClient, queryContract} from "./helpers.js";
import {Buffer} from 'buffer';

const terraSwapFactoryAddr = "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
const astroportFactoryAddr = "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
const terraSwapPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"
const astroportPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"

const chainID="localterra"
const nodeURL ="https://bombay-lcd.terra.dev"
const walletMnemonic="quality vacuum heart guard buzz spike sight swarm shove special gym robust assume sudden deposit grid alcohol choice devote leader tilt noodle tide penalty"

async function migrateLiquidity(fromPairAddr: string, toPairAddr: string, amount: string) {
    //const cl = newClient(nodeURL, chainID, walletMnemonic);
    const cl = newClient();

    if (!await isAllowedAmountInPool(cl, fromPairAddr, amount, {"pool": {}})) {
        console.log("This amount not allowed in pool");
        return
    }

    let liqToken = await liquidityToken(cl, fromPairAddr, {"pair": {}});
    console.log("liqToken: ", liqToken);

    let withdrawResponse = await withdraw(cl, liqToken, {
        "send": {
            "contract": fromPairAddr,
            "amount": amount,
            "msg": Buffer.from('{"withdraw_liquidity": {}}').toString('base64')
        }
    });
    console.log("withdrawResp: ", withdrawResponse);

    let assetsInf = await assetsInfo(cl, fromPairAddr, {"pair": {}});
    if (!await isExistsPair(cl, assetsInf)) {
        await createPair(cl, assetsInf);
    }
    console.log("assetInf: ", assetsInf);

    let msgPL = await msgProvideLiquidity(assetsInf, withdrawResponse);
    console.log("msgPL: ", msgPL);

    let response = await provideLiquidity(cl, toPairAddr, msgPL);

    return response
}

async function isAllowedAmountInPool(cl: Client, pairAddr: string, amount: string, msg: Object) {
    let response = await queryContract(cl.terra, pairAddr, msg);
    console.log("pool:", response);
    return parseFloat(amount) <= parseFloat(response.total_share);
}

async function liquidityToken(cl: Client, pairAddr: string, msg: Object) {
    let response = await queryContract(cl.terra, pairAddr, msg);
    return response.liquidity_token
}

async function withdraw(cl: Client, liquidityTokenAddr: string, msg: Object) {
    return await executeContract(cl.terra, cl.wallet, liquidityTokenAddr, msg)
}

async function assetsInfo(cl: Client, pairAddr: string, msg: Object) {
    return await queryContract(cl.terra, pairAddr, msg);
}

async function isExistsPair(cl: Client, assetInfo: any) {
    try {
        await queryContract(cl.terra, terraSwapFactoryAddr, {"pair": {"asset_infos": assetInfo.asset_infos}});
        return true
    } catch (e: any) {
        console.log(e.response);
    }
    return false
}

async function createPair(cl: Client, assetsInfo: any) {
    return await executeContract(cl.terra, cl.wallet, terraSwapFactoryAddr,
        {
            "create_pair": {
                "asset_infos": assetsInfo.asset_infos,
            }
        });
}

async function msgProvideLiquidity(assetsInfo: any, withdrawResponse: Object) {
    let msgPL = {
        "provide_liquidity": {
            "assets": [{
                "info": {},
                "amount": "100"
            }, {
                "info": {},
                "amount": "1000"
            }],
        }
    }

    msgPL.provide_liquidity.assets[0].info = assetsInfo.asset_infos[0]
    msgPL.provide_liquidity.assets[1].info = assetsInfo.asset_infos[1]

    return msgPL
}

async function provideLiquidity(cl: Client, pairAddr: string, msg: any) {
    let coins = {}
    msg.provide_liquidity.assets.forEach(function (asset: any){
        if (asset.info.hasOwnProperty("native_token")){
            // @ts-ignore
            coins[`${asset.info.native_token.denom}`] = asset.amount;
        }
    })
    console.log("coins: ", coins);
    //return await executeContract(cl.terra, cl.wallet, pairAddr, msg, coins);
}

export async function pairs(cl: Client, factoryAddr: string, msg: Object) {
    return await queryContract(cl.terra, factoryAddr, msg);
}

async function main() {

    //const cl = newClient(nodeURL, chainID, walletMnemonic);
    try {
        await migrateLiquidity(terraSwapPairAddr, astroportPairAddr, "20");
    } catch (e: any) {
        console.log(e.response);
    }
}
main().catch(console.log);