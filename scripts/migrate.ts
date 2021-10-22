import 'dotenv/config'
import {ARTIFACTS_PATH, migrate, newClient, readArtifact, uploadContract} from "./helpers.js"
import { join } from "path"
import {config} from "./migrate_configs.js";

async function main() {
    const {terra, wallet} = newClient()
    console.log(`chainID: ${terra.config.chainID} wallet: ${wallet.key.accAddress}`)
    const network = readArtifact(terra.config.chainID)
    console.log('network:', network)

    console.log("uploading...");
    const newCodeId = await uploadContract(terra, wallet, join(ARTIFACTS_PATH, config.file_path)!);

    console.log('migrating...');
    const migrateResult = await migrate(terra, wallet, config.contract_address, newCodeId, config.message);

    console.log("migration complete: ");
    console.log(migrateResult);

}

main().catch(console.log)
