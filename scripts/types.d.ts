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
        fee_address?: string,
        generator_address: string,
        owner: string,
    }
}

interface GeneratorInitMsg {
    config: {
        owner: string
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
    stakingInitMsg: StakingInitMsg,
    generatorInitMsg: GeneratorInitMsg,
    registerVestingAccounts: RegisterVestingAccounts
}

interface MigrationConfig {
    contract_address: string,
    file_path: string,
    message: object
}
