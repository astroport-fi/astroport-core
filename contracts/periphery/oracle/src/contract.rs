use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::querier::{query_cumulative_prices, query_pair_info, query_prices};
use crate::state::{Config, PriceCumulativeLast, CONFIG, PRICE_LAST};
use astroport::asset::{Asset, AssetInfo};
use astroport::factory::PairType;
use astroport::querier::query_token_precision;
use cosmwasm_bignumber::{Decimal256, Uint256};
use cosmwasm_std::{
    entry_point, to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
    Uint128,
};
use cw2::set_contract_version;

// version info for migration info
const CONTRACT_NAME: &str = "astroport-oracle";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

const PERIOD: u64 = 86400;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    let factory_contract = deps.api.addr_validate(msg.factory_contract.as_ref())?;
    let pair_info = query_pair_info(
        &deps.querier,
        factory_contract.clone(),
        msg.asset_infos.clone(),
    )?;

    if pair_info.pair_type != (PairType::Xyk {}) {
        return Err(ContractError::InvalidToken {});
    }

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

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Consult { token, amount } => to_binary(&consult(deps, token, amount)?),
    }
}

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

        Uint256::from(price) * Decimal256::from_ratio(Uint256::from(amount), Uint256::from(one))
    } else {
        Uint256::from(amount) * price_average
    })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
    Ok(Response::default())
}
