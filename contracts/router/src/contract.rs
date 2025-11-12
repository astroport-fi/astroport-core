#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    from_json, to_json_binary, wasm_execute, Addr, Binary, Decimal, Deps, DepsMut, Empty, Env,
    MessageInfo, Reply, Response, StdError, StdResult, SubMsg, SubMsgResult, Uint128,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;

use astroport::asset::{addr_opt_validate, AssetInfo, AssetInfoExt};
use astroport::pair::{QueryMsg as PairQueryMsg, ReverseSimulationResponse, SimulationResponse};
use astroport::querier::query_pair_info;
use astroport::router::{Cw20HookMsg, ExecuteMsg, QueryMsg, SwapOperation, SwapResponseData};

use crate::error::ContractError;
use crate::operations::execute_swap_operation;
use crate::state::{ReplyData, CONFIG, REPLY_DATA};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const AFTER_SWAP_REPLY_ID: u64 = 1;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: Empty,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
///   it depending on the received template.
///
/// * **ExecuteMsg::ExecuteSwapOperations {
///             operations,
///             minimum_receive,
///             to
///         }** Performs swap operations with the specified parameters.
///
/// * **ExecuteMsg::ExecuteSwapOperation { operation, to }** Execute a single swap operation.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Receive(msg) => receive_cw20(deps, env, msg),
        ExecuteMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            max_spread,
        } => execute_swap_operations(
            deps,
            env,
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
    }
}

/// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
///
/// * **cw20_msg** is an object of type [`Cw20ReceiveMsg`].
pub fn receive_cw20(
    deps: DepsMut,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response, ContractError> {
    match from_json(&cw20_msg.msg)? {
        Cw20HookMsg::ExecuteSwapOperations {
            operations,
            minimum_receive,
            to,
            max_spread,
        } => execute_swap_operations(
            deps,
            env,
            Addr::unchecked(cw20_msg.sender),
            operations,
            minimum_receive,
            to,
            max_spread,
        ),
    }
}

/// Performs swap operations with the specified parameters.
///
/// * **sender** address that swaps tokens.
///
/// * **operations** all swap operations to perform.
///
/// * **minimum_receive** used to guarantee that the ask amount is above a minimum amount.
///
/// * **to** recipient of the ask tokens.
#[allow(clippy::too_many_arguments)]
pub fn execute_swap_operations(
    deps: DepsMut,
    env: Env,
    sender: Addr,
    operations: Vec<SwapOperation>,
    minimum_receive: Option<Uint128>,
    to: Option<String>,
    max_spread: Option<Decimal>,
) -> Result<Response, ContractError> {
    assert_operations(&operations)?;

    let to = addr_opt_validate(deps.api, &to)?.unwrap_or(sender);
    let target_asset_info = operations.last().unwrap().ask_asset_info.clone();
    let operations_len = operations.len();

    let messages = operations
        .into_iter()
        .enumerate()
        .map(|(operation_index, op)| {
            if operation_index == operations_len - 1 {
                wasm_execute(
                    env.contract.address.to_string(),
                    &ExecuteMsg::ExecuteSwapOperation {
                        operation: op,
                        to: Some(to.to_string()),
                        max_spread,
                        single: operations_len == 1,
                    },
                    vec![],
                )
                .map(|inner_msg| SubMsg::reply_on_success(inner_msg, AFTER_SWAP_REPLY_ID))
            } else {
                wasm_execute(
                    env.contract.address.to_string(),
                    &ExecuteMsg::ExecuteSwapOperation {
                        operation: op,
                        to: None,
                        max_spread,
                        single: operations_len == 1,
                    },
                    vec![],
                )
                .map(SubMsg::new)
            }
        })
        .collect::<StdResult<Vec<_>>>()?;

    let prev_balance = target_asset_info.query_pool(&deps.querier, &to)?;
    REPLY_DATA.save(
        deps.storage,
        &ReplyData {
            asset_info: target_asset_info,
            prev_balance,
            minimum_receive,
            receiver: to.to_string(),
        },
    )?;

    Ok(Response::new().add_submessages(messages))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn reply(deps: DepsMut, _env: Env, msg: Reply) -> Result<Response, ContractError> {
    match msg {
        Reply {
            id: AFTER_SWAP_REPLY_ID,
            result: SubMsgResult::Ok(..),
        } => {
            let reply_data = REPLY_DATA.load(deps.storage)?;
            let receiver_balance = reply_data
                .asset_info
                .query_pool(&deps.querier, reply_data.receiver)?;
            let swap_amount = receiver_balance.checked_sub(reply_data.prev_balance)?;

            if let Some(minimum_receive) = reply_data.minimum_receive {
                if swap_amount < minimum_receive {
                    return Err(ContractError::AssertionMinimumReceive {
                        receive: minimum_receive,
                        amount: swap_amount,
                    });
                }
            }

            // Reply data makes sense ONLY if the first token in multi-hop swap is native.
            let data = to_json_binary(&SwapResponseData {
                return_amount: swap_amount,
            })?;

            Ok(Response::new().set_data(data))
        }
        _ => Err(StdError::generic_err("Failed to process reply").into()),
    }
}

/// Exposes all the queries available in the contract.
/// ## Queries
/// * **QueryMsg::Config {}** Returns general router parameters using a [`ConfigResponse`] object.
/// * **QueryMsg::SimulateSwapOperations {
///             offer_amount,
///             operations,
///         }** Simulates one or multiple swap operations and returns the end result in a [`SimulateSwapOperationsResponse`] object.
/// * **QueryMsg::ReverseSimulateSwapOperations {
///            ask_amount,
///           operations,
///        }** Simulates one or multiple swap operations in reverse and returns the end result in a [`Uint128`] object.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::SimulateSwapOperations {
            offer_amount,
            operations,
        } => Ok(to_json_binary(&simulate_swap_operations(
            deps,
            offer_amount,
            operations,
        )?)?),
        QueryMsg::ReverseSimulateSwapOperations {
            ask_amount,
            operations,
        } => Ok(to_json_binary(&simulate_reverse_swap_operations(
            deps, ask_amount, operations,
        )?)?),
    }
}

