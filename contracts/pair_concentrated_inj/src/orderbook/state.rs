use std::str::FromStr;

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    attr, Attribute, Decimal, Decimal256, Env, QuerierWrapper, StdError, StdResult, Storage,
    Uint256,
};
use cw_storage_plus::Item;
use injective_cosmwasm::{
    InjectiveQuerier, InjectiveQueryWrapper, MarketId, MarketType, SubaccountId,
};

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::cosmwasm_ext::ConvertInto;
use astroport::pair_concentrated_inj::{
    OrderbookConfig, OrderbookStateResponse, UpdateOrderBookParams,
};

use crate::orderbook::consts::ORDER_SIZE_LIMITS;
use crate::orderbook::error::OrderbookError;
use crate::orderbook::utils::{calc_market_ids, get_subaccount};

macro_rules! validate_param {
    ($name:ident, $val:expr, $min:expr, $max:expr) => {
        if $val < $min || $val > $max {
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
    /// Market which is being used to deploy liquidity to
    pub market_id: MarketId,
    /// Subaccount used for the orderbook
    pub subaccount: SubaccountId,
    /// Stores asset infos. We duplicate it in OB state to decrease noop gas usage on begin blocker.
    pub asset_infos: Vec<AssetInfo>,
    /// Minimum allowed price tick size in the orderbook
    pub min_price_tick_size: Decimal256,
    /// Minimum allowed quantity tick size in the orderbook
    pub min_quantity_tick_size: Decimal256,
    /// This flag is set when trades, deposits or withdrawals have occurred in the previous block.
    pub need_reconcile: bool,
    /// Last balances of the subaccount on the previous begin blocker
    pub last_balances: Vec<Asset>,
    /// The number of trades on each side of the order book.
    /// The higher this number is, the more gas the contract consumes on begin blocker and
    /// the more liquidity the contract places in the order book.
    pub orders_number: u8,
    /// The minimum base order size allowed in the order book.
    pub min_base_order_size: u32,
    /// The minimum quote order size allowed in the order book.
    pub min_quote_order_size: u32,
    /// The percentage of the pool's liquidity that will be placed in the order book.
    pub liquidity_percent: Decimal,
    /// Whether the begin blocker execution is allowed or not. Default: true
    pub enabled: bool,
}

const OB_CONFIG: Item<OrderbookState> = Item::new("orderbook_config");

impl OrderbookState {
    pub fn new(
        querier: QuerierWrapper<InjectiveQueryWrapper>,
        env: &Env,
        asset_infos: &[AssetInfo],
        base_precision: u8,
        orderbook_config: OrderbookConfig,
    ) -> StdResult<Self> {
        let market_id = MarketId::new(orderbook_config.market_id.clone())?;

        Self::validate(querier, asset_infos, &market_id, &orderbook_config)?;

        let mut state = Self {
            market_id,
            subaccount: get_subaccount(&env.contract.address),
            asset_infos: asset_infos.to_vec(),
            min_price_tick_size: Default::default(),
            min_quantity_tick_size: Default::default(),
            need_reconcile: true,
            last_balances: vec![
                asset_infos[0].with_balance(0u8),
                asset_infos[1].with_balance(0u8),
            ],
            min_base_order_size: orderbook_config.min_base_order_size,
            min_quote_order_size: orderbook_config.min_quote_order_size,
            liquidity_percent: orderbook_config.liquidity_percent,
            orders_number: orderbook_config.orders_number,
            enabled: true,
        };

        state.set_ticks(querier, base_precision)?;

        Ok(state)
    }

    pub fn load(storage: &dyn Storage) -> StdResult<OrderbookState> {
        OB_CONFIG.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        OB_CONFIG.save(storage, self)
    }

    /// Validates orderbook params
    fn validate(
        querier: QuerierWrapper<InjectiveQueryWrapper>,
        asset_infos: &[AssetInfo],
        market_id: &MarketId,
        orderbook_config: &OrderbookConfig,
    ) -> StdResult<()> {
        Self::validate_orders_number(orderbook_config.orders_number)?;
        Self::validate_liquidity_percent(orderbook_config.liquidity_percent)?;
        Self::validate_min_base_order_size(orderbook_config.min_base_order_size)?;
        Self::validate_min_quote_order_size(orderbook_config.min_quote_order_size)?;

        let market_ids = calc_market_ids(asset_infos)?;

        if market_id.as_str() == market_ids[1] {
            // If we call this from instantiate context, we could just swap asset_infos to have correct order.
            // However, in that case we'll need to invert initial price scale which is bad UX.
            // We want to avoid implicit actions thus we prohibit pair creation for market id with wrong order.
            return Err(StdError::generic_err(format!(
                    "Pair asset infos have different order than market: {first}-{second} while market has {second}-{first}",
                    first = asset_infos[0], second = asset_infos[1]
                )));
        } else if market_id.as_str() != market_ids[0] {
            return Err(StdError::generic_err(format!(
                "Invalid market id. Must be: {}",
                market_ids[0]
            )));
        }

        market_id
            .clone()
            .validate(&InjectiveQuerier::new(&querier), MarketType::Spot)?;

        Ok(())
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
            Decimal::from_str("0.01")?,
            Decimal::percent(100)
        );
        Ok(())
    }

    fn validate_min_base_order_size(min_base_order_size: u32) -> StdResult<()> {
        validate_param!(min_base_order_size, min_base_order_size, 1, u32::MAX);
        Ok(())
    }

    fn validate_min_quote_order_size(min_quote_order_size: u32) -> StdResult<()> {
        validate_param!(min_quote_order_size, min_quote_order_size, 1, u32::MAX);
        Ok(())
    }

    /// Querying exchange module, converting into [`Decimal256`] and caching tick sizes.
    /// Cashed values help to save gas on begin blocker iterations.
    pub fn set_ticks(
        &mut self,
        querier: QuerierWrapper<InjectiveQueryWrapper>,
        base_precision: u8,
    ) -> StdResult<()> {
        let querier = InjectiveQuerier::new(&querier);
        let market_info = querier
            .query_spot_market(&self.market_id)?
            .market
            .ok_or_else(|| OrderbookError::MarketNotFound(self.market_id.as_str().to_string()))?;

        let new_min_price_tick_size: Decimal256 = market_info.min_price_tick_size.conv()?;

        // Injective uses integer values without precision for min_quantity_tick_size
        // (even though it has FPDecimal type) thus we convert it to Decimal256 with precision
        let new_min_quantity_tick_size_raw: Decimal256 =
            market_info.min_quantity_tick_size.conv()?;
        let new_min_quantity_tick_size = Decimal256::from_ratio(
            new_min_quantity_tick_size_raw.to_uint_floor(),
            Uint256::from(10u8).pow(base_precision as u32),
        );

        if new_min_price_tick_size == self.min_price_tick_size
            && new_min_quantity_tick_size == self.min_quantity_tick_size
        {
            return Err(StdError::generic_err("Ticks are already up to date"));
        }

        self.min_price_tick_size = new_min_price_tick_size;
        self.min_quantity_tick_size = new_min_quantity_tick_size;

        Ok(())
    }

    /// Set flag to trigger reconciliation on the next begin blocker
    pub fn reconcile(self, storage: &mut dyn Storage) -> StdResult<()> {
        OB_CONFIG.save(
            storage,
            &OrderbookState {
                need_reconcile: true,
                ..self
            },
        )
    }

    /// Set flag that reconciliation is done. Save current subaccount balances.
    pub fn reconciliation_done(
        self,
        storage: &mut dyn Storage,
        new_balances: Vec<Asset>,
    ) -> StdResult<()> {
        OB_CONFIG.save(
            storage,
            &OrderbookState {
                need_reconcile: false,
                last_balances: new_balances,
                ..self
            },
        )
    }

    pub fn update_params(
        self,
        storage: &mut dyn Storage,
        params: UpdateOrderBookParams,
    ) -> StdResult<Vec<Attribute>> {
        let mut attributes: Vec<_> = vec![];

        if let Some(orders_number) = params.orders_number {
            Self::validate_orders_number(orders_number)?;
            OB_CONFIG
                .update(storage, |mut ob_state| -> StdResult<OrderbookState> {
                    ob_state.orders_number = orders_number;
                    Ok(ob_state)
                })
                .map(|_| ())?;
            attributes.push(attr("orders_number", orders_number.to_string()));
        }

        if let Some(min_base_order_size) = params.min_base_order_size {
            Self::validate_min_base_order_size(min_base_order_size)?;
            OB_CONFIG
                .update(storage, |mut ob_state| -> StdResult<OrderbookState> {
                    ob_state.min_base_order_size = min_base_order_size;
                    Ok(ob_state)
                })
                .map(|_| ())?;
            attributes.push(attr("min_base_order_size", min_base_order_size.to_string()));
        }

        if let Some(min_quote_order_size) = params.min_quote_order_size {
            Self::validate_min_quote_order_size(min_quote_order_size)?;
            OB_CONFIG
                .update(storage, |mut ob_state| -> StdResult<OrderbookState> {
                    ob_state.min_quote_order_size = min_quote_order_size;
                    Ok(ob_state)
                })
                .map(|_| ())?;
            attributes.push(attr(
                "min_quote_order_size",
                min_quote_order_size.to_string(),
            ));
        }

        if let Some(liquidity_percent) = params.liquidity_percent {
            Self::validate_liquidity_percent(liquidity_percent)?;
            OB_CONFIG
                .update(storage, |mut ob_state| -> StdResult<OrderbookState> {
                    ob_state.liquidity_percent = liquidity_percent;
                    Ok(ob_state)
                })
                .map(|_| ())?;
            attributes.push(attr("liquidity_percent", liquidity_percent.to_string()));
        }

        Ok(attributes)
    }
}

impl From<OrderbookState> for OrderbookStateResponse {
    fn from(value: OrderbookState) -> Self {
        Self {
            market_id: value.market_id.as_str().to_string(),
            subaccount: value.subaccount.as_str().to_string(),
            min_price_tick_size: value.min_price_tick_size,
            min_quantity_tick_size: value.min_quantity_tick_size,
            need_reconcile: value.need_reconcile,
            last_balances: value.last_balances,
            orders_number: value.orders_number,
            min_base_order_size: value.min_base_order_size,
            min_quote_order_size: value.min_quote_order_size,
            liquidity_percent: value.liquidity_percent,
            enabled: value.enabled,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Display;

    use cosmwasm_std::testing::MockStorage;

    use super::*;

    fn error_params<T>(name: &str, min: T, max: T, val: T) -> StdError
    where
        T: Display,
    {
        StdError::generic_err(format!(
            "Incorrect orderbook params: must be {min} <= {name} <= {max}, but value is {val}",
            name = name,
            min = min,
            max = max,
            val = val
        ))
    }

    #[test]
    fn check_update_params() {
        let min_liquidity_percent = Decimal::from_str("0.01").unwrap();
        let max_liquidity_percent = Decimal::percent(100);
        let min_orders_number = ORDER_SIZE_LIMITS.start();
        let max_orders_number = ORDER_SIZE_LIMITS.end();
        let min_order_size = 1_u32;

        let mut storage = MockStorage::default();
        let ob_state = OrderbookState {
            market_id: MarketId::unchecked(
                "0x1c79dac019f73e4060494ab1b4fcba734350656d6fc4d474f6a238c13c6f9ced",
            ),
            subaccount: SubaccountId::unchecked(
                "0xc7dca7c15c364865f77a4fb67ab11dc95502e6fe000000000000000000000001",
            ),
            asset_infos: vec![],
            min_price_tick_size: Default::default(),
            min_quantity_tick_size: Default::default(),
            need_reconcile: false,
            last_balances: vec![],
            orders_number: 0,
            liquidity_percent: Default::default(),
            min_base_order_size: Default::default(),
            min_quote_order_size: Default::default(),
            enabled: true,
        };

        OB_CONFIG.save(&mut storage, &ob_state).unwrap();

        let mut params = UpdateOrderBookParams {
            orders_number: None,
            min_base_order_size: None,
            min_quote_order_size: None,
            liquidity_percent: Some(Decimal::percent(0)),
        };

        // Should fail since liquidity percent is less than 0.01
        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params(
                "liquidity_percent",
                min_liquidity_percent,
                max_liquidity_percent,
                Decimal::percent(0)
            ),
        );

        // Should fail if liquidity is bigger than 100%
        params.liquidity_percent = Some(Decimal::percent(101));

        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params(
                "liquidity_percent",
                min_liquidity_percent,
                max_liquidity_percent,
                Decimal::percent(101)
            ),
        );

        // Should fail if orders_number is less than 1
        params.liquidity_percent = None;
        params.orders_number = Some(0);

        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params("orders_number", min_orders_number, max_orders_number, &0),
        );

        // Should fail if orders_number is bigger than ORDER_SIZE_LIMITS.end()

        params.orders_number = Some(ORDER_SIZE_LIMITS.end() + 1);

        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params(
                "orders_number",
                min_orders_number,
                max_orders_number,
                &(ORDER_SIZE_LIMITS.end() + 1)
            ),
        );

        // Should fail if min_base_order_size is less than 1
        params.orders_number = None;
        params.min_base_order_size = Some(0);

        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params("min_base_order_size", min_order_size, u32::MAX, 0),
        );

        // Should fail if min_quote_order_size is less than 1
        params.min_base_order_size = None;
        params.min_quote_order_size = Some(0);

        let res = ob_state
            .clone()
            .update_params(&mut storage, params.clone())
            .unwrap_err();

        assert_eq!(
            res,
            error_params("min_quote_order_size", min_order_size, u32::MAX, 0),
        );

        // Should pass if all params are valid
        let params = UpdateOrderBookParams {
            orders_number: Some(5),
            min_base_order_size: Some(22),
            min_quote_order_size: Some(33),
            liquidity_percent: Some(Decimal::percent(50)),
        };

        let res = ob_state
            .update_params(&mut storage, params.clone())
            .unwrap();

        let ob_state = OB_CONFIG.load(&mut storage).unwrap();

        assert_eq!(ob_state.orders_number, 5);
        assert_eq!(ob_state.min_base_order_size, 22);
        assert_eq!(ob_state.min_quote_order_size, 33);
        assert_eq!(ob_state.liquidity_percent, Decimal::percent(50));

        assert_eq!(res.len(), 4);
        assert_eq!(res[0], attr("orders_number", "5"));
        assert_eq!(res[1], attr("min_base_order_size", "22"));
        assert_eq!(res[2], attr("min_quote_order_size", "33"));
        assert_eq!(res[3], attr("liquidity_percent", "0.5"));
    }
}
