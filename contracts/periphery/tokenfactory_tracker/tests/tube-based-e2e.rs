#![cfg(not(tarpaulin_include))]
#![cfg(feature = "tests-tube")]
#![allow(dead_code)]

use std::collections::HashMap;

use cosmwasm_std::{coin, Uint128};
use neutron_std::types::cosmos::base::v1beta1::Coin as NeutronCoin;
use neutron_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgCreateDenom, MsgMint, MsgSetBeforeSendHook, MsgSetBeforeSendHookResponse,
    MsgUpdateParams,
};
use neutron_test_tube::cosmrs::proto::cosmos::bank::v1beta1::{
    MsgSend, MsgSendResponse, QueryBalanceRequest,
};
use neutron_test_tube::cosmrs::proto::prost::Message;
use neutron_test_tube::{Adminmodule, Bank, NeutronTestApp, TokenFactory, Wasm};
use test_tube::cosmrs::proto::cosmos::auth::v1beta1::{
    ModuleAccount, QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse,
};
use test_tube::cosmrs::proto::cosmos::base::v1beta1::Coin as CosmosCoin;
use test_tube::{Account, Module, Runner, RunnerExecuteResult, RunnerResult, SigningAccount};

use astroport::tokenfactory_tracker::{InstantiateMsg, QueryMsg};
use neutron_std::shim::Any;
use neutron_std::types::cosmos::adminmodule::adminmodule::MsgSubmitProposal;
use neutron_std::types::osmosis::tokenfactory::{Params, WhitelistedHook};

const TRACKER_WASM: &str = "./tests/test_data/astroport_tokenfactory_tracker.wasm";

const ADMIN_MODULE_ADDR: &str = "neutron1hxskfdxpp5hqgtjj6am6nkjefhfzj359x0ar3z";

fn proto_coin(amount: u128, denom: &str) -> NeutronCoin {
    NeutronCoin {
        denom: denom.to_string(),
        amount: amount.to_string(),
    }
}

struct TestSuite<'a> {
    wasm: Wasm<'a, NeutronTestApp>,
    bank: Bank<'a, NeutronTestApp>,
    tf: TokenFactory<'a, NeutronTestApp>,
    adm: Adminmodule<'a, NeutronTestApp>,
    owner: SigningAccount,
    tokenfactory_module_address: String,
}

impl<'a> TestSuite<'a> {
    fn new(app: &'a NeutronTestApp) -> Self {
        let wasm = Wasm::new(app);
        let bank = Bank::new(app);
        let adm = Adminmodule::new(app);
        let tf = TokenFactory::new(app);
        let signer = app
            .init_admin_account(&[coin(1_500_000e6 as u128, "untrn")])
            .unwrap();

        let module_account = ModuleAccount::decode(
            app.query::<QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse>(
                "/cosmos.auth.v1beta1.Query/ModuleAccountByName",
                &QueryModuleAccountByNameRequest {
                    name: "tokenfactory".to_string(),
                },
            )
            .unwrap()
            .account
            .unwrap()
            .value
            .as_slice(),
        )
        .unwrap();

        Self {
            wasm,
            bank,
            tf,
            adm,
            owner: signer,
            tokenfactory_module_address: module_account.base_account.unwrap().address,
        }
    }

    fn create_denom(&self, subdenom: &str) -> String {
        let denom = self
            .tf
            .create_denom(
                MsgCreateDenom {
                    sender: self.owner.address(),
                    subdenom: subdenom.to_string(),
                },
                &self.owner,
            )
            .unwrap()
            .data
            .new_token_denom;

        denom
    }

    fn mint(&self, denom: &str, amount: impl Into<Uint128>, to: &str) {
        let amount: Uint128 = amount.into();

        // Pass through minter
        self.tf
            .mint(
                MsgMint {
                    sender: self.owner.address(),
                    amount: Some(proto_coin(amount.u128(), denom)),
                    mint_to_address: self.owner.address(),
                },
                &self.owner,
            )
            .unwrap();

        // Send to user
        self.send(
            &self.owner,
            to.to_string(),
            proto_coin(amount.u128(), denom),
        )
        .unwrap();
    }

