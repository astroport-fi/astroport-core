#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, StdResult};
use cw2::set_contract_version;
use osmosis_std::types::cosmos::auth::v1beta1::{AuthQuerier, ModuleAccount};

use astroport::tokenfactory_tracker::{InstantiateMsg, SudoMsg};

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG, TOTAL_SUPPLY_HISTORY};

const TOKEN_FACTORY_MODULE_NAME: &str = "tokenfactory";
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

    // Determine tokenfactory module address
    let ModuleAccount { base_account, .. } = AuthQuerier::new(&deps.querier)
        .module_account_by_name(TOKEN_FACTORY_MODULE_NAME.to_string())?
        .account
        .expect("tokenfactory module account not found")
        .try_into()
        .map_err(|_| StdError::generic_err("Failed to decode tokenfactory module account"))?;
    let tokenfactory_module_address = base_account
        .expect("tokenfactory base account not found")
        .address;

    let config = Config {
        tracked_denom: msg.tracked_denom.clone(),
        tokenfactory_module_address,
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
        // is cancelled. This call is NOT gas metered.
        // NOTE: Contract logic relies on the fact SudoMsg::BlockBeforeSend is always called before SudoMsg::TrackBeforeSend.
        // Ref: https://github.com/osmosis-labs/cosmos-sdk/blob/55b53a127b6937d66a40084e9f7383a3762ea7f5/x/bank/keeper/send.go#L210-L223
        SudoMsg::BlockBeforeSend { amount, .. } => {
            let config = CONFIG.load(deps.storage)?;

            // Ensure the denom being sent is the tracked denom
            // If this isn't checked, another token could be tracked with the same
            // contract and that will skew the real numbers
            if amount.denom != config.tracked_denom {
                Err(ContractError::InvalidDenom {
                    expected_denom: config.tracked_denom,
                })
            } else {
                Ok(Response::default())
            }
        }
        // TrackBeforeSend is called before a send - if an error is returned it will
        // be ignored and the send will continue
        // Minting a token directly to an address is also tracked
        SudoMsg::TrackBeforeSend { from, to, amount } => {
            let config = CONFIG.load(deps.storage)?;

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
    use std::marker::PhantomData;

    use cosmwasm_std::testing::{MockApi, MockStorage};
    use cosmwasm_std::{
        from_json,
        testing::{mock_env, mock_info},
        to_json_binary, Coin, ContractResult, Empty, OwnedDeps, Querier, QuerierResult,
        QueryRequest, SystemError, SystemResult, Uint128, Uint64,
    };
    use osmosis_std::types::cosmos::auth::v1beta1::{
        BaseAccount, QueryModuleAccountByNameResponse,
    };

    use astroport::tokenfactory_tracker::QueryMsg;

    use crate::query::query;

    use super::*;

    const OWNER: &str = "owner";
    const DENOM: &str = "factory/contract0/token";
    const MODULE_ADDRESS: &str = "tokenfactory_module";

    struct CustomMockQuerier;

    impl CustomMockQuerier {
        pub fn handle_query(&self, request: &QueryRequest<Empty>) -> QuerierResult {
            match &request {
                QueryRequest::Stargate { path, .. }
                    if path == "/cosmos.auth.v1beta1.Query/ModuleAccountByName" =>
                {
                    let module_account = ModuleAccount {
                        base_account: Some(BaseAccount {
                            address: MODULE_ADDRESS.to_string(),
                            pub_key: None,
                            account_number: 0,
                            sequence: 0,
                        }),
                        name: TOKEN_FACTORY_MODULE_NAME.to_string(),
                        permissions: vec![],
                    };
                    let response = QueryModuleAccountByNameResponse {
                        account: Some(module_account.to_any()),
                    };

                    SystemResult::Ok(ContractResult::Ok(to_json_binary(&response).unwrap()))
                }
                _ => SystemResult::Err(SystemError::UnsupportedRequest {
                    kind: "Unsupported".to_string(),
                }),
            }
        }
    }

    impl Querier for CustomMockQuerier {
        fn raw_query(&self, bin_request: &[u8]) -> QuerierResult {
            let request: QueryRequest<Empty> = match from_json(bin_request) {
                Ok(v) => v,
                Err(e) => {
                    return SystemResult::Err(SystemError::InvalidRequest {
                        error: format!("Parsing query request: {e}"),
                        request: bin_request.into(),
                    })
                }
            };
            self.handle_query(&request)
        }
    }

    fn mock_custom_dependencies() -> OwnedDeps<MockStorage, MockApi, CustomMockQuerier, Empty> {
        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier: CustomMockQuerier,
            custom_query_type: PhantomData,
        }
    }

    // Basic operations for testing calculations
    struct TestOperation {
        from: String,
        to: String,
        amount: Uint128,
    }

    #[test]
    fn track_token_balances() {
        let mut deps = mock_custom_dependencies();
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
            InstantiateMsg {
                tracked_denom: DENOM.to_string(),
            },
        )
        .unwrap();

        for TestOperation { from, to, amount } in operations {
            sudo(
                deps.as_mut(),
                env.clone(),
                SudoMsg::TrackBeforeSend {
                    from,
                    to,
                    amount: Coin {
                        denom: DENOM.to_string(),
                        amount,
                    },
                },
            )
            .unwrap();
        }

        env.block.time = env.block.time.plus_seconds(10);

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user1".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user1_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user2".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user2_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user3".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user3_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user3".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user3_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user4".to_string(),
                timestamp: None,
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_user4_balance).unwrap());

        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::TotalSupplyAt {
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_total_supply).unwrap());

        let balance = query(
            deps.as_ref(),
            env,
            QueryMsg::TotalSupplyAt { timestamp: None },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&expected_total_supply).unwrap());
    }

    #[test]
    fn no_track_other_token() {
        let mut deps = mock_custom_dependencies();
        let env = mock_env();
        let info = mock_info(OWNER, &[]);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            InstantiateMsg {
                tracked_denom: DENOM.to_string(),
            },
        )
        .unwrap();

        // The contract only tracks a specific denom, this should result in
        // an error
        let err = sudo(
            deps.as_mut(),
            env.clone(),
            SudoMsg::BlockBeforeSend {
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
            QueryMsg::BalanceAt {
                address: "user1".to_string(),
                timestamp: Some(Uint64::from(env.block.time.seconds())),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&Uint128::zero()).unwrap());
    }
}
