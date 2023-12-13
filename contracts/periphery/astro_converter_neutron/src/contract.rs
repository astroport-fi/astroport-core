#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    attr, coin, ensure, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};
use cw2::set_contract_version;
use cw_utils::nonpayable;
use neutron_sdk::bindings::msg::{IbcFee, NeutronMsg};
use neutron_sdk::bindings::query::NeutronQuery;
use neutron_sdk::query::min_ibc_fee::query_min_ibc_fee;
use neutron_sdk::sudo::msg::{RequestPacketTimeoutHeight, TransferSudoMsg};

use astro_token_converter::contract::{burn, convert, cw20_receive};
use astro_token_converter::error::ContractError;
use astro_token_converter::state::CONFIG;
use astroport::asset::AssetInfo;
use astroport::astro_converter::{Config, ExecuteMsg, InstantiateMsg, DEFAULT_TIMEOUT};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Denom used to pay IBC fees
const FEE_DENOM: &str = "untrn";

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    astro_token_converter::contract::instantiate(deps, env, info, msg)
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<NeutronMsg>, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    match msg {
        ExecuteMsg::Receive(cw20_msg) => cw20_receive(config, info, cw20_msg),
        ExecuteMsg::Convert {} => convert(config, info),
        ExecuteMsg::TransferForBurning { timeout } => {
            ibc_transfer_for_burning(deps.as_ref(), env, info, config, timeout)
        }
        ExecuteMsg::Burn {} => burn(deps.into_empty().querier, env, info, config),
    }
}

