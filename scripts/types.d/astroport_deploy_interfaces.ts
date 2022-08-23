interface Treasury {
    admin: string,
    initMsg: {
        admins: string[],
        mutable: boolean
    },
    label: string
}

interface Staking {
    admin: string,
    initMsg: {
        owner: string,
        token_code_id: number,
        deposit_token_addr: string,
        marketing: {
            project: string,
            description: string,
            marketing: string,
            logo: {
                url: string
            }
        }
    },
    label: string
}

interface Factory {
    admin: string,
    initMsg: {
        owner: string,
        pair_configs: PairConfig[],
        token_code_id: number,
        fee_address?: string,
        generator_address?: string,
        whitelist_code_id: number
    },
    label: string,
    proposeNewOwner: {
        owner: string,
        expires_in: string
    }
}

interface Router {
    admin: string,
    initMsg: {
        astroport_factory: string
    },
    label: string
}

interface Maker {
    admin: string,
    initMsg: {
        owner: string,
        factory_contract: string,
        staking_contract: string,
        astro_token_contract: string,
        governance_contract?: string,
        governance_percent?: string,
        max_spread: "0.5"
    },
    label: string
}

interface Vesting {
    admin: string,
    initMsg: {
        owner: string,
        token_addr: string,
    },
    label: string
}

interface Generator {
    admin: string,
    initMsg: {
        owner: string,
        allowed_reward_proxies: string[],
        astro_token: string,
        start_block: string,
        tokens_per_block: string,
        vesting_contract: string,
        factory: string,
        whitelist_code_id: number,
    },
    label: string
}

interface PairConfig {
    code_id: number,
    pair_type: { xyk: {} } | { stable: {}},
    total_fee_bps: number,
    maker_fee_bps: number
    is_disabled: false,
    is_generator_disabled: false
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
    treasury: Treasury,
    staking: Staking,
    factory: Factory,
    router: Router,
    maker: Maker,
    vesting: Vesting,
    generator: Generator,
    registerVestingAccounts?: RegisterVestingAccounts
}