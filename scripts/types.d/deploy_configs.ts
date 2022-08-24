import {readArtifact} from "../helpers.js";

let chainConfigs = readArtifact(`${process.env.CHAIN_ID}-deploy-configs`);

export const deployConfigs: Config = {
    treasury: chainConfigs.treasury,
    staking: chainConfigs.staking,
    factory: chainConfigs.factory,
    router: chainConfigs.router,
    maker: chainConfigs.maker,
    vesting: chainConfigs.vesting,
    generator: chainConfigs.generator,
    createPairs: chainConfigs.create_pairs
}