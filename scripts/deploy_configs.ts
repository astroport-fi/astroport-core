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
            token_code_id: 2,
            deposit_token_addr: 'terra1qxxlalvsdjd07p07y3rc5fu6ll8k4tme7cye8y',
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
    astroTokenContractAddress: 'terra1qxxlalvsdjd07p07y3rc5fu6ll8k4tme7cye8y'
}
