export const testnet: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: undefined,
        }
    },
    pairConfig : {
        code_id: 0,
        pair_type:"xyk",
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: "stable",
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
        pair_type: "xyk",
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: "stable",
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
        pair_type: "xyk",
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    pairStableConfig : {
        code_id: 0,
        pair_type: "stable",
        total_fee_bps: 0,
        maker_fee_bps: 0,
    },
    astroTokenContractAddress: 'terra1x46rqay4d3cssq8gxxvqz8xt6nwlz4td20k38v'
}
