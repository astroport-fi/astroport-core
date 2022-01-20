use cosmwasm_std::{
    entry_point, from_binary, to_binary, Addr, Api, Binary, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, QueryRequest, Response, StdError, StdResult, Uint128, WasmMsg, WasmQuery,
};

use crate::error::ContractError;
use crate::operations::execute_swap_operation;
use crate::state::{Config, CONFIG};

use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo, PairInfo};
use astroport::pair::{QueryMsg as PairQueryMsg, SimulationResponse};
use astroport::querier::query_pair_info;
use astroport::router::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    SimulateSwapOperationsResponse, SwapOperation, MAX_SWAP_OPERATIONS,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;
use std::collections::HashMap;
use terra_cosmwasm::{SwapResponse, TerraMsgWrapper, TerraQuerier};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-router";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    CONFIG.save(
        deps.storage,
        &Config {
            astroport_factory: addr_validate_to_lower(deps.api, &msg.astroport_factory)?,
        },
    )?;

    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
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
/// * **ExecuteMsg::ExecuteSwapOperation { operation, to }** Execute swap operation.
/// Swap all offer asset to ask asset.
///
/// * **ExecuteMsg::AssertMinimumReceive {
///             asset_info,
///             prev_balance,
///             minimum_receive,
///             receiver
///         }** Performs minimum receive amount assertion.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, info, msg),
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
        } => execute_swap_operations(
            deps,
            env,
            info.clone(),
            info.sender,
            operations,
            minimum_receive,
            to,
        ),
        ExecuteMsg::ExecuteSwapOperation { operation, to } => {
            execute_swap_operation(deps, env, info, operation, to)
        }
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
            addr_validate_to_lower(deps.api, &receiver)?,
        ),
    }
}

/// ## Description
/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
/// If the template is not found in the received message, then an [`ContractError`] is returned,
/// otherwise returns the [`Response`] with the specified attributes if the operation was successful
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
///
/// * **cw20_msg** is the object of type [`Cw20ReceiveMsg`].
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
    let sender = addr_validate_to_lower(deps.api, &cw20_msg.sender)?;
    match from_binary(&cw20_msg.msg)? {
        Cw20HookMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
        } => {
            let to_addr = if let Some(to_addr) = to {
                Some(addr_validate_to_lower(deps.api, to_addr.as_str())?)
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
            )
        }
    }
}

/// ## Description
/// Performs swap operations with the specified parameters.
/// Returns an [`ContractError`] on failureб otherwise returns [`Response`] with the specified messages of type [`TerraMsgWrapper`] to execute if the operation is successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **sender** is the object of type [`Addr`]. Sets the default recipient of the swap operation.
///
/// * **operations** is a vector that contains object of type [`SwapOperation`]. Sets the number of transactions for exchange.
///
/// * **minimum_receive** is the object of type [`Option<Uint128>`]. Used to minimum amount assertion.
///
/// * **to** is the object of type [`Option<Addr>`]. Sets the recipient of the swap operation.
pub fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    sender: Addr,
    operations: Vec<SwapOperation>,
    minimum_receive: Option<Uint128>,
    to: Option<Addr>,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
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
        addr_validate_to_lower(deps.api, to.as_str())?
    } else {
        sender
    };

    let target_asset_info = operations.last().unwrap().get_target_asset_info();

    let mut operation_index = 0;
    let mut messages: Vec<CosmosMsg<TerraMsgWrapper>> = operations
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
                })?,
            }))
        })
        .collect::<StdResult<Vec<CosmosMsg<TerraMsgWrapper>>>>()?;

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
/// Performs minimum receive amount assertion.
/// Returns an [`ContractError`] on failure, otherwise returns default object of type [`Response`]
/// if the operation is successful.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **asset_info** is the object of type [`AssetInfo`].
///
/// * **prev_balance** is the object of type [`Uint128`].
///
/// * **minimum_receive** is the object of type [`Uint128`].
///
/// * **receiver** is the object of type [`Addr`]. Sets recipient for which the receive minimum amount assertion will be performed.
fn assert_minimum_receive(
    deps: Deps,
    asset_info: AssetInfo,
    prev_balance: Uint128,
    minimum_receive: Uint128,
    receiver: Addr,
) -> Result<Response<TerraMsgWrapper>, ContractError> {
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
/// Available the query messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **msg** is the object of type [`QueryMsg`].
///
/// ## Queries
/// * **QueryMsg::Config {}** Returns information about the controls settings in a [`ConfigResponse`] object.
/// * **QueryMsg::SimulateSwapOperations {
///             offer_amount,
///             operations,
///         }** Returns information about the simulation of the swap operations in a
/// [`SimulateSwapOperationsResponse`] object.
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
/// Returns an [`ContractError`] on failure, otherwise returns information about the controls
/// settings in a [`ConfigResponse`] object.
/// ## Params
/// * **deps** is the object of type [`Deps`].
pub fn query_config(deps: Deps) -> Result<ConfigResponse, ContractError> {
    let state = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        astroport_factory: state.astroport_factory.into_string(),
    };

    Ok(resp)
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}

