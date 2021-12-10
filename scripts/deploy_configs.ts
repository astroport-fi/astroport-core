import { toEncodedBinary } from "./helpers";

export const configDefault: Config = {
    stakingInitMsg: {
        config: {
            token_code_id: 0,
            deposit_token_addr: '',
        }
    },
    generatorInitMsg: {
        config: {
            owner: '',
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
            generator_address: '',
            fee_address: undefined,
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
    },
    initialPools: [
        {
            identifier: "AstroUst",
            assetInfos: [
                {
                    token: {
                        contract_addr: ""
                    }
                },
                {
                    native_token: { denom: "uusd" }
                }
            ],
            pairType: { xyk: {} }
        },
        {
            identifier: "LunaUst",
            assetInfos: [
                {
                    native_token: { denom: "uluna" }
                },
                {
                    native_token: { denom: "uusd" }
                }
            ],
            pairType: { stable: {} },
            initParams: toEncodedBinary({ amp: 100 })
        },
        {
            identifier: "AncUst",
            assetInfos: [
                {
                    token: {
                        contract_addr: "terra1747mad58h0w4y589y3sk84r5efqdev9q4r02pc"
                    }
                },
                {
                    native_token: { denom: "uusd" }
                }
            ],
            pairType: { xyk: {} },
            initGenerator: {
                generatorAllocPoint: 1000000
            }
        },
        {
            identifier: "MirUst",
            assetInfos: [
                {
                    token: {
                        contract_addr: "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u"
                    }
                },
                {
                    native_token: { denom: "uusd" }
                }
            ],
            pairType: { xyk: {} },
            initOracle: true,
            initGenerator: {
                generatorAllocPoint: 1000000,
                generatorProxy: {
                    artifactName: "astroport_generator_proxy_to_mirror.wasm",
                    rewardContractAddr: "terra1a06dgl27rhujjphsn4drl242ufws267qxypptx",
                    rewardTokenAddr: "terra10llyp6v3j3her8u3ce66ragytu45kcmd9asj3u"
                }
            }
        },
        {
            identifier: "BlunaLuna",
            assetInfos: [
                {
                    token: {
                        contract_addr: "terra1u0t35drzyy0mujj8rkdyzhe264uls4ug3wdp3x"
                    }
                },
                {
                    native_token: { denom: "uusd" }
                }
            ],
            pairType: { stable: {} },
            initParams: toEncodedBinary({ amp: 100 })
        }
    ]
}
