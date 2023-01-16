use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Api, Binary, CosmosMsg, Decimal, Deps, DepsMut, Env,
    MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::error::ContractError;
use crate::operations::execute_swap_operation;
use crate::state::{Config, CONFIG};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::{QueryMsg as PairQueryMsg, SimulationResponse};
use astroport::querier::query_pair_info;
use astroport::router::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    SimulateSwapOperationsResponse, SwapOperation, MAX_SWAP_OPERATIONS,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::Cw20ReceiveMsg;
use std::collections::HashMap;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-router";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns a default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating the contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            astroport_factory: deps.api.addr_validate(&msg.astroport_factory)?,
        },
    )?;

    Ok(Response::default())
}

/// ## Description
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
/// it depending on the received template.
///
/// * **ExecuteMsg::ExecuteSwapOperations {
///             operations,
///             minimum_receive,
///             to
///         }** Performs swap operations with the specified parameters.
///
/// * **ExecuteMsg::ExecuteSwapOperation { operation, to }** Execute a single swap operation.
///
/// * **ExecuteMsg::AssertMinimumReceive {
///             asset_info,
///             prev_balance,
///             minimum_receive,
///             receiver
///         }** Checks if an ask amount is higher than or equal to the minimum amount to receive.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            max_spread,
        } => execute_swap_operations(
            deps,
            env,
            info.clone(),
            info.sender,
            operations,
            minimum_receive,
            to,
            max_spread,
        ),
        ExecuteMsg::ExecuteSwapOperation {
            operation,
            to,
            max_spread,
            single,
        } => execute_swap_operation(deps, env, info, operation, to, max_spread, single),
        ExecuteMsg::AssertMinimumReceive {
            asset_info,
            prev_balance,
            minimum_receive,
            receiver,
        } => assert_minimum_receive(
            deps.as_ref(),
            asset_info,
            prev_balance,
            minimum_receive,
            deps.api.addr_validate(&receiver)?,
        ),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If no template is found in the received message, then a [`ContractError`] is returned,
/// otherwise it returns a [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`].
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    let sender = deps.api.addr_validate(&cw20_msg.sender)?;
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            max_spread,
        } => {
            let to_addr = if let Some(to_addr) = to {
                Some(deps.api.addr_validate(to_addr.as_str())?)
            } else {
                None
            };

            execute_swap_operations(
                deps,
                env,
                info,
                sender,
                operations,
                minimum_receive,
                to_addr,
                max_spread,
            )
        }
    }
}

/// ## Description
/// Performs swap operations with the specified parameters.
/// Returns an [`ContractError`] on failure, otherwise returns [`Response`] to execute if the operation is successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **sender** is an object of type [`Addr`]. This is the address that swaps tokens.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`]. These are all the swap operations to perform.
///
/// * **minimum_receive** is an object of type [`Option<Uint128>`]. Used to guarantee that the ask amount is above a minimum amount.
///
/// * **to** is the object of type [`Option<Addr>`]. Sets the recipient of the swap operation.
#[allow(clippy::too_many_arguments)]
pub fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    sender: Addr,
    operations: Vec<SwapOperation>,
    minimum_receive: Option<Uint128>,
    to: Option<Addr>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(ContractError::MustProvideOperations {});
    }

    if operations_len > MAX_SWAP_OPERATIONS {
        return Err(ContractError::SwapLimitExceeded {});
    }

    // Assert the operations are properly set
    assert_operations(deps.api, &operations)?;

    let to = if let Some(to) = to {
        deps.api.addr_validate(to.as_str())?
    } else {
        sender
    };

    let target_asset_info = operations.last().unwrap().get_target_asset_info();

    let mut operation_index = 0;
    let mut messages: Vec<CosmosMsg> = operations
        .into_iter()
        .map(|op| {
            operation_index += 1;
            Ok(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: env.contract.address.to_string(),
                funds: vec![],
                msg: to_binary(&ExecuteMsg::ExecuteSwapOperation {
                    operation: op,
                    to: if operation_index == operations_len {
                        Some(to.to_string())
                    } else {
                        None
                    },
                    max_spread,
                    single: operations_len == 1,
                })?,
            }))
        })
        .collect::<StdResult<Vec<CosmosMsg>>>()?;

    // Execute minimum amount assertion
    if let Some(minimum_receive) = minimum_receive {
        let receiver_balance = target_asset_info.query_pool(&deps.querier, to.clone())?;
        messages.push(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            funds: vec![],
            msg: to_binary(&ExecuteMsg::AssertMinimumReceive {
                asset_info: target_asset_info,
                prev_balance: receiver_balance,
                minimum_receive,
                receiver: to.to_string(),
            })?,
        }));
    }

    Ok(Response::new().add_messages(messages))
}

