import {getLPTokenName, newClient, queryContract, readArtifact, toEncodedBinary} from "./helpers.js";
import {LCDClient, LocalTerra} from "@terra-money/terra.js";
import { chainConfigs } from "./types.d/chain_configs.js";

type IncentiveInfo = {
    lp_token_name: string,
    lp_token_address: string,
    old_alloc_points: string,
    new_alloc_points?: string,
    diff_amount?: string,
}

async function checkDiffIncentives(terra: LCDClient | LocalTerra, network: any, currentIncentives: [], newIncentives: []) {
    let diffPoolsIncentives: IncentiveInfo[] = [];

    for ( let currentIncentive of currentIncentives ) {
        let lpTokenName = await getLPTokenName(terra, currentIncentive);

        diffPoolsIncentives.push({
            lp_token_name: `${lpTokenName}`,
            lp_token_address: `${currentIncentive[0]}`,
            old_alloc_points: `${currentIncentive[1]}`,
            new_alloc_points: "Not found!",
            diff_amount: "Not found!",
        });
    }

    for ( let newIncentive of newIncentives ) {
        let lpTokenName = await getLPTokenName(terra, newIncentive);
        let isAlreadyExistIncentive = false;

        for ( const {index, incentives} of diffPoolsIncentives.map((incentives, index) => ({incentives, index})) ) {
            if ( newIncentive[0] == incentives['lp_token_address'] ) {
                if ( newIncentive[1] != incentives['old_alloc_points'] ) {
                    diffPoolsIncentives[index]['new_alloc_points'] = newIncentive[1];
                    diffPoolsIncentives[index]['diff_amount'] = String(Number(newIncentive[1]) - Number(incentives['old_alloc_points']));
                } else {
                    if (index > -1) {
                        diffPoolsIncentives.splice(index, 1);
                    }
                }
                isAlreadyExistIncentive = true;
            }
        }

        if (!isAlreadyExistIncentive) {
            diffPoolsIncentives.push({
                lp_token_name: `${lpTokenName}`,
                lp_token_address: `${newIncentive[0]}`,
                old_alloc_points: "Not found!",
                new_alloc_points: `${newIncentive[1]}`,
                diff_amount: `${newIncentive[1]}`,
            });
        }
    }

    return diffPoolsIncentives
}

function createProposal(executable_msg: any, order: string, contract_addr: string){
    console.log(`Internal proposal message:\n${JSON.stringify(executable_msg, null, 2)}\n`)

    let binary = toEncodedBinary(executable_msg);
    console.log(`Executable message in binary:\n${binary}\n`)

    let proposal = {
        order: order,
        msg: {
            wasm: {
                execute: {
                    contract_addr: contract_addr,
                    msg: binary,
                    funds: []
                }
            }
        }
    };

    console.log(`Final proposal message:\n${JSON.stringify([proposal], null, 2)}`);
    return proposal
}

async function main() {
    const { terra } = newClient();
    console.log(`chainID: ${terra.config.chainID}`);

    const network = readArtifact(terra.config.chainID);

    if (chainConfigs.generator.new_incentives_pools) {
        let active_pools = await queryContract(terra, network.generatorAddress, {config: {}}).then(res => res.active_pools);
        let diff_pools_incentives = await checkDiffIncentives(terra, network, active_pools, chainConfigs.generator.new_incentives_pools);

        if (diff_pools_incentives.length > 0 ) {
            console.table(diff_pools_incentives);
            createProposal({
                setup_pools: {
                    pools: chainConfigs.generator.new_incentives_pools
                }
            }, "1", network.generatorAddress);
        } else {
            console.log("New pools incentives are the same.");
        }
    } else {
        throw "New suggested incentives pools not found!"
    }
}

main().catch(console.error)