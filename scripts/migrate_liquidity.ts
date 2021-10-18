import {Client, executeContract, newClient, queryContract,} from "./helpers.js";
import {Buffer} from 'buffer';

const terraSwapFactoryAddr = "terra18qpjm4zkvqnpjpw0zn0tdr8gdzvt8au35v45xf"
const astroTokenAddr = "terra1swfvqw3t3xchsscpy35zqhl0edf65wyvws7v83"
const terraSwapPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"
const astroportPairAddr = "terra18um88jh26gwq5varc570ze8m24q79q9n02sd33"

async function createPair(cl: Client) {
    let response = await executeContract(cl.terra,
        cl.wallet,
        terraSwapFactoryAddr,
        {
            "create_pair": {
                "asset_infos": [
                    {
                        "token": {
                            "contract_addr": astroTokenAddr
                        }
                    },
                    {
                        "native_token": {
                            "denom": "uluna"
                        }
                    }
                ]
            }
        });
    console.log(response);
}

// let msg = {
//     "provide_liquidity": {
//         "assets": [
//             {
//                 "info" : {
//                     "token": {
//                         "contract_addr": astroTokenAddr
//                     }
//                 },
//                 "amount": "100"
//             },
//             {
//                 "info" : {
//                     "native_token": {
//                         "denom": "uluna"
//                     }
//                 },
//                 "amount": "10000"
//             }
//         ]
//     }
// }

async function provideLiquidity(cl: Client, pairAddr: string, msg: Object) {
    return await executeContract(cl.terra, cl.wallet, pairAddr, msg, {"uluna": 1000});
}

async function isAllowedAmountInPool(cl: Client, pairAddr: string, amount: string, msg: Object) {
    let response = await queryContract(cl.terra, pairAddr, msg);
    return parseFloat(amount) <= parseFloat(response.total_share);
}

async function msgProvideLiquidity(cl: Client, pairAddr: string, msg: Object, withdrawResponse: Object) {
    let response = await queryContract(cl.terra, pairAddr, msg);
    let msgPL = {
        "provide_liquidity": {
            "assets": [{
                "info": {},
                "amount": "1000"
            }, {
                "info": {},
                "amount": "1000"
            }],
        }
    }

    msgPL.provide_liquidity.assets[0].info = response.asset_infos[0]
    msgPL.provide_liquidity.assets[1].info = response.asset_infos[1]

    return msgPL
}

async function liquidityToken(cl: Client, pairAddr: string, msg: Object) {
    let response = await queryContract(cl.terra, pairAddr, msg);
    return response.liquidity_token
}

async function withdraw(cl: Client, liquidityTokenAddr: string, msg: Object) {
    return await executeContract(cl.terra, cl.wallet, liquidityTokenAddr, msg)
}

async function migrateLiquidity(cl: Client, fromPairAddr: string, toPairAddr: string, amount: string) {
    if (!await isAllowedAmountInPool(cl, fromPairAddr, amount, {"pool": {}})) {
        console.log("This amount not allowed in pool");
        return
    }

    let liqToken = await liquidityToken(cl, fromPairAddr, {"pair": {}});
    let response = await withdraw(cl, liqToken, {
        "send": {
            "contract": fromPairAddr,
            "amount": amount,
            "msg": Buffer.from('{"withdraw_liquidity": {}}').toString('base64')
        }
    });

    let msgPL = await msgProvideLiquidity(cl, fromPairAddr, {"pair": {}}, response)
    response = await provideLiquidity(cl, toPairAddr, msgPL);

    return response
}

async function main() {
    const client = newClient();
    let response = await migrateLiquidity(client, terraSwapPairAddr, astroportPairAddr, "100");
    console.log(response);

    response = await queryContract(client.terra, terraSwapPairAddr, {"pool": {}});
    console.log(response);
}
main().catch(console.log)