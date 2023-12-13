use crate::ContractError;
use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use astroport::pair::{Cw20HookMsg as PairCw20HookMsg, QueryMsg as PairQueryMsg};

use astroport::factory::PairType;
use astroport::generator::QueryMsg as GeneratorQueryMsg;
use astroport::querier::{query_balance, query_pair_info, query_token_balance};
use astroport::shared_multisig::{Config, PoolType, ProvideParams};
use cosmwasm_std::{
    attr, to_json_binary, Addr, Attribute, CosmosMsg, Decimal, QuerierWrapper, StdError, StdResult,
    Uint128, WasmMsg,
};
use cw20::Cw20ExecuteMsg;
use itertools::Itertools;

pub(crate) fn prepare_provide_after_withdraw_msg(
    querier: &QuerierWrapper,
    cfg: &Config,
    burn_amount: Uint128,
    burn_pool: &Addr,
    provide_params: ProvideParams,
    attributes: &mut Vec<Attribute>,
) -> Result<CosmosMsg, ContractError> {
    // we should check if migration pool exists and than provide
    let (migration_pool, _) = get_pool_info(querier, cfg, PoolType::Migration)?;

    let assets: Vec<Asset> = querier.query_wasm_smart(
        burn_pool,
        &PairQueryMsg::Share {
            amount: burn_amount,
        },
    )?;

    attributes.push(attr("second_action", "provide"));
    attributes.push(attr("provide_pool", migration_pool.to_string().as_str()));
    attributes.push(attr("provide_assets", assets.iter().join(", ")));

    Ok(prepare_provide_msg(
        &migration_pool,
        assets,
        provide_params.slippage_tolerance,
        provide_params.auto_stake,
    )?)
}

pub(crate) fn prepare_withdraw_msg(
    querier: &QuerierWrapper,
    account_addr: &Addr,
    pair: &Addr,
    lp_token: &Addr,
    amount: Option<Uint128>,
) -> Result<(CosmosMsg, Uint128), ContractError> {
    let total_amount = query_token_balance(querier, lp_token, account_addr)?;
    let burn_amount = amount.unwrap_or(total_amount);
    if burn_amount > total_amount {
        return Err(ContractError::BalanceToSmall(
            account_addr.to_string(),
            total_amount.to_string(),
        ));
    }

    Ok((
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: lp_token.to_string(),
            msg: to_json_binary(&Cw20ExecuteMsg::Send {
                contract: pair.to_string(),
                msg: to_json_binary(&PairCw20HookMsg::WithdrawLiquidity { assets: vec![] })?,
                amount: burn_amount,
            })?,
            funds: vec![],
        }),
        burn_amount,
    ))
}

pub(crate) fn prepare_provide_msg(
    contract_addr: &Addr,
    assets: Vec<Asset>,
    slippage_tolerance: Option<Decimal>,
    auto_stake: Option<bool>,
) -> StdResult<CosmosMsg> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: contract_addr.to_string(),
        funds: assets
            .iter()
            .map(|asset| asset.as_coin())
            .collect::<StdResult<_>>()?,
        msg: to_json_binary(&PairExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance,
            auto_stake,
            receiver: None,
        })?,
    }))
}

pub(crate) fn check_provide_assets(
    querier: &QuerierWrapper,
    account: &Addr,
    assets: &[Asset],
    cfg: &Config,
) -> Result<(), ContractError> {
    for asset in assets {
        let denom = check_denom(&asset.info, cfg)?;

        let balance = query_balance(querier, account, denom)?;
        if asset.amount > balance {
            return Err(ContractError::AssetBalanceMismatch(
                asset.info.to_string(),
                balance.to_string(),
            ));
        }
    }

    Ok(())
}

pub(crate) fn check_denom(asset_info: &AssetInfo, cfg: &Config) -> Result<String, ContractError> {
    let denom = match &asset_info {
        AssetInfo::NativeToken { denom } => &**denom,
        AssetInfo::Token { .. } => return Err(ContractError::UnsupportedCw20 {}),
    };

    if cfg.denom1 != denom && cfg.denom2 != denom {
        return Err(ContractError::InvalidAsset(denom.to_string()));
    }

    Ok(denom.to_string())
}

pub(crate) fn check_pool(
    querier: &QuerierWrapper,
    contract_addr: &Addr,
    cfg: &Config,
) -> Result<(), ContractError> {
    // check pair assets
    let pair: PairInfo = querier.query_wasm_smart(contract_addr, &PairQueryMsg::Pair {})?;
    for asset_info in &pair.asset_infos {
        check_denom(asset_info, cfg)?;
    }

    // check if pair is registered in the factory
    let pair_info: PairInfo = query_pair_info(querier, &cfg.factory_addr, &pair.asset_infos)
        .map_err(|_| {
            ContractError::Std(StdError::generic_err(format!(
                "The pair is not registered: {}-{}",
                cfg.denom1, cfg.denom2
            )))
        })?;

    // check if pool type is either xyk or PCL
    if !pair_info.pair_type.eq(&PairType::Xyk {})
        && !pair_info
            .pair_type
            .eq(&PairType::Custom("concentrated".to_string()))
    {
        return Err(ContractError::PairTypeError {});
    }

    Ok(())
}

pub(crate) fn get_pool_info(
    querier: &QuerierWrapper,
    cfg: &Config,
    pool_type: PoolType,
) -> Result<(Addr, Addr), ContractError> {
    match pool_type {
        PoolType::Target => match &cfg.target_pool {
            Some(target_pool) => {
                let pair_info: PairInfo =
                    querier.query_wasm_smart(target_pool, &PairQueryMsg::Pair {})?;
                Ok((target_pool.clone(), pair_info.liquidity_token))
            }
            None => Err(ContractError::TargetPoolError {}),
        },
        PoolType::Migration => match &cfg.migration_pool {
            Some(migration_pool) => {
                let pair_info: PairInfo =
                    querier.query_wasm_smart(migration_pool, &PairQueryMsg::Pair {})?;
                Ok((migration_pool.clone(), pair_info.liquidity_token))
            }
            None => Err(ContractError::MigrationPoolError {}),
        },
    }
}

pub(crate) fn check_generator_deposit(
    querier: &QuerierWrapper,
    generator_addr: &Addr,
    lp_token: &Addr,
    user: &Addr,
) -> Result<(), ContractError> {
    let generator_total_amount: Uint128 = querier.query_wasm_smart(
        generator_addr,
        &GeneratorQueryMsg::Deposit {
            lp_token: lp_token.to_string(),
            user: user.to_string(),
        },
    )?;

    if !generator_total_amount.is_zero() {
        return Err(ContractError::GeneratorAmountError {});
    }

    Ok(())
}
