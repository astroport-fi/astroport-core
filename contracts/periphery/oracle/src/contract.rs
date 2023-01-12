use crate::error::ContractError;
use crate::querier::{query_cumulative_prices, query_pair_info, query_prices};
use crate::state::{Config, PriceCumulativeLast, CONFIG, PRICE_LAST};
use astroport::asset::{Asset, AssetInfo};
use astroport::oracle::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use astroport::pair::TWAP_PRECISION;
use astroport::querier::query_token_precision;

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

/// ## Description
/// Creates a new contract with the specified parameters in the [`InstantiateMsg`].
/// Returns a [`Response`] with the specified attributes if the operation was successful,
/// or a [`ContractError`] if the contract was not created.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
///
/// * **info** is an object of type [`MessageInfo`].
/// * **msg** is a message of type [`InstantiateMsg`] which contains the basic settings for creating the contract.
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

    let factory_contract = deps.api.addr_validate(msg.factory_contract.as_ref())?;
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
/// Exposes all the execute functions available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **env** is an object of type [`Env`].
///
/// * **_info** is an object of type [`MessageInfo`].
///
/// * **msg** is an object of type [`ExecuteMsg`].
///
/// ## Queries
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

/// ## Description
/// Updates the local TWAP values for the tokens in the target Astroport pool.
/// Returns a default object of type [`Response`] if the operation was successful,
/// otherwise returns a [`ContractError`].
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **env** is an object of type [`Env`].
pub fn update(deps: DepsMut, env: Env) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let price_last = PRICE_LAST.load(deps.storage)?;

    let prices = query_cumulative_prices(&deps.querier, config.pair.contract_addr)?;
    let time_elapsed = env.block.time.seconds() - price_last.block_timestamp_last;

    // Ensure that at least one full period has passed since the last update
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
/// Exposes all the queries available in the contract.
/// ## Params
/// * **deps** is an object of type [`Deps`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **msg** is an object of type [`QueryMsg`].
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
/// Multiplies a token amount by its latest TWAP value and returns the result as a [`Uint256`] if the operation was successful
/// or returns [`StdError`] on failure.
/// ## Params
/// * **deps** is an object of type [`DepsMut`].
///
/// * **token** is an object of type [`AssetInfo`]. This is the token for which we multiply its TWAP value by an amount.
///
/// * **amount** is an object of type [`Uint128`]. This is the amount of tokens we multiply the TWAP by.
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
        // Get the token's precision
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
        Uint256::from(amount) * price_average / price_precision
    })
}

/// ## Description
/// Used for contract migration. Returns the default object of type [`Response`].
/// ## Params
/// * **_deps** is an object of type [`DepsMut`].
///
/// * **_env** is an object of type [`Env`].
///
/// * **_msg** is an object of type [`MigrateMsg`].
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    let contract_version = get_contract_version(deps.storage)?;

    match contract_version.contract.as_ref() {
        "astroport-oracle" => match contract_version.version.as_ref() {
            "1.0.0" => {}
            _ => return Err(StdError::generic_err("Invalid contract version")),
        },
        _ => return Err(StdError::generic_err("Invalid contract name")),
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default()
        .add_attribute("previous_contract_name", &contract_version.contract)
        .add_attribute("previous_contract_version", &contract_version.version)
        .add_attribute("new_contract_name", CONTRACT_NAME)
        .add_attribute("new_contract_version", CONTRACT_VERSION))
}
