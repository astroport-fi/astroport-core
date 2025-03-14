use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, ensure, from_json, to_json_vec, Addr, Api, Attribute, CosmosMsg, Decimal, Decimal256,
    Deps, Empty, Env, QuerierWrapper, QueryRequest, ReplyOn, StdError, StdResult, Storage, SubMsg,
    Uint128,
};
use cw_storage_plus::Item;
use itertools::Itertools;
use neutron_std::types::cosmos::base::query::v1beta1::PageRequest;
use neutron_std::types::neutron::dex::{
    DexQuerier, MsgCancelLimitOrder, MsgCancelLimitOrderResponse,
    QueryAllLimitOrderTrancheUserByAddressRequest,
};

use astroport::asset::{Asset, Decimal256Ext};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::pair_concentrated_duality::UpdateDualityOrderbook;
use astroport::pair_concentrated_duality::{OrderbookConfig, ReplyIds};
use astroport_pcl_common::state::{Config, Precisions};

use crate::error::ContractError;
use crate::orderbook::consts::{
    MAX_AVG_PRICE_ADJ_PERCENT, MAX_LIQUIDITY_PERCENT, MIN_AVG_PRICE_ADJ_PERCENT,
    MIN_LIQUIDITY_PERCENT, ORDERS_NUMBER_LIMITS,
};
use crate::orderbook::custom_types::CustomQueryAllLimitOrderTrancheUserByAddressResponse;
use crate::orderbook::utils::SpotOrdersFactory;

macro_rules! validate_param {
    ($name:ident, $val:expr, $min:expr, $max:expr) => {
        if !($min..=$max).contains(&$val) {
            return Err(StdError::generic_err(format!(
                "Incorrect orderbook params: must be {min} <= {name} <= {max}, but value is {val}",
                name = stringify!($name),
                min = $min,
                max = $max,
                val = $val
            )));
        }
    };
}

#[cw_serde]
pub struct OrderbookState {
    /// The address of the orderbook sync executor. If none the sync is permissionless.
    pub executor: Option<Addr>,
    /// The number of trades on each side of the order book.
    /// The higher this number is, the more gas the contract consumes.
    pub orders_number: u8,
    /// The minimum asset0 order size allowed in the order book.
    pub min_asset_0_order_size: Uint128,
    /// The minimum asset1 order size allowed in the order book.
    pub min_asset_1_order_size: Uint128,
    /// The percentage of the pool's liquidity that will be placed in the order book.
    pub liquidity_percent: Decimal,
    /// Array with tranche keys of all posted orders.
    pub orders: Vec<String>,
    /// Last recorded balances on the orderbook.
    pub last_balances: Vec<Asset>,
    /// Whether the orderbook integration enabled or not.
    pub enabled: bool,
    /// Snapshot of total balances before entering reply
    pub pre_reply_balances: Vec<Asset>,
    /// Due to possible rounding issues on Duality side we have to set price tolerance,
    /// which serves as a worsening factor for the end price from PCL.
    /// Should be relatively low something like 1-10 bps.
    pub avg_price_adjustment: Decimal,
}

const OB_CONFIG: Item<OrderbookState> = Item::new("orderbook_config");

impl OrderbookState {
    pub fn new(api: &dyn Api, orderbook_config: OrderbookConfig) -> StdResult<Self> {
        let config = Self {
            orders_number: orderbook_config.orders_number,
            min_asset_0_order_size: orderbook_config.min_asset_0_order_size,
            min_asset_1_order_size: orderbook_config.min_asset_1_order_size,
            liquidity_percent: orderbook_config.liquidity_percent,
            orders: vec![],
            last_balances: vec![],
            enabled: false,
            executor: orderbook_config.executor.map(Addr::unchecked),
            pre_reply_balances: vec![],
            avg_price_adjustment: orderbook_config.avg_price_adjustment,
        };
        config.validate(api)?;

        Ok(config)
    }

    #[inline]
    pub fn load(storage: &dyn Storage) -> StdResult<OrderbookState> {
        OB_CONFIG.load(storage)
    }

    #[inline]
    pub fn save(self, storage: &mut dyn Storage) -> StdResult<()> {
        OB_CONFIG.save(storage, &self)
    }

