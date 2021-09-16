export const testnet: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: undefined,
        }
    },
    pairConfig : {
        code_id: 0,
        pair_type: { xyk: {} },
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: {stable:{}},
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    astroTokenContractAddress: undefined,
}

export const bombay: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: undefined,
        }
    },

    pairConfig : {
        code_id: 0,
        pair_type: { xyk: {} },
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: {stable:{}},
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    astroTokenContractAddress: undefined,
}

export const local: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: undefined,
        }
    },
    pairConfig : {
        code_id: 0,
        pair_type: { xyk: {} },
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: {stable:{}},
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    astroTokenContractAddress: 'terra1pclllzxzua8xq9hstcsm4ly9ut2sk42s29l75c'
}
