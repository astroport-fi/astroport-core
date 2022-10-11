use anyhow::Result;
use astroport_governance::voting_escrow_delegation as escrow_delegation;
use cosmwasm_std::{to_binary, Addr, Empty, QueryRequest, StdResult, Uint128, WasmQuery};
use cw_multi_test::{App, AppResponse, Contract, ContractWrapper, Executor};

use cw721_base::helpers::Cw721Contract;

pub struct DelegationHelper {
    pub delegation_instance: Addr,
    pub nft_instance: Addr,
    pub nft_helper: Cw721Contract<Empty, Empty>,
}

impl DelegationHelper {
    pub fn contract_escrow_delegation_template() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new_with_empty(
            voting_escrow_delegation::contract::execute,
            voting_escrow_delegation::contract::instantiate,
            voting_escrow_delegation::contract::query,
        )
        .with_reply_empty(voting_escrow_delegation::contract::reply);
        Box::new(contract)
    }

    pub fn contract_nft_template() -> Box<dyn Contract<Empty>> {
        let contract = ContractWrapper::new(
            astroport_nft::contract::execute,
            astroport_nft::contract::instantiate,
            astroport_nft::contract::query,
        );
        Box::new(contract)
    }

    fn instantiate_delegation(
        router: &mut App,
        owner: Addr,
        escrow_addr: Addr,
        delegation_id: u64,
        nft_id: u64,
    ) -> (Addr, Addr) {
        let delegation_addr = router
            .instantiate_contract(
                delegation_id,
                owner.clone(),
                &escrow_delegation::InstantiateMsg {
                    owner: owner.to_string(),
                    nft_code_id: nft_id,
                    voting_escrow_addr: escrow_addr.to_string(),
                },
                &[],
                String::from("Astroport Escrow Delegation"),
                None,
            )
            .unwrap();

        let res = router
            .wrap()
            .query::<astroport_governance::voting_escrow_delegation::Config>(&QueryRequest::Wasm(
                WasmQuery::Smart {
                    contract_addr: delegation_addr.to_string(),
                    msg: to_binary(&escrow_delegation::QueryMsg::Config {}).unwrap(),
                },
            ))
            .unwrap();

        (delegation_addr, res.nft_addr)
    }

    pub fn init(router: &mut App, owner: Addr, escrow_addr: Addr) -> Self {
        let delegation_id =
            router.store_code(DelegationHelper::contract_escrow_delegation_template());
        let nft_id = router.store_code(DelegationHelper::contract_nft_template());

        let (delegation_addr, nft_addr) = DelegationHelper::instantiate_delegation(
            router,
            owner,
            escrow_addr,
            delegation_id,
            nft_id,
        );

        let nft_helper = cw721_base::helpers::Cw721Contract(
            nft_addr.clone(),
            Default::default(),
            Default::default(),
        );

        DelegationHelper {
            delegation_instance: delegation_addr,
            nft_instance: nft_addr,
            nft_helper,
        }
    }

    pub fn create_delegation(
        &self,
        router: &mut App,
        user: &str,
        bps: u16,
        expire_time: u64,
        token_id: String,
        recipient: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.delegation_instance.clone(),
            &escrow_delegation::ExecuteMsg::CreateDelegation {
                bps,
                expire_time,
                token_id,
                recipient,
            },
            &[],
        )
    }

    pub fn extend_delegation(
        &self,
        router: &mut App,
        user: &str,
        bps: u16,
        expire_time: u64,
        token_id: String,
    ) -> Result<AppResponse> {
        router.execute_contract(
            Addr::unchecked(user),
            self.delegation_instance.clone(),
            &escrow_delegation::ExecuteMsg::ExtendDelegation {
                bps,
                expire_time,
                token_id,
            },
            &[],
        )
    }

    pub fn adjusted_balance(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Option<u64>,
    ) -> StdResult<Uint128> {
        router
            .wrap()
            .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.delegation_instance.to_string(),
                msg: to_binary(&escrow_delegation::QueryMsg::AdjustedBalance {
                    account: user.to_string(),
                    timestamp,
                })
                .unwrap(),
            }))
    }

    pub fn delegated_balance(
        &self,
        router: &mut App,
        user: &str,
        timestamp: Option<u64>,
    ) -> StdResult<Uint128> {
        router
            .wrap()
            .query::<Uint128>(&QueryRequest::Wasm(WasmQuery::Smart {
                contract_addr: self.delegation_instance.to_string(),
                msg: to_binary(&escrow_delegation::QueryMsg::DelegatedVotingPower {
                    account: user.to_string(),
                    timestamp,
                })
                .unwrap(),
            }))
    }
}
