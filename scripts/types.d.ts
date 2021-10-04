interface StakingInitMsg{
    config: {
        token_code_id: number
        deposit_token_addr?: string
    }
}

interface FactoryInitMsg{
    config: {
        pair_configs: PairConfig[],
        token_code_id: number,
        init_hook?: string,
        fee_address?: string,
        gov?: string,
    }
}

interface GaugeInitMsd{
    config:{
        token?: string,
        dev_addr?: string,
        tokens_per_block: string,
        start_block: number,
        bonus_end_block: number,
    }
}

type PairType = {
    xyk:{}
} | {
    stable: {}
}


interface PairConfig{
    code_id: number,
    pair_type: PairType,
    total_fee_bps: number,
    maker_fee_bps: number
}

type StartDate = string
type EndDate = string
type Amount = string

type VestingAccountSchedule = [StartDate, EndDate, Amount]

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
    gaugeInitMsg: GaugeInitMsd,
    astroTokenContractAddress: string | undefined
    registerVestingAccounts: RegisterVestingAccounts
}
