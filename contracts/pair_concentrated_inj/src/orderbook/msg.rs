use cosmwasm_schema::cw_serde;

#[cw_serde]
/// SudoMsg layout is defined within Injective core. We can not change it.
pub enum SudoMsg {
    BeginBlocker {},
    Deactivate {},
}
