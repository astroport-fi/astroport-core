use crate::test_utils::escrow_helper::EscrowHelper;
use crate::{mint_tokens, store_whitelist_code};
use anyhow::Result as AnyResult;
use astroport::asset::{token_asset_info, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::vesting::{Cw20HookMsg as VestingHookMsg, VestingAccount};
use astroport::vesting::{InstantiateMsg, VestingSchedule, VestingSchedulePoint};
use astroport_governance::generator_controller::{ExecuteMsg, QueryMsg};
use astroport_governance::generator_controller::{UserInfoResponse, VotedPoolInfoResponse};
use cosmwasm_std::{to_binary, Addr, StdResult, Uint128, Uint64};
use cw20::Cw20ExecuteMsg;
use cw_multi_test::{App, AppResponse, ContractWrapper, Executor};

pub struct ControllerHelper {
    pub owner: String,
    pub generator: Addr,
    pub controller: Addr,
    pub factory: Addr,
    pub escrow_helper: EscrowHelper,
    pub vesting: Addr,
}

impl ControllerHelper {
    pub fn init(router: &mut App, owner: &Addr) -> Self {
        let escrow_helper = EscrowHelper::init(router, owner.clone());

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair::contract::execute,
                astroport_pair::contract::instantiate,
                astroport_pair::contract::query,
            )
            .with_reply_empty(astroport_pair::contract::reply),
        );

        let pair_code_id = router.store_code(pair_contract);

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );

        let factory_code_id = router.store_code(factory_contract);

        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                pair_type: PairType::Xyk {},
                total_fee_bps: 100,
                maker_fee_bps: 10,
                is_disabled: false,
                is_generator_disabled: false,
            }],
            token_code_id: escrow_helper.astro_token_code_id,
            fee_address: None,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
        };

        let factory = router
            .instantiate_contract(factory_code_id, owner.clone(), &msg, &[], "Factory", None)
            .unwrap();

        let generator_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_generator::contract::execute,
                astroport_generator::contract::instantiate,
                astroport_generator::contract::query,
            )
            .with_reply_empty(astroport_generator::contract::reply),
        );

        let vesting_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_vesting::contract::execute,
            astroport_vesting::contract::instantiate,
            astroport_vesting::contract::query,
        ));
        let vesting_code_id = router.store_code(vesting_contract);

        let init_msg_vesting = InstantiateMsg {
            owner: owner.to_string(),
            vesting_token: token_asset_info(escrow_helper.astro_token.clone()),
        };

        let vesting_instance = router
            .instantiate_contract(
                vesting_code_id,
                owner.clone(),
                &init_msg_vesting,
                &[],
                "Vesting",
                None,
            )
            .unwrap();

        let whitelist_code_id = store_whitelist_code(router);
        let generator_code_id = router.store_code(generator_contract);

        let init_msg = astroport::generator::InstantiateMsg {
            owner: owner.to_string(),
            factory: factory.to_string(),
            generator_controller: None,
            voting_escrow: Some(escrow_helper.escrow_instance.to_string()),
            guardian: None,
            astro_token: token_asset_info(escrow_helper.astro_token.clone()),
            tokens_per_block: Uint128::new(10_000000),
            start_block: Uint64::from(router.block_info().height),
            vesting_contract: vesting_instance.to_string(),
            whitelist_code_id,
        };

        let generator = router
            .instantiate_contract(
                generator_code_id,
                owner.clone(),
                &init_msg,
                &[],
                String::from("Generator"),
                None,
            )
            .unwrap();

        let controller_contract = Box::new(ContractWrapper::new_with_empty(
            generator_controller::contract::execute,
            generator_controller::contract::instantiate,
            generator_controller::contract::query,
        ));

        let controller_code_id = router.store_code(controller_contract);
        let init_msg = astroport_governance::generator_controller::InstantiateMsg {
            owner: owner.to_string(),
            escrow_addr: escrow_helper.escrow_instance.to_string(),
            generator_addr: generator.to_string(),
            factory_addr: factory.to_string(),
            pools_limit: 5,
        };

        let controller = router
            .instantiate_contract(
                controller_code_id,
                owner.clone(),
                &init_msg,
                &[],
                String::from("Controller"),
                None,
            )
            .unwrap();

        mint_tokens(
            router,
            owner.clone(),
            &escrow_helper.astro_token,
            &owner,
            1_000_000_000_000000,
        );

        // Register vesting account
        let msg = Cw20ExecuteMsg::Send {
            contract: vesting_instance.to_string(),
            msg: to_binary(&VestingHookMsg::RegisterVestingAccounts {
                vesting_accounts: vec![VestingAccount {
                    address: generator.to_string(),
                    schedules: vec![VestingSchedule {
                        start_point: VestingSchedulePoint {
                            time: router.block_info().time.seconds(),
                            amount: Uint128::new(100000_000000),
                        },
                        end_point: None,
                    }],
                }],
            })
            .unwrap(),
            amount: Uint128::new(100000_000000),
        };

        router
            .execute_contract(owner.clone(), escrow_helper.astro_token.clone(), &msg, &[])
            .unwrap();

        // Setup controller in generator contract
        router
            .execute_contract(
                owner.clone(),
                generator.clone(),
                &astroport::generator::ExecuteMsg::UpdateConfig {
                    vesting_contract: None,
                    generator_controller: Some(controller.to_string()),
                    guardian: None,
                    voting_escrow: None,
                    checkpoint_generator_limit: None,
                },
                &[],
            )
            .unwrap();

        Self {
            owner: owner.to_string(),
            generator,
            controller,
            factory,
            escrow_helper,
            vesting: vesting_instance,
        }
    }

    pub fn init_cw20_token(&self, router: &mut App, name: &str) -> AnyResult<Addr> {
        let msg = astroport::token::InstantiateMsg {
            name: name.to_string(),
            symbol: name.to_string(),
            decimals: 6,
            initial_balances: vec![],
            mint: None,
            marketing: None,
        };

        router.instantiate_contract(
            self.escrow_helper.astro_token_code_id,
            Addr::unchecked(self.owner.clone()),
            &msg,
            &[],
            name.to_string(),
            None,
        )
    }

    pub fn create_pool(&self, router: &mut App, token1: &Addr, token2: &Addr) -> AnyResult<Addr> {
        let asset_infos = [
            AssetInfo::Token {
                contract_addr: token1.clone(),
            },
            AssetInfo::Token {
                contract_addr: token2.clone(),
            },
        ];

        router.execute_contract(
            Addr::unchecked(self.owner.clone()),
            self.factory.clone(),
            &astroport::factory::ExecuteMsg::CreatePair {
                pair_type: PairType::Xyk {},
                asset_infos: asset_infos.clone(),
                init_params: None,
            },
            &[],
        )?;

        let res: PairInfo = router.wrap().query_wasm_smart(
            self.factory.clone(),
            &astroport::factory::QueryMsg::Pair { asset_infos },
        )?;

        Ok(res.liquidity_token)
    }

    pub fn create_pool_with_tokens(
        &self,
        router: &mut App,
        name1: &str,
        name2: &str,
    ) -> AnyResult<Addr> {
        let token1 = self.init_cw20_token(router, name1).unwrap();
        let token2 = self.init_cw20_token(router, name2).unwrap();

        self.create_pool(router, &token1, &token2)
    }

    pub fn vote(
        &self,
        router: &mut App,
        user: &str,
        votes: Vec<(impl Into<String>, u16)>,
    ) -> AnyResult<AppResponse> {
        let msg = ExecuteMsg::Vote {
            votes: votes
                .into_iter()
                .map(|(pool, apoints)| (pool.into(), apoints))
                .collect(),
        };

        router.execute_contract(Addr::unchecked(user), self.controller.clone(), &msg, &[])
    }

    pub fn gauge(&self, router: &mut App, sender: &str) -> AnyResult<AppResponse> {
        router.execute_contract(
            Addr::unchecked(sender),
            self.controller.clone(),
            &ExecuteMsg::TunePools {},
            &[],
        )
    }

    pub fn query_user_info(&self, router: &mut App, user: &str) -> StdResult<UserInfoResponse> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::UserInfo {
                user: user.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info(
        &self,
        router: &mut App,
        pool: &str,
    ) -> StdResult<VotedPoolInfoResponse> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::PoolInfo {
                pool_addr: pool.to_string(),
            },
        )
    }

    pub fn query_voted_pool_info_at_period(
        &self,
        router: &mut App,
        pool: &str,
        period: u64,
    ) -> StdResult<VotedPoolInfoResponse> {
        router.wrap().query_wasm_smart(
            self.controller.clone(),
            &QueryMsg::PoolInfoAtPeriod {
                pool_addr: pool.to_string(),
                period,
            },
        )
    }
}
