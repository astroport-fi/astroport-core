#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdResult};
use cw2::set_contract_version;

use astroport::tokenfactory_tracker::{InstantiateMsg, SudoMsg};

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG, TOTAL_SUPPLY_HISTORY};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // TODO: There is a Stargate query that can be used to get the all the module
    // addresses. Need to confirm that this will actually work on Neutron
    // let accounts = AuthQuerier::new(&deps.querier).module_accounts()?;
    // type URL is /cosmos.auth.v1beta1.ModuleAccount

    let config = Config {
        tracked_denom: msg.tracked_denom.clone(),
        // Temporary save the module address until we can fetch on init
        tokenfactory_module_address: msg.tokenfactory_module_address,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", CONTRACT_NAME)
        .add_attribute("tracked_denom", config.tracked_denom)
        .add_attribute(
            "tokenfactory_module_address",
            config.tokenfactory_module_address,
        ))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // BlockBeforeSend is called before a send - if an error is returned the send
        // is cancelled
        SudoMsg::BlockBeforeSend { .. } => Ok(Response::default()),
        // TrackBeforeSend is called before a send - if an error is returned it will
        // be ignored and the send will continue
        // Minting a token directly to an address is also tracked
        SudoMsg::TrackBeforeSend { from, to, amount } => {
            let config = CONFIG.load(deps.storage)?;

            // Ensure the denom being sent is the tracked denom
            // If this isn't checked, another token could be tracked with the same
            // contract and that will skew the real numbers
            if amount.denom != config.tracked_denom {
                return Err(ContractError::InvalidDenom {
                    expected_denom: config.tracked_denom,
                });
            }

            // If the token is minted directly to an address, we don't need to subtract
            // as the sender is the module address
            if from != config.tokenfactory_module_address {
                BALANCES.update(
                    deps.storage,
                    &from,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_sub(amount.amount)?)
                    },
                )?;
            } else {
                // Minted new tokens
                TOTAL_SUPPLY_HISTORY.update(
                    deps.storage,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_add(amount.amount)?)
                    },
                )?;
            }

            // When burning tokens, the receiver is the token factory module address
            // Sending tokens to the module address isn't allowed by the chain
            if to != config.tokenfactory_module_address {
                BALANCES.update(
                    deps.storage,
                    &to,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_add(amount.amount)?)
                    },
                )?;
            } else {
                // Burned tokens
                TOTAL_SUPPLY_HISTORY.update(
                    deps.storage,
                    env.block.time.seconds(),
                    |balance| -> StdResult<_> {
                        Ok(balance.unwrap_or_default().checked_sub(amount.amount)?)
                    },
                )?;
            }

            Ok(Response::default())
        }
    }
}

#[cfg(test)]
mod tests {

    use cosmwasm_std::{
        testing::{mock_dependencies, mock_env, mock_info},
        to_json_binary, Coin, Uint128, Uint64,
    };

    use crate::query::query;

    use super::*;

    pub const OWNER: &str = "owner";
    pub const DENOM: &str = "factory/contract0/token";
    pub const MODULE_ADDRESS: &str = "tokenfactory_module";

    // Basic operations for testing calculations
    struct TestOperation {
        from: String,
        to: String,
        amount: Uint128,
    }

    #[test]
    fn track_token_balances() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();
        let info = mock_info(OWNER, &[]);

        let operations = vec![
            // Simulate a mint
            TestOperation {
                from: MODULE_ADDRESS.to_string(),
                to: "user1".to_string(),
                amount: Uint128::from(100u128),
            },
            TestOperation {
                from: "user1".to_string(),
                to: "user2".to_string(),
                amount: Uint128::from(50u128),
            },
            TestOperation {
                from: "user1".to_string(),
                to: "user3".to_string(),
                amount: Uint128::from(50u128),
            },
            TestOperation {
                from: "user2".to_string(),
                to: "user3".to_string(),
                amount: Uint128::from(50u128),
            },
            // Simulate a mint
            TestOperation {
                from: MODULE_ADDRESS.to_string(),
                to: "user4".to_string(),
                amount: Uint128::from(100u128),
            },
            // Simulate a burn
            TestOperation {
                from: "user4".to_string(),
                to: MODULE_ADDRESS.to_string(),
                amount: Uint128::from(99u128),
            },
        ];

        let expected_user1_balance = Uint128::zero();
        let expected_user2_balance = Uint128::zero();
        let expected_user3_balance = Uint128::from(100u128);
        let expected_user4_balance = Uint128::from(1u128);
        let expected_total_supply = Uint128::from(101u128);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport::tokenfactory_tracker::InstantiateMsg {
                tracked_denom: DENOM.to_string(),
                tokenfactory_module_address: MODULE_ADDRESS.to_string(),
                // owner: OWNER.to_string(),
            },
        )
        .unwrap();

        for operation in operations {
            sudo(
                deps.as_mut(),
                env.clone(),
                astroport::tokenfactory_tracker::SudoMsg::TrackBeforeSend {
                    from: operation.from,
                    to: operation.to,
                    amount: Coin {
                        denom: DENOM.to_string(),
                        amount: operation.amount,
                    },
                },
            )
            .unwrap();
        }

        env.block.time = env.block.time.plus_seconds(10);

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user1".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user1_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user2".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user2_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user3".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user3_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user3".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user3_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user4".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user4_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::TotalSupplyAt {
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_total_supply).unwrap());

        let balance = query(
            deps.as_ref(),
            env,
            astroport::tokenfactory_tracker::QueryMsg::TotalSupplyAt { timestamp: None },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_total_supply).unwrap());
    }

    #[test]
    fn no_track_other_token() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info(OWNER, &[]);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            astroport::tokenfactory_tracker::InstantiateMsg {
                tracked_denom: DENOM.to_string(),
                tokenfactory_module_address: MODULE_ADDRESS.to_string(),
                // owner: OWNER.to_string(),
            },
        )
        .unwrap();

        // The contract only tracks a specific denom, this should result in
        // an error
        let err = sudo(
            deps.as_mut(),
            env.clone(),
            astroport::tokenfactory_tracker::SudoMsg::TrackBeforeSend {
                from: MODULE_ADDRESS.to_string(),
                to: "user1".to_string(),
                amount: Coin {
                    denom: "OTHER_DENOM".to_string(),
                    amount: Uint128::from(100u128),
                },
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::InvalidDenom {
                expected_denom: DENOM.to_string()
            }
        );

        // Verify that it was not tracked
        let balance = query(
            deps.as_ref(),
            env.clone(),
            astroport::tokenfactory_tracker::QueryMsg::BalanceAt {
                address: "user1".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&Uint128::zero()).unwrap());
    }
}
