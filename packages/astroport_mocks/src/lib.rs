#![cfg(not(tarpaulin_include))]

use std::{cell::RefCell, rc::Rc};

use cosmwasm_std::Addr;

pub mod coin_registry;
pub mod factory;
pub mod generator;
pub mod pair;
pub mod pair_concentrated;
pub mod pair_concentrated_inj;
pub mod pair_stable;
pub mod staking;
pub mod token;
pub mod vesting;
pub mod whitelist;
pub mod xastro;

pub const ASTROPORT: &str = "astroport";

pub fn astroport_address() -> Addr {
    Addr::unchecked(ASTROPORT)
}

pub use cw_multi_test;
use cw_multi_test::{App, Module, WasmKeeper};
pub use {
    coin_registry::{MockCoinRegistry, MockCoinRegistryBuilder},
    factory::{MockFactory, MockFactoryBuilder},
    generator::{MockGenerator, MockGeneratorBuilder},
    pair::{MockXykPair, MockXykPairBuilder},
    pair_concentrated::{MockConcentratedPair, MockConcentratedPairBuilder},
    pair_concentrated_inj::{MockConcentratedPairInj, MockConcentratedPairInjBuilder},
    pair_stable::{MockStablePair, MockStablePairBuilder},
    staking::{MockStaking, MockStakingBuilder},
    token::{MockToken, MockTokenBuilder},
    vesting::{MockVesting, MockVestingBuilder},
    xastro::{MockXastro, MockXastroBuilder},
};

pub type WKApp<B, A, S, C, X, D, I, G> = Rc<
    RefCell<App<B, A, S, C, WasmKeeper<<C as Module>::ExecT, <C as Module>::QueryT>, X, D, I, G>>,
>;