    fn burn(&self, denom: &str, amount: impl Into<Uint128>) {
        let amount: Uint128 = amount.into();

        self.tf
            .burn(
                MsgBurn {
                    sender: self.owner.address(),
                    amount: Some(proto_coin(amount.u128(), &denom)),
                    burn_from_address: self.owner.address(),
                },
                &self.owner,
            )
            .unwrap();

        // Trigger hook
        self.send(&self.owner, self.owner.address(), proto_coin(1, denom))
            .unwrap();
    }

    fn send(
        &self,
        signer: &SigningAccount,
        to_address: String,
        amount: NeutronCoin,
    ) -> RunnerExecuteResult<MsgSendResponse> {
        self.bank.send(
            MsgSend {
                from_address: signer.address(),
                to_address,
                amount: vec![CosmosCoin {
                    denom: amount.denom,
                    amount: amount.amount,
                }],
            },
            signer,
        )
    }

    fn instantiate_tracker(&self, denom: &str) -> (u64, String) {
        let code_id = self
            .wasm
            .store_code(&std::fs::read(TRACKER_WASM).unwrap(), None, &self.owner)
            .unwrap()
            .data
            .code_id;

        let init_msg = InstantiateMsg {
            tokenfactory_module_address: self.tokenfactory_module_address.clone(),
            tracked_denom: denom.to_string(),
            track_over_seconds: true,
        };
        let tracker_addr = self
            .wasm
            .instantiate(code_id, &init_msg, None, Some("label"), &[], &self.owner)
            .unwrap()
            .data
            .address;

        (code_id, tracker_addr)
    }

    fn set_before_send_hook(&self, denom: &str, app: &NeutronTestApp) -> String {
        let (code_id, tracker_addr) = self.instantiate_tracker(denom);

        // Tokenfactory update params message
        let tfmsg = MsgUpdateParams {
            authority: ADMIN_MODULE_ADDR.to_string(),
            params: Some(Params {
                // set proper params & hooks below
                denom_creation_fee: vec![],
                denom_creation_gas_consume: Some(0),
                fee_collector_address: "".to_string(),
                whitelisted_hooks: vec![WhitelistedHook {
                    code_id,
                    denom_creator: self.owner.address(),
                }],
            }),
        };

        // encode it to Any
        let tfmsg_any = Any {
            type_url: "/osmosis.tokenfactory.v1beta1.MsgUpdateParams".to_string(),
            value: tfmsg.encode_to_vec(),
        };

        // submit as a proposal
        let msg = MsgSubmitProposal {
            messages: vec![tfmsg_any],
            proposer: self.owner.address(),
        };

        self.adm.submit_proposal(msg, &self.owner).unwrap();

        let set_hook_msg = MsgSetBeforeSendHook {
            sender: self.owner.address(),
            denom: denom.to_string(),
            contract_addr: tracker_addr.to_string(),
        };
        app.execute::<_, MsgSetBeforeSendHookResponse>(
            set_hook_msg,
            "/osmosis.tokenfactory.v1beta1.MsgSetBeforeSendHook",
            &self.owner,
        )
        .unwrap();

        tracker_addr
    }

    fn balance_at(
        &self,
        tracker_addr: &str,
        user: &str,
        timestamp: Option<u64>,
    ) -> RunnerResult<Uint128> {
        self.wasm.query(
            &tracker_addr,
            &QueryMsg::BalanceAt {
                address: user.to_string(),
                unit: timestamp,
            },
        )
    }

    fn supply_at(&self, tracker_addr: &str, timestamp: Option<u64>) -> RunnerResult<Uint128> {
        self.wasm
            .query(&tracker_addr, &QueryMsg::TotalSupplyAt { unit: timestamp })
    }
}

#[test]
fn ensure_tracking_on_mint() {
    let app = NeutronTestApp::new();
    let ts = TestSuite::new(&app);

    let denom = ts.create_denom("test");
    let tracker_addr = ts.set_before_send_hook(&denom, &app);

    let user = app.init_account(&[]).unwrap();

    let balance_before = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_before.u128(), 0u128);

    // Total supply is also 0
    let supply_before = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_before.u128(), 0u128);

    ts.mint(&denom, 1000u128, &user.address());

    // Move time forward so SnapshotMap can be queried
    app.increase_time(10);

    let bank_bal = ts
        .bank
        .query_balance(&QueryBalanceRequest {
            address: user.address(),
            denom: denom.clone(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount;
    assert_eq!(bank_bal, 1000u128.to_string());

    let balance_after = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_after.u128(), 1000u128);
    let supply_after = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_after.u128(), 1000u128);
}

