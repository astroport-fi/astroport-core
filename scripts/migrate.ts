import 'dotenv/config'
import { ARTIFACTS_PATH, migrate, newClient, readArtifact, uploadContract } from "./helpers.js"
import { join } from "path"

async function main() {
    const { terra, wallet } = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('Network:', network)

    console.log("Uploading...");

    let config = {
        contract_address: "address",
        file_path: "astroport_contract.wasm",
        message: {}
    }

    const newCodeId = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, config.file_path)!);

    console.log('Migrating...');
    const migrateResult = await migrate(terra, wallet, config.contract_address, newCodeId, config.message);

    console.log("Migration complete: ");
    console.log(migrateResult);

}

main().catch(console.log)
