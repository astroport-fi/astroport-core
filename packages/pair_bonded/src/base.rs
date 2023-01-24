use crate::error::ContractError;
use crate::state::CONFIG;
use astroport::asset::{addr_opt_validate, Asset, AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::pair::{
    ConfigResponse, CumulativePricesResponse, Cw20HookMsg, InstantiateMsg, PoolResponse,
    ReverseSimulationResponse, SimulationResponse,
};
use astroport::pair_bonded::{Config, ExecuteMsg, QueryMsg};
use cosmwasm_std::{
    from_binary, to_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw2::set_contract_version;
use cw20::Cw20ReceiveMsg;

pub trait PairBonded<'a> {
    /// Contract name that is used for migration.
    const CONTRACT_NAME: &'a str;
    /// Contract version that is used for migration.
    const CONTRACT_VERSION: &'a str = env!("CARGO_PKG_VERSION");

    /// Creates a new contract with the specified parameters in [`InstantiateMsg`].
    fn instantiate(
        &self,
        deps: DepsMut,
        env: Env,
        _info: MessageInfo,
        msg: InstantiateMsg,
    ) -> Result<Response, ContractError> {
        msg.asset_infos[0].check(deps.api)?;
        msg.asset_infos[1].check(deps.api)?;

        if msg.asset_infos[0] == msg.asset_infos[1] {
            return Err(ContractError::DoublingAssets {});
        }

        set_contract_version(deps.storage, Self::CONTRACT_NAME, Self::CONTRACT_VERSION)?;

        let config = Config {
            pair_info: PairInfo {
                contract_addr: env.contract.address,
                liquidity_token: Addr::unchecked(""),
                asset_infos: msg.asset_infos.clone(),
                pair_type: PairType::Custom(String::from("Bonded")),
            },
            factory_addr: deps.api.addr_validate(&msg.factory_addr)?,
        };

        CONFIG.save(deps.storage, &config)?;

        Ok(Response::new())
    }

    /// Exposes all the execute functions available in the contract.
    ///
    /// ## Variants
    /// * **ExecuteMsg::UpdateConfig { params: Binary }**  Not supported.
    ///
    /// * **ExecuteMsg::Receive(msg)** Receives a message of type [`Cw20ReceiveMsg`] and processes
    /// it depending on the received template.
    ///
    /// * **ExecuteMsg::ProvideLiquidity {
    ///             assets,
    ///             slippage_tolerance,
    ///             auto_stake,
    ///             receiver,
    ///         }**  Not supported.
    ///
    /// * **ExecuteMsg::Swap {
    ///             offer_asset,
    ///             belief_price,
    ///             max_spread,
    ///             to,
    ///         }** Performs an swap using the specified parameters. (It needs to be implemented)
    ///
    /// * **ExecuteMsg::AssertAndSend {
    ///             offer_asset,
    ///             belief_price,
    ///             max_spread,
    ///             ask_asset_info,
    ///             receiver,
    ///             sender,
    ///         }** (internal) Is used as a sub-execution to send received tokens to the receiver and check the spread/price.
    fn execute(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<Response, ContractError> {
        match msg {
            ExecuteMsg::UpdateConfig { .. } => Err(ContractError::NotSupported {}),
            ExecuteMsg::Receive(msg) => self.receive_cw20(deps, env, info, msg),
            ExecuteMsg::ProvideLiquidity { .. } => Err(ContractError::NotSupported {}),
            ExecuteMsg::Swap {
                offer_asset,
                belief_price,
                max_spread,
                to,
            } => self.execute_swap(deps, env, info, offer_asset, belief_price, max_spread, to),
            ExecuteMsg::AssertAndSend {
                offer_asset,
                ask_asset_info,
                receiver,
                sender,
            } => self.assert_receive_and_send(
                deps,
                env,
                info,
                sender,
                offer_asset,
                ask_asset_info,
                receiver,
            ),
        }
    }

    /// Exposes all the queries available in the contract.
    ///
    /// ## Queries
    /// * **QueryMsg::Pair {}** Returns information about the pair in an object of type [`PairInfo`].
    ///
    /// * **QueryMsg::Pool {}** Returns information about the amount of assets in the pair contract as
    /// well as the amount of LP tokens issued using an object of type [`PoolResponse`].
    ///
    /// * **QueryMsg::Share { amount }** Returns the amount of assets that could be withdrawn from the pool
    /// using a specific amount of LP tokens. The result is returned in a vector that contains objects of type [`Asset`].
    ///
    /// * **QueryMsg::Simulation { offer_asset }** Returns the result of a swap simulation using a [`SimulationResponse`] object.
    ///
    /// * **QueryMsg::ReverseSimulation { ask_asset }** Returns the result of a reverse swap simulation using
    /// a [`ReverseSimulationResponse`] object.
    ///
    /// * **QueryMsg::CumulativePrices {}** Returns information about cumulative prices for the assets in the
    /// pool using a [`CumulativePricesResponse`] object.
    ///
    /// * **QueryMsg::Config {}** Returns the configuration for the pair contract using a [`ConfigResponse`] object.
    fn query(&self, deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::Pair {} => to_binary(&self.query_pair_info(deps)?),
            QueryMsg::Pool {} => to_binary(&self.query_pool(deps)?),
            QueryMsg::Share { .. } => to_binary(&Vec::<Asset>::new()),
            QueryMsg::Simulation { offer_asset } => {
                to_binary(&self.query_simulation(deps, env, offer_asset)?)
            }
            QueryMsg::ReverseSimulation { ask_asset } => {
                to_binary(&self.query_reverse_simulation(deps, env, ask_asset)?)
            }
            QueryMsg::CumulativePrices {} => to_binary(&self.query_cumulative_prices(deps, env)?),
            QueryMsg::Config {} => to_binary(&self.query_config(deps)?),
        }
    }

    /// Receives a message of type [`Cw20ReceiveMsg`] and processes it depending on the received template.
    ///
    /// * **cw20_msg** CW20 receive message to process.
    fn receive_cw20(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        cw20_msg: Cw20ReceiveMsg,
    ) -> Result<Response, ContractError> {
        match from_binary(&cw20_msg.msg) {
            Ok(Cw20HookMsg::Swap {
                belief_price,
                max_spread,
                to,
                ..
            }) => {
                // Only asset contract can execute this message
                let mut authorized = false;
                let config = CONFIG.load(deps.storage)?;

                for pool in config.pair_info.asset_infos {
                    if let AssetInfo::Token { contract_addr, .. } = &pool {
                        if contract_addr == &info.sender {
                            authorized = true;
                        }
                    }
                }

                if !authorized {
                    return Err(ContractError::Unauthorized {});
                }

                let to_addr = addr_opt_validate(deps.api, &to)?;
                let contract_addr = info.sender.clone();
                let sender = deps.api.addr_validate(&cw20_msg.sender)?;
                self.swap(
                    deps,
                    env,
                    info,
                    sender,
                    Asset {
                        info: AssetInfo::Token { contract_addr },
                        amount: cw20_msg.amount,
                    },
                    belief_price,
                    max_spread,
                    to_addr,
                )
            }
            Ok(Cw20HookMsg::WithdrawLiquidity { .. }) => Err(ContractError::NotSupported {}),
            Err(err) => Err(err.into()),
        }
    }

    /// Performs an swap operation with the specified parameters. The trader must approve the
    /// pool contract to transfer offer assets from their wallet.
    ///
    /// * **sender** sender of the swap operation.
    ///
    /// * **offer_asset** proposed asset for swapping.
    ///
    /// * **belief_price** used to calculate the maximum swap spread.
    ///
    /// * **max_spread** sets the maximum spread of the swap operation.
    ///
    /// * **to** sets the recipient of the swap operation.
    ///
    /// NOTE - the address that wants to swap should approve the pair contract to pull the offer token.
    #[allow(clippy::too_many_arguments)]
    fn execute_swap(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    ) -> Result<Response, ContractError> {
        offer_asset.info.check(deps.api)?;
        if !offer_asset.is_native_token() {
            return Err(ContractError::Cw20DirectSwap {});
        }

        let to_addr = addr_opt_validate(deps.api, &to)?;

        self.swap(
            deps,
            env,
            info.clone(),
            info.sender,
            offer_asset,
            belief_price,
            max_spread,
            to_addr,
        )
    }

    /// Performs a swap with the specified parameters.
    /// ### Must be implemented
    #[allow(clippy::too_many_arguments)]
    fn swap(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        sender: Addr,
        offer_asset: Asset,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<Addr>,
    ) -> Result<Response, ContractError>;

    /// Returns information about the pair contract in an object of type [`PairInfo`].
    fn query_pair_info(&self, deps: Deps) -> StdResult<PairInfo> {
        let config = CONFIG.load(deps.storage)?;
        Ok(config.pair_info)
    }

    /// Returns the amounts of assets in the pair contract in an object of type [`PoolResponse`].
    fn query_pool(&self, deps: Deps) -> StdResult<PoolResponse> {
        let config = CONFIG.load(deps.storage)?;
        let (assets, total_share) = self.pool_info(&config)?;

        let resp = PoolResponse {
            assets,
            total_share,
        };

        Ok(resp)
    }

    /// Returns information about a swap simulation in a [`SimulationResponse`] object.
    fn query_simulation(
        &self,
        deps: Deps,
        env: Env,
        offer_asset: Asset,
    ) -> StdResult<SimulationResponse>;

    /// Returns information about a reverse swap simulation in a [`ReverseSimulationResponse`] object.
    /// ### Must be implemented
    fn query_reverse_simulation(
        &self,
        deps: Deps,
        env: Env,
        ask_asset: Asset,
    ) -> StdResult<ReverseSimulationResponse>;

    /// Returns information about cumulative prices for the assets in the pool using a [`CumulativePricesResponse`] object.
    fn query_cumulative_prices(
        &self,
        deps: Deps,
        _env: Env,
    ) -> StdResult<CumulativePricesResponse> {
        let config = CONFIG.load(deps.storage)?;
        let (assets, total_share) = self.pool_info(&config)?;

        let resp = CumulativePricesResponse {
            assets,
            total_share,
            price0_cumulative_last: Uint128::zero(),
            price1_cumulative_last: Uint128::zero(),
        };

        Ok(resp)
    }

    /// Returns the pair contract configuration in a [`ConfigResponse`] object.
    fn query_config(&self, _deps: Deps) -> StdResult<ConfigResponse> {
        Ok(ConfigResponse {
            block_time_last: 0u64,
            params: None,
        })
    }

    /// Returns the total amount of assets in the pool.
    fn pool_info(&self, config: &Config) -> StdResult<([Asset; 2], Uint128)> {
        let pools = [
            Asset {
                amount: Uint128::zero(),
                info: config.pair_info.asset_infos[0].clone(),
            },
            Asset {
                amount: Uint128::zero(),
                info: config.pair_info.asset_infos[1].clone(),
            },
        ];

        Ok((pools, Uint128::zero()))
    }

    /// Performs an swap operation with the specified parameters. The trader must approve the
    /// pool contract to transfer offer assets from their wallet.
    ///
    /// * **sender** sender of the swap operation.
    ///
    /// * **offer_asset** proposed asset for swapping.
    ///
    /// * **ask_asset_info** ask asset info.
    ///
    /// * **receiver** receiver of the swap operation.
    ///
    /// * **belief_price** used to calculate the maximum swap spread.
    ///
    /// * **max_spread** sets the maximum spread of the swap operation.
    #[allow(clippy::too_many_arguments)]
    fn assert_receive_and_send(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        sender: Addr,
        offer_asset: Asset,
        ask_asset_info: AssetInfo,
        receiver: Addr,
    ) -> Result<Response, ContractError> {
        if env.contract.address != info.sender {
            // Only allowed to be sent by the contract itself
            return Err(ContractError::Unauthorized {});
        }

        let offer_amount = offer_asset.amount;
        let return_amount = ask_asset_info.query_pool(&deps.querier, env.contract.address)?;

        // Compute the tax for the receiving asset (if it is a native one)
        let mut return_asset = Asset {
            info: ask_asset_info.clone(),
            amount: return_amount,
        };

        let tax_amount = return_asset.compute_tax(&deps.querier)?;
        return_asset.amount -= tax_amount;

        Ok(Response::new()
            .add_message(return_asset.into_msg(&deps.querier, receiver.clone())?)
            .add_attribute("action", "swap")
            .add_attribute("sender", sender.to_string())
            .add_attribute("receiver", receiver.to_string())
            .add_attribute("offer_asset", offer_asset.info.to_string())
            .add_attribute("ask_asset", ask_asset_info.to_string())
            .add_attribute("offer_amount", offer_amount.to_string())
            .add_attribute("return_amount", return_amount.to_string())
            .add_attribute("tax_amount", tax_amount.to_string())
            .add_attribute("spread_amount", "0")
            .add_attribute("commission_amount", "0")
            .add_attribute("maker_fee_amount", "0"))
    }
}
