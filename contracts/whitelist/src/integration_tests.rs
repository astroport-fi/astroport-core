use anyhow::{anyhow, Result};
use assert_matches::assert_matches;
use astroport::whitelist::{AdminListResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use cosmwasm_std::{to_binary, Addr, CosmosMsg, WasmMsg, Coin};
use cw1::Cw1Contract;
use derivative::Derivative;
use serde::{de::DeserializeOwned, Serialize};

use classic_test_tube::{TerraTestApp, SigningAccount, Wasm, Module, Account, cosmrs::proto::cosmwasm::wasm::v1::MsgExecuteContractResponse, ExecuteResponse, RunnerError};

fn contract_cw1() -> Vec<u8> {
    std::fs::read("../../../../artifacts/cw1.wasm").unwrap()
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Suite<'a> {
    /// Application mock
    app: &'a TerraTestApp,
    /// Wasm mock
    #[derivative(Debug = "ignore")]
    wasm: Wasm<'a, TerraTestApp>,
    /// Special account
    #[derivative(Debug = "ignore")]
    pub owner: SigningAccount,
    /// ID of stored code for cw1 contract
    cw1_id: u64,
}

impl<'a> Suite<'a> {
    pub fn init(app: &'a TerraTestApp) -> Result<Suite<'a>> {
        let wasm = Wasm::new(app);

        // Set balances
        let owner = app.init_account(
            &[
                Coin::new(200u128, "uusd"),
                Coin::new(200u128, "uluna"),
            ]
        ).unwrap();

        let cw1_id = wasm.store_code(&contract_cw1(), None, &owner).unwrap().data.code_id;

        Ok(Suite { app, wasm, owner, cw1_id })
    }

    pub fn instantiate_cw1_contract(&mut self, admins: Vec<String>, mutable: bool) -> Cw1Contract {
        let contract = Addr::unchecked(self.wasm.instantiate(
            self.cw1_id, 
            &InstantiateMsg { admins, mutable }, 
            Some(&self.owner.address()), 
            Some("Whitelist"), 
            &[], 
            &self.owner,
        ).unwrap().data.address);
        Cw1Contract(contract)
    }

    pub fn execute<M>(
        &mut self,
        sender_contract: Addr,
        target_contract: &Addr,
        msg: M,
    ) -> Result<ExecuteResponse<MsgExecuteContractResponse>>
    where
        M: Serialize + DeserializeOwned,
    {
        let execute: ExecuteMsg = ExecuteMsg::Execute {
            msgs: vec![CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: target_contract.to_string(),
                msg: to_binary(&msg)?,
                funds: vec![],
            })],
        };
        self.wasm.execute(
            sender_contract.as_str(), 
            &execute, 
            &[], 
            &self.owner
        )
        .map_err(|err| anyhow!(err))
    }

    pub fn query<M>(&self, target_contract: Addr, msg: M) -> Result<AdminListResponse, RunnerError>
    where
        M: Serialize + DeserializeOwned,
    {
        self.wasm.query::<M, AdminListResponse>(target_contract.as_str(), &msg)
    }
}

#[test]
fn proxy_freeze_message() {
    let app = TerraTestApp::new();
    let mut suite = Suite::init(&app).unwrap();

    let first_contract = suite.instantiate_cw1_contract(vec![suite.owner.address()], true);
    let second_contract =
        suite.instantiate_cw1_contract(vec![first_contract.addr().to_string()], true);
    assert_ne!(second_contract, first_contract);

    let freeze_msg: ExecuteMsg = ExecuteMsg::Freeze {};
    assert_matches!(
        suite.execute(first_contract.addr(), &second_contract.addr(), freeze_msg),
        Ok(_)
    );

    let query_msg: QueryMsg = QueryMsg::AdminList {};
    assert_matches!(
        suite.query(second_contract.addr(), query_msg),
        Ok(
            AdminListResponse {
                mutable,
                ..
            }) => {
            assert!(!mutable)
        }
    );
}
