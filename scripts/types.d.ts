//import * as pt from "./pairtype";

interface StakingInitMsg{
    config: {
        token_code_id: number
        deposit_token_addr?: string
    }
}

interface FactoryInitMsg{
    config: {
        pair_configs: PairConfig[]
        token_code_id: number,
        init_hook?: string,
        fee_address?: string,
    }
}

interface PairConfig{
    code_id: number,
    pair_type: string,
    total_fee_bps: number,
    maker_fee_bps: number
}

interface Config {
    stakingInitMsg: StakingInitMsg
    pairConfig: PairConfig,
    pairStableConfig: PairConfig,
    astroTokenContractAddress: string | undefined
}