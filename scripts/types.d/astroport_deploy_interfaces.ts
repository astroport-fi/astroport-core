interface GeneralInfo {
    multisig: string
}

type InitialBalance = {
    address: string,
    amount: string
}

type Marketing = {
    project: string,
    description: string,
    marketing: string,
    logo: {
        url: string
    }
}

interface Token {
    admin: string,
    initMsg: {
        name: string,
        symbol: string,
        decimals: number,
        initial_balances: InitialBalance[],
        marketing: Marketing
    },
    label: string
}

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
        marketing: Marketing
    },
    label: string
}

interface PairConfig {
    code_id: number,
    pair_type: { xyk: {} } | { stable: {} },
    total_fee_bps: number,
    maker_fee_bps: number,
    is_disabled: boolean,
    is_generator_disabled: boolean
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
    change_owner: boolean,
    proposeNewOwner: {
        owner: string,
        expires_in: number
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
        astro_token: NativeAsset | TokenAsset,
        governance_contract?: string,
        governance_percent?: string,
        max_spread: "0.5"
    },
    label: string
}

type VestingAccountSchedule = {
    start_point: {
        time: string,
        amount: string
    },
    end_point?: {
        time: string,
        amount: string
    }
}

interface VestingAccount {
    address: string
    schedules: VestingAccountSchedule[]
}

interface Vesting {
    admin: string,
    initMsg: {
        owner: string,
        vesting_token: NativeAsset | TokenAsset,
    },
    label: string,
    registration: {
        msg: {
            register_vesting_accounts: {
                vesting_accounts: VestingAccount[]
            }
        },
        amount: string
    }
}

interface Generator {
    admin: string,
    initMsg: {
        owner: string,
        astro_token: NativeAsset | TokenAsset,
        start_block: string,
        tokens_per_block: string,
        vesting_contract: string,
        factory: string,
        whitelist_code_id: number,
    },
    label: string,
    change_owner: boolean,
    proposeNewOwner: {
        owner: string,
        expires_in: number
    }
}

interface GeneratorProxy {
    admin: string,
    initMsg: {
        generator_contract_addr: string,
        pair_addr: string,
        lp_token_addr: string,
        reward_contract_addr: string,
        reward_token_addr: string
    },
    label: string
}

type NativeAsset = {
    native_token: {
        denom: string,
    }
}

type TokenAsset = {
    token: {
        contract_addr: string
    }
}

interface Pair {
    identifier: string,
    assetInfos: (NativeAsset | TokenAsset)[],
    pairType: { xyk: {} } | { stable: {} },
    initParams?: any,
    initOracle?: boolean,
    initGenerator?: {
        generatorAllocPoint: string
    }
}

interface CreatePairs {
    pairs: Pair[]
}

interface Oracle {
    admin: string,
    initMsg: {
        factory_contract: string,
        asset_infos: (NativeAsset | TokenAsset)[]
    },
    label: string
}

interface Config {
    token: Token,
    treasury: Treasury,
    staking: Staking,
    factory: Factory,
    router: Router,
    maker: Maker,
    vesting: Vesting,
    generator: Generator,
    generatorProxy: GeneratorProxy,
    createPairs: CreatePairs,
    oracle: Oracle,
    generalInfo: GeneralInfo
}