/// ## Description
/// Returns an [`ContractError`] on failure, otherwise returns information about the simulation of
/// the swap operations in a [`SimulateSwapOperationsResponse`] object.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **offer_amount** is the object of type [`Uint128`]. Sets a offer amount.
///
/// * **operations** is a vector that contains object of type [`SwapOperation`].
fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> Result<SimulateSwapOperationsResponse, ContractError> {
    let config: Config = CONFIG.load(deps.storage)?;
    let astroport_factory = config.astroport_factory;
    let terra_querier = TerraQuerier::new(&deps.querier);

    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(ContractError::MustProvideOperations {});
    }

    if operations_len > MAX_SWAP_OPERATIONS {
        return Err(ContractError::SwapLimitExceeded {});
    }

    assert_operations(deps.api, &operations)?;

    let mut operation_index = 0;
    let mut offer_amount = offer_amount;
    for operation in operations.into_iter() {
        operation_index += 1;

        match operation {
            SwapOperation::NativeSwap {
                offer_denom,
                ask_denom,
            } => {
                // Deduct tax before query simulation
                // because last swap is swap_send
                if operation_index == operations_len {
                    let asset = Asset {
                        info: AssetInfo::NativeToken {
                            denom: offer_denom.clone(),
                        },
                        amount: offer_amount,
                    };

                    offer_amount = offer_amount.checked_sub(asset.compute_tax(&deps.querier)?)?;
                }

                let res: SwapResponse = terra_querier.query_swap(
                    Coin {
                        denom: offer_denom,
                        amount: offer_amount,
                    },
                    ask_denom,
                )?;

                offer_amount = res.receive.amount;
            }
            SwapOperation::AstroSwap {
                offer_asset_info,
                ask_asset_info,
            } => {
                let pair_info: PairInfo = query_pair_info(
                    &deps.querier,
                    astroport_factory.clone(),
                    &[offer_asset_info.clone(), ask_asset_info.clone()],
                )?;

                // Deduct tax before querying simulation
                if let AssetInfo::NativeToken { denom } = offer_asset_info.clone() {
                    let asset = Asset {
                        info: AssetInfo::NativeToken { denom },
                        amount: offer_amount,
                    };

                    offer_amount = offer_amount.checked_sub(asset.compute_tax(&deps.querier)?)?;
                }

                let mut res: SimulationResponse =
                    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                        contract_addr: pair_info.contract_addr.to_string(),
                        msg: to_binary(&PairQueryMsg::Simulation {
                            offer_asset: Asset {
                                info: offer_asset_info.clone(),
                                amount: offer_amount,
                            },
                        })?,
                    }))?;

                // Deduct tax after querying simulation
                if let AssetInfo::NativeToken { denom } = ask_asset_info.clone() {
                    let asset = Asset {
                        info: AssetInfo::NativeToken { denom },
                        amount: res.return_amount,
                    };

                    res.return_amount = res
                        .return_amount
                        .checked_sub(asset.compute_tax(&deps.querier)?)?;
                }

                offer_amount = res.return_amount;
            }
        }
    }

    Ok(SimulateSwapOperationsResponse {
        amount: offer_amount,
    })
}

/// ## Description
/// Validates assets in operations. Returns an [`ContractError`] on failure, otherwise returns [`Ok`].
/// ## Params
/// * **api** is the object of type [`Api`].
///
/// * **operations** is a vector that contains object of type [`SwapOperation`].
fn assert_operations(api: &dyn Api, operations: &[SwapOperation]) -> Result<(), ContractError> {
    let mut ask_asset_map: HashMap<String, bool> = HashMap::new();
    for operation in operations.iter() {
        let (offer_asset, ask_asset) = match operation {
            SwapOperation::NativeSwap {
                offer_denom,
                ask_denom,
            } => (
                AssetInfo::NativeToken {
                    denom: offer_denom.clone(),
                },
                AssetInfo::NativeToken {
                    denom: ask_denom.clone(),
                },
            ),
            SwapOperation::AstroSwap {
                offer_asset_info,
                ask_asset_info,
            } => (offer_asset_info.clone(), ask_asset_info.clone()),
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
    use cosmwasm_std::testing::mock_dependencies;
    let deps = mock_dependencies(&[]);
    // empty error
    assert_eq!(true, assert_operations(deps.as_ref().api, &vec![]).is_err());

    // uluna output
    assert_eq!(
        true,
        assert_operations(
            deps.as_ref().api,
            &vec![
                SwapOperation::NativeSwap {
                    offer_denom: "uusd".to_string(),
                    ask_denom: "uluna".to_string(),
                },
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
                SwapOperation::NativeSwap {
                    offer_denom: "uusd".to_string(),
                    ask_denom: "uluna".to_string(),
                },
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

    // multiple output token types error
    assert_eq!(
        true,
        assert_operations(
            deps.as_ref().api,
            &vec![
                SwapOperation::NativeSwap {
                    offer_denom: "uusd".to_string(),
                    ask_denom: "ukrw".to_string(),
                },
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
}
