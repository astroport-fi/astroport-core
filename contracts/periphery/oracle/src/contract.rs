use crate::error::ContractError;
use crate::querier::{query_cumulative_prices, query_pair_info, query_prices};
use crate::state::{Config, PriceCumulativeLast, CONFIG, PRICE_LAST};
use astroport::asset::{addr_validate_to_lower, Asset, AssetInfo};
use astroport::oracle::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use astroport::pair::TWAP_PRECISION;
use astroport::querier::query_token_precision;
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128,
};
use cw2::set_contract_version;

/// Contract name that is used for migration.
const CONTRACT_NAME: &str = "astroport-oracle";
/// Contract version that is used for migration.
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Update time interval that is used for update method
const PERIOD: u64 = 86400;

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns the [`Response`] with the specified attributes if the operation was successful, or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
///
/// * **info** is the object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating a contract
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    msg.asset_infos[0].check(deps.api)?;
    msg.asset_infos[1].check(deps.api)?;

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_contract = addr_validate_to_lower(deps.api, msg.factory_contract.as_ref())?;
    let pair_info = query_pair_info(
        &deps.querier,
        factory_contract.clone(),
        msg.asset_infos.clone(),
    )?;

    let config = Config {
        owner: info.sender,
        factory: factory_contract,
        asset_infos: msg.asset_infos,
        pair: pair_info.clone(),
    };
    CONFIG.save(deps.storage, &config)?;
    let prices = query_cumulative_prices(&deps.querier, pair_info.contract_addr)?;

    let price = PriceCumulativeLast {
        price0_cumulative_last: prices.price0_cumulative_last,
        price1_cumulative_last: prices.price1_cumulative_last,
        price_0_average: Decimal256::zero(),
        price_1_average: Decimal256::zero(),
        block_timestamp_last: env.block.time.seconds(),
    };
    PRICE_LAST.save(deps.storage, &price)?;
    Ok(Response::default())
}

/// ## Description
/// Available the execute messages of the contract.
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **env** is the object of type [`Env`].
///
/// * **_info** is the object of type [`MessageInfo`].
///
/// * **msg** is the object of type [`ExecuteMsg`].
///
/// ## Queries
/// * **ExecuteMsg::Update {}** Updates prices for the specified time interval that sets in the
/// [`PERIOD`] constant.
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

/// ## Description
/// Updates prices for the specified time interval that sets in the **Period** variable.
/// Returns the default object of type [`Response`] if the operation was successful,
/// otherwise returns the [`ContractError`].
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **env** is the object of type [`Env`].
pub fn update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let prices = query_cumulative_prices(&deps.querier, config.pair.contract_addr)?;
    let time_elapsed = env.block.time.seconds() - price_last.block_timestamp_last;

    // ensure that at least one full period has passed since the last update
    if time_elapsed < PERIOD {
        return Err(ContractError::WrongPeriod {});
    }

    let price_0_average = Decimal256::from_ratio(
        Uint256::from(
            prices
                .price0_cumulative_last
                .wrapping_sub(price_last.price0_cumulative_last),
        ),
        time_elapsed,
    );

    let price_1_average = Decimal256::from_ratio(
        Uint256::from(
            prices
                .price1_cumulative_last
                .wrapping_sub(price_last.price1_cumulative_last),
        ),
        time_elapsed,
    );

    let prices = PriceCumulativeLast {
        price0_cumulative_last: prices.price0_cumulative_last,
        price1_cumulative_last: prices.price1_cumulative_last,
        price_0_average,
        price_1_average,
        block_timestamp_last: env.block.time.seconds(),
    };
    PRICE_LAST.save(deps.storage, &prices)?;
    Ok(Response::default())
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
/// * **QueryMsg::Consult { token, amount }** Validates assets and calculates a new average
/// amount with updated precision
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Consult { token, amount } => to_binary(&consult(deps, token, amount)?),
    }
}

/// ## Description
/// Validates assets and calculates a new average amount with updated precision.
/// Returns the average amount of type [`Uint256`] if the operation was successful,
/// or returns [`StdError`] on failure.
/// ## Params
/// * **deps** is the object of type [`DepsMut`].
///
/// * **token** is the object of type [`AssetInfo`].
///
/// * **amount** is the object of type [`Uint128`].
fn consult(deps: Deps, token: AssetInfo, amount: Uint128) -> Result<Uint256, StdError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let price_average = if config.asset_infos[0].equal(&token) {
        price_last.price_0_average
    } else if config.asset_infos[1].equal(&token) {
        price_last.price_1_average
    } else {
        return Err(StdError::generic_err("Invalid Token"));
    };

    Ok(if price_average.is_zero() {
        // get precision
        let p = query_token_precision(&deps.querier, token.clone())?;
        let one = Uint128::new(10_u128.pow(p.into()));

        let price = query_prices(
            &deps.querier,
            config.pair.contract_addr,
            Asset {
                info: token,
                amount: one,
            },
        )
        .unwrap()
        .return_amount;

        Uint256::from(price).multiply_ratio(Uint256::from(amount), Uint256::from(one))
    } else {
        let price_precision = Uint256::from(10_u128.pow(TWAP_PRECISION.into()));
        Uint256::from(amount) * price_average / Decimal256::from_uint256(price_precision)
    })
}

/// ## Description
/// Used for migration of contract. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is the object of type [`DepsMut`].
///
/// * **_env** is the object of type [`Env`].
///
/// * **_msg** is the object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
