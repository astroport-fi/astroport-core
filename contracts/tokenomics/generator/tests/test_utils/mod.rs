use astroport_governance::utils::{get_period, EPOCH_START};
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::Timestamp;
use terra_multi_test::{AppBuilder, BankKeeper, TerraApp, TerraMock};

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod controller_helper;
#[allow(clippy::all)]
#[allow(dead_code)]
pub mod escrow_helper;

pub fn mock_app() -> TerraApp {
    let mut env = mock_env();
    env.block.time = Timestamp::from_seconds(EPOCH_START);
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

pub trait TerraAppExtension {
    fn next_block(&mut self, time: u64);
    fn block_period(&self) -> u64;
}

impl TerraAppExtension for TerraApp {
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
