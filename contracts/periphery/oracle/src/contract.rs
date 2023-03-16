use crate::error::ContractError;
use crate::migration::PRICE_LAST_V100;
use crate::querier::{query_cumulative_prices, query_prices};
use crate::state::{
    get_precision, store_precisions, Config, PriceCumulativeLast, CONFIG, PRICE_LAST,
};
use astroport::asset::{Asset, AssetInfo};
use astroport::oracle::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use astroport::pair::TWAP_PRECISION;
use astroport::querier::query_pair_info;

use cosmwasm_std::{
    entry_point, to_binary, Binary, Decimal256, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult, Uint128, Uint256,
};
use cw2::{get_contract_version, set_contract_version};

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-oracle";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Time between two consecutive TWAP updates.
pub const PERIOD: u64 = 86400;

/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    let factory_contract = deps.api.addr_validate(&msg.factory_contract)?;

    for asset_info in &msg.asset_infos {
        asset_info.check(deps.api)?;
        store_precisions(deps.branch(), asset_info, &factory_contract)?;
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let pair_info = query_pair_info(&deps.querier, &factory_contract, &msg.asset_infos)?;

    let config = Config {
        owner: info.sender,
        factory: factory_contract,
        asset_infos: msg.asset_infos,
        pair: pair_info.clone(),
    };
    CONFIG.save(deps.storage, &config)?;

    let prices = query_cumulative_prices(deps.querier, pair_info.contract_addr)?;
    let average_prices = prices
        .cumulative_prices
        .iter()
        .cloned()
        .map(|(from, to, _)| (from, to, Decimal256::zero()))
        .collect();

    let price = PriceCumulativeLast {
        cumulative_prices: prices.cumulative_prices,
        average_prices,
        block_timestamp_last: env.block.time.seconds(),
    };
    PRICE_LAST.save(deps.storage, &price)?;

    Ok(Response::default())
}

/// Exposes all the execute functions available in the contract.
///
/// ## Variants
/// * **ExecuteMsg::Update {}** Updates the local TWAP values for the assets in the Astroport pool.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Update {} => update(deps, env),
    }
}

/// Updates the local TWAP values for the tokens in the target Astroport pool.
pub fn update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let prices = query_cumulative_prices(deps.querier, config.pair.contract_addr)?;
    let time_elapsed = env.block.time.seconds() - price_last.block_timestamp_last;

    // Ensure that at least one full period has passed since the last update
    if time_elapsed < PERIOD {
        return Err(ContractError::WrongPeriod {});
    }

    let mut average_prices = vec![];
    for (asset1_last, asset2_last, price_last) in price_last.cumulative_prices.iter() {
        for (asset1, asset2, price) in prices.cumulative_prices.iter() {
            if asset1.equal(asset1_last) && asset2.equal(asset2_last) {
                average_prices.push((
                    asset1.clone(),
                    asset2.clone(),
                    Decimal256::from_ratio(
                        Uint256::from(price.wrapping_sub(*price_last)),
                        time_elapsed,
                    ),
                ));
            }
        }
    }

    let prices = PriceCumulativeLast {
        cumulative_prices: prices.cumulative_prices,
        average_prices,
        block_timestamp_last: env.block.time.seconds(),
    };
    PRICE_LAST.save(deps.storage, &prices)?;
    Ok(Response::default())
}

/// Exposes all the queries available in the contract.
///
/// ## Queries
/// * **QueryMsg::Consult { token, amount }** Validates assets and calculates a new average
/// amount with updated precision
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Consult { token, amount } => to_binary(&consult(deps, token, amount)?),
    }
}

/// Multiplies a token amount by its latest TWAP value.
/// * **token** token for which we multiply its TWAP value by an amount.
///
/// * **amount** amount of tokens we multiply the TWAP by.
fn consult(
    deps: Deps,
    token: AssetInfo,
    amount: Uint128,
) -> Result<Vec<(AssetInfo, Uint256)>, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let mut average_prices = vec![];
    for (from, to, value) in price_last.average_prices {
        if from.equal(&token) {
            average_prices.push((to, value));
        }
    }

    if average_prices.is_empty() {
        return Err(StdError::generic_err("Invalid Token"));
    }

    // Get the token's precision
    let p = get_precision(deps.storage, &token)?;
    let one = Uint128::new(10_u128.pow(p.into()));

    average_prices
        .iter()
        .map(|(asset, price_average)| {
            if price_average.is_zero() {
                let price = query_prices(
                    deps.querier,
                    config.pair.contract_addr.clone(),
                    Asset {
                        info: token.clone(),
                        amount: one,
                    },
                    Some(asset.clone()),
                )?
                .return_amount;
                Ok((
                    asset.clone(),
                    Uint256::from(price).multiply_ratio(Uint256::from(amount), Uint256::from(one)),
                ))
            } else {
                let price_precision = Uint256::from(10_u128.pow(TWAP_PRECISION.into()));
                Ok((
                    asset.clone(),
                    Uint256::from(amount) * *price_average / price_precision,
                ))
            }
        })
        .collect::<Result<Vec<(AssetInfo, Uint256)>, StdError>>()
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(mut deps: DepsMut, _env: Env, _msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-oracle" => match contract_version.version.as_ref() {
            "1.0.0" | "1.0.1" | "1.0.2" => {
                let config = CONFIG.load(deps.storage)?;
                let price_last_v100 = PRICE_LAST_V100.load(deps.storage)?;

                let cumulative_prices = vec![
                    (
                        config.asset_infos[0].clone(),
                        config.asset_infos[1].clone(),
                        price_last_v100.price0_cumulative_last,
                    ),
                    (
                        config.asset_infos[1].clone(),
                        config.asset_infos[0].clone(),
                        price_last_v100.price1_cumulative_last,
                    ),
                ];
                let average_prices = vec![
                    (
                        config.asset_infos[0].clone(),
                        config.asset_infos[1].clone(),
                        price_last_v100.price_0_average,
                    ),
                    (
                        config.asset_infos[1].clone(),
                        config.asset_infos[0].clone(),
                        price_last_v100.price_1_average,
                    ),
                ];

                PRICE_LAST.save(
                    deps.storage,
                    &PriceCumulativeLast {
                        cumulative_prices,
                        average_prices,
                        block_timestamp_last: price_last_v100.block_timestamp_last,
                    },
                )?;
                for asset_info in &config.asset_infos {
                    store_precisions(deps.branch(), asset_info, &config.factory)?;
                }
            }
            _ => return Err(ContractError::MigrationError {}),
        },
        _ => return Err(ContractError::MigrationError {}),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::new()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
