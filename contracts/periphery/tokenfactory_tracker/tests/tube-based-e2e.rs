use std::collections::HashMap;

use cosmwasm_std::{coin, Uint128};
use osmosis_std::types::cosmos::auth::v1beta1::{
    ModuleAccount, QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse,
};
use osmosis_std::types::cosmos::bank::v1beta1::{MsgSend, QueryBalanceRequest};
use osmosis_std::types::cosmos::base::v1beta1::Coin;
use osmosis_std::types::osmosis::tokenfactory::v1beta1::{
    MsgBurn, MsgMint, MsgSetBeforeSendHook, MsgSetBeforeSendHookResponse,
};
use osmosis_test_tube::osmosis_std::types::osmosis::tokenfactory::v1beta1::MsgCreateDenom;
use osmosis_test_tube::{OsmosisTestApp, TokenFactory};
use test_tube::{Account, Bank, Module, Runner, RunnerResult, SigningAccount, Wasm};

use astroport::tokenfactory_tracker::{InstantiateMsg, QueryMsg};

const TRACKER_WASM: &str = "./tests/test_data/astroport_tokenfactory_tracker.wasm";

struct TestSuite<'a> {
    wasm: Wasm<'a, OsmosisTestApp>,
    bank: Bank<'a, OsmosisTestApp>,
    tf: TokenFactory<'a, OsmosisTestApp>,
    owner: SigningAccount,
    tokenfactory_module_address: String,
}

impl<'a> TestSuite<'a> {
    fn new(app: &'a OsmosisTestApp) -> Self {
        let wasm = Wasm::new(app);
        let bank = Bank::new(app);
        let tf = TokenFactory::new(app);
        let signer = app
            .init_account(&[coin(1_500_000e6 as u128, "uosmo")])
            .unwrap();

        let ModuleAccount { base_account, .. } = app
            .query::<QueryModuleAccountByNameRequest, QueryModuleAccountByNameResponse>(
                "/cosmos.auth.v1beta1.Query/ModuleAccountByName",
                &QueryModuleAccountByNameRequest {
                    name: "tokenfactory".to_string(),
                },
            )
            .unwrap()
            .account
            .unwrap()
            .try_into()
            .unwrap();

        Self {
            wasm,
            bank,
            tf,
            owner: signer,
            tokenfactory_module_address: base_account.unwrap().address,
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
        self.tf
            .mint(
                MsgMint {
                    sender: self.owner.address(),
                    amount: Some(Coin {
                        denom: denom.to_string(),
                        amount: amount.to_string(),
                    }),
                    mint_to_address: to.to_string(),
                },
                &self.owner,
            )
            .unwrap();
    }

    fn instantiate_tracker(&self, denom: &str) -> String {
        let code_id = self
            .wasm
            .store_code(&std::fs::read(TRACKER_WASM).unwrap(), None, &self.owner)
            .unwrap()
            .data
            .code_id;

        let init_msg = InstantiateMsg {
            tokenfactory_module_address: self.tokenfactory_module_address.clone(),
            tracked_denom: denom.to_string(),
        };
        let tracker_addr = self
            .wasm
            .instantiate(code_id, &init_msg, None, Some("label"), &[], &self.owner)
            .unwrap()
            .data
            .address;

        tracker_addr
    }

    fn set_before_send_hook(&self, denom: &str, tracker_addr: &str, app: &OsmosisTestApp) {
        let set_hook_msg = MsgSetBeforeSendHook {
            sender: self.owner.address(),
            denom: denom.to_string(),
            cosmwasm_address: tracker_addr.to_string(),
        };
        app.execute::<_, MsgSetBeforeSendHookResponse>(
            set_hook_msg,
            MsgSetBeforeSendHook::TYPE_URL,
            &self.owner,
        )
        .unwrap();
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
                timestamp,
            },
        )
    }

    fn supply_at(&self, tracker_addr: &str, timestamp: Option<u64>) -> RunnerResult<Uint128> {
        self.wasm
            .query(&tracker_addr, &QueryMsg::TotalSupplyAt { timestamp })
    }
}

#[test]
fn ensure_tracking_on_mint() {
    let app = OsmosisTestApp::new();
    let ts = TestSuite::new(&app);

    let denom = ts.create_denom("test");
    let tracker_addr = ts.instantiate_tracker(&denom);
    ts.set_before_send_hook(&denom, &tracker_addr, &app);

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
    let app = OsmosisTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");
    let tracker_addr = ts.instantiate_tracker(&denom);
    ts.set_before_send_hook(&denom, &tracker_addr, &app);

    // Mint tokens to owner
    ts.mint(&denom, 1000u128, &ts.owner.address());

    let user = app.init_account(&[]).unwrap();

    let balance_before = ts.balance_at(&tracker_addr, &user.address(), None).unwrap();
    assert_eq!(balance_before.u128(), 0u128);

    // Send owner -> user
    ts.bank
        .send(
            MsgSend {
                from_address: ts.owner.address(),
                to_address: user.address(),
                amount: vec![coin(1000u128, &denom).into()],
            },
            &ts.owner,
        )
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
    let app = OsmosisTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");
    let tracker_addr = ts.instantiate_tracker(&denom);
    ts.set_before_send_hook(&denom, &tracker_addr, &app);

    // Mint tokens to owner
    ts.mint(&denom, 1000u128, &ts.owner.address());

    app.increase_time(10);

    let balance_before = ts
        .balance_at(&tracker_addr, &ts.owner.address(), None)
        .unwrap();
    assert_eq!(balance_before.u128(), 1000u128);

    // Burn from owner
    ts.tf
        .burn(
            MsgBurn {
                sender: ts.owner.address(),
                amount: Some(coin(1000u128, &denom).into()),
                burn_from_address: ts.owner.address(),
            },
            &ts.owner,
        )
        .unwrap();

    app.increase_time(10);

    let balance_after = ts
        .balance_at(&tracker_addr, &ts.owner.address(), None)
        .unwrap();
    assert_eq!(balance_after.u128(), 0u128);
    let supply_after = ts.supply_at(&tracker_addr, None).unwrap();
    assert_eq!(supply_after.u128(), 0u128);
}

#[test]
fn ensure_sending_to_module_prohibited() {
    let app = OsmosisTestApp::new();
    let ts = TestSuite::new(&app);
    let denom = ts.create_denom("test");
    let tracker_addr = ts.instantiate_tracker(&denom);
    ts.set_before_send_hook(&denom, &tracker_addr, &app);

    // Mint tokens to owner
    ts.mint(&denom, 1000u128, &ts.owner.address());

    // Send owner -> tokenfactory module address
    let err = ts
        .bank
        .send(
            MsgSend {
                from_address: ts.owner.address(),
                to_address: ts.tokenfactory_module_address.clone(),
                amount: vec![coin(1000u128, &denom).into()],
            },
            &ts.owner,
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
    let app = OsmosisTestApp::new();
    let ts = TestSuite::new(&app);

    let denom = ts.create_denom("test");
    let tracker_addr = ts.instantiate_tracker(&denom);
    ts.set_before_send_hook(&denom, &tracker_addr, &app);

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

        let block_ts = app.get_block_timestamp().seconds();
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
