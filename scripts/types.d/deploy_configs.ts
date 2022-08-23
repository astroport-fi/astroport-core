import Ajv from "ajv";
import {readArtifact} from "../helpers.js";
const ajv = new Ajv();
const schema = {

}

let configs = readArtifact("deploys_configs")

export const deployConfigs: Config = {
    treasury: configs.treasury,
    staking: configs.staking,
    factory: configs.factory,
    router: configs.router,
    maker: configs.maker,
    vesting: configs.vesting,
    generator: configs.generator,
}

deployConfigs.staking.initMsg.deposit_token_addr = "hello";