export const configDefault: Config = {
    stakingConfig: {
        configInitMsg:{
            config: {
                token_code_id: 0,
                deposit_token_addr: '',
            }
        }
    },
    generatorConfig: {
        configInitMsg: {
            config: {
                allowed_reward_proxies: [],
                astro_token: '',
                start_block: '1',
                tokens_per_block: process.env.TOKEN_PER_BLOCK!,
                vesting_contract: '',
            }
        }
    },
    factoryConfig: {
        configInitMsg: {
            pair_configs: [
                {
                    code_id: 0,
                    pair_type: { xyk: {} },
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                },
                {
                    code_id: 0,
                    pair_type: { stable: {} },
                    total_fee_bps: 0,
                    maker_fee_bps: 0
                }
            ],
            token_code_id: 0,
            init_hook: undefined,
            fee_address: undefined
        }
    },
    astroTokenContractAddress: '',
    registerVestingAccounts: {
        register_vesting_accounts: {
            vesting_accounts: [
                {
                    address: '',
                    schedules: [
                        {
                            start_point: {
                                time: String(new Date(2021, 10, 6).getTime()),
                                amount: process.env.VESTING_START_POINT_AMOUNT!
                            }
                        }
                    ]
                }
            ]
        }
    },
    tokenConfig: {
        configInitMsg: {
            name: process.env.TOKEN_NAME!,
            symbol: process.env.TOKEN_SYMBOL!,
            decimals: Number(process.env.TOKEN_DECIMALS!),
            initial_balances: [
                {
                    address: process.env.TOKEN_INITIAL_AMOUNT_ADDRESS!,
                    amount: process.env.TOKEN_INITIAL_AMOUNT!
                },
            ],
            mint: {
                minter: process.env.TOKEN_MINTER!,
                cap: process.env.TOKEN_CAPACITY!
            }
        }
    },
    routerConfig: {
        configInitMsg: {
            astroport_factory: ''
        }
    },
    vestingConfig: {
        configInitMsg: {
            owner: '',
            token_addr: '',
        }
    },
    makerConfig: {
        configInitMsg: {
            factory_contract: '',
            staking_contract: '',
            astro_token_contract: '',
        }
    }
}
