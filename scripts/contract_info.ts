import 'dotenv/config'
import {
    newClient,
    readArtifact,
    queryContractInfo,
    queryCodeInfo,
    queryContractQuery,
    queryContractRaw
} from "./helpers.js"

async function main() {
    const {terra, wallet} = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('Network:', network)

    console.log('Contract info...');
    console.log(await queryContractInfo(terra, network.treasuryAddress));

    console.log('Code info...');
    console.log(await queryCodeInfo(terra, network.treasuryCodeID));

    // console.log("Detailed info...");
    // console.log(await queryContractQuery(terra, network.treasuryAddress, {
    //
    // }));

    console.log("raw info...");
    console.log(await queryContractRaw(terra, "/terra/wasm/v1beta1/contracts/terra1zg5uheafxcyw3kzjatcvetxd4xks2fup06eas4/store",
        {
            raw_query: Buffer.from(JSON.stringify({
                contract_addr: "terra1zg5uheafxcyw3kzjatcvetxd4xks2fup06eas4",
                key: ""
            }), 'utf-8').toString(
                'base64'
            ),
        }));
}

main().catch(console.log)
