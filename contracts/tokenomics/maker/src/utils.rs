use crate::error::ContractError;
use crate::state::BRIDGES;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::maker::{
    Config, ExecuteMsg, SecondReceiverConfig, SecondReceiverParams, MAX_SECOND_RECEIVER_CUT,
};
use astroport::pair::Cw20HookMsg;
use astroport::querier::query_pair_info;

use cosmwasm_std::{
    coins, to_binary, wasm_execute, Addr, Binary, CosmosMsg, Decimal, Deps, Env, QuerierWrapper,
    StdError, StdResult, SubMsg, Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;

/// The default bridge depth for a fee token
pub const BRIDGES_INITIAL_DEPTH: u64 = 0;
/// Maximum amount of bridges to use in a multi-hop swap
pub const BRIDGES_MAX_DEPTH: u64 = 2;
/// Swap execution depth limit
pub const BRIDGES_EXECUTION_MAX_DEPTH: u64 = 3;

/// The function checks from<>to pool exists and creates swap message.
///
/// * **from** asset we want to swap.
///
/// * **to** asset we want to swap to.
///
/// * **amount_in** amount of tokens to swap.
pub fn try_build_swap_msg(
    querier: &QuerierWrapper,
    cfg: &Config,
    from: &AssetInfo,
    to: &AssetInfo,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    let pool = get_pool(querier, &cfg.factory_contract, from, to)?;
    let msg = build_swap_msg(cfg.max_spread, &pool, from, Some(to), amount_in)?;
    Ok(msg)
}

/// This function creates swap message.
///
/// * **max_spread** max allowed spread.
///
/// * **pool** pool's information.
///
/// * **from**  asset we want to swap.
///
/// * **to** asset we want to swap to.
///
/// * **amount_in** amount of tokens to swap.
pub fn build_swap_msg(
    max_spread: Decimal,
    pool: &PairInfo,
    from: &AssetInfo,
    to: Option<&AssetInfo>,
    amount_in: Uint128,
) -> Result<SubMsg, ContractError> {
    if from.is_native_token() {
        let offer_asset = Asset {
            info: from.clone(),
            amount: amount_in,
        };

        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: pool.contract_addr.to_string(),
            msg: to_binary(&astroport::pair::ExecuteMsg::Swap {
                offer_asset: offer_asset.clone(),
                ask_asset_info: to.cloned(),
                belief_price: None,
                max_spread: Some(max_spread),
                to: None,
            })?,
            funds: vec![offer_asset.as_coin()?],
        }))
    } else {
        Ok(SubMsg::new(WasmMsg::Execute {
            contract_addr: from.to_string(),
            msg: to_binary(&cw20::Cw20ExecuteMsg::Send {
                contract: pool.contract_addr.to_string(),
                amount: amount_in,
                msg: to_binary(&Cw20HookMsg::Swap {
                    ask_asset_info: to.cloned(),
                    belief_price: None,
                    max_spread: Some(max_spread),
                    to: None,
                })?,
            })?,
            funds: vec![],
        }))
    }
}

/// This function builds distribute messages. It swap all assets through bridges if needed.
///
/// * **bridge_assets** array with assets we want to swap and then to distribute.
///
/// * **depth** current depth of the swap. It is intended to prevent dead loops in recursive calls.
pub fn build_distribute_msg(
    env: Env,
    bridge_assets: Vec<AssetInfo>,
    depth: u64,
) -> StdResult<SubMsg> {
    let msg = if !bridge_assets.is_empty() {
        // Swap bridge assets
        SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::SwapBridgeAssets {
                assets: bridge_assets,
                depth,
            })?,
            funds: vec![],
        })
    } else {
        // Update balances and distribute rewards
        SubMsg::new(WasmMsg::Execute {
            contract_addr: env.contract.address.to_string(),
            msg: to_binary(&ExecuteMsg::DistributeAstro {})?,
            funds: vec![],
        })
    };

    Ok(msg)
}