pub fn ibc_transfer_for_burning(
    deps: Deps<NeutronQuery>,
    env: Env,
    info: MessageInfo,
    config: Config,
    timeout: Option<u64>,
) -> Result<Response<NeutronMsg>, ContractError> {
    nonpayable(&info)?;
    match config.old_astro_asset_info {
        AssetInfo::NativeToken { denom } => {
            let ntrn_bal = deps
                .querier
                .query_balance(&env.contract.address, FEE_DENOM)?
                .amount;

            ensure!(
                ntrn_bal.u128() >= 200_000,
                StdError::generic_err("Contract requires at least 0.2 NTRN in balance")
            );

            let amount = deps
                .querier
                .query_balance(&env.contract.address, &denom)?
                .amount;

            ensure!(
                !amount.is_zero(),
                StdError::generic_err("No tokens to transfer")
            );

            let timeout = timeout.unwrap_or(DEFAULT_TIMEOUT);
            let burn_params = config.outpost_burn_params.expect("No outpost burn params");

            let fee = min_ntrn_ibc_fee(
                query_min_ibc_fee(deps)
                    .map_err(|err| StdError::generic_err(err.to_string()))?
                    .min_fee,
            );

            let ibc_transfer_msg = NeutronMsg::IbcTransfer {
                source_port: "transfer".to_string(),
                source_channel: burn_params.old_astro_transfer_channel,
                sender: env.contract.address.to_string(),
                receiver: burn_params.terra_burn_addr,
                token: coin(amount.u128(), denom),
                timeout_height: RequestPacketTimeoutHeight {
                    revision_number: None,
                    revision_height: None,
                },
                timeout_timestamp: env.block.time.plus_seconds(timeout).nanos(),
                memo: "".to_string(),
                fee,
            };

            Ok(Response::new()
                .add_message(ibc_transfer_msg)
                .add_attributes([
                    attr("action", "ibc_transfer_for_burning"),
                    attr("type", "ibc:astro"),
                    attr("amount", amount),
                ]))
        }
        AssetInfo::Token { .. } => Err(ContractError::IbcTransferError {}),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(_deps: DepsMut, _env: Env, _msg: TransferSudoMsg) -> StdResult<Response> {
    // Neutron requires sudo endpoint to be implemented
    Ok(Response::new())
}

fn min_ntrn_ibc_fee(fee: IbcFee) -> IbcFee {
    IbcFee {
        recv_fee: fee.recv_fee,
        ack_fee: fee
            .ack_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
        timeout_fee: fee
            .timeout_fee
            .into_iter()
            .filter(|a| a.denom == FEE_DENOM)
            .collect(),
    }
}

#[cfg(test)]
mod testing {
    use std::marker::PhantomData;

    use cosmwasm_std::testing::{mock_env, mock_info, MockApi, MockQuerier, MockStorage};
    use cosmwasm_std::{
        coins, to_json_binary, Coin, ContractResult, CosmosMsg, OwnedDeps, SubMsg, SystemResult,
    };
    use neutron_sdk::query::min_ibc_fee::MinIbcFeeResponse;

    use astroport::astro_converter::OutpostBurnParams;

    use super::*;

    fn mock_neutron_dependencies(
        balances: &[(&str, &[Coin])],
    ) -> OwnedDeps<MockStorage, MockApi, MockQuerier<NeutronQuery>, NeutronQuery> {
        let neutron_custom_handler = |request: &NeutronQuery| {
            let contract_result: ContractResult<_> = match request {
                NeutronQuery::MinIbcFee {} => to_json_binary(&MinIbcFeeResponse {
                    min_fee: IbcFee {
                        recv_fee: vec![],
                        ack_fee: coins(100_000, FEE_DENOM),
                        timeout_fee: coins(100_000, FEE_DENOM),
                    },
                })
                .into(),
                _ => unimplemented!("Unsupported query request: {:?}", request),
            };
            SystemResult::Ok(contract_result)
        };

        OwnedDeps {
            storage: MockStorage::default(),
            api: MockApi::default(),
            querier: MockQuerier::new(balances).with_custom_handler(neutron_custom_handler),
            custom_query_type: PhantomData,
        }
    }

    #[test]
    fn test_neutron_ibc_transfer() {
        let deps = mock_neutron_dependencies(&[]);
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
            deps.as_ref(),
            mock_env(),
            info.clone(),
            config.clone(),
            None,
        )
        .unwrap_err();
        assert_eq!(err, ContractError::IbcTransferError {});

        config.old_astro_asset_info = AssetInfo::native("ibc/old_astro");

        let env = mock_env();
        let deps = mock_neutron_dependencies(&[(
            env.contract.address.as_str(),
            &coins(100, "ibc/old_astro"),
        )]);
        let err = ibc_transfer_for_burning(
            deps.as_ref(),
            env.clone(),
            info.clone(),
            config.clone(),
            None,
        )
        .unwrap_err();
        assert_eq!(
            err.to_string(),
            "Generic error: Contract requires at least 0.2 NTRN in balance"
        );

        let deps = mock_neutron_dependencies(&[(
            env.contract.address.as_str(),
            &[coin(200_000, FEE_DENOM)],
        )]);
        let err = ibc_transfer_for_burning(
            deps.as_ref(),
            mock_env(),
            info.clone(),
            config.clone(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "Generic error: No tokens to transfer");

        let deps = mock_neutron_dependencies(&[(
            env.contract.address.as_str(),
            &[coin(100, "ibc/old_astro"), coin(200_000, FEE_DENOM)],
        )]);
        let res = ibc_transfer_for_burning(deps.as_ref(), env.clone(), info, config.clone(), None)
            .unwrap();

        assert_eq!(
            res.messages,
            [SubMsg::new(CosmosMsg::Custom(NeutronMsg::IbcTransfer {
                source_port: "transfer".to_string(),
                source_channel: outpost_params.old_astro_transfer_channel.to_string(),
                sender: env.contract.address.to_string(),
                receiver: outpost_params.terra_burn_addr.to_string(),
                token: coin(100, "ibc/old_astro"),
                timeout_height: RequestPacketTimeoutHeight {
                    revision_number: None,
                    revision_height: None,
                },
                timeout_timestamp: env.block.time.plus_seconds(DEFAULT_TIMEOUT).nanos(),
                memo: "".to_string(),
                fee: IbcFee {
                    recv_fee: vec![],
                    ack_fee: coins(100_000, FEE_DENOM),
                    timeout_fee: coins(100_000, FEE_DENOM),
                },
            }))]
        );
    }
}
