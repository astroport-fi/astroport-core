use cosmwasm_std::{Addr, Binary, Deps, DepsMut, Env, Event, MessageInfo, Response, StdResult, to_binary, Uint128};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InitMsg, QueryAddressResponse, QueryMsg};
use crate::querier::{query_get_pair, query_pair_info};
use crate::state::{BRIDGES, read_state, State, STATE, store_state};

use terraswap::asset::{Asset, AssetInfo, PairInfo};

#[entry_point]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: InitMsg,
) -> Result<Response, ContractError> {
    let state = State {
        owner: info.sender,
        factory: msg.factory,
        staking: msg.staking,
        astro_token: msg.astro,
    };
    store_state.save(deps.storage, &state)?;
    Ok(Response::default())
}

#[entry_point]
pub fn execute(
    mut deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::SetBridge { token, bridge } => set_bridge(deps, env, info, token, bridge),
        ExecuteMsg::Convert { token1, token2 } => try_convert(&mut deps, env, info, token1, token2),
        ExecuteMsg::ConvertMultiple { token1, token2 } => convert_multiple(&mut deps, env, info, token1, token2),
    }
}

pub fn set_bridge(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    token: Addr,
    bridge: Addr,
) -> Result<Response, ContractError> {
    let state = STATE.load(deps.storage).unwrap();
    let mut response = Response::default();

    if info.sender != state.owner {
        return Err(ContractError::Unauthorized {});
    }
    if token == state.astro_token || token == bridge {
        return Err(ContractError::InvalidBridge {});
    }
    BRIDGES.save(deps.storage, &token, &bridge)?;
    let event = Event::new("SetBridge")
        .attr("Token", token.to_string())
        .attr("Bridge", bridge.to_string());
    response.add_event(event);
    Ok(response)
}

pub fn try_convert(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: AssetInfo,
    token1: AssetInfo,
) -> Result<Response, ContractError> {
    convert(deps, env, info, token0, token1)
}


pub fn convert_multiple(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: Vec<AssetInfo>,
    token1: Vec<AssetInfo>,
) -> Result<Response, ContractError> {
    let mut response = Default::default();
    let len = token0.len();
    for i in 0..len {
        let res = convert(deps, env.clone(), info.clone(), token0[i], token1[i]).unwrap();
        for msg in res.messages {
            responce.messages.push( msg);
        }
    }
    response
}

fn convert(
    deps: &mut DepsMut,
    env: Env,
    info: MessageInfo,
    token0: AssetInfo,
    token1: AssetInfo,
)
{
    let state = STATE.load(deps.storage)?;
    let mut response = Response::default();

    // IUniswapV2Pair pair = IUniswapV2Pair(factory.getPair(token0, token1));
    // require(address(pair) != address(0), "SushiMaker: Invalid pair");
    let pair: PairInfo = query_pair_info(
        &deps.querier,
        state.factory,
        &[token0.clone(), token1.clone()],
    )?;
    // balanceOf: S1 - S4: OK
    // transfer: X1 - X5: OK
    // IERC20(address(pair)).safeTransfer(
    //     address(pair),
    //     pair.balanceOf(address(this))
    // );
    // X1 - X5: OK
    // (uint256 amount0, uint256 amount1) = pair.burn(address(this));
    let amount0 = Uint128::zero();
    let amount1 = Uint128::zero();

    // if (token0 != pair.token0()) {
    //     (amount0, amount1) = (amount1, amount0);
    // }
    if token0 != pair.asset_infos[0] {
        (amount0, amount1) = (amount1, amount0);
    }

    let amount = convert_step(token0, token1, amount0, amount1);

    let event = Event::new("LogConvert")
        .attr("sender", info.sender.to_string())
        .attr("token0", token0.to_string())
        .attr("token1", token1.to_string())
        .attr("amount0", amount0.to_string())
        .attr("amount1", amount1.to_string())
        .attr("amount", amount.to_string());

    response.events.push(event);
    Ok(response)
    // emit LogConvert(
    //     msg.sender,
    //     token0,
    //     token1,
    //     amount0,
    //     amount1,
    //     convert_step(token0, token1, amount0, amount1)
    // );
}

fn convert_step(token0: AssetInfo, token1: AssetInfo, amount0: Uint128, amount1: Uint128) -> Uint128
{
    return Uint128::zero();
}


pub fn query(
    deps: Deps,
    msg: QueryMsg,
) -> StdResult<Binary> {
    match msg {
        QueryMsg::BridgeFor { token } => to_binary(&query_get_bridge(deps, token)?),
    }
}

fn query_get_bridge(deps: Deps, token: Addr) -> StdResult<QueryAddressResponse> {
    let state = STATE.load(deps.storage).unwrap();
    let bridge = BRIDGES.load(deps.storage, &token).unwrap_or(state.astro_token);
    Ok(QueryAddressResponse { address: bridge })
}