/// This function checks that there is a direct pool to swap to $ASTRO.
/// Otherwise it looks for an intermediate token to swap to $ASTRO.
///
/// * **factory_contract** address of the factory contract.
///
/// * **from_token** asset we want to swap.
///
/// * **bridge_token** asset we want to swap through.
///
/// * **astro_token** represents $ASTRO.
///
/// * **depth** current recursion depth of the validation.
///
/// * **amount** is an amount of from_token.
pub fn validate_bridge(
    deps: Deps,
    factory_contract: &Addr,
    from_token: &AssetInfo,
    bridge_token: &AssetInfo,
    astro_token: &AssetInfo,
    depth: u64,
) -> Result<PairInfo, ContractError> {
    // Check if the bridge pool exists
    let bridge_pool = get_pool(&deps.querier, factory_contract, from_token, bridge_token)?;

    // Check if the bridge token - ASTRO pool exists
    let astro_pool = get_pool(&deps.querier, factory_contract, bridge_token, astro_token);
    if astro_pool.is_err() {
        if depth >= BRIDGES_MAX_DEPTH {
            return Err(ContractError::MaxBridgeDepth(depth));
        }

        // Check if next level of bridge exists
        let next_bridge_token = BRIDGES
            .load(deps.storage, bridge_token.to_string())
            .map_err(|_| ContractError::InvalidBridgeDestination(from_token.to_string()))?;

        validate_bridge(
            deps,
            factory_contract,
            bridge_token,
            &next_bridge_token,
            astro_token,
            depth + 1,
        )?;
    }

    Ok(bridge_pool)
}

/// This function checks that there is a pool to swap between `from` and `to`. In case of success
/// returns [`PairInfo`] of selected pool.
///
/// * **factory_contract** address of the factory contract.
///
/// * **from** source asset.
///
/// * **to** destination asset.
pub fn get_pool(
    querier: &QuerierWrapper,
    factory_contract: &Addr,
    from: &AssetInfo,
    to: &AssetInfo,
) -> Result<PairInfo, ContractError> {
    query_pair_info(
        querier,
        factory_contract.clone(),
        &[from.clone(), to.clone()],
    )
    .map_err(|_| ContractError::InvalidBridgeNoPool(from.to_string(), to.to_string()))
}

/// For native tokens of type [`AssetInfo`] uses method [`astro_satellite_package::ExecuteMsg::TransferAstro`]
/// to send a token amount to a recipient.
///
/// For a token of type [`AssetInfo`] we use the default method [`Cw20ExecuteMsg::Send`]
pub fn build_send_msg(
    asset: &Asset,
    recipient: impl Into<String>,
    msg: Option<Binary>,
) -> StdResult<CosmosMsg> {
    let recipient = recipient.into();

    match &asset.info {
        AssetInfo::Token { contract_addr } => Ok(CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: contract_addr.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Send {
                contract: recipient,
                amount: asset.amount,
                msg: msg.unwrap_or_default(),
            })?,
            funds: vec![],
        })),
        AssetInfo::NativeToken { denom } => Ok(CosmosMsg::Wasm(wasm_execute(
            recipient,
            &astro_satellite_package::ExecuteMsg::TransferAstro {},
            coins(asset.amount.u128(), denom),
        )?)),
    }
}

/// Updates the parameters that describe the second receiver of fees
pub fn update_second_receiver_cfg(
    deps: Deps,
    cfg: &mut Config,
    params: &Option<SecondReceiverParams>,
) -> StdResult<()> {
    if let Some(params) = params {
        if params.second_receiver_cut > MAX_SECOND_RECEIVER_CUT
            || params.second_receiver_cut.is_zero()
        {
            return Err(StdError::generic_err(format!(
                "Incorrect second receiver percent of its share. Should be in range: 0 < {} <= {}",
                params.second_receiver_cut, MAX_SECOND_RECEIVER_CUT
            )));
        };

        cfg.second_receiver_cfg = Some(SecondReceiverConfig {
            second_fee_receiver: deps
                .api
                .addr_validate(params.second_fee_receiver.as_str())?,
            second_receiver_cut: params.second_receiver_cut,
        });
    }

    Ok(())
}
