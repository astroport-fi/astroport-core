use cosmwasm_std::{
    ensure, from_json, Decimal256, DepsMut, Env, MessageInfo, Response, StdError, SubMsg, Uint128,
};
use cw2::set_contract_version;

use astroport::pair::ReplyIds;
use astroport::token_factory::tf_create_denom_msg;
use astroport::{
    asset::PairInfo, pair::InstantiateMsg, pair_concentrated::UpdatePoolParams,
    pair_concentrated_duality::ConcentratedDualityParams,
};
use astroport_pcl_common::{
    state::{AmpGamma, Config, PoolParams, PoolState, Precisions, PriceState},
    utils::check_asset_infos,
};

use crate::error::ContractError;
use crate::orderbook::state::OrderbookState;
use crate::state::CONFIG;

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Tokenfactory LP token subdenom
pub const LP_SUBDENOM: &str = "astroport/share";
/// An LP token's precision.
pub const LP_TOKEN_PRECISION: u8 = 6;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    if msg.asset_infos.len() != 2 {
        return Err(StdError::generic_err("asset_infos must contain exactly two elements").into());
    }

    check_asset_infos(deps.api, &msg.asset_infos)?;

    // Duality orderbook supports only native assets
    ensure!(
        msg.asset_infos.iter().all(|x| x.is_native_token()),
        ContractError::NonNativeAsset {}
    );

    let ConcentratedDualityParams {
        main_params: params,
        orderbook_config: ob_config,
    } = from_json(
        msg.init_params
            .ok_or(ContractError::InitParamsNotFound {})?,
    )?;

    if params.price_scale.is_zero() {
        return Err(StdError::generic_err("Initial price scale can not be zero").into());
    }

    if params.track_asset_balances.unwrap_or_default() {
        return Err(StdError::generic_err(
            "Balance tracking is not available with Duality integration",
        )
        .into());
    };

    let factory_addr = deps.api.addr_validate(&msg.factory_addr)?;

    Precisions::store_precisions(deps.branch(), &msg.asset_infos, &factory_addr)?;

    // Initializing cumulative prices
    let cumulative_prices = vec![
        (
            msg.asset_infos[0].clone(),
            msg.asset_infos[1].clone(),
            Uint128::zero(),
        ),
        (
            msg.asset_infos[1].clone(),
            msg.asset_infos[0].clone(),
            Uint128::zero(),
        ),
    ];

    let ob_state = OrderbookState::new(deps.api, ob_config)?;
    ob_state.save(deps.storage)?;

    let mut pool_params = PoolParams::default();
    pool_params.update_params(UpdatePoolParams {
        mid_fee: Some(params.mid_fee),
        out_fee: Some(params.out_fee),
        fee_gamma: Some(params.fee_gamma),
        repeg_profit_threshold: Some(params.repeg_profit_threshold),
        min_price_scale_delta: Some(params.min_price_scale_delta),
        ma_half_time: Some(params.ma_half_time),
        allowed_xcp_profit_drop: params.allowed_xcp_profit_drop,
        xcp_profit_losses_threshold: params.xcp_profit_losses_threshold,
    })?;

    let pool_state = PoolState {
        initial: AmpGamma::default(),
        future: AmpGamma::new(params.amp, params.gamma)?,
        future_time: env.block.time.seconds(),
        initial_time: 0,
        price_state: PriceState {
            oracle_price: params.price_scale.into(),
            last_price: params.price_scale.into(),
            price_scale: params.price_scale.into(),
            last_price_update: env.block.time.seconds(),
            xcp_profit: Decimal256::zero(),
            xcp_profit_real: Decimal256::zero(),
            xcp_profit_losses: Decimal256::zero(),
        },
    };

    let config = Config {
        pair_info: PairInfo {
            contract_addr: env.contract.address.clone(),
            liquidity_token: "".to_owned(),
            asset_infos: msg.asset_infos.clone(),
            pair_type: msg.pair_type,
        },
        factory_addr,
        block_time_last: env.block.time.seconds(),
        cumulative_prices,
        pool_params,
        pool_state,
        owner: None,
        track_asset_balances: false,
        fee_share: None,
        tracker_addr: None,
    };

    CONFIG.save(deps.storage, &config)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        tf_create_denom_msg(env.contract.address.to_string(), LP_SUBDENOM),
        ReplyIds::CreateDenom as u64,
    );

    Ok(Response::new().add_submessage(sub_msg))
}
