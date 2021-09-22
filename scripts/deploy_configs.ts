export const testnet: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: undefined,
        }
    },
    gaugeInitMsg: {
        "config": {
            token: undefined,
            dev_addr: undefined,
            tokens_per_block: String(10000000),
            start_block: 100000,
            bonus_end_block: 500000,
        }
    },
    factoryInitMsg: {
        config:{
            pair_configs: [
                {
                    code_id: 0,
                    pair_type: { xyk: {}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                },
                {
                    code_id: 0,
                    pair_type: {stable:{}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                }
            ],
            token_code_id: 1,
            init_hook: undefined,
            fee_address: undefined
        }
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
    gaugeInitMsg: {
        "config": {
            token: undefined,
            dev_addr: undefined,
            tokens_per_block: String(10000000),
            start_block: 100000,
            bonus_end_block: 500000,
        }
    },
    factoryInitMsg: {
        config:{
            pair_configs: [
                {
                    code_id: 0,
                    pair_type: { xyk: {}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                },
                {
                    code_id: 0,
                    pair_type: {stable:{}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                }
            ],
            token_code_id: 1,
            init_hook: undefined,
            fee_address: undefined
        }
    },
    astroTokenContractAddress: undefined,
}

export const local: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5',
        }
    },
    gaugeInitMsg: {
        "config": {
            token: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5',
            dev_addr: undefined,
            tokens_per_block: String(10000000),
            start_block: 100000,
            bonus_end_block: 500000,
        }
    },
    factoryInitMsg: {
        config:{
            pair_configs: [
                {
                    code_id: 0,
                    pair_type: { xyk: {}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                },
                {
                    code_id: 0,
                    pair_type: {stable:{}},
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                }
            ],
            token_code_id: 1,
            init_hook: undefined,
            fee_address: undefined
        }
    },
    astroTokenContractAddress: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5'
}
