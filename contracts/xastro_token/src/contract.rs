use cosmwasm_std::{
    attr, entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Uint128,
};
use cw20::{AllAccountsResponse, BalanceResponse, Cw20Coin, Cw20ReceiveMsg};
use cw20_base::allowances::{
    deduct_allowance, execute_decrease_allowance, execute_increase_allowance, query_allowance,
};

use crate::state::{capture_total_supply_history, get_total_supply_at, BALANCES};
use cw2::set_contract_version;
use cw20_base::contract::{
    execute_update_marketing, execute_upload_logo, query_download_logo, query_marketing_info,
    query_minter, query_token_info,
};
use cw20_base::enumerable::query_all_allowances;
use cw20_base::msg::ExecuteMsg;
use cw20_base::state::{MinterData, TokenInfo, TOKEN_INFO};
use cw20_base::ContractError;
use cw_storage_plus::Bound;

use astroport::xastro_token::{InstantiateMsg, QueryMsg};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-xastro-token";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

// settings for pagination
const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the default object of type [`Response`] if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Check valid token info
    msg.validate()?;

    // Create initial accounts
    let total_supply = create_accounts(&mut deps, &env, msg.initial_balances.as_slice())?;

    capture_total_supply_history(deps.storage, &env, total_supply)?;

    // Check supply cap
    if let Some(limit) = msg.get_cap() {
        if total_supply > limit {
            return Err(StdError::generic_err("Initial supply greater than cap"));
        }
    }

    let mint = match msg.mint {
        Some(m) => Some(MinterData {
            minter: deps.api.addr_validate(&m.minter)?,
            cap: m.cap,
        }),
        None => None,
    };

    // Store token info
    let data = TokenInfo {
        name: msg.name,
        symbol: msg.symbol,
        decimals: msg.decimals,
        total_supply,
        mint,
    };

    TOKEN_INFO.save(deps.storage, &data)?;

    Ok(Response::default())
}

pub fn create_accounts(deps: &mut DepsMut, env: &Env, accounts: &[Cw20Coin]) -> StdResult<Uint128> {
    let mut total_supply = Uint128::zero();

    for row in accounts {
        let address = deps.api.addr_validate(&row.address)?;
        BALANCES.save(deps.storage, &address, &row.amount, env.block.height)?;
        total_supply += row.amount;
    }

    Ok(total_supply)
}

/// ## Description
/// Default CW20 execute method implementation.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Transfer { recipient, amount } => {
            execute_transfer(deps, env, info, recipient, amount)
        }
        ExecuteMsg::Burn { amount } => execute_burn(deps, env, info, amount),
        ExecuteMsg::Send {
            contract,
            amount,
            msg,
        } => execute_send(deps, env, info, contract, amount, msg),
        ExecuteMsg::Mint { recipient, amount } => execute_mint(deps, env, info, recipient, amount),
        ExecuteMsg::IncreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_increase_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::DecreaseAllowance {
            spender,
            amount,
            expires,
        } => execute_decrease_allowance(deps, env, info, spender, amount, expires),
        ExecuteMsg::TransferFrom {
            owner,
            recipient,
            amount,
        } => execute_transfer_from(deps, env, info, owner, recipient, amount),
        ExecuteMsg::BurnFrom { owner, amount } => execute_burn_from(deps, env, info, owner, amount),
        ExecuteMsg::SendFrom {
            owner,
            contract,
            amount,
            msg,
        } => execute_send_from(deps, env, info, owner, contract, amount, msg),
        ExecuteMsg::UpdateMarketing {
            project,
            description,
            marketing,
        } => execute_update_marketing(deps, env, info, project, description, marketing),
        ExecuteMsg::UploadLogo(logo) => execute_upload_logo(deps, env, info, logo),
    }
}

/// ## Description
/// Default CW20 execute_transfer method implementation with saving history.
pub fn execute_transfer(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&recipient)?;

    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "transfer")
        .add_attribute("from", info.sender)
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

