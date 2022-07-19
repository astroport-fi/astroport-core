use astroport_governance::utils::{get_period, EPOCH_START};
use cw_multi_test::App;

#[allow(clippy::all)]
#[allow(dead_code)]
pub mod controller_helper;
#[allow(clippy::all)]
#[allow(dead_code)]
pub mod escrow_helper;

pub fn mock_app() -> App {
    let mut app = App::default();
    app.next_block(EPOCH_START);
    app
}

pub trait AppExtension {
    fn next_block(&mut self, time: u64);
    fn block_period(&self) -> u64;
}

impl AppExtension for App {
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
