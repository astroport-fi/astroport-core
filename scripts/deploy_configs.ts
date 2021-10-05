export const testnet: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 11,
            deposit_token_addr: 'terra18x4r44npdzrk0k9pzvy7h4d38ep3rmadsewzsh',
        }
    },
    generatorInitMsg: {
        config: {
            allowed_reward_proxies: [],
            astro_token: '',
            start_block: '1',
            tokens_per_block: String(10000000),
            vesting_contract: '',
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
    astroTokenContractAddress: 'terra18x4r44npdzrk0k9pzvy7h4d38ep3rmadsewzsh',
    registerVestingAccounts: {
        register_vesting_accounts:{
            vesting_accounts: [
                {
                    address: '', // dynamic field
                    schedules:[
                        [
                            String(new Date( 2022, 1, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2022, 6, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2023, 1, 1).getTime()),
                            String(new Date( 2024, 1, 1).getTime()),
                            String(1),
                        ],
                    ]
                }
            ]
        }
    }
}

export const bombay: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 9652,
            deposit_token_addr: 'terra1qqhw3t3p5349rs83m5mqjxft76c82yf99s9jjz',
        }
    },
    generatorInitMsg: {
        config: {
            allowed_reward_proxies: [],
            astro_token: '',
            start_block: '1',
            tokens_per_block: String(10000000),
            vesting_contract: '',
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
            token_code_id: 9652,
            init_hook: undefined,
            fee_address: undefined
        }
    },
    astroTokenContractAddress: 'terra1qqhw3t3p5349rs83m5mqjxft76c82yf99s9jjz',
    registerVestingAccounts: {
        register_vesting_accounts:{
            vesting_accounts: [
                {
                    address: '', // dynamic field
                    schedules:[
                        [
                            String(new Date( 2022, 1, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2022, 6, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2023, 1, 1).getTime()),
                            String(new Date( 2024, 1, 1).getTime()),
                            String(1),
                        ],
                    ]
                }
            ]
        }
    }
}

export const local: Config = {
    stakingInitMsg: {
        "config": {
            token_code_id: 1,
            deposit_token_addr: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5',
        }
    },
    generatorInitMsg: {
        "config": {
            astro_token: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5',
            allowed_reward_proxies: [],
            start_block: '1',
            tokens_per_block: String(10000000),
            vesting_contract: '',
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
    astroTokenContractAddress: 'terra18vd8fpwxzck93qlwghaj6arh4p7c5n896xzem5',
    registerVestingAccounts: {
        register_vesting_accounts:{
            vesting_accounts: [
                {
                    address: '', // dynamic field
                    schedules:[
                        [
                            String(new Date( 2022, 1, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2022, 6, 1).getTime()),
                            String(new Date( 2023, 1, 1).getTime()),
                            String(1),
                        ],
                        [
                            String(new Date( 2023, 1, 1).getTime()),
                            String(new Date( 2024, 1, 1).getTime()),
                            String(1),
                        ],
                    ]
                }
            ]
        }
    }
}
