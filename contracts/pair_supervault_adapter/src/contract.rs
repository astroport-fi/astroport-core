use crate::error::ContractError;
use crate::external::{duality_swap, simulate_duality_swap, SVExecuteMsg, SvQuerier};
use crate::reply::ReplyIds;
use crate::state::{
    Config, ProvideTmpData, WithdrawTmpData, CONFIG, PROVIDE_TMP_DATA, WITHDRAW_TMP_DATA,
};
use astroport::asset::{addr_opt_validate, Asset, AssetInfo, CoinsExt, PairInfo};
use astroport::common::LP_SUBDENOM;
use astroport::factory::PairType;
use astroport::pair;
use astroport::pair::ExecuteMsg;
use astroport::token_factory::{tf_burn_msg, tf_create_denom_msg};
use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, coin, ensure, ensure_eq, from_json, wasm_execute, Addr, Decimal, DepsMut, Env,
    MessageInfo, Response, SubMsg, Uint128,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, one_coin};

/// Contract name that is used for migration.
pub const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
/// Contract version that is used for migration.
pub const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cw_serde]
pub struct SuperVaultAdapterParams {
    /// The address of the SuperVault contract.
    pub vault_addr: String,
}

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: pair::InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    ensure_eq!(
        msg.asset_infos.len(),
        2,
        ContractError::InvalidNumberOfAssets {}
    );

    let init_params: SuperVaultAdapterParams = from_json(
        &msg.init_params
            .ok_or(ContractError::InitParamsNotFound {})?,
    )?;

    let vault_addr = Addr::unchecked(init_params.vault_addr);

    let sv_config = SvQuerier::new(vault_addr.clone()).query_config(deps.querier)?;

    // Ensure denoms in the init msg match the SuperVault config
    msg.asset_infos.iter().try_for_each(|info| match info {
        AssetInfo::Token { .. } => Err(ContractError::NonNativeAsset {}),
        AssetInfo::NativeToken { denom } => {
            if denom != &sv_config.pair_data.token_0.denom
                && denom != &sv_config.pair_data.token_1.denom
            {
                Err(ContractError::InvalidDenom(denom.clone()))
            } else {
                Ok(())
            }
        }
    })?;

    let pair_info = PairInfo {
        contract_addr: env.contract.address.clone(),
        liquidity_token: "".to_string(),
        asset_infos: msg.asset_infos.clone(),
        pair_type: PairType::Custom("sv_adapter".to_string()),
    };

    CONFIG.save(
        deps.storage,
        &Config {
            pair_info,
            factory_addr: deps.api.addr_validate(&msg.factory_addr)?,
            vault_addr,
            vault_lp_denom: sv_config.lp_denom,
            denoms: [
                sv_config.pair_data.token_0.denom.clone(),
                sv_config.pair_data.token_1.denom.clone(),
            ],
        },
    )?;

    // Create LP token
    let sub_msg = SubMsg::reply_on_success(
        tf_create_denom_msg(env.contract.address.to_string(), LP_SUBDENOM),
        ReplyIds::CreateDenom as u64,
    );

    Ok(Response::default().add_submessage(sub_msg))
}

/// Exposes all the execute functions available in the contract
#[cfg_attr(not(feature = "library"), cosmwasm_std::entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Swap {
            to, belief_price, ..
        } => swap(deps, info, env, belief_price, to),
        ExecuteMsg::ProvideLiquidity {
            assets,
            auto_stake,
            receiver,
            min_lp_to_receive,
            ..
        } => provide_liquidity(deps, info, assets, receiver, min_lp_to_receive, auto_stake),
        ExecuteMsg::WithdrawLiquidity {
            min_assets_to_receive,
            ..
        } => withdraw_liquidity(deps, env, info, min_assets_to_receive),
        _ => Err(ContractError::NotSupported {}),
    }
}

/// Performs swap operation with the specified parameters.
pub fn swap(
    deps: DepsMut,
    info: MessageInfo,
    env: Env,
    belief_price: Option<Decimal>,
    to_addr: Option<String>,
) -> Result<Response, ContractError> {
    let coin_in = one_coin(&info)?;

    let config = CONFIG.load(deps.storage)?;

    ensure!(
        coin_in.denom == config.denoms[0] || coin_in.denom == config.denoms[1],
        ContractError::InvalidDenom(coin_in.denom.clone())
    );

    let receiver = addr_opt_validate(deps.api, &to_addr)?.unwrap_or_else(|| info.sender.clone());
    let denom_out = if coin_in.denom == config.denoms[0] {
        config.denoms[1].clone()
    } else {
        config.denoms[0].clone()
    };

    let duality_swap_msg = duality_swap(&env, &receiver, &coin_in, &denom_out, belief_price)?;
    let return_amount = simulate_duality_swap(deps.querier, duality_swap_msg.clone())?;

    Ok(Response::new()
        .add_message(duality_swap_msg)
        .add_attributes([
            attr("action", "swap"),
            attr("sender", info.sender),
            attr("receiver", receiver),
            attr("offer_asset", AssetInfo::native(&denom_out).to_string()),
            attr("ask_asset", AssetInfo::native(&denom_out).to_string()),
            attr("offer_amount", coin_in.amount),
            attr("return_amount", return_amount),
            attr("spread_amount", "0"),
            attr("commission_amount", "0"),
            attr("maker_fee_amount", "0"),
        ]))
}

pub fn provide_liquidity(
    deps: DepsMut,
    info: MessageInfo,
    assets: Vec<Asset>,
    receiver: Option<String>,
    min_lp_to_receive: Option<Uint128>,
    auto_stake: Option<bool>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    info.funds
        .assert_coins_properly_sent(&assets, &config.pair_info.asset_infos)?;

    let receiver = addr_opt_validate(deps.api, &receiver)?.unwrap_or_else(|| info.sender.clone());
    let auto_stake = auto_stake.unwrap_or(false);

    PROVIDE_TMP_DATA.save(
        deps.storage,
        &ProvideTmpData {
            assets,
            receiver,
            auto_stake,
            min_lp_to_receive,
        },
    )?;

    let vault_msg = wasm_execute(&config.vault_addr, &SVExecuteMsg::Deposit {}, info.funds)?;
    let sub_msg = SubMsg::reply_on_success(vault_msg, ReplyIds::PostProvide as u64);

    Ok(Response::new().add_submessage(sub_msg))
}

/// Withdraw liquidity from the pool.
pub fn withdraw_liquidity(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    let lp_amount = must_pay(&info, &config.pair_info.liquidity_token)?;

    WITHDRAW_TMP_DATA.save(
        deps.storage,
        &WithdrawTmpData {
            lp_amount,
            receiver: info.sender.clone(),
            min_assets_to_receive,
        },
    )?;

    let burn_msg = SubMsg::new(tf_burn_msg(
        env.contract.address,
        coin(
            lp_amount.u128(),
            config.pair_info.liquidity_token.to_string(),
        ),
    ));

    let vault_msg = wasm_execute(
        &config.vault_addr,
        &SVExecuteMsg::Withdraw { amount: lp_amount },
        vec![coin(lp_amount.u128(), &config.vault_lp_denom)],
    )?;
    let sv_withdraw_msg = SubMsg::reply_on_success(vault_msg, ReplyIds::PostWithdraw as u64);

    Ok(Response::new().add_submessages([burn_msg, sv_withdraw_msg]))
}
