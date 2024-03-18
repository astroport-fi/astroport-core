#![cfg(not(tarpaulin_include))]

use astroport_governance::utils::{get_period, EPOCH_START};

use astroport_mocks::cw_multi_test::{AppBuilder, MockStargate, StargateApp as TestApp};

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod controller_helper;
pub mod delegation_helper;
#[allow(clippy::all)]
#[allow(dead_code)]
pub mod escrow_helper;

pub fn mock_app() -> TestApp {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {});
    app.next_block(EPOCH_START);
    app
}

pub trait AppExtension {
    fn next_block(&mut self, time: u64);
    fn block_period(&self) -> u64;
}

impl AppExtension for TestApp {
    fn next_block(&mut self, time: u64) {
        self.update_block(|block| {
            block.time = block.time.plus_seconds(time);
            block.height += 1
        });
    }

    fn block_period(&self) -> u64 {
        get_period(self.block_info().time.seconds()).unwrap()
    }
}
