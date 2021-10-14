interface StakingConfig {
    configInitMsg: {
        config: {
            token_code_id: number
            deposit_token_addr?: string
        }
    }
}

interface FactoryConfig {
    configInitMsg: {
        pair_configs: PairConfig[],
        token_code_id: number,
        init_hook?: string,
        fee_address?: string,
        gov?: string,
    }
}

interface GeneratorConfig {
    configInitMsg: {
        config: {
            allowed_reward_proxies: string[],
            astro_token: string,
            start_block: string,
            tokens_per_block: string,
            vesting_contract: string,
        }
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

interface VestingConfig {
    configInitMsg: {
        owner: string,
        token_addr: string,
    }
}

interface RouterConfig {
    configInitMsg: {
        astroport_factory: string
    }
}

interface MakerConfig {
    configInitMsg: {
        factory_contract: string,
        staking_contract: string,
        astro_token_contract: string,
    }
}

interface TokenConfig {
    configInitMsg: {
        name: string,
        symbol: string,
        decimals: number,
        initial_balances: [
            {
                address: string,
                amount: string
            }
        ],
        mint: {
            minter: string,
            cap: string
        }
    }
}

interface Config {
    factoryConfig: FactoryConfig,
    stakingConfig: StakingConfig,
    generatorConfig: GeneratorConfig,
    astroTokenContractAddress: string | undefined,
    registerVestingAccounts: RegisterVestingAccounts,
    tokenConfig: TokenConfig,
    routerConfig: RouterConfig,
    vestingConfig: VestingConfig,
    makerConfig: MakerConfig,
}