/// ## Description
/// Checks if an ask amount is equal to or above a minimum amount.
/// Returns a [`ContractError`] on failure, otherwise returns a default object of type [`Response`]
/// if the operation is successful.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **asset_info** is an object of type [`AssetInfo`]. Specifies the asset to check the ask amount for.
///
/// * **prev_balance** is an object of type [`Uint128`]. This is the previous balance that the swap receive had before getting `ask` assets.
///
/// * **minimum_receive** is an object of type [`Uint128`]. This is the minimum amount of `ask` assets to receive.
///
/// * **receiver** is an object of type [`Addr`]. This is the address that received `ask` assets.
fn assert_minimum_receive(
    deps: Deps,
    asset_info: AssetInfo,
    prev_balance: Uint128,
    minimum_receive: Uint128,
    receiver: Addr,
) -> Result<Response, ContractError> {
    asset_info.check(deps.api)?;
    let receiver_balance = asset_info.query_pool(&deps.querier, receiver)?;
    let swap_amount = receiver_balance.checked_sub(prev_balance)?;

    if swap_amount < minimum_receive {
        return Err(ContractError::AssertionMinimumReceive {
            receive: minimum_receive,
            amount: swap_amount,
        });
    }

    Ok(Response::default())
}

/// ## Description
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns general router parameters using a [`ConfigResponse`] object.
/// * **QueryMsg::SimulateSwapOperations {
///             offer_amount,
///             operations,
///         }** Simulates one or multiple swap operations and returns the end result in a [`SimulateSwapOperationsResponse`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Config {} => Ok(to_binary(&query_config(deps)?)?),
        QueryMsg::SimulateSwapOperations {
            offer_amount,
            operations,
        } => Ok(to_binary(&simulate_swap_operations(
            deps,
            offer_amount,
            operations,
        )?)?),
    }
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns general contract settings in a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
pub fn query_config(deps: Deps) -> Result<ConfigResponse, ContractError> {
    let state = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        astroport_factory: state.astroport_factory.into_string(),
    };

    Ok(resp)
}

/// ## Description
/// Used for contract migration. Returns a default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-router" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}

