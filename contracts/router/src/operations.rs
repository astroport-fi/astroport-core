use cosmwasm_std::{
    coins, to_json_binary, wasm_execute, Decimal, DepsMut, Env, MessageInfo, Response, StdResult,
    WasmMsg,
};
use cw20::Cw20ExecuteMsg;

use astroport::asset::{Asset, AssetInfo};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use astroport::router::SwapOperation;

use crate::error::ContractError;

/// Execute a swap operation.
///
/// * **operation** to perform (native or Astro swap with offer and ask asset information).
///
/// * **to** address that receives the ask assets.
///
/// * **single** defines whether this swap is single or part of a multi hop route.
pub fn execute_swap_operation(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    operation: SwapOperation,
    to: Option<String>,
    max_spread: Option<Decimal>,
    single: bool,
) -> Result<Response, ContractError> {
    if env.contract.address != info.sender {
        return Err(ContractError::Unauthorized {});
    }

    let offer_asset = Asset {
        amount: operation
            .offer_asset_info
            .query_pool(&deps.querier, &env.contract.address)?,
        info: operation.offer_asset_info,
    };

    let message = asset_into_swap_msg(
        operation.pair_address,
        offer_asset,
        operation.ask_asset_info,
        max_spread,
        to,
        single,
    )?;

    Ok(Response::new().add_message(message))
}

/// Creates a message of type [`CosmosMsg`] representing a swap operation.
///
/// * **pair_contract** Astroport pair contract for which the swap operation is performed.
///
/// * **offer_asset** asset that is swapped. It also mentions the amount to swap.
///
/// * **ask_asset_info** asset that is swapped to.
///
/// * **max_spread** max spread enforced for the swap.
///
/// * **to** address that receives the ask assets.
///
/// * **single** defines whether this swap is single or part of a multi hop route.
pub fn asset_into_swap_msg(
    pair_contract: String,
    offer_asset: Asset,
    ask_asset_info: AssetInfo,
    max_spread: Option<Decimal>,
    to: Option<String>,
    single: bool,
) -> StdResult<WasmMsg> {
    // Disabling spread assertion if this swap is part of a multi hop route
    let belief_price = if single { None } else { Some(Decimal::MAX) };

    match &offer_asset.info {
        AssetInfo::NativeToken { denom } => wasm_execute(
            pair_contract,
            &PairExecuteMsg::Swap {
                offer_asset: offer_asset.clone(),
                ask_asset_info: Some(ask_asset_info),
                belief_price,
                max_spread,
                to,
            },
            coins(offer_asset.amount.u128(), denom),
        ),
        AssetInfo::Token { contract_addr } => wasm_execute(
            contract_addr.to_string(),
            &Cw20ExecuteMsg::Send {
                contract: pair_contract,
                amount: offer_asset.amount,
                msg: to_json_binary(&astroport::pair::Cw20HookMsg::Swap {
                    ask_asset_info: Some(ask_asset_info),
                    belief_price,
                    max_spread,
                    to,
                })?,
            },
            vec![],
        ),
    }
}
