// standard_v1 is anything before the custom Astroport v1.1.1,
// specifically we're upgrading from cw-plus/cw20-ics20 v0.15
pub mod standard_v1 {
    use cosmwasm_schema::cw_serde;

    use cw_storage_plus::Item;

    #[cw_serde]
    pub struct Config {
        pub default_timeout: u64,
        pub default_gas_limit: Option<u64>,
    }

    pub const CONFIG: Item<Config> = Item::new("ics20_config");
}
