use std::cmp::Ordering;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, coin, ensure, Addr, Api, Attribute, Coin, CosmosMsg, Decimal, Decimal256, Deps, Env,
    ReplyOn, StdError, StdResult, Storage, SubMsg, Uint128,
};
use cw_storage_plus::Item;
use itertools::Itertools;
use neutron_sdk::proto_types::cosmos::base::query::v1beta1::PageRequest;
use neutron_sdk::proto_types::neutron::dex::{
    DexQuerier, MsgCancelLimitOrder, MsgCancelLimitOrderResponse, MsgWithdrawFilledLimitOrder,
};

use astroport::asset::{Asset, AssetInfo, AssetInfoExt, Decimal256Ext, DecimalAsset};
use astroport::cosmwasm_ext::IntegerToDecimal;
use astroport::pair_concentrated_duality::UpdateDualityOrderbook;
use astroport::pair_concentrated_duality::{OrderbookConfig, ReplyIds};
use astroport_pcl_common::calc_d;
use astroport_pcl_common::state::{Config, Precisions};

use crate::error::ContractError;
use crate::orderbook::consts::{MAX_LIQUIDITY_PERCENT, MIN_LIQUIDITY_PERCENT, ORDER_SIZE_LIMITS};
use crate::orderbook::error::OrderbookError;
use crate::orderbook::execute::CumulativeTrade;
use crate::orderbook::utils::{compute_swap, SpotOrdersFactory};