/// ## Description
/// Returns a [`ContractError`] on failure, otherwise returns the end result of a simulation for one or multiple swap
/// operations using a [`SimulateSwapOperationsResponse`] object.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **offer_amount** is an object of type [`Uint128`]. This is the amount of offer assets being swapped.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`].
/// These are all the swap operations for which we perform a simulation.
fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> Result<SimulateSwapOperationsResponse, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let astroport_factory = config.astroport_factory;

    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(ContractError::MustProvideOperations {});
    }

    if operations_len > MAX_SWAP_OPERATIONS {
        return Err(ContractError::SwapLimitExceeded {});
    }

    assert_operations(deps.api, &operations)?;

    let mut return_amount = offer_amount;
    for operation in operations.into_iter() {
        match operation {
            SwapOperation::AstroSwap {
                offer_asset_info,
                ask_asset_info,
            } => {
                let pair_info: PairInfo = query_pair_info(
                    &deps.querier,
                    astroport_factory.clone(),
                    &[offer_asset_info.clone(), ask_asset_info.clone()],
                )?;

                // Deduct tax
                if let AssetInfo::NativeToken { denom } = offer_asset_info.clone() {
                    let asset = Asset {
                        info: AssetInfo::NativeToken { denom },
                        amount: return_amount,
                    };

                    return_amount = return_amount.checked_sub(asset.compute_tax(&deps.querier)?)?;
                }

                let mut res: SimulationResponse =
                    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                        contract_addr: pair_info.contract_addr.to_string(),
                        msg: to_binary(&PairQueryMsg::Simulation {
                            offer_asset: Asset {
                                info: offer_asset_info.clone(),
                                amount: return_amount,
                            },
                        })?,
                    }))?;

                // Deduct tax
                if let AssetInfo::NativeToken { denom } = ask_asset_info.clone() {
                    let asset = Asset {
                        info: AssetInfo::NativeToken { denom },
                        amount: res.return_amount,
                    };

                    res.return_amount = res
                        .return_amount
                        .checked_sub(asset.compute_tax(&deps.querier)?)?;
                }

                return_amount = res.return_amount;
            }
            SwapOperation::NativeSwap { .. } => {
                return Err(ContractError::NativeSwapNotSupported {})
            }
        }
    }

    Ok(SimulateSwapOperationsResponse {
        amount: return_amount,
    })
}

/// ## Description
/// Validates swap operations. Returns a [`ContractError`] on failure, otherwise returns [`Ok`].
/// ## Params
/// * **api** is an object of type [`Api`].
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`]. These are all the swap operations we check.
fn assert_operations(api: &dyn Api, operations: &[SwapOperation]) -> Result<(), ContractError> {
    let mut ask_asset_map: HashMap<String, bool> = HashMap::new();
    for operation in operations.iter() {
        let (offer_asset, ask_asset) = match operation {
            SwapOperation::AstroSwap {
                offer_asset_info,
                ask_asset_info,
            } => (offer_asset_info.clone(), ask_asset_info.clone()),
            SwapOperation::NativeSwap { .. } => {
                return Err(ContractError::NativeSwapNotSupported {})
            }
        };
        offer_asset.check(api)?;
        ask_asset.check(api)?;

        ask_asset_map.remove(&offer_asset.to_string());
        ask_asset_map.insert(ask_asset.to_string(), true);
    }

    if ask_asset_map.keys().len() != 1 {
        return Err(StdError::generic_err("invalid operations; multiple output token").into());
    }

    Ok(())
}

#[test]
fn test_invalid_operations() {
    use cosmwasm_std::coins;
    use cosmwasm_std::testing::mock_dependencies_with_balance;
    let deps = mock_dependencies_with_balance(&coins(2, "token"));
    // empty error
    assert_eq!(true, assert_operations(deps.as_ref().api, &vec![]).is_err());

    // uluna output
    assert_eq!(
        true,
        assert_operations(
            deps.as_ref().api,
            &vec![
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "ukrw".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                }
            ]
        )
        .is_ok()
    );

    // asset0002 output
    assert_eq!(
        true,
        assert_operations(
            deps.as_ref().api,
            &vec![
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "ukrw".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0002"),
                    },
                },
            ]
        )
        .is_ok()
    );

    // Multiple output token type errors
    assert_eq!(
        true,
        assert_operations(
            deps.as_ref().api,
            &vec![
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "ukrw".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0001"),
                    },
                    ask_asset_info: AssetInfo::NativeToken {
                        denom: "uaud".to_string(),
                    },
                },
                SwapOperation::AstroSwap {
                    offer_asset_info: AssetInfo::NativeToken {
                        denom: "uluna".to_string(),
                    },
                    ask_asset_info: AssetInfo::Token {
                        contract_addr: Addr::unchecked("asset0002"),
                    },
                },
            ]
        )
        .is_err()
    );

    // Native swap operations are not implemented
    assert!(assert_operations(
        deps.as_ref().api,
        &vec![SwapOperation::NativeSwap {
            offer_denom: "uusd".to_string(),
            ask_denom: "uluna".to_string(),
        },]
    )
    .is_err())
}
