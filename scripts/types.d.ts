interface StakingInitMsg{
    config: {
        token_code_id: number
        deposit_token_addr?: string
    }
}

interface Config {
    stakingInitMsg: StakingInitMsg
    astroTokenContractAddress: string | undefined
}