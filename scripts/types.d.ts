interface StakingInitMsg {
    config: {
        token_code_id: number
        deposit_token_addr?: string
    }
}

interface FactoryInitMsg {
    config: {
        pair_configs: PairConfig[],
        token_code_id: number,
        init_hook?: string,
        fee_address?: string,
        generator_address?: string,
        gov?: string,
    }
}

interface GeneratorInitMsd {
    config: {
        allowed_reward_proxies: string[],
        astro_token: string,
        start_block: string,
        tokens_per_block: string,
        vesting_contract: string,
    }
}

type PairType = {
    xyk: {}
} | {
    stable: {}
}


interface PairConfig {
    code_id: number,
    pair_type: PairType,
    total_fee_bps: number,
    maker_fee_bps: number
}

type PointDate = string
type Amount = string

type VestingAccountSchedule = {
    start_point: {
        time: PointDate,
        amount: Amount
    },
    end_point?: {
        time: PointDate,
        amount: Amount
    }
}

interface VestingAccount {
    address: string
    schedules: VestingAccountSchedule[]
}

interface RegisterVestingAccountsType {
    vesting_accounts: VestingAccount[]
}

interface RegisterVestingAccounts {
    register_vesting_accounts: RegisterVestingAccountsType
}

interface Config {
    factoryInitMsg: FactoryInitMsg,
    stakingInitMsg: StakingInitMsg
    generatorInitMsg: GeneratorInitMsd,
    astroTokenContractAddress: string | undefined
    registerVestingAccounts: RegisterVestingAccounts
}