/// ## Description
/// Default CW20 execute_burn method implementation with saving history.
pub fn execute_burn(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    // lower balance
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;

    // reduce total_supply
    let token_info = TOKEN_INFO.update(deps.storage, |mut info| -> StdResult<_> {
        info.total_supply = info.total_supply.checked_sub(amount)?;
        Ok(info)
    })?;

    capture_total_supply_history(deps.storage, &env, token_info.total_supply)?;

    let res = Response::new()
        .add_attribute("action", "burn")
        .add_attribute("from", info.sender)
        .add_attribute("amount", amount);
    Ok(res)
}

/// ## Description
/// Default CW20 execute_mint method implementation with saving history.
pub fn execute_mint(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let mut config = TOKEN_INFO.load(deps.storage)?;
    if config.mint.is_none() || config.mint.as_ref().unwrap().minter != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    // update supply and enforce cap
    config.total_supply += amount;
    if let Some(limit) = config.get_cap() {
        if config.total_supply > limit {
            return Err(ContractError::CannotExceedCap {});
        }
    }

    TOKEN_INFO.save(deps.storage, &config)?;

    capture_total_supply_history(deps.storage, &env, config.total_supply)?;

    // add amount to recipient balance
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "mint")
        .add_attribute("to", recipient)
        .add_attribute("amount", amount);
    Ok(res)
}

/// ## Description
/// Default CW20 execute_send method implementation with saving history.
pub fn execute_send(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    if amount == Uint128::zero() {
        return Err(ContractError::InvalidZeroAmount {});
    }

    let rcpt_addr = deps.api.addr_validate(&contract)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &info.sender,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new()
        .add_attribute("action", "send")
        .add_attribute("from", &info.sender)
        .add_attribute("to", &contract)
        .add_attribute("amount", amount)
        .add_message(
            Cw20ReceiveMsg {
                sender: info.sender.into(),
                amount,
                msg,
            }
            .into_cosmos_msg(contract)?,
        );
    Ok(res)
}

/// ## Description
/// Default CW20 execute_transfer_from method implementation with saving history.
pub fn execute_transfer_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    recipient: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&recipient)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let res = Response::new().add_attributes(vec![
        attr("action", "transfer_from"),
        attr("from", owner),
        attr("to", recipient),
        attr("by", info.sender),
        attr("amount", amount),
    ]);
    Ok(res)
}

/// ## Description
/// Default CW20 execute_burn_from method implementation with saving history.
pub fn execute_burn_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // lower balance
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;

    // reduce total_supply
    let token_info = TOKEN_INFO.update(deps.storage, |mut meta| -> StdResult<_> {
        meta.total_supply = meta.total_supply.checked_sub(amount)?;
        Ok(meta)
    })?;

    capture_total_supply_history(deps.storage, &env, token_info.total_supply)?;

    let res = Response::new().add_attributes(vec![
        attr("action", "burn_from"),
        attr("from", owner),
        attr("by", info.sender),
        attr("amount", amount),
    ]);
    Ok(res)
}

