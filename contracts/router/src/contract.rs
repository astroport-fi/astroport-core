use cosmwasm_std::{
    entry_point, from_binary, to_binary, wasm_execute, Addr, Api, Binary, Decimal, Deps, DepsMut,
    Env, MessageInfo, Reply, Response, StdError, StdResult, SubMsg, SubMsgResult, Uint128,
};
use cw2::{get_contract_version, set_contract_version};
use cw20::Cw20ReceiveMsg;

use astroport::asset::{addr_opt_validate, Asset, AssetInfo};
use astroport::pair::{QueryMsg as PairQueryMsg, SimulationResponse};
use astroport::querier::query_pair_info;
use astroport::router::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg,
    SimulateSwapOperationsResponse, SwapOperation, SwapResponseData, MAX_SWAP_OPERATIONS,
};

use crate::error::ContractError;
use crate::operations::execute_swap_operation;
use crate::state::{Config, ReplyData, CONFIG, REPLY_DATA};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-router";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const AFTER_SWAP_REPLY_ID: u64 = 1;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
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

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
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
    match from_binary(&cw20_msg.msg)? {
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
    assert_operations(deps.api, &operations)?;

    let to = addr_opt_validate(deps.api, &to)?.unwrap_or(sender);
    let target_asset_info = operations.last().unwrap().get_target_asset_info();
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
            let data = to_binary(&SwapResponseData {
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

/// Returns general contract settings in a [`ConfigResponse`] object.
pub fn query_config(deps: Deps) -> Result<ConfigResponse, ContractError> {
    let state = CONFIG.load(deps.storage)?;
    let resp = ConfigResponse {
        astroport_factory: state.astroport_factory.into_string(),
    };

    Ok(resp)
}

/// Manages contract migration.
#[cfg(not(tarpaulin_include))]
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-router" => match contract_version.version.as_ref() {
            "1.1.1" => {}
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    };

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}

/// Returns the end result of a simulation for one or multiple swap
/// operations using a [`SimulateSwapOperationsResponse`] object.
///
/// * **offer_amount** amount of offer assets being swapped.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`].
/// These are all the swap operations for which we perform a simulation.
fn simulate_swap_operations(
    deps: Deps,
    offer_amount: Uint128,
    operations: Vec<SwapOperation>,
) -> Result<SimulateSwapOperationsResponse, ContractError> {
    assert_operations(deps.api, &operations)?;

    let config = CONFIG.load(deps.storage)?;
    let astroport_factory = config.astroport_factory;
    let mut return_amount = offer_amount;

    for operation in operations.into_iter() {
        match operation {
            SwapOperation::AstroSwap {
                offer_asset_info,
                ask_asset_info,
            } => {
                let pair_info = query_pair_info(
                    &deps.querier,
                    astroport_factory.clone(),
                    &[offer_asset_info.clone(), ask_asset_info.clone()],
                )?;

                let res: SimulationResponse = deps.querier.query_wasm_smart(
                    pair_info.contract_addr,
                    &PairQueryMsg::Simulation {
                        offer_asset: Asset {
                            info: offer_asset_info.clone(),
                            amount: return_amount,
                        },
                        ask_asset_info: Some(ask_asset_info.clone()),
                    },
                )?;

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

/// Validates swap operations.
///
/// * **operations** is a vector that contains objects of type [`SwapOperation`]. These are all the swap operations we check.
fn assert_operations(api: &dyn Api, operations: &[SwapOperation]) -> Result<(), ContractError> {
    let operations_len = operations.len();
    if operations_len == 0 {
        return Err(ContractError::MustProvideOperations {});
    }

    if operations_len > MAX_SWAP_OPERATIONS {
        return Err(ContractError::SwapLimitExceeded {});
    }

    let mut prev_ask_asset: Option<AssetInfo> = None;

    for operation in operations {
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

        if offer_asset.equal(&ask_asset) {
            return Err(ContractError::DoublingAssetsPath {
                offer_asset: offer_asset.to_string(),
                ask_asset: ask_asset.to_string(),
            });
        }

        if let Some(prev_ask_asset) = prev_ask_asset {
            if prev_ask_asset != offer_asset {
                return Err(ContractError::InvalidPathOperations {
                    prev_ask_asset: prev_ask_asset.to_string(),
                    next_offer_asset: offer_asset.to_string(),
                    next_ask_asset: ask_asset.to_string(),
                });
            }
        }

        prev_ask_asset = Some(ask_asset);
    }

    Ok(())
}

#[cfg(test)]
mod testing {
    use super::*;

    #[test]
    fn test_invalid_operations() {
        use cosmwasm_std::testing::mock_dependencies;
        let deps = mock_dependencies();
        // Empty error
        assert_eq!(true, assert_operations(deps.as_ref().api, &[]).is_err());

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
                    },
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
    }
}