/// Manages the contract migration.
#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: Empty) -> Result<Response, ContractError> {
    unimplemented!()
}

/// Returns the end result of a simulation for one or multiple swap
/// operations using a [`SimulateSwapOperationsResponse`] object.
///
/// * **offer_amount** amount of offer assets being swapped.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`].
///   These are all the swap operations for which we perform a simulation.
fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> Result<Uint128, ContractError> {
    assert_operations(&operations)?;

    let mut return_amount = offer_amount;

    for operation in operations.into_iter() {
        let res: SimulationResponse = deps.querier.query_wasm_smart(
            &operation.pair_address,
            &PairQueryMsg::Simulation {
                offer_asset: operation.offer_asset_info.with_balance(return_amount),
                ask_asset_info: Some(operation.ask_asset_info.clone()),
            },
        )?;

        return_amount = res.return_amount;
    }

    Ok(return_amount)
}

fn simulate_reverse_swap_operations(
    deps: Deps,
    ask_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> Result<Uint128, ContractError> {
    assert_operations(&operations)?;

    let config = CONFIG.load(deps.storage)?;
    let mut step_amount = ask_amount;

    for operation in operations.into_iter().rev() {
        let pair_info = query_pair_info(
            &deps.querier,
            &config.astroport_factory,
            &[
                operation.offer_asset_info.clone(),
                operation.ask_asset_info.clone(),
            ],
        )?;

        let res: ReverseSimulationResponse = deps.querier.query_wasm_smart(
            pair_info.contract_addr,
            &PairQueryMsg::ReverseSimulation {
                offer_asset_info: Some(operation.offer_asset_info.clone()),
                ask_asset: operation.ask_asset_info.with_balance(step_amount),
            },
        )?;

        step_amount = res.offer_amount;
    }

    Ok(step_amount)
}

/// Validates swap operations.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`]. These are all the swap operations we check.
fn assert_operations(operations: &[SwapOperation]) -> Result<(), ContractError> {
    if operations.is_empty() {
        return Err(ContractError::MustProvideOperations {});
    }

    let mut prev_ask_asset: Option<AssetInfo> = None;

    for operation in operations {
        if operation.offer_asset_info.equal(&operation.ask_asset_info) {
            return Err(ContractError::DoublingAssetsPath {
                offer_asset: operation.offer_asset_info.to_string(),
                ask_asset: operation.ask_asset_info.to_string(),
            });
        }

        if let Some(prev_ask_asset) = prev_ask_asset {
            if prev_ask_asset != operation.offer_asset_info {
                return Err(ContractError::InvalidPathOperations {
                    prev_ask_asset: prev_ask_asset.to_string(),
                    next_offer_asset: operation.offer_asset_info.to_string(),
                    next_ask_asset: operation.ask_asset_info.to_string(),
                });
            }
        }

        prev_ask_asset = Some(operation.ask_asset_info.clone());
    }

    Ok(())
}

#[cfg(test)]
mod testing {
    use super::*;

    #[test]
    fn test_invalid_operations() {
        // Empty error
        assert!(assert_operations(&[]).is_err());

        // uluna output
        assert_operations(&[
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
            },
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
        ])
        .unwrap();

        // asset0002 output
        assert_operations(&[
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
            },
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
            },
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0002"),
                },
            },
        ])
        .unwrap();

        // Multiple output token type errors
        assert_operations(&[
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "ukrw".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
            },
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0001"),
                },
                ask_asset_info: AssetInfo::NativeToken {
                    denom: "uaud".to_string(),
                },
            },
            SwapOperation {
                pair_address: "".to_string(),
                offer_asset_info: AssetInfo::NativeToken {
                    denom: "uluna".to_string(),
                },
                ask_asset_info: AssetInfo::Token {
                    contract_addr: Addr::unchecked("asset0002"),
                },
            },
        ])
        .unwrap_err();
    }
}