macro_rules! validate_param {
    ($name:ident, $val:expr, $min:expr, $max:expr) => {
        if !($min..$max).contains(&$val) {
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
    pub last_balances: Vec<Coin>,
    /// Whether the orderbook integration enabled or not.
    pub enabled: bool,
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
            enabled: orderbook_config.enable,
            executor: orderbook_config.executor.map(Addr::unchecked),
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

        self.validate(api)?;

        Ok(attrs)
    }

    fn validate_orders_number(orders_number: u8) -> StdResult<()> {
        validate_param!(
            orders_number,
            orders_number,
            *ORDER_SIZE_LIMITS.start(),
            *ORDER_SIZE_LIMITS.end()
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

    pub fn validate(&self, api: &dyn Api) -> StdResult<()> {
        Self::validate_orders_number(self.orders_number)?;
        Self::validate_liquidity_percent(self.liquidity_percent)?;

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
        deps: Deps,
        addr: &Addr,
        force_update: bool,
    ) -> StdResult<Vec<Coin>> {
        if !force_update && self.last_balances.is_empty() {
            Ok(vec![])
        } else {
            let dex_querier = DexQuerier::new(&deps.querier);
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
                                .filter_map(|coin| coin)
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
                    Ok(coin(amount.u128(), denom))
                })
                .collect()
        }
    }

    /// Convert orderbook balances into DecimalAsset.
    /// It is required that self.last_balances is updated before this method.
    pub fn query_ob_liquidity_dec(
        &self,
        precisions: &Precisions,
    ) -> Result<Vec<DecimalAsset>, ContractError> {
        self.last_balances
            .iter()
            .map(|coin| {
                let asset = Asset::native(&coin.denom, coin.amount);
                asset
                    .to_decimal_asset(precisions.get_precision(&asset.info)?)
                    .map_err(Into::into)
            })
            .collect()
    }

    /// Fetch all orders and save their tranche keys in the state.
    pub fn fetch_all_orders(&mut self, deps: Deps, addr: &Addr) -> Result<(), OrderbookError> {
        self.orders = DexQuerier::new(&deps.querier)
            .limit_order_tranche_user_all_by_address(
                addr.to_string(),
                Some(PageRequest {
                    key: Default::default(),
                    offset: 0,
                    limit: (self.orders_number * 2) as u64,
                    count_total: false,
                    reverse: false,
                }),
            )
            .map(|res| {
                res.limit_orders
                    .into_iter()
                    .map(|order| order.tranche_key)
                    .collect()
            })?;

        Ok(())
    }

    /// Cancel orders and withdraw all balances from the orderbook.
    pub fn cancel_orders(&self, addr: &Addr) -> Vec<CosmosMsg> {
        self.orders
            .iter()
            .flat_map(|tranche_key| {
                let cancel_msg = MsgCancelLimitOrder {
                    creator: addr.to_string(),
                    tranche_key: tranche_key.clone(),
                }
                .into();
                let withdraw_msg = MsgWithdrawFilledLimitOrder {
                    creator: addr.to_string(),
                    tranche_key: tranche_key.clone(),
                }
                .into();

                [cancel_msg, withdraw_msg]
            })
            .collect()
    }

    /// Fetch orderbook and check whether any of the orders have been executed.
    /// Return CumulativeTrade object which is the difference between last and current balances.
    /// Cache new balances in the state.
    pub fn fetch_cumulative_trade(
        &mut self,
        deps: Deps,
        addr: &Addr,
        precisions: &Precisions,
    ) -> Result<Option<CumulativeTrade>, ContractError> {
        let mut new_balances = self.query_ob_liquidity(deps, addr, false)?;
        if !new_balances.is_empty() {
            if self.last_balances[0] != new_balances[0] {
                new_balances.swap(0, 1);
            }

            let bal_diffs = self
                .last_balances
                .iter()
                .zip(new_balances.iter())
                .map(|(a, b)| b.amount.abs_diff(a.amount))
                .collect_vec();

            let diff_to_dec_asset = |ind: usize| -> Result<_, ContractError> {
                let asset_info = AssetInfo::native(&new_balances[ind].denom);
                let precision = precisions.get_precision(&asset_info)?;
                Ok(asset_info.with_dec_balance(bal_diffs[ind].to_decimal256(precision)?))
            };

            let maybe_trade = match self.last_balances[0].amount.cmp(&new_balances[0].amount) {
                // We sold asset 0 for asset 1
                Ordering::Less => Some(CumulativeTrade {
                    base_asset: diff_to_dec_asset(1)?,
                    quote_asset: diff_to_dec_asset(0)?,
                }),
                // We bought asset 0 with asset 1
                Ordering::Greater => Some(CumulativeTrade {
                    base_asset: diff_to_dec_asset(0)?,
                    quote_asset: diff_to_dec_asset(1)?,
                }),
                // No trade happened
                Ordering::Equal => None,
            };

            self.last_balances = new_balances;

            Ok(maybe_trade)
        } else {
            Ok(None)
        }
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
        let d = calc_d(&ixs, &amp_gamma)?;

        let mut orders_factory = SpotOrdersFactory::new(
            &config.pair_info.asset_infos,
            asset_0_precision,
            asset_1_precision,
        );

        // Equal heights algorithm
        for i in 1..=self.orders_number {
            let i_dec = Decimal256::from_integer(i);

            let asset_1_sell_amount = asset_1_trade_size * i_dec;
            let asset_0_sell_amount =
                compute_swap(&ixs, asset_1_sell_amount, 0, config, amp_gamma, d)?;

            let sell_amount = asset_0_sell_amount / i_dec;

            let sell_price = if i > 1 {
                (asset_1_sell_amount - orders_factory.orderbook_one_side_liquidity(false))
                    / sell_amount
            } else {
                asset_1_sell_amount / sell_amount
            };

            let asset_0_buy_amount = asset_0_trade_size * i_dec;
            let asset_1_buy_amount =
                compute_swap(&ixs, asset_0_buy_amount, 1, config, amp_gamma, d)?;

            let buy_amount = asset_1_buy_amount / i_dec;

            let buy_price = if i > 1 {
                (asset_0_buy_amount - orders_factory.orderbook_one_side_liquidity(true))
                    / buy_amount
            } else {
                asset_0_buy_amount / buy_amount
            };

            // If at some point the price becomes zero, we don't post new orders
            if sell_price.is_zero() || buy_price.is_zero() {
                return Ok(vec![]);
            }

            orders_factory.sell(sell_price, sell_amount);
            orders_factory.buy(buy_price, buy_amount);
        }

        Ok(orders_factory.collect_spot_orders(&env.contract.address))
    }

    /// Flatten all messages into one vector and add a callback to the last message only
    /// if orderbook integration is enabled.
    pub fn flatten_msgs_and_add_callback(&self, messages: &[Vec<CosmosMsg>]) -> Vec<SubMsg> {
        let mut submsgs = messages.concat().into_iter().map(SubMsg::new).collect_vec();

        if let (true, Some(last)) = (self.enabled, submsgs.last_mut()) {
            last.id = ReplyIds::PostLimitOrderCb as u64;
            last.reply_on = ReplyOn::Success;
        }

        submsgs
    }
}
