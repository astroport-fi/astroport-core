import 'dotenv/config'
import {
    newClient,
    readArtifact,
    queryContractRaw, toDecodedBinary, strToEncodedBinary, getRemoteFile
} from "./helpers.js"
import {LCDClient} from "@terra-money/terra.js";

const ASTROPORT_CHANGE_LOG_NAME = process.env.ASTROPORT_CHANGE_LOG_NAME! || String('core_pisco')
const ASTROPORT_CHANGE_LOG_URL = process.env.ASTROPORT_CHANGE_LOG_URL! || String("https://raw.githubusercontent.com/astroport-fi/astroport-changelog/main/terra-2/pisco-1/core_pisco.json")

function contractInfo(local_name: string, address: string, remote_name: string, version: string) {
    return {
        local_name,
        address,
        remote_name,
        version
    };
}

async function generateTable(terra: LCDClient) {
    let network = readArtifact(ASTROPORT_CHANGE_LOG_NAME)
    for (const key in network) {
        const value = network[key];
        let end_point = `/cosmwasm/wasm/v1/contract/${value}/raw/${strToEncodedBinary("contract_info")}`;

        // each contract should be saved with `address` substring name in .json config file
        if ( key.includes("address") ){
            await queryContractRaw(terra, end_point)
                .then(resp => {
                    let result = JSON.parse(toDecodedBinary(resp.data).toString());

                    console.table(contractInfo(`${key}`, `${value}`, `${result.contract}`, `${result.version}`))
                })
                .catch(err => {console.log(`${key}: ${err}`)});
        }
    }
}

async function main() {
    const {terra, wallet} = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('Network:', network)

    getRemoteFile(ASTROPORT_CHANGE_LOG_NAME, ASTROPORT_CHANGE_LOG_URL)
    await generateTable(terra)
}


main().catch(console.log)
