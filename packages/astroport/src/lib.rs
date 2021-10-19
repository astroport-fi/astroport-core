pub mod asset;
pub mod factory;
pub mod generator;
pub mod generator_proxy;
pub mod hook;
pub mod maker;
pub mod oracle;
pub mod pair;
pub mod querier;
pub mod router;
pub mod staking;
pub mod token;
pub mod vesting;

#[cfg(test)]
mod mock_querier;

#[cfg(test)]
mod testing;

#[allow(clippy::all)]
mod uints {
    use uint::construct_uint;
    construct_uint! {
        pub struct U256(4);
    }
}

pub use uints::U256;
