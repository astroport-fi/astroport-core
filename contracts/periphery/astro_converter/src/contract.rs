#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, coins, ensure, from_json, to_json_binary, wasm_execute, Api, BankMsg, Binary,
    CosmosMsg, CustomMsg, Deps, DepsMut, Env, IbcMsg, IbcTimeout, MessageInfo, QuerierWrapper,
    Response, StdError, StdResult,
};
use cw2::set_contract_version;
use cw20::{Cw20ExecuteMsg, Cw20QueryMsg, Cw20ReceiveMsg};
use cw_utils::{must_pay, nonpayable};

use astroport::asset::{addr_opt_validate, validate_native_denom, AssetInfo};
use astroport::astro_converter::{
    Config, Cw20HookMsg, ExecuteMsg, InstantiateMsg, QueryMsg, DEFAULT_TIMEOUT,
};

use crate::error::ContractError;
use crate::state::CONFIG;

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    init(deps, env, info, msg)
}

pub fn init(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    validate_native_denom(&msg.new_astro_denom)?;
    msg.old_astro_asset_info.check(deps.api)?;

    if msg.old_astro_asset_info.is_ibc() {
        ensure!(
            msg.outpost_burn_params.is_some(),
            StdError::generic_err("Burn params must be specified on outpost")
        );
    }

    ensure!(
        msg.old_astro_asset_info != AssetInfo::native(&msg.new_astro_denom),
        StdError::generic_err("Cannot convert to the same asset")
    );

    let attrs = [
        attr("contract_name", CONTRACT_NAME),
        attr("astro_old_denom", msg.old_astro_asset_info.to_string()),
        attr("astro_new_denom", &msg.new_astro_denom),
    ];

    CONFIG.save(
        deps.storage,
        &Config {
            old_astro_asset_info: msg.old_astro_asset_info,
            new_astro_denom: msg.new_astro_denom,
            outpost_burn_params: msg.outpost_burn_params,
        },
    )?;

    Ok(Response::default().add_attributes(attrs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    match msg {
        ExecuteMsg::Receive(cw20_msg) => cw20_receive(deps.api, config, info, cw20_msg),
        ExecuteMsg::Convert { receiver } => convert(deps.api, config, info, receiver),
        ExecuteMsg::TransferForBurning { timeout } => {
            ibc_transfer_for_burning(deps.querier, env, info, config, timeout)
        }
        ExecuteMsg::Burn {} => burn(deps.querier, env, info, config),
    }
}

pub fn cw20_receive<M: CustomMsg>(
    api: &dyn Api,
    config: Config,
    info: MessageInfo,
    cw20_msg: Cw20ReceiveMsg,
) -> Result<Response<M>, ContractError> {
    match config.old_astro_asset_info {
        AssetInfo::Token { contract_addr } => {
            if info.sender == contract_addr {
                let receiver = from_json::<Cw20HookMsg>(&cw20_msg.msg)?.receiver;
                addr_opt_validate(api, &receiver)?;

                let receiver = receiver.unwrap_or_else(|| cw20_msg.sender);
                let bank_msg = BankMsg::Send {
                    to_address: receiver.clone(),
                    amount: coins(cw20_msg.amount.u128(), config.new_astro_denom),
                };

                Ok(Response::new().add_message(bank_msg).add_attributes([
                    attr("action", "convert"),
                    attr("receiver", receiver),
                    attr("type", "cw20:astro"),
                    attr("amount", cw20_msg.amount),
                ]))
            } else {
                Err(ContractError::UnsupportedCw20Token(info.sender))
            }
        }
        AssetInfo::NativeToken { .. } => Err(ContractError::InvalidEndpoint {}),
    }
}

pub fn convert<M: CustomMsg>(
    api: &dyn Api,
    config: Config,
    info: MessageInfo,
    receiver: Option<String>,
) -> Result<Response<M>, ContractError> {
    match config.old_astro_asset_info {
        AssetInfo::NativeToken { denom } => {
            let amount = must_pay(&info, &denom)?;
            addr_opt_validate(api, &receiver)?;

            let receiver = receiver.unwrap_or_else(|| info.sender.to_string());
            let bank_msg = BankMsg::Send {
                to_address: receiver.clone(),
                amount: coins(amount.u128(), config.new_astro_denom),
            };

            Ok(Response::new().add_message(bank_msg).add_attributes([
                attr("action", "convert"),
                attr("receiver", receiver),
                attr("type", "ibc:astro"),
                attr("amount", amount),
            ]))
        }
        AssetInfo::Token { .. } => Err(ContractError::InvalidEndpoint {}),
    }
}

pub fn ibc_transfer_for_burning(
    querier: QuerierWrapper,
    env: Env,
    info: MessageInfo,
    config: Config,
    timeout: Option<u64>,
) -> Result<Response, ContractError> {
    nonpayable(&info)?;
    match config.old_astro_asset_info {
        AssetInfo::NativeToken { denom } => {
            let amount = querier.query_balance(&env.contract.address, &denom)?.amount;

            ensure!(
                !amount.is_zero(),
                StdError::generic_err("No tokens to transfer")
            );

            let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
            let burn_params = config.outpost_burn_params.expect("No outpost burn params");

            let ibc_transfer_msg = IbcMsg::Transfer {
                channel_id: burn_params.old_astro_transfer_channel,
                to_address: burn_params.terra_burn_addr,
                amount: coin(amount.u128(), denom),
                timeout: IbcTimeout::with_timestamp(env.block.time.plus_seconds(timeout)),
            };

            Ok(Response::new()
                .add_message(CosmosMsg::Ibc(ibc_transfer_msg))
                .add_attributes([
                    attr("action", "ibc_transfer_for_burning"),
                    attr("type", "ibc:astro"),
                    attr("amount", amount),
                ]))
        }
        AssetInfo::Token { .. } => Err(ContractError::IbcTransferError {}),
    }
}

pub fn burn<M: CustomMsg>(
    querier: QuerierWrapper,
    env: Env,
    info: MessageInfo,
    config: Config,
) -> Result<Response<M>, ContractError> {
    nonpayable(&info)?;
    match config.old_astro_asset_info {
        AssetInfo::Token { contract_addr } => {
            let amount = querier
                .query_wasm_smart::<cw20::BalanceResponse>(
                    &contract_addr,
                    &Cw20QueryMsg::Balance {
                        address: env.contract.address.to_string(),
                    },
                )?
                .balance;

            ensure!(
                !amount.is_zero(),
                StdError::generic_err("No tokens to burn")
            );

            let burn_msg = wasm_execute(contract_addr, &Cw20ExecuteMsg::Burn { amount }, vec![])?;

            Ok(Response::new().add_message(burn_msg).add_attributes([
                attr("action", "burn"),
                attr("type", "cw20:astro"),
                attr("amount", amount),
            ]))
        }
        AssetInfo::NativeToken { .. } => Err(ContractError::BurnError {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&CONFIG.load(deps.storage)?),
    }
}

#[cfg(test)]
mod testing {
    use cosmwasm_std::testing::{
        mock_dependencies, mock_dependencies_with_balance, mock_env, mock_info, MockApi,
        MockQuerier,
    };
    use cosmwasm_std::{
        from_json, to_json_binary, Addr, ContractResult, Empty, SubMsg, SystemResult, Uint128,
        WasmMsg, WasmQuery,
    };
    use cw_utils::PaymentError::{MissingDenom, NoFunds};

    use astroport::astro_converter::OutpostBurnParams;

    use super::*;

    #[test]
    fn test_instantiate() {
        let mut deps = mock_dependencies();
        let mut msg = InstantiateMsg {
            old_astro_asset_info: AssetInfo::native("uastro"),
            new_astro_denom: "uastro".to_string(),
            outpost_burn_params: None,
        };
        let info = mock_info("creator", &[]);
        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Generic error: Cannot convert to the same asset"
        );

        msg.old_astro_asset_info = AssetInfo::native("ibc/old_astro");

        let err = instantiate(deps.as_mut(), mock_env(), info.clone(), msg.clone()).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Burn params must be specified on outpost"
        );

        msg.outpost_burn_params = Some(OutpostBurnParams {
            terra_burn_addr: "terra1xxx".to_string(),
            old_astro_transfer_channel: "channel-1".to_string(),
        });

        instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();

        let config_data = query(deps.as_ref(), mock_env(), QueryMsg::Config {}).unwrap();
        let config = from_json::<Config>(&config_data).unwrap();

        assert_eq!(
            config,
            Config {
                old_astro_asset_info: AssetInfo::native("ibc/old_astro"),
                new_astro_denom: "uastro".to_string(),
                outpost_burn_params: Some(OutpostBurnParams {
                    terra_burn_addr: "terra1xxx".to_string(),
                    old_astro_transfer_channel: "channel-1".to_string(),
                }),
            }
        );
    }

    #[test]
    fn test_cw20_convert() {
        let mut config = Config {
            old_astro_asset_info: AssetInfo::native("uastro"),
            new_astro_denom: "ibc/astro".to_string(),
            outpost_burn_params: None,
        };
        let mock_api = MockApi::default();

        let mut cw20_msg = Cw20ReceiveMsg {
            sender: "sender".to_string(),
            amount: 100u128.into(),
            msg: to_json_binary(&Empty {}).unwrap(),
        };
        let err = cw20_receive::<Empty>(
            &mock_api,
            config.clone(),
            mock_info("random_cw20", &[]),
            cw20_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::InvalidEndpoint {});

        config.old_astro_asset_info = AssetInfo::cw20_unchecked("terra1xxx");

        let err = cw20_receive::<Empty>(
            &mock_api,
            config.clone(),
            mock_info("random_cw20", &[]),
            cw20_msg.clone(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            ContractError::UnsupportedCw20Token(Addr::unchecked("random_cw20"))
        );

        let res = cw20_receive::<Empty>(
            &mock_api,
            config.clone(),
            mock_info("terra1xxx", &[]),
            cw20_msg.clone(),
        )
        .unwrap();

        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: cw20_msg.sender.clone(),
                amount: coins(cw20_msg.amount.u128(), config.new_astro_denom.clone())
            }))]
        );

        cw20_msg.msg = to_json_binary(&Cw20HookMsg {
            receiver: Some("receiver".to_string()),
        })
        .unwrap();
        let res = cw20_receive::<Empty>(
            &mock_api,
            config.clone(),
            mock_info("terra1xxx", &[]),
            cw20_msg.clone(),
        )
        .unwrap();

        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "receiver".to_string(),
                amount: coins(cw20_msg.amount.u128(), config.new_astro_denom)
            }))]
        );
    }

    #[test]
    fn test_native_convert() {
        let mut config = Config {
            old_astro_asset_info: AssetInfo::cw20_unchecked("terra1xxx"),
            new_astro_denom: "ibc/astro".to_string(),
            outpost_burn_params: None,
        };
        let mock_api = MockApi::default();

        let info = mock_info("sender", &[]);
        let err = convert::<Empty>(&mock_api, config.clone(), info, None).unwrap_err();
        assert_eq!(err, ContractError::InvalidEndpoint {});

        config.old_astro_asset_info = AssetInfo::native("ibc/old_astro");

        let info = mock_info("sender", &[]);
        let err = convert::<Empty>(&mock_api, config.clone(), info, None).unwrap_err();
        assert_eq!(err, ContractError::PaymentError(NoFunds {}));

        let info = mock_info("sender", &coins(100, "random_coin"));
        let err = convert::<Empty>(&mock_api, config.clone(), info, None).unwrap_err();
        assert_eq!(
            err,
            ContractError::PaymentError(MissingDenom("ibc/old_astro".to_string()))
        );

        let info = mock_info("sender", &coins(100, "ibc/old_astro"));
        let res = convert::<Empty>(&mock_api, config.clone(), info.clone(), None).unwrap();
        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: info.sender.to_string(),
                amount: coins(100, config.new_astro_denom.clone())
            }))]
        );

        let res = convert::<Empty>(
            &mock_api,
            config.clone(),
            info.clone(),
            Some("receiver".to_string()),
        )
        .unwrap();
        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Bank(BankMsg::Send {
                to_address: "receiver".to_string(),
                amount: coins(100, config.new_astro_denom)
            }))]
        );
    }

    #[test]
    fn test_ibc_transfer() {
        let deps = mock_dependencies();
        let outpost_params = OutpostBurnParams {
            terra_burn_addr: "terra1xxx".to_string(),
            old_astro_transfer_channel: "channel-1".to_string(),
        };
        let mut config = Config {
            old_astro_asset_info: AssetInfo::cw20_unchecked("terra1xxx"),
            new_astro_denom: "ibc/astro".to_string(),
            outpost_burn_params: Some(outpost_params.clone()),
        };

        let info = mock_info("permissionless", &[]);
        let err = ibc_transfer_for_burning(
            deps.as_ref().querier,
            mock_env(),
            info.clone(),
            config.clone(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::IbcTransferError {});

        config.old_astro_asset_info = AssetInfo::native("ibc/old_astro");

        let err = ibc_transfer_for_burning(
            deps.as_ref().querier,
            mock_env(),
            info.clone(),
            config.clone(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "Generic error: No tokens to transfer");

        let deps = mock_dependencies_with_balance(&coins(100, "ibc/old_astro"));
        let env = mock_env();
        let res = ibc_transfer_for_burning(
            deps.as_ref().querier,
            env.clone(),
            info,
            config.clone(),
            None,
        )
        .unwrap();

        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Ibc(IbcMsg::Transfer {
                channel_id: outpost_params.old_astro_transfer_channel,
                to_address: outpost_params.terra_burn_addr,
                amount: coin(100, "ibc/old_astro"),
                timeout: env.block.time.plus_seconds(DEFAULT_TIMEOUT).into(),
            }))]
        );
    }

    fn querier_wrapper_with_cw20_balances(
        mock_querier: &mut MockQuerier,
        balances: Vec<(Addr, Uint128)>,
    ) -> QuerierWrapper {
        let wasm_handler = move |query: &WasmQuery| match query {
            WasmQuery::Smart { contract_addr, msg } if contract_addr == "terra1xxx" => {
                let contract_result: ContractResult<_> = match from_json(msg) {
                    Ok(Cw20QueryMsg::Balance { address }) => {
                        let balance = balances
                            .iter()
                            .find_map(|(addr, balance)| {
                                if addr == &address {
                                    Some(balance)
                                } else {
                                    None
                                }
                            })
                            .cloned()
                            .unwrap_or_else(Uint128::zero);
                        to_json_binary(&cw20::BalanceResponse { balance }).into()
                    }
                    _ => unimplemented!(),
                };
                SystemResult::Ok(contract_result)
            }
            _ => unimplemented!(),
        };
        mock_querier.update_wasm(wasm_handler);

        QuerierWrapper::new(&*mock_querier)
    }

    #[test]
    fn test_burn() {
        let deps = mock_dependencies();
        let mut config = Config {
            old_astro_asset_info: AssetInfo::native("ibc/old_astro"),
            new_astro_denom: "ibc/astro".to_string(),
            outpost_burn_params: None,
        };

        let info = mock_info("permissionless", &[]);
        let err = burn::<Empty>(
            deps.as_ref().querier,
            mock_env(),
            info.clone(),
            config.clone(),
        )
        .unwrap_err();
        assert_eq!(err, ContractError::BurnError {});

        config.old_astro_asset_info = AssetInfo::cw20_unchecked("terra1xxx");

        let env = mock_env();
        let mut mock_querier: MockQuerier = MockQuerier::new(&[]);
        let querier_wrapper = querier_wrapper_with_cw20_balances(
            &mut mock_querier,
            vec![(env.contract.address.clone(), 0u128.into())],
        );
        let err =
            burn::<Empty>(querier_wrapper, mock_env(), info.clone(), config.clone()).unwrap_err();
        assert_eq!(err.to_string(), "Generic error: No tokens to burn");

        let env = mock_env();
        let mut mock_querier: MockQuerier = MockQuerier::new(&[]);
        let querier_wrapper = querier_wrapper_with_cw20_balances(
            &mut mock_querier,
            vec![(env.contract.address.clone(), 100u128.into())],
        );
        let res = burn::<Empty>(querier_wrapper, env.clone(), info, config.clone()).unwrap();

        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.old_astro_asset_info.to_string(),
                msg: to_json_binary(&Cw20ExecuteMsg::Burn {
                    amount: 100u128.into()
                })
                .unwrap(),
                funds: vec![],
            }))]
        );
    }
}
