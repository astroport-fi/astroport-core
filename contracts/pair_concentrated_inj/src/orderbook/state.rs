use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal256, Env, QuerierWrapper, StdError, StdResult, Storage};
use cw_storage_plus::Item;
use injective_cosmwasm::{
    InjectiveQuerier, InjectiveQueryWrapper, MarketId, MarketType, SubaccountId,
};

use astroport::asset::{Asset, AssetInfo, AssetInfoExt};
use astroport::cosmwasm_ext::ConvertInto;
use astroport::pair_concentrated_inj::OrderbookStateResponse;

use crate::orderbook::consts::{MIN_TRADES_TO_AVG_LIMITS, ORDER_SIZE_LIMITS};
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
    /// Minimum number of trades to accumulate average trade size.
    /// Orderbook integration will not be enabled until this number is reached.
    pub min_trades_to_avg: u32,
    /// Whether the pool is ready to integrate with the orderbook (MIN_TRADES_TO_AVG is reached)
    pub ready: bool,
}

const OB_CONFIG: Item<OrderbookState> = Item::new("orderbook_config");

impl OrderbookState {
    pub fn new(
        querier: QuerierWrapper<InjectiveQueryWrapper>,
        env: &Env,
        market_id: &str,
        orders_number: u8,
        min_trades_to_avg: u32,
        asset_infos: &[AssetInfo],
    ) -> StdResult<Self> {
        let market_id = MarketId::new(market_id)?;

        Self::validate(
            querier,
            asset_infos,
            &market_id,
            orders_number,
            min_trades_to_avg,
        )?;

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
            orders_number,
            min_trades_to_avg,
            ready: false,
        };

        state.set_ticks(querier)?;

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
        orders_number: u8,
        min_trades_to_avg: u32,
    ) -> StdResult<()> {
        validate_param!(
            orders_number,
            orders_number,
            *ORDER_SIZE_LIMITS.start(),
            *ORDER_SIZE_LIMITS.end()
        );

        validate_param!(
            min_trades_to_avg,
            min_trades_to_avg,
            *MIN_TRADES_TO_AVG_LIMITS.start(),
            *MIN_TRADES_TO_AVG_LIMITS.end()
        );

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

    /// Querying exchange module, converting into [`Decimal256`] and caching tick sizes.
    /// Cashed values help to save gas on begin blocker iterations.
    fn set_ticks(&mut self, querier: QuerierWrapper<InjectiveQueryWrapper>) -> StdResult<()> {
        let querier = InjectiveQuerier::new(&querier);
        let market_info = querier
            .query_spot_market(&self.market_id)?
            .market
            .ok_or_else(|| OrderbookError::MarketNotFound(self.market_id.clone().into()))?;

        self.min_price_tick_size = market_info.min_price_tick_size.conv()?;
        self.min_quantity_tick_size = market_info.min_quantity_tick_size.conv()?;

        Ok(())
    }

    /// Set flag to trigger reconciliation on next begin blocker
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

    /// If min_trades_to_avg has been reached, set ready flag to true.
    pub fn ready(&mut self, ready: bool) {
        self.ready = ready;
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
            min_trades_to_avg: value.min_trades_to_avg,
            ready: value.ready,
        }
    }
}