    pub fn update_config(
        &mut self,
        api: &dyn Api,
        update_config: UpdateDualityOrderbook,
    ) -> StdResult<Vec<Attribute>> {
        let mut attrs = vec![];
        if let Some(enable) = update_config.enable {
            attrs.push(attr("enabled", enable.to_string()));
            self.enabled = enable;
        }

        ensure!(
            !update_config.remove_executor || update_config.executor.is_none(),
            StdError::generic_err(
                "Both 'remove_executor' and 'executor' cannot be set at the same time"
            )
        );

        if update_config.remove_executor {
            attrs.push(attr("removed_executor", "true"));
            self.executor = None;
        }

        if let Some(executor) = update_config.executor {
            attrs.push(attr("new_executor", &executor));
            self.executor = Some(Addr::unchecked(&executor));
        }

        if let Some(orders_number) = update_config.orders_number {
            attrs.push(attr("orders_number", orders_number.to_string()));
            self.orders_number = orders_number;
        }

        if let Some(min_asset_0_order_size) = update_config.min_asset_0_order_size {
            attrs.push(attr("min_asset_0_order_size", min_asset_0_order_size));
            self.min_asset_0_order_size = min_asset_0_order_size;
        }

        if let Some(min_asset_1_order_size) = update_config.min_asset_1_order_size {
            attrs.push(attr("min_asset_1_order_size", min_asset_1_order_size));
            self.min_asset_1_order_size = min_asset_1_order_size;
        }

        if let Some(liquidity_percent) = update_config.liquidity_percent {
            attrs.push(attr("liquidity_percent", liquidity_percent.to_string()));
            self.liquidity_percent = liquidity_percent;
        }

        if let Some(avg_price_adjustment) = update_config.avg_price_adjustment {
            attrs.push(attr(
                "avg_price_adjustment",
                avg_price_adjustment.to_string(),
            ));
            self.avg_price_adjustment = avg_price_adjustment;
        }

        self.validate(api)?;

        Ok(attrs)
    }

    fn validate_orders_number(orders_number: u8) -> StdResult<()> {
        validate_param!(
            orders_number,
            orders_number,
            *ORDERS_NUMBER_LIMITS.start(),
            *ORDERS_NUMBER_LIMITS.end()
        );
        Ok(())
    }

    fn validate_liquidity_percent(liquidity_percent: Decimal) -> StdResult<()> {
        validate_param!(
            liquidity_percent,
            liquidity_percent,
            MIN_LIQUIDITY_PERCENT,
            MAX_LIQUIDITY_PERCENT
        );
        Ok(())
    }

    fn validate_avg_price_adjustment(avg_price_adjustment: Decimal) -> StdResult<()> {
        validate_param!(
            avg_price_adjustment,
            avg_price_adjustment,
            MIN_AVG_PRICE_ADJ_PERCENT,
            MAX_AVG_PRICE_ADJ_PERCENT
        );
        Ok(())
    }

    pub fn validate(&self, api: &dyn Api) -> StdResult<()> {
        Self::validate_orders_number(self.orders_number)?;
        Self::validate_liquidity_percent(self.liquidity_percent)?;
        Self::validate_avg_price_adjustment(self.avg_price_adjustment)?;

        ensure!(
            !self.min_asset_0_order_size.is_zero(),
            StdError::generic_err("min_asset_0_order_size must be greater than zero")
        );
        ensure!(
            !self.min_asset_1_order_size.is_zero(),
            StdError::generic_err("min_asset_1_order_size must be greater than zero")
        );

        self.executor
            .as_ref()
            .map(|addr| api.addr_validate(addr.as_str()))
            .transpose()?;

        Ok(())
    }

    /// Query orderbook liquidity.
    /// If the force flag is false, this functions doesn't query orderbook if last balances are empty.
    /// This hack helps us to avoid querying orderbook if integration is disabled.
    pub fn query_ob_liquidity(
        &self,
        querier: QuerierWrapper,
        addr: &Addr,
        force_update: bool,
    ) -> StdResult<Vec<Asset>> {
        if !force_update && self.last_balances.is_empty() {
            Ok(vec![])
        } else {
            let dex_querier = DexQuerier::new(&querier);
            self.orders
                .iter()
                .map(|order_key| {
                    dex_querier
                        .simulate_cancel_limit_order(Some(MsgCancelLimitOrder {
                            creator: addr.to_string(),
                            tranche_key: order_key.to_owned(),
                        }))
                        .and_then(|res| match res.resp {
                            None
                            | Some(MsgCancelLimitOrderResponse {
                                taker_coin_out: None,
                                maker_coin_out: None,
                            }) => Err(StdError::generic_err("Unexpected duality response")),
                            Some(MsgCancelLimitOrderResponse {
                                taker_coin_out,
                                maker_coin_out,
                            }) => Ok([taker_coin_out, maker_coin_out]
                                .into_iter()
                                .flatten()
                                .collect_vec()),
                        })
                })
                .flatten_ok()
                .collect::<StdResult<Vec<_>>>()?
                .into_iter()
                .into_group_map_by(|coin| coin.denom.clone())
                .into_iter()
                .map(|(denom, coins)| {
                    let amounts: Vec<Uint128> = coins
                        .iter()
                        .map(|proto_coin| proto_coin.amount.parse())
                        .try_collect()?;
                    let amount: Uint128 = amounts.iter().sum();
                    Ok(Asset::native(denom, amount))
                })
                .collect()
        }
    }

