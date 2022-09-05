import 'dotenv/config'
import {
    newClient,
    readArtifact,
    queryContractInfo,
    queryCodeInfo,
    queryContractRaw, toDecodedBinary, strToEncodedBinary
} from "./helpers.js"

async function main() {
    const {terra, wallet} = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('Network:', network)

    console.log('Contract info:');
    console.log(await queryContractInfo(terra, network.generatorAddress));

    console.log('Code info:');
    console.log(await queryCodeInfo(terra, network.treasuryCodeID));

    console.log(`Config about address: ${network.generatorAddress}`);
    console.log(await queryContractRaw(terra, `/terra/wasm/v1beta1/contracts/${network.generatorAddress}/store`,
    {
        query_msg: Buffer.from(JSON.stringify({
            config: {}
        }), 'utf-8').toString('base64'),
    }));

    console.log(`Info about address: ${network.generatorAddress}`);
    let resp = await queryContractRaw(terra, `/terra/wasm/v1beta1/contracts/${network.generatorAddress}/store/raw`,
        {
            key: strToEncodedBinary("contract_info")
        });
    console.log(toDecodedBinary(resp.data).toString());
}


main().catch(console.log)
