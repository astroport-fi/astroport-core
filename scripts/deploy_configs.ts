export const configDefault: Config = {
    stakingInitMsg: {
        config: {
            token_code_id: 0,
            deposit_token_addr: '',
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
        config: {
            owner: '',
            pair_xyk_config: {
                code_id: 0,
                total_fee_bps: 0,
                maker_fee_bps: 0
            },
            pair_stable_config: {
                code_id: 0,
                total_fee_bps: 0,
                maker_fee_bps: 0
            },
            token_code_id: 0,
            generator_address: '',
            fee_address: undefined,
            gov: undefined,
        }
    },
    registerVestingAccounts: {
        register_vesting_accounts: {
            vesting_accounts: [
                {
                    address: '', // dynamic field
                    schedules: [
                        {
                            start_point: {
                                time: String(new Date(2021, 10, 6).getTime()),
                                amount: String("63072000000000")
                            }
                        }
                    ]
                }
            ]
        }
    }
}