/// ## Description
/// Default CW20 execute_send_from method implementation with saving history.
pub fn execute_send_from(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    owner: String,
    contract: String,
    amount: Uint128,
    msg: Binary,
) -> Result<Response, ContractError> {
    let rcpt_addr = deps.api.addr_validate(&contract)?;
    let owner_addr = deps.api.addr_validate(&owner)?;

    // deduct allowance before doing anything else have enough allowance
    deduct_allowance(deps.storage, &owner_addr, &info.sender, &env.block, amount)?;

    // move the tokens to the contract
    BALANCES.update(
        deps.storage,
        &owner_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> {
            Ok(balance.unwrap_or_default().checked_sub(amount)?)
        },
    )?;
    BALANCES.update(
        deps.storage,
        &rcpt_addr,
        env.block.height,
        |balance: Option<Uint128>| -> StdResult<_> { Ok(balance.unwrap_or_default() + amount) },
    )?;

    let attrs = vec![
        attr("action", "send_from"),
        attr("from", &owner),
        attr("to", &contract),
        attr("by", &info.sender),
        attr("amount", amount),
    ];

    // create a send message
    let msg = Cw20ReceiveMsg {
        sender: info.sender.into(),
        amount,
        msg,
    }
    .into_cosmos_msg(contract)?;

    let res = Response::new().add_message(msg).add_attributes(attrs);
    Ok(res)
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
/// * **Balance { address: String }** Returns the current balance of the given address, 0 if unset.
///
/// * **BalanceAt { address, block }** Returns balance of the given address at the given block.
///
/// * **TotalSupplyAt { block }** Returns total supply at the given block.
///
/// * **TokenInfo {}** Returns the metadata on the contract - name, decimals, supply, etc.
///
/// * **Minter {}** Returns who can mint and the hard cap on maximum tokens after minting.
///
/// * **QueryMsg::Allowance { owner, spender }** Returns how much spender can use from owner account, 0 if unset.
///
/// * **QueryMsg::AllAllowances { owner, start_after, limit }** Returns all allowances this owner has approved. Supports pagination.
///
/// * **QueryMsg::AllAccounts { start_after, limit }** Returns all accounts that have balances. Supports pagination.
///
/// * **QueryMsg::MarketingInfo {}** Returns more metadata on the contract.
///
/// * **QueryMsg::DownloadLogo {}** Downloads the mbeded logo data (if stored on chain)
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Balance { address } => to_binary(&query_balance(deps, address)?),
        QueryMsg::BalanceAt { address, block } => {
            to_binary(&query_balance_at(deps, address, block)?)
        }
        QueryMsg::TotalSupplyAt { block } => to_binary(&get_total_supply_at(deps.storage, block)?),
        QueryMsg::TokenInfo {} => to_binary(&query_token_info(deps)?),
        QueryMsg::Minter {} => to_binary(&query_minter(deps)?),
        QueryMsg::Allowance { owner, spender } => {
            to_binary(&query_allowance(deps, owner, spender)?)
        }
        QueryMsg::AllAllowances {
            owner,
            start_after,
            limit,
        } => to_binary(&query_all_allowances(deps, owner, start_after, limit)?),
        QueryMsg::AllAccounts { start_after, limit } => {
            to_binary(&query_all_accounts(deps, start_after, limit)?)
        }
        QueryMsg::MarketingInfo {} => to_binary(&query_marketing_info(deps)?),
        QueryMsg::DownloadLogo {} => to_binary(&query_download_logo(deps)?),
    }
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **address** is the object of type [`String`].
pub fn query_balance(deps: Deps, address: String) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load(deps.storage, &address)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address at the given block.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **address** is the object of type [`String`].
///
/// * **block** is the object of type [`u64`].
pub fn query_balance_at(deps: Deps, address: String, block: u64) -> StdResult<BalanceResponse> {
    let address = deps.api.addr_validate(&address)?;
    let balance = BALANCES
        .may_load_at_height(deps.storage, &address, block)?
        .unwrap_or_default();
    Ok(BalanceResponse { balance })
}

