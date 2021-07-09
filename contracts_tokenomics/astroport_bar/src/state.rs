use cosmwasm_std::{Addr, CanonicalAddr, StdResult, Storage};
use cw_storage_plus::Item;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub token_code_id: u64,
    pub deposit_token_addr: CanonicalAddr,
    pub share_token_addr: CanonicalAddr,
}

pub const CONFIG: Item<Config> = Item::new("\u{0}\u{6}config");