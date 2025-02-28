use serde::Deserialize;

// !!! Workaround which fixes an invalid type in the original neutron-std crate
#[derive(Deserialize)]
pub struct CustomLimitOrderTrancheUser {
    pub tranche_key: String,
    // !!! We don't need these fields thus making serde ignore them
    // pub trade_pair_id: Option<TradePairId>,
    // pub tick_index_taker_to_maker: i64,
    // pub address: String,
    // pub shares_owned: String,
    // pub shares_withdrawn: String,
    // pub shares_cancelled: String,
    // pub order_type: i32,
}
#[derive(Deserialize)]
pub struct CustomQueryAllLimitOrderTrancheUserByAddressResponse {
    pub limit_orders: Vec<CustomLimitOrderTrancheUser>,
    // pub pagination: Option<PageResponse>,
}
