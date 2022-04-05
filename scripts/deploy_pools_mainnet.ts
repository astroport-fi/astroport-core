import 'dotenv/config'
import {
    newClient,
    writeArtifact,
    readArtifact,
    executeContract,
    queryContract,
    toEncodedBinary,
    NativeAsset,
    TokenAsset
} from './helpers.js'

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    if (network.tokenAddress == "") {
        throw new Error("Token address is not set, deploy ASTRO first")
    }

    if (network.factoryAddress == "") {
        throw new Error("Factory address is not set, deploy factory first")
    }

    let pools =  [
        {
            identifier: "AstroUst",
            assetInfos: [
                new TokenAsset(network.tokenAddress).getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "BlunaLuna",
            assetInfos: [
                new TokenAsset("terra1kc87mu460fwkqte29rquh4hc20m54fxwtsx7gp").getInfo(),
                new NativeAsset("uluna").getInfo(),
            ],
            pairType: { stable: {} },
            initParams: toEncodedBinary({ amp: 1 })
        },
        {
            identifier: "UstLuna",
            assetInfos: [
                new NativeAsset("uusd").getInfo(),
                new NativeAsset("uluna").getInfo(),
            ],
            pairType: { xyk: {} }
        },
        {
            identifier: "AncUst",
            assetInfos: [
                new TokenAsset("terra14z56l0fp2lsf86zy3hty2z47ezkhnthtr9yq76").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "UstMir",
            assetInfos: [
                new NativeAsset("uusd").getInfo(),
                new TokenAsset("terra15gwkyepfc6xgca5t5zefzwy42uts8l2m4g40k6").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "MineUst",
            assetInfos: [
                new TokenAsset("terra1kcthelkax4j9x8d3ny6sdag0qmxxynl3qtcrpy").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "OrionUst",
            assetInfos: [
                new TokenAsset("terra1mddcdx0ujx89f38gu7zspk2r2ffdl5enyz2u03").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "SttUst",
            assetInfos: [
                new TokenAsset("terra13xujxcrc9dqft4p9a8ls0w3j0xnzm6y2uvve8n").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "VkrUst",
            assetInfos: [
                new TokenAsset("terra1dy9kmlm4anr92e42mrkjwzyvfqwz66un00rwr5").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "PsiUst",
            assetInfos: [
                new TokenAsset("terra12897djskt9rge8dtmm86w654g7kzckkd698608").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        },
        {
            identifier: "ApolloUst",
            assetInfos: [
                new TokenAsset("terra100yeqvww74h4yaejj6h733thgcafdaukjtw397").getInfo(),
                new NativeAsset("uusd").getInfo(),
            ],
            pairType: { xyk: {} },
        }
    ]

    for (let i = 0; i < pools.length; i++) {
        let pool = pools[i]
        let pool_pair_key = "pool" + pool.identifier
        let pool_lp_token_key = "lpToken" + pool.identifier

        // Create pool
        if (!network[pool_pair_key]) {
            console.log(`Creating pool ${pool.identifier}...`)
            let res = await executeContract(terra, wallet, network.factoryAddress, {
                create_pair: {
                    pair_type: pool.pairType,
                    asset_infos: pool.assetInfos,
                    init_params: pool.initParams
                }
            })

            network[pool_pair_key] = res.logs[0].eventsByType.from_contract.pair_contract_addr[0]

            let pool_info = await queryContract(terra, network[pool_pair_key], {
                pair: {}
            })

            network[pool_lp_token_key] = pool_info.liquidity_token

            console.log(`Pair successfully created! Address: ${network[pool_pair_key]}`)
            writeArtifact(network, terra.config.chainID)
        }
    }

    console.log('FINISH')
}

main().catch(console.log)
