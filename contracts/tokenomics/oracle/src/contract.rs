use crate::error::ContractError;
use crate::msg::{ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::{
    Config, PriceConfig, PriceSourceChecked, PriceSourceUnchecked, CONFIG, PRICE_CONFIGS,
};
use astroport::asset::AssetInfo;
use cosmwasm_std::{
    entry_point, to_binary, Addr, Api, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdError, StdResult,
};
use terra_cosmwasm::TerraQuerier;

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response> {
    let config = Config {
        owner: deps.api.addr_validate(msg.owner.as_ref())?,
    };
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::UpdateConfig { owner } => update_config(deps, info, owner),
        ExecuteMsg::SetAssetInfo {
            asset_info: asset,
            price_source,
        } => set_asset(deps, info, asset, price_source),
    }
}

pub fn update_config(
    deps: DepsMut,
    info: MessageInfo,
    owner: Option<String>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;

    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    };
    config.owner = option_string_to_addr(deps.api, owner, config.owner)?;
    CONFIG.save(deps.storage, &config)?;
    Ok(Response::default())
}

pub fn set_asset(
    deps: DepsMut,
    info: MessageInfo,
    asset_info: AssetInfo,
    price_source_unchecked: PriceSourceUnchecked,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let asset_reference = get_reference(&asset_info);
    if info.sender != config.owner {
        return Err(ContractError::Unauthorized {});
    }
    let price_source: PriceSourceChecked = match price_source_unchecked {
        PriceSourceUnchecked::Native { denom } => PriceSourceChecked::Native { denom },
        PriceSourceUnchecked::TerraswapUusdPair { pair_address } => {
            PriceSourceChecked::TerraswapUusdPair {
                pair_address: deps.api.addr_validate(&pair_address)?,
            }
        }
        PriceSourceUnchecked::Fixed { price } => PriceSourceChecked::Fixed { price },
    };
    PRICE_CONFIGS.save(
        deps.storage,
        asset_reference.as_slice(),
        &PriceConfig { price_source },
    )?;
    Ok(Response::default())
}

pub fn get_reference(asset_info: &AssetInfo) -> Vec<u8> {
    match asset_info {
        AssetInfo::NativeToken { denom } => denom.as_bytes().to_vec(),
        AssetInfo::Token { contract_addr } => contract_addr.as_bytes().to_vec(),
    }
}

fn option_string_to_addr(
    api: &dyn Api,
    option_string: Option<String>,
    default: Addr,
) -> StdResult<Addr> {
    match option_string {
        Some(input_addr) => api.addr_validate(&input_addr),
        None => Ok(default),
    }
}

#[entry_point]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&query_config(deps, env)?),
        QueryMsg::AssetPriceByReference { asset_reference } => {
            to_binary(&query_asset_price(deps, env, asset_reference)?)
        }
        QueryMsg::AssetPrice { asset_info: asset } => {
            let asset_reference = get_reference(&asset);
            to_binary(&query_asset_price(deps, env, asset_reference)?)
        }
        QueryMsg::AssetPriceConfig { asset_info: asset } => {
            to_binary(&query_asset_price_config(deps, env, asset)?)
        }
    }
}

fn query_config(deps: Deps, _env: Env) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        owner: config.owner.into(),
    })
}

fn query_asset_price(deps: Deps, _env: Env, asset_reference: Vec<u8>) -> StdResult<Decimal> {
    let price_config = PRICE_CONFIGS.load(deps.storage, asset_reference.as_slice())?;
    match price_config.price_source {
        PriceSourceChecked::Native { denom } => {
            let terra_querier = TerraQuerier::new(&deps.querier);
            // NOTE: Exchange rate returns how much of the quote (second argument) is required to
            // buy one unit of the base_denom (first argument).
            // We want to know how much uusd we need to buy 1 of the target currency
            let asset_prices_query = terra_querier
                .query_exchange_rates(denom, vec!["uusd".to_string()])?
                .exchange_rates
                .pop();
            match asset_prices_query {
                Some(exchange_rate_item) => Ok(exchange_rate_item.exchange_rate),
                None => Err(StdError::generic_err("No native price found")),
            }
        }
        PriceSourceChecked::TerraswapUusdPair { .. } => {
            // TODO: implement
            Ok(Decimal::one())
        }
        PriceSourceChecked::Fixed { price } => Ok(price),
    }
}

fn query_asset_price_config(deps: Deps, _env: Env, asset: AssetInfo) -> StdResult<PriceConfig> {
    let asset_reference = get_reference(&asset);
    let price_config = PRICE_CONFIGS.load(deps.storage, asset_reference.as_slice())?;
    Ok(price_config)
}
