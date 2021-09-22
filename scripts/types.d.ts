//import * as pt from "./pairtype";

interface StakingInitMsg{
    config: {
        token_code_id: number
        deposit_token_addr?: string
    }
}

interface FactoryInitMsg{
    config: {
        pair_configs: PairConfig[],
        token_code_id: number,
        init_hook?: string,
        fee_address?: string,
    }
}

interface GaugeInitMsd{
    config:{
        token?: string,
        dev_addr?: string,
        tokens_per_block: string,
        start_block: number,
        bonus_end_block: number,
    }
}

type PairType = {
    xyk:{}
} | {
    stable: {}
}


interface PairConfig{
    code_id: number,
    pair_type: PairType,
    total_fee_bps: number,
    maker_fee_bps: number
}

interface Config {
    factoryInitMsg: FactoryInitMsg,
    stakingInitMsg: StakingInitMsg
    gaugeInitMsg: GaugeInitMsd,
    // pairConfig: PairConfig,
    // pairStableConfig: PairConfig,
    astroTokenContractAddress: string | undefined
}