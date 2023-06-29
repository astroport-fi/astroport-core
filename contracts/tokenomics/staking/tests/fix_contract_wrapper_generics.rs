use std::{cell::RefCell, fmt::Debug, rc::Rc};

use cosmwasm_std::{Api, CustomQuery, Storage};
use cw_multi_test::{
    App, Bank, ContractWrapper, Distribution, Gov, Ibc, Module, Staking, WasmKeeper,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

pub type WKApp<B, A, S, C, X, D, I, G> = Rc<
    RefCell<App<B, A, S, C, WasmKeeper<<C as Module>::ExecT, <C as Module>::QueryT>, X, D, I, G>>,
>;

pub fn store_code<B, A, S, C, X, D, I, G>(app: &WKApp<B, A, S, C, X, D, I, G>) -> u64
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
{
    use astroport_staking as cnt;
    let contract = Box::new(
        ContractWrapper::new_with_empty(
            cnt::contract::execute,
            cnt::contract::instantiate,
            cnt::contract::query,
        )
        .with_reply_empty(cnt::contract::reply),
    );

    app.borrow_mut().store_code(contract)
}