#[test]
fn ensure_tracking_on_send() {
    let app = NeutronTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");
    let tracker_addr = ts.set_before_send_hook(&denom, &app);

    // Mint tokens to owner
    ts.mint(&denom, 1000u128, &ts.owner.address());

    let user = app.init_account(&[]).unwrap();

    let balance_before = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_before.u128(), 0u128);

    // Send owner -> user
    ts.send(&ts.owner, user.address(), proto_coin(1000u128, &denom))
        .unwrap();

    app.increase_time(10);

    let bank_bal = ts
        .bank
        .query_balance(&QueryBalanceRequest {
            address: user.address(),
            denom: denom.clone(),
        })
        .unwrap()
        .balance
        .unwrap()
        .amount;
    assert_eq!(bank_bal, 1000u128.to_string());

    let balance_after = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_after.u128(), 1000u128);
    let supply_after = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_after.u128(), 1000u128);
}

#[test]
fn ensure_tracking_on_burn() {
    let app = NeutronTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");
    let tracker_addr = ts.set_before_send_hook(&denom, &app);

    let user = app
        .init_account(&[coin(1_500_000e6 as u128, "untrn")])
        .unwrap();

    // Mint tokens to user
    ts.mint(&denom, 1000u128, &user.address());

    // Mint 1 token to owner to be able to trigger hook on burn
    ts.mint(&denom, 1u128, &ts.owner.address());

    app.increase_time(10);

    let balance_before = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_before.u128(), 1000u128);

    // Send back to minter
    ts.send(&user, ts.owner.address(), proto_coin(1000u128, &denom))
        .unwrap();
    // Burn from minter
    ts.burn(&denom, 1000u128);

    app.increase_time(10);

    let balance_after = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_after.u128(), 0u128);
    let supply_after = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_after.u128(), 1u128);
}

#[test]
fn ensure_sending_to_module_prohibited() {
    let app = NeutronTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");

    // Mint tokens to owner
    ts.mint(&denom, 1000u128, &ts.owner.address());

    // Send owner -> tokenfactory module address
    let err = ts
        .send(
            &ts.owner,
            ts.tokenfactory_module_address.clone(),
            proto_coin(1000u128, &denom),
        )
        .unwrap_err();

    assert!(
        err.to_string().contains(&format!(
            "{} is not allowed to receive funds: unauthorized",
            ts.tokenfactory_module_address
        )),
        "Unexpected error message: {err}",
    )
}

#[test]
fn test_historical_queries() {
    let app = NeutronTestApp::new();
    let ts = TestSuite::new(&app);

    let denom = ts.create_denom("test");
    let tracker_addr = ts.set_before_send_hook(&denom, &app);

    let user = app.init_account(&[]).unwrap();

    let balance_before = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_before.u128(), 0u128);
    // Total supply is also 0
    let supply_before = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_before.u128(), 0u128);

    let mut history: HashMap<u64, Uint128> = HashMap::new();
    let mut acc = 0u128;
    for i in 0..20 {
        ts.mint(&denom, 1000u128, &user.address());

        acc += 1000u128;

        let block_ts = app.get_block_time_seconds() as u64;
        // Balance change takes place in the next block. Add 1 to ensure we'll query the next block
        history.insert(block_ts + 1, acc.into());

        app.increase_time(10 * i);
    }

    // Shift time by 1 day
    app.increase_time(86400);

    for (block_ts, amount) in history {
        let balance = ts
            .balance_at(&tracker_addr, &user.address(), Some(block_ts))
            .unwrap();
        assert_eq!(balance, amount);

        let total_supply = ts.supply_at(&tracker_addr, Some(block_ts)).unwrap();
        assert_eq!(total_supply, amount);
    }
}
