import {Client, executeContract, newClient, queryContract} from "./helpers";
import {Buffer} from 'buffer';

const terraSwapFactoryAddr = "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
const astroportFactoryAddr = "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
const terraSwapPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"
const astroportPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"

async function migrateLiquidity(cl: Client, fromPairAddr: string, toPairAddr: string, amount: string) {
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
            "msg": Buffer.from(JSON.stringify({"withdraw_liquidity": {}})).toString('base64')
        }
    });
    console.log("withdrawResp: ", withdrawResponse);

    let assetsInf = await assetsInfo(cl, fromPairAddr, {"pair": {}});
    if (!await isExistsPair(cl, assetsInf)) {
        await createPair(cl, assetsInf);
    }
    console.log("assetInf: ", assetsInf);

    let assetsAmount = withdrawAssetsAmount(withdrawResponse);
    console.log("amount in withdraw: ", assetsAmount);

    let msgPL = msgProvideLiquidity(assetsInf, assetsAmount);
    console.log("msgPL: ", msgPL.provide_liquidity.assets);

    let response = await provideLiquidity(cl, toPairAddr, msgPL);
    console.log("provide: ", response);
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
        await queryContract(cl.terra, astroportFactoryAddr, {"pair": {"asset_infos": assetInfo.asset_infos}});
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

function withdrawAssetsAmount(withdrawResponse: any) {
    let parsedLog = JSON.parse(withdrawResponse.raw_log);
    let eventsWD = parsedLog[0].events;

    let attributes = []
    for (let i=0; i<eventsWD.length; i++){
        if (eventsWD[i]['type'] === "from_contract"){
            attributes = eventsWD[i]['attributes'];
            break;
        }
    }

    let refundAsset = ""
    for (let i=0; i<attributes.length; i++) {
        if (attributes[i]["key"] == "refund_assets"){
            refundAsset = attributes[i]["value"];
            break;
        }
    }
    let assetsAmount = refundAsset.split(",");
    console.log("assetsAmount: ", assetsAmount);

    assetsAmount[0] = assetsAmount[0].trim().replace(/(^\d+)(.+$)/i,'$1');
    assetsAmount[1] = assetsAmount[1].trim().replace(/(^\d+)(.+$)/i,'$1');

    return assetsAmount
}

function msgProvideLiquidity(assetsInfo: any, assetsAmount: any) {
    let msgPL = {
        "provide_liquidity": {
            "assets": [{
                "info": {},
                "amount": assetsAmount[0]
            }, {
                "info": {},
                "amount": assetsAmount[1]
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

    if (Object.keys(coins).length === 0) {
        return await executeContract(cl.terra, cl.wallet, pairAddr, msg);
    } else {
        return await executeContract(cl.terra, cl.wallet, pairAddr, msg, coins);
    }
}

async function pairs(cl: Client, factoryAddress: string, msg: Object) {
    return await queryContract(cl.terra, factoryAddress, msg);
}

async function tokenInfo(cl: Client, tokenAddress: string, msg: Object) {
    return  await queryContract(cl.terra, tokenAddress, msg);
}

module.exports = {
    newClient,
    pairs,
    tokenInfo,
    migrateLiquidity,
};

async function main() {
    const chainID="bombay-12"
    const nodeURL ="https://bombay-lcd.terra.dev"
    const walletMnemonic="quality vacuum heart guard buzz spike sight swarm shove special gym robust assume sudden deposit grid alcohol choice devote leader tilt noodle tide penalty"

    let cl = newClient(nodeURL, chainID, walletMnemonic);


    let totalPairs = []
    let response = await pairs(cl, terraSwapFactoryAddr, {"pairs": {"limit": 30}});
    totalPairs.push(...response.pairs);

    do {
        response = await pairs(cl, terraSwapFactoryAddr, {"pairs": {"limit": 30, "start_after": totalPairs[totalPairs.length - 1].asset_infos}});
        totalPairs.push(...response.pairs);
    } while ( response.pairs.length > 0);


    console.log(totalPairs);
    console.log(totalPairs.length);
}

main().catch();
