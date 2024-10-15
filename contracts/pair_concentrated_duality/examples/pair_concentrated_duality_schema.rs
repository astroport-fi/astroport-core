use cosmwasm_schema::write_api;

use astroport::pair::{ExecuteMsgExt, InstantiateMsg, QueryMsg};
use astroport::pair_concentrated_duality::DualityPairMsg;

fn main() {
    write_api! {
        instantiate: InstantiateMsg,
        query: QueryMsg,
        execute: ExecuteMsgExt<DualityPairMsg>
    }
}