/// ## Description
/// Returns an [`StdError`] on failure, otherwise returns balance of the given address at the given block.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field object of type [`String`].
///
/// * **limit** is an [`Option`] field object of type [`u32`].
pub fn query_all_accounts(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<AllAccountsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let accounts: Result<Vec<_>, _> = BALANCES
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(StdResult::unwrap)
        .map(|(a, _)| String::from_utf8(a))
        .collect();

    Ok(AllAccountsResponse {
        accounts: accounts?,
    })
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info, MOCK_CONTRACT_ADDR};
    use cosmwasm_std::{
        coins, Addr, BlockInfo, ContractInfo, CosmosMsg, StdError, SubMsg, Timestamp, WasmMsg,
    };
    use cw20::{Cw20Coin, MinterResponse, TokenInfoResponse};

    use super::*;

    pub struct MockEnvParams {
        pub block_time: Timestamp,
        pub block_height: u64,
    }

    impl Default for MockEnvParams {
        fn default() -> Self {
            MockEnvParams {
                block_time: Timestamp::from_nanos(1_571_797_419_879_305_533),
                block_height: 1,
            }
        }
    }

    pub fn test_mock_env(mock_env_params: MockEnvParams) -> Env {
        Env {
            block: BlockInfo {
                height: mock_env_params.block_height,
                time: mock_env_params.block_time,
                chain_id: "cosmos-testnet-14002".to_string(),
            },
            contract: ContractInfo {
                address: Addr::unchecked(MOCK_CONTRACT_ADDR),
            },
        }
    }

    fn get_balance<T: Into<String>>(deps: Deps, address: T) -> Uint128 {
        query_balance(deps, address.into()).unwrap().balance
    }

    // this will set up the instantiation for other tests
    fn do_instantiate_with_minter(
        deps: DepsMut,
        addr: &str,
        amount: Uint128,
        minter: &str,
        cap: Option<Uint128>,
    ) -> TokenInfoResponse {
        _do_instantiate(
            deps,
            addr,
            amount,
            Some(MinterResponse {
                minter: minter.to_string(),
                cap,
            }),
        )
    }

    // this will set up the instantiation for other tests
    fn do_instantiate(deps: DepsMut, addr: &str, amount: Uint128) -> TokenInfoResponse {
        _do_instantiate(deps, addr, amount, None)
    }

    // this will set up the instantiation for other tests
    fn _do_instantiate(
        mut deps: DepsMut,
        addr: &str,
        amount: Uint128,
        mint: Option<MinterResponse>,
    ) -> TokenInfoResponse {
        let instantiate_msg = InstantiateMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![Cw20Coin {
                address: addr.to_string(),
                amount,
            }],
            mint: mint.clone(),
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.branch(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        let meta = query_token_info(deps.as_ref()).unwrap();
        assert_eq!(
            meta,
            TokenInfoResponse {
                name: "Auto Gen".to_string(),
                symbol: "AUTO".to_string(),
                decimals: 3,
                total_supply: amount,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr), amount);
        assert_eq!(query_minter(deps.as_ref()).unwrap(), mint,);
        meta
    }

    mod instantiate {
        use super::*;

        #[test]
        fn basic() {
            let mut deps = mock_dependencies(&[]);
            let amount = Uint128::from(11223344u128);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                mint: None,
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
        }

        #[test]
        fn mintable() {
            let mut deps = mock_dependencies(&[]);
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let limit = Uint128::new(511223344);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: "addr0000".into(),
                    amount,
                }],
                mint: Some(MinterResponse {
                    minter: minter.clone(),
                    cap: Some(limit),
                }),
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
            assert_eq!(0, res.messages.len());

            assert_eq!(
                query_token_info(deps.as_ref()).unwrap(),
                TokenInfoResponse {
                    name: "Cash Token".to_string(),
                    symbol: "CASH".to_string(),
                    decimals: 9,
                    total_supply: amount,
                }
            );
            assert_eq!(
                get_balance(deps.as_ref(), "addr0000"),
                Uint128::new(11223344)
            );
            assert_eq!(
                query_minter(deps.as_ref()).unwrap(),
                Some(MinterResponse {
                    minter,
                    cap: Some(limit),
                }),
            );
        }

        #[test]
        fn mintable_over_cap() {
            let mut deps = mock_dependencies(&[]);
            let amount = Uint128::new(11223344);
            let minter = String::from("asmodat");
            let limit = Uint128::new(11223300);
            let instantiate_msg = InstantiateMsg {
                name: "Cash Token".to_string(),
                symbol: "CASH".to_string(),
                decimals: 9,
                initial_balances: vec![Cw20Coin {
                    address: String::from("addr0000"),
                    amount,
                }],
                mint: Some(MinterResponse {
                    minter,
                    cap: Some(limit),
                }),
            };
            let info = mock_info("creator", &[]);
            let env = mock_env();
            let err = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap_err();
            assert_eq!(
                err,
                StdError::generic_err("Initial supply greater than cap").into()
            );
        }
    }

    #[test]
    fn can_mint_by_minter() {
        let mut deps = mock_dependencies(&[]);

        let genesis = String::from("genesis");
        let amount = Uint128::new(11223344);
        let minter = String::from("asmodat");
        let limit = Uint128::new(511223344);
        do_instantiate_with_minter(deps.as_mut(), &genesis, amount, &minter, Some(limit));

        // minter can mint coins to some winner
        let winner = String::from("lucky");
        let prize = Uint128::new(222_222_222);
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: prize,
        };

        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(0, res.messages.len());
        assert_eq!(get_balance(deps.as_ref(), genesis), amount);
        assert_eq!(get_balance(deps.as_ref(), winner.clone()), prize);

        // but cannot mint nothing
        let msg = ExecuteMsg::Mint {
            recipient: winner.clone(),
            amount: Uint128::zero(),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // but if it exceeds cap (even over multiple rounds), it fails
        // cap is enforced
        let msg = ExecuteMsg::Mint {
            recipient: winner,
            amount: Uint128::new(333_222_222),
        };
        let info = mock_info(minter.as_ref(), &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::CannotExceedCap {});
    }

    #[test]
    fn others_cannot_mint() {
        let mut deps = mock_dependencies(&[]);
        do_instantiate_with_minter(
            deps.as_mut(),
            &String::from("genesis"),
            Uint128::new(1234),
            &String::from("minter"),
            None,
        );

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("anyone else", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn no_one_mints_if_minter_unset() {
        let mut deps = mock_dependencies(&[]);
        do_instantiate(deps.as_mut(), &String::from("genesis"), Uint128::new(1234));

        let msg = ExecuteMsg::Mint {
            recipient: String::from("lucky"),
            amount: Uint128::new(222),
        };
        let info = mock_info("genesis", &[]);
        let env = mock_env();
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::Unauthorized {});
    }

    #[test]
    fn instantiate_multiple_accounts() {
        let mut deps = mock_dependencies(&[]);
        let amount1 = Uint128::from(11223344u128);
        let addr1 = String::from("addr0001");
        let amount2 = Uint128::from(7890987u128);
        let addr2 = String::from("addr0002");
        let instantiate_msg = InstantiateMsg {
            name: "Bash Shell".to_string(),
            symbol: "BASH".to_string(),
            decimals: 6,
            initial_balances: vec![
                Cw20Coin {
                    address: addr1.clone(),
                    amount: amount1,
                },
                Cw20Coin {
                    address: addr2.clone(),
                    amount: amount2,
                },
            ],
            mint: None,
        };
        let info = mock_info("creator", &[]);
        let env = mock_env();
        let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
        assert_eq!(0, res.messages.len());

        assert_eq!(
            query_token_info(deps.as_ref()).unwrap(),
            TokenInfoResponse {
                name: "Bash Shell".to_string(),
                symbol: "BASH".to_string(),
                decimals: 6,
                total_supply: amount1 + amount2,
            }
        );
        assert_eq!(get_balance(deps.as_ref(), addr1), amount1);
        assert_eq!(get_balance(deps.as_ref(), addr2), amount2);
    }

    #[test]
    fn transfer() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let addr2 = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot transfer nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: too_much,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // cannot send from empty account
        let info = mock_info(addr2.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Transfer {
            recipient: addr1.clone(),
            amount: transfer,
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = test_mock_env(MockEnvParams {
            block_height: 100_000,
            ..Default::default()
        });
        let msg = ExecuteMsg::Transfer {
            recipient: addr2.clone(),
            amount: transfer,
        };
        let res = execute(deps.as_mut(), env, info, msg).unwrap();
        assert_eq!(res.messages.len(), 0);

        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
        assert_eq!(get_balance(deps.as_ref(), addr2.clone()), transfer);
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1, 100_000)
                .unwrap()
                .balance,
            amount1
        );
        assert_eq!(
            query_balance_at(deps.as_ref(), addr2, 100_000)
                .unwrap()
                .balance,
            Uint128::zero()
        );
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
    }

    #[test]
    fn burn() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let amount1 = Uint128::from(12340000u128);
        let burn = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot burn nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn {
            amount: Uint128::zero(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // cannot burn more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Burn { amount: too_much };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );

        // valid burn reduces total supply
        let info = mock_info(addr1.as_ref(), &[]);
        let env = test_mock_env(MockEnvParams {
            block_height: 200_000,
            ..Default::default()
        });
        let msg = ExecuteMsg::Burn { amount: burn };
        execute(deps.as_mut(), env, info, msg).unwrap();

        let remainder = amount1.checked_sub(burn).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1, 200_000)
                .unwrap()
                .balance,
            amount1
        );
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            remainder
        );
        assert_eq!(
            get_total_supply_at(&deps.storage, 200_000).unwrap(),
            remainder
        );
    }

    #[test]
    fn send() {
        let mut deps = mock_dependencies(&coins(2, "token"));
        let addr1 = String::from("addr0001");
        let contract = String::from("addr0002");
        let amount1 = Uint128::from(12340000u128);
        let transfer = Uint128::from(76543u128);
        let too_much = Uint128::from(12340321u128);
        let send_msg = Binary::from(r#"{"some":123}"#.as_bytes());

        do_instantiate(deps.as_mut(), &addr1, amount1);

        // cannot send nothing
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: Uint128::zero(),
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert_eq!(err, ContractError::InvalidZeroAmount {});

        // cannot send more than we have
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: too_much,
            msg: send_msg.clone(),
        };
        let err = execute(deps.as_mut(), env, info, msg).unwrap_err();
        assert!(matches!(err, ContractError::Std(StdError::Overflow { .. })));

        // valid transfer
        let info = mock_info(addr1.as_ref(), &[]);
        let env = mock_env();
        let msg = ExecuteMsg::Send {
            contract: contract.clone(),
            amount: transfer,
            msg: send_msg.clone(),
        };
        let res = execute(deps.as_mut(), env.clone(), info, msg).unwrap();
        assert_eq!(res.messages.len(), 1);

        // ensure proper send message sent
        // this is the message we want delivered to the other side
        let binary_msg = Cw20ReceiveMsg {
            sender: addr1.clone(),
            amount: transfer,
            msg: send_msg,
        }
        .into_binary()
        .unwrap();
        // and this is how it must be wrapped for the vm to process it
        assert_eq!(
            res.messages[0],
            SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: contract.clone(),
                msg: binary_msg,
                funds: vec![],
            }))
        );

        // ensure balance is properly transferred
        let remainder = amount1.checked_sub(transfer).unwrap();
        assert_eq!(get_balance(deps.as_ref(), addr1.clone()), remainder);
        assert_eq!(get_balance(deps.as_ref(), contract.clone()), transfer);
        assert_eq!(
            query_token_info(deps.as_ref()).unwrap().total_supply,
            amount1
        );
        assert_eq!(
            query_balance_at(deps.as_ref(), addr1, env.block.height)
                .unwrap()
                .balance,
            Uint128::zero()
        );
        assert_eq!(
            query_balance_at(deps.as_ref(), contract, env.block.height)
                .unwrap()
                .balance,
            Uint128::zero()
        );
    }

    #[test]
    fn snapshots_are_taken_and_retrieved_correctly() {
        let mut deps = mock_dependencies(&[]);

        let addr1 = String::from("addr1");
        let addr2 = String::from("addr2");

        let mut current_total_supply = Uint128::new(100_000);
        let mut current_block = 12_345;
        let mut current_addr1_balance = current_total_supply;
        let mut current_addr2_balance = Uint128::zero();

        let minter = String::from("minter");
        do_instantiate_with_minter(deps.as_mut(), &addr1, current_total_supply, &minter, None);

        let mut expected_total_supplies = vec![(current_block, current_total_supply)];
        let mut expected_addr1_balances = vec![(current_block, current_addr1_balance)];
        let mut expected_addr2_balances: Vec<(u64, Uint128)> = vec![];

        // Mint to addr2 3 times
        for _i in 0..3 {
            current_block += 100_000;

            let mint_amount = Uint128::new(20_000);
            current_total_supply += mint_amount;
            current_addr2_balance += mint_amount;

            let info = mock_info(minter.as_str(), &[]);
            let env = test_mock_env(MockEnvParams {
                block_height: current_block,
                ..Default::default()
            });

            let msg = ExecuteMsg::Mint {
                recipient: addr2.clone(),
                amount: mint_amount,
            };

            execute(deps.as_mut(), env, info, msg).unwrap();

            expected_total_supplies.push((current_block, current_total_supply));
            expected_addr2_balances.push((current_block, current_addr2_balance));
        }

        // Transfer from addr1 to addr2 4 times
        for _i in 0..4 {
            current_block += 60_000;

            let transfer_amount = Uint128::new(10_000);
            current_addr1_balance = current_addr1_balance - transfer_amount;
            current_addr2_balance += transfer_amount;

            let info = mock_info(addr1.as_str(), &[]);
            let env = test_mock_env(MockEnvParams {
                block_height: current_block,
                ..Default::default()
            });

            let msg = ExecuteMsg::Transfer {
                recipient: addr2.clone(),
                amount: transfer_amount,
            };

            execute(deps.as_mut(), env, info, msg).unwrap();

            expected_addr1_balances.push((current_block, current_addr1_balance));
            expected_addr2_balances.push((current_block, current_addr2_balance));
        }

        // Burn from addr2 3 times
        for _i in 0..3 {
            current_block += 50_000;

            let burn_amount = Uint128::new(20_000);
            current_total_supply = current_total_supply - burn_amount;
            current_addr2_balance = current_addr2_balance - burn_amount;

            let info = mock_info(addr2.as_str(), &[]);

            let env = test_mock_env(MockEnvParams {
                block_height: current_block,
                ..Default::default()
            });

            let msg = ExecuteMsg::Burn {
                amount: burn_amount,
            };

            execute(deps.as_mut(), env, info, msg).unwrap();

            expected_total_supplies.push((current_block, current_total_supply));
            expected_addr2_balances.push((current_block, current_addr2_balance));
        }

        // Check total supplies;
        let mut total_supply_previous_value = Uint128::zero();
        for (block, expected_total_supply) in expected_total_supplies {
            // block before gives previous value
            assert_eq!(
                get_total_supply_at(&deps.storage, block - 1).unwrap(),
                total_supply_previous_value
            );

            // block gives expected value
            assert_eq!(
                get_total_supply_at(&deps.storage, block).unwrap(),
                expected_total_supply,
            );

            // block after still gives expected value
            assert_eq!(
                get_total_supply_at(&deps.storage, block + 10).unwrap(),
                expected_total_supply,
            );

            total_supply_previous_value = expected_total_supply;
        }

        // Check addr1 balances
        let mut balance_previous_value = Uint128::zero();
        for (block, expected_balance) in expected_addr1_balances {
            // block before gives previous value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr1.clone(), block - 10)
                    .unwrap()
                    .balance,
                balance_previous_value
            );

            // block gives previous value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr1.clone(), block)
                    .unwrap()
                    .balance,
                balance_previous_value
            );

            // only block after still gives expected value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr1.clone(), block + 1)
                    .unwrap()
                    .balance,
                expected_balance
            );

            balance_previous_value = expected_balance;
        }

        // Check addr2 balances
        let mut balance_previous_value = Uint128::zero();
        for (block, expected_balance) in expected_addr2_balances {
            // block before gives previous value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr2.clone(), block - 10)
                    .unwrap()
                    .balance,
                balance_previous_value
            );

            // block gives previous value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr2.clone(), block)
                    .unwrap()
                    .balance,
                balance_previous_value
            );

            // only block after still gives expected value
            assert_eq!(
                query_balance_at(deps.as_ref(), addr2.clone(), block + 1)
                    .unwrap()
                    .balance,
                expected_balance
            );

            balance_previous_value = expected_balance;
        }
    }
}