    /// Fetch all orders and save their tranche keys in the state.
    pub fn fetch_all_orders(&mut self, deps: Deps, addr: &Addr) -> StdResult<()> {
        let query_msg = to_json_vec(&QueryRequest::<Empty>::Stargate {
            path: "/neutron.dex.Query/LimitOrderTrancheUserAllByAddress".to_string(),
            data: QueryAllLimitOrderTrancheUserByAddressRequest {
                address: addr.to_string(),
                pagination: Some(PageRequest {
                    key: Default::default(),
                    offset: 0,
                    limit: (self.orders_number * 2) as u64,
                    count_total: false,
                    reverse: false,
                }),
            }
            .into(),
        })?;

        let response_raw = deps
            .querier
            .raw_query(&query_msg)
            .into_result()
            .map_err(|err| StdError::generic_err(err.to_string()))?
            .into_result()
            .map_err(StdError::generic_err)?;

        self.orders = from_json::<CustomQueryAllLimitOrderTrancheUserByAddressResponse>(
            &response_raw,
        )
        .map(|res| {
            res.limit_orders
                .into_iter()
                .map(|order| order.tranche_key)
                .collect()
        })?;

        Ok(())
    }

    /// Cancel orders and automatically withdraw all balances from the orderbook.
    pub fn cancel_orders(&self, addr: &Addr) -> Vec<CosmosMsg> {
        self.orders
            .iter()
            .map(|tranche_key| {
                MsgCancelLimitOrder {
                    creator: addr.to_string(),
                    tranche_key: tranche_key.clone(),
                }
                .into()
            })
            .collect()
    }

    /// Construct an array with new orders.
    /// Return an empty array if orderbook integration is disabled.
    pub fn deploy_orders(
        &self,
        env: &Env,
        config: &Config,
        balances: &[Decimal256],
        precisions: &Precisions,
    ) -> Result<Vec<CosmosMsg>, ContractError> {
        // Orderbook is disabled. No need to deploy orders.
        if !self.enabled {
            return Ok(vec![]);
        }

        let liquidity_percent_to_deploy =
            Decimal256::from(self.liquidity_percent) / Decimal256::from_integer(2u128);

        let asset_0_liquidity = balances[0] * liquidity_percent_to_deploy;
        let asset_1_liquidity = balances[1] * liquidity_percent_to_deploy;

        let asset_0_precision = precisions.get_precision(&config.pair_info.asset_infos[0])?;
        let asset_1_precision = precisions.get_precision(&config.pair_info.asset_infos[1])?;

        let min_asset_0_order_size = self
            .min_asset_0_order_size
            .to_decimal256(asset_0_precision)?;

        let min_asset_1_order_size = self
            .min_asset_1_order_size
            .to_decimal256(asset_1_precision)?;

        let asset_0_trade_size = asset_0_liquidity / Decimal256::from_integer(self.orders_number);
        let asset_1_trade_size = asset_1_liquidity / Decimal256::from_integer(self.orders_number);

        if asset_0_trade_size < min_asset_0_order_size
            || asset_1_trade_size < min_asset_1_order_size
        {
            return Ok(vec![]);
        }

        let amp_gamma = config.pool_state.get_amp_gamma(env);
        let mut ixs = balances.to_vec();
        ixs[1] *= config.pool_state.price_state.price_scale;

        let mut orders_factory = SpotOrdersFactory::new(
            &config.pair_info.asset_infos,
            asset_0_precision,
            asset_1_precision,
            self.avg_price_adjustment.into(),
        );

        let success = orders_factory.construct_orders(
            config,
            amp_gamma,
            &ixs,
            asset_0_trade_size,
            asset_1_trade_size,
            self.orders_number,
        )?;

        if success {
            Ok(orders_factory.collect_spot_orders(&env.contract.address))
        } else {
            Ok(vec![])
        }
    }

    /// Flatten all messages into one vector and add a callback to the last message only
    /// if orderbook integration is enabled.
    pub fn flatten_msgs_and_add_callback(
        &mut self,
        total_liquidity: &[Asset],
        messages: &[Vec<CosmosMsg>],
        order_msgs: Vec<CosmosMsg>,
    ) -> Vec<SubMsg> {
        let is_empty_order_msgs = order_msgs.is_empty();
        let mut submsgs = messages
            .concat()
            .into_iter()
            .chain(order_msgs)
            .map(SubMsg::new)
            .collect_vec();

        if let (true, false, Some(last)) = (self.enabled, is_empty_order_msgs, submsgs.last_mut()) {
            last.id = ReplyIds::PostLimitOrderCb as u64;
            last.reply_on = ReplyOn::Success;
        }

        self.pre_reply_balances = total_liquidity.to_vec();

        submsgs
    }
}
