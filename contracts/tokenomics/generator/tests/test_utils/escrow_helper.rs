use anyhow::Result;
use astroport::{staking as xastro, token as astro};
use astroport_governance::voting_escrow::{
    Cw20HookMsg, ExecuteMsg, InstantiateMsg, LockInfoResponse, QueryMsg, VotingPowerResponse,
};
use cosmwasm_std::{attr, to_binary, Addr, QueryRequest, StdResult, Uint128, WasmQuery};
use cw20::{BalanceResponse, Cw20ExecuteMsg, Cw20QueryMsg, MinterResponse};
use terra_multi_test::{AppResponse, ContractWrapper, Executor, TerraApp};

pub const MULTIPLIER: u64 = 1000000;

pub struct EscrowHelper {
    pub owner: Addr,
    pub astro_token: Addr,
    pub staking_instance: Addr,
    pub xastro_token: Addr,
    pub escrow_instance: Addr,
    pub astro_token_code_id: u64,
}

impl EscrowHelper {
    pub fn init(router: &mut TerraApp, owner: Addr) -> Self {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_token::contract::execute,
            astroport_token::contract::instantiate,
            astroport_token::contract::query,
        ));

        let astro_token_code_id = router.store_code(astro_token_contract);

        let msg = astro::InstantiateMsg {
            name: String::from("Astro token"),
            symbol: String::from("ASTRO"),
            decimals: 6,
            initial_balances: vec![],
            mint: Some(MinterResponse {
                minter: owner.to_string(),
                cap: None,
            }),
        };

        let astro_token = router
            .instantiate_contract(
                astro_token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                None,
            )
            .unwrap();

        let staking_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_staking::contract::execute,
                astroport_staking::contract::instantiate,
                astroport_staking::contract::query,
            )
            .with_reply_empty(astroport_staking::contract::reply),
        );

        let staking_code_id = router.store_code(staking_contract);

        let msg = xastro::InstantiateMsg {
            owner: owner.to_string(),
            token_code_id: astro_token_code_id,
            deposit_token_addr: astro_token.to_string(),
        };
        let staking_instance = router
            .instantiate_contract(
                staking_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("xASTRO"),
                None,
            )
            .unwrap();

        let res = router
            .wrap()
            .query::<xastro::ConfigResponse>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: staking_instance.to_string(),
                msg: to_binary(&xastro::QueryMsg::Config {}).unwrap(),
            }))
            .unwrap();

        let voting_contract = Box::new(ContractWrapper::new_with_empty(
            voting_escrow::contract::execute,
            voting_escrow::contract::instantiate,
            voting_escrow::contract::query,
        ));

        let voting_code_id = router.store_code(voting_contract);

        let msg = InstantiateMsg {
            owner: owner.to_string(),
            guardian_addr: "guardian".to_string(),
            deposit_token_addr: res.share_token_addr.to_string(),
            marketing: None,
        };
        let voting_instance = router
            .instantiate_contract(
                voting_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("vxASTRO"),
                None,
            )
            .unwrap();

        Self {
            owner,
            xastro_token: res.share_token_addr,
            astro_token,
            staking_instance,
            escrow_instance: voting_instance,
            astro_token_code_id,
        }
    }

    pub fn mint_xastro(&self, router: &mut TerraApp, to: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
        let msg = Cw20ExecuteMsg::Mint {
            recipient: String::from(to),
            amount: Uint128::from(amount),
        };
        let res = router
            .execute_contract(self.owner.clone(), self.astro_token.clone(), &msg, &[])
            .unwrap();
        assert_eq!(res.events[1].attributes[1], attr("action", "mint"));
        assert_eq!(res.events[1].attributes[2], attr("to", String::from(to)));
        assert_eq!(
            res.events[1].attributes[3],
            attr("amount", Uint128::from(amount))
        );

        let to_addr = Addr::unchecked(to);
        let msg = Cw20ExecuteMsg::Send {
            contract: self.staking_instance.to_string(),
            msg: to_binary(&xastro::Cw20HookMsg::Enter {}).unwrap(),
            amount: Uint128::from(amount),
        };
        router
            .execute_contract(to_addr, self.astro_token.clone(), &msg, &[])
            .unwrap();
    }

    pub fn check_xastro_balance(&self, router: &mut TerraApp, user: &str, amount: u64) {
        let amount = amount * MULTIPLIER;
        let res: BalanceResponse = router
            .wrap()
            .query_wasm_smart(
                self.xastro_token.clone(),
                &Cw20QueryMsg::Balance {
                    address: user.to_string(),
                },
            )
            .unwrap();
        assert_eq!(res.balance.u128(), amount as u128);
    }

    pub fn create_lock(
        &self,
        router: &mut TerraApp,
        user: &str,
        time: u64,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.escrow_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::CreateLock { time }).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_amount(
        &self,
        router: &mut TerraApp,
        user: &str,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.escrow_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::ExtendLockAmount {}).unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(user),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn deposit_for(
        &self,
        router: &mut TerraApp,
        from: &str,
        to: &str,
        amount: f32,
    ) -> Result<AppResponse> {
        let amount = (amount * MULTIPLIER as f32) as u64;
        let cw20msg = Cw20ExecuteMsg::Send {
            contract: self.escrow_instance.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::DepositFor {
                user: to.to_string(),
            })
            .unwrap(),
        };
        router.execute_contract(
            Addr::unchecked(from),
            self.xastro_token.clone(),
            &cw20msg,
            &[],
        )
    }

    pub fn extend_lock_time(
        &self,
        router: &mut TerraApp,
        user: &str,
        time: u64,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.escrow_instance.clone(),
            &ExecuteMsg::ExtendLockTime { time },
            &[],
        )
    }

    pub fn withdraw(&self, router: &mut TerraApp, user: &str) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.escrow_instance.clone(),
            &ExecuteMsg::Withdraw {},
            &[],
        )
    }

    pub fn update_blacklist(
        &self,
        router: &mut TerraApp,
        append_addrs: Option<Vec<String>>,
        remove_addrs: Option<Vec<String>>,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked("owner"),
            self.escrow_instance.clone(),
            &ExecuteMsg::UpdateBlacklist {
                append_addrs,
                remove_addrs,
            },
            &[],
        )
    }

    pub fn query_user_vp(&self, router: &mut TerraApp, user: &str) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.escrow_instance.clone(),
                &QueryMsg::UserVotingPower {
                    user: user.to_string(),
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_vp_at(&self, router: &mut TerraApp, user: &str, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.escrow_instance.clone(),
                &QueryMsg::UserVotingPowerAt {
                    user: user.to_string(),
                    time,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_user_vp_at_period(
        &self,
        router: &mut TerraApp,
        user: &str,
        period: u64,
    ) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.escrow_instance.clone(),
                &QueryMsg::UserVotingPowerAtPeriod {
                    user: user.to_string(),
                    period,
                },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp(&self, router: &mut TerraApp) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(self.escrow_instance.clone(), &QueryMsg::TotalVotingPower {})
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at(&self, router: &mut TerraApp, time: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.escrow_instance.clone(),
                &QueryMsg::TotalVotingPowerAt { time },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_total_vp_at_period(&self, router: &mut TerraApp, period: u64) -> StdResult<f32> {
        router
            .wrap()
            .query_wasm_smart(
                self.escrow_instance.clone(),
                &QueryMsg::TotalVotingPowerAtPeriod { period },
            )
            .map(|vp: VotingPowerResponse| vp.voting_power.u128() as f32 / MULTIPLIER as f32)
    }

    pub fn query_lock_info(
        &self,
        router: &mut TerraApp,
        user: &str,
    ) -> StdResult<LockInfoResponse> {
        router.wrap().query_wasm_smart(
            self.escrow_instance.clone(),
            &QueryMsg::LockInfo {
                user: user.to_string(),
            },
        )
    }
}
