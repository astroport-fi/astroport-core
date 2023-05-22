use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;

use anyhow::{anyhow, Result as AnyResult};
use astroport::cosmwasm_ext::ConvertInto;
use cosmwasm_schema::cw_serde;
use cosmwasm_schema::schemars::JsonSchema;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    attr, coin, coins, to_binary, Addr, Api, BankMsg, Binary, BlockInfo, Coin, CustomMsg,
    CustomQuery, Decimal256, DepsMut, Empty, Env, GovMsg, IbcMsg, IbcQuery, MemoryStorage,
    OverflowError, Querier, Reply, Response, StdError, Storage, SubMsgResponse, SubMsgResult,
};
use cw_multi_test::{
    App, AppResponse, BankKeeper, CosmosRouter, DistributionKeeper, FailingModule, Module, Router,
    StakeKeeper, WasmKeeper,
};
use cw_utils::parse_instantiate_response_data;
use injective_cosmwasm::{
    Deposit, InjectiveMsg, InjectiveMsgWrapper, InjectiveQuery, InjectiveQueryWrapper, MarketId,
    OrderType, SpotMarket, SpotMarketResponse, SpotOrder, SubaccountDepositResponse, SubaccountId,
    TraderSpotOrdersResponse, TrimmedSpotLimitOrder,
};
use injective_math::FPDecimal;
use injective_testing::{generate_inj_address, InjectiveAddressGenerator};
use itertools::Itertools;

use crate::helper::f64_to_dec;
use astroport_factory::error::ContractError;
use astroport_factory::state::{PAIRS, TMP_PAIR_INFO};
use astroport_pair_concentrated_injective::orderbook::msg::SudoMsg;
use astroport_pair_concentrated_injective::orderbook::utils::{calc_hash, get_subaccount};

// This is dirty workaround cuz we can't simulate real gas in cw_multitest
const GAS_PER_BEGIN_BLOCK: u128 = 100_000;
const GAS_PRICE: u128 = 1_000_000_000; // 0.000000001 INJ per gas unit

pub fn factory_reply<T, C>(
    deps: DepsMut<C>,
    _env: Env,
    msg: Reply,
) -> Result<Response<T>, ContractError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    let tmp = TMP_PAIR_INFO.load(deps.storage)?;
    if PAIRS.has(deps.storage, &tmp.pair_key) {
        return Err(ContractError::PairWasRegistered {});
    }

    let data = msg.result.unwrap().data.unwrap();
    let res = parse_instantiate_response_data(&data)
        .map_err(|e| StdError::generic_err(format!("{e}")))?;

    let pair_contract = deps.api.addr_validate(&res.contract_address)?;

    PAIRS.save(deps.storage, &tmp.pair_key, &pair_contract)?;

    Ok(Response::new().add_attributes(vec![
        attr("action", "register"),
        attr("pair_contract_addr", pair_contract),
    ]))
}

pub fn cl_pair_reply<T, C>(
    deps: DepsMut<C>,
    _env: Env,
    msg: Reply,
) -> Result<Response<T>, astroport_pair_concentrated::error::ContractError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    match msg {
        Reply {
            id: 1,
            result:
                SubMsgResult::Ok(SubMsgResponse {
                    data: Some(data), ..
                }),
        } => {
            let mut config = astroport_pair_concentrated::state::CONFIG.load(deps.storage)?;

            if config.pair_info.liquidity_token != Addr::unchecked("") {
                return Err(astroport_pair_concentrated::error::ContractError::Unauthorized {});
            }

            let init_response = parse_instantiate_response_data(data.as_slice())
                .map_err(|e| StdError::generic_err(format!("{e}")))?;
            config.pair_info.liquidity_token =
                deps.api.addr_validate(&init_response.contract_address)?;
            astroport_pair_concentrated::state::CONFIG.save(deps.storage, &config)?;
            Ok(Response::new()
                .add_attribute("liquidity_token_addr", config.pair_info.liquidity_token))
        }
        _ => Err(astroport_pair_concentrated::error::ContractError::FailedToParseReply {}),
    }
}

// Both these structs are private in injective_cosmwasm thus we need to copy them here
#[cw_serde]
struct QueryContractRegistrationInfoResponse {
    contract: Option<RegisteredContract>,
}
#[cw_serde]
pub struct RegisteredContract {
    pub gas_limit: u64,
    pub gas_price: u64,
    pub is_executable: bool,
    pub code_id: u64,
    pub admin_address: String,
}

pub type InjApp = App<
    BankKeeper,
    MockApi,
    MemoryStorage,
    InjMockModule,
    WasmKeeper<InjectiveMsgWrapper, InjectiveQueryWrapper>,
>;

pub fn mock_inj_app<F>(init_fn: F) -> InjApp
where
    F: FnOnce(
        &mut Router<
            BankKeeper,
            InjMockModule,
            WasmKeeper<InjectiveMsgWrapper, InjectiveQueryWrapper>,
            StakeKeeper,
            DistributionKeeper,
            FailingModule<IbcMsg, IbcQuery, Empty>,
            FailingModule<GovMsg, Empty, Empty>,
        >,
        &dyn Api,
        &mut dyn Storage,
    ),
{
    cw_multi_test::AppBuilder::new()
        .with_custom(InjMockModule::new())
        .with_wasm::<InjMockModule, WasmKeeper<InjectiveMsgWrapper, InjectiveQueryWrapper>>(
            WasmKeeper::new_with_custom_address_generator(InjectiveAddressGenerator()),
        )
        .build(init_fn)
}

pub trait InjAppExt {
    fn create_market(&mut self, base_denom: &str, quote_denom: &str) -> AnyResult<String>;
    fn enable_contract(&mut self, contract_addr: Addr) -> AnyResult<()>;
    fn deactivate_contract(&mut self, contract_addr: Addr) -> AnyResult<AppResponse>;
    fn begin_blocker(&mut self, block: &BlockInfo, gas_free: bool) -> AnyResult<()>;
}

impl InjAppExt for InjApp {
    fn create_market(&mut self, base_denom: &str, quote_denom: &str) -> AnyResult<String> {
        self.init_modules(|router, _, _| {
            let market_id_hash = calc_hash(base_denom.as_bytes(), quote_denom.as_bytes());
            let market_id = MarketId::new(market_id_hash)?;
            router
                .custom
                .orderbook
                .borrow_mut()
                .entry(market_id.clone())
                .or_insert(vec![]);
            router.custom.markets.borrow_mut().insert(
                market_id.clone(),
                (base_denom.to_string(), quote_denom.to_string()),
            );

            Ok(market_id.into())
        })
    }

    fn enable_contract(&mut self, contract_addr: Addr) -> AnyResult<()> {
        self.init_modules(|router, _, _| {
            router
                .custom
                .enabled_contracts
                .borrow_mut()
                .insert(contract_addr, true);

            Ok(())
        })
    }

    fn deactivate_contract(&mut self, contract_addr: Addr) -> AnyResult<AppResponse> {
        self.init_modules(|router, _, _| {
            router
                .custom
                .enabled_contracts
                .borrow_mut()
                .insert(contract_addr.clone(), false);
        });

        self.wasm_sudo(contract_addr, &SudoMsg::Deactivate {})
    }

    fn begin_blocker(&mut self, block: &BlockInfo, gas_free: bool) -> AnyResult<()> {
        let contracts = self.init_modules(|router, _, _| {
            router
                .custom
                .enabled_contracts
                .borrow()
                .iter()
                .filter_map(|(addr, enabled)| if *enabled { Some(addr) } else { None })
                .cloned()
                .collect_vec()
        });

        for contract in contracts {
            self.wasm_sudo(contract.clone(), &SudoMsg::BeginBlocker {})?;
            if !gas_free {
                self.init_modules(|router, api, storage| {
                    router
                        .execute(
                            api,
                            storage,
                            block,
                            contract.clone(),
                            BankMsg::Send {
                                to_address: router.custom.gas_fee_receiver.to_string(),
                                amount: coins(GAS_PER_BEGIN_BLOCK * GAS_PRICE, "inj"),
                            }
                            .into(),
                        )
                        .map_err(|err| {
                            err.root_cause()
                                .downcast_ref::<OverflowError>()
                                .map(|err| {
                                    anyhow!(
                                    "Contract {} failed to pay gas fees on begin blocker: {err}",
                                    contract
                                )
                                })
                                .unwrap_or(err)
                        })
                })?;
            }
        }

        Ok(())
    }
}

pub struct InjMockModule {
    pub deposit: RefCell<HashMap<SubaccountId, Vec<Coin>>>,
    pub module_addr: Addr,
    pub gas_fee_receiver: Addr,
    pub orderbook: RefCell<HashMap<MarketId, Vec<(Addr, SpotOrder)>>>,
    pub markets: RefCell<HashMap<MarketId, (String, String)>>,
    pub enabled_contracts: RefCell<HashMap<Addr, bool>>,
}

impl InjMockModule {
    pub fn new() -> Self {
        Self {
            deposit: Default::default(),
            module_addr: generate_inj_address(),
            gas_fee_receiver: generate_inj_address(),
            orderbook: Default::default(),
            markets: Default::default(),
            enabled_contracts: Default::default(),
        }
    }
}

impl Module for InjMockModule {
    type ExecT = InjectiveMsgWrapper;
    type QueryT = InjectiveQueryWrapper;
    type SudoT = Empty;

    fn execute<ExecC, QueryC>(
        &self,
        api: &dyn Api,
        storage: &mut dyn Storage,
        router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        block: &BlockInfo,
        _sender: Addr,
        msg: Self::ExecT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        match msg.msg_data {
            InjectiveMsg::Deposit {
                sender,
                subaccount_id,
                amount,
            } => {
                if get_subaccount(&sender) != subaccount_id {
                    return Err(
                        StdError::generic_err("subaccount_id does not belong to sender").into(),
                    );
                }

                self.deposit
                    .borrow_mut()
                    .entry(subaccount_id)
                    .and_modify(|v| {
                        v.iter_mut()
                            .find_map(|coin| {
                                if coin.denom == amount.denom {
                                    coin.amount += amount.amount;
                                    Some(())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_else(|| v.push(amount.clone()))
                    })
                    .or_insert_with(|| vec![amount.clone()]);
                router.execute(
                    api,
                    storage,
                    block,
                    sender,
                    BankMsg::Send {
                        to_address: self.module_addr.to_string(),
                        amount: vec![amount],
                    }
                    .into(),
                )?;
            }
            InjectiveMsg::Withdraw {
                sender,
                subaccount_id,
                amount,
            } => {
                if get_subaccount(&sender) != subaccount_id {
                    return Err(
                        StdError::generic_err("subaccount_id does not belong to sender").into(),
                    );
                }

                self.deposit
                    .borrow_mut()
                    .entry(subaccount_id)
                    .and_modify(|v| {
                        v.iter_mut().for_each(|coin| {
                            if coin.denom == amount.denom {
                                router
                                    .execute(
                                        api,
                                        storage,
                                        block,
                                        self.module_addr.clone(),
                                        BankMsg::Send {
                                            to_address: sender.to_string(),
                                            amount: vec![amount.clone()],
                                        }
                                        .into(),
                                    )
                                    .unwrap();
                                coin.amount -= amount.amount;
                            }
                        });
                    });
            }
            InjectiveMsg::CreateSpotMarketOrder { sender, order } => {
                if get_subaccount(&sender) != order.order_info.subaccount_id {
                    return Err(
                        StdError::generic_err("subaccount_id does not belong to sender").into(),
                    );
                }

                let mut markets = self.orderbook.borrow_mut();
                let order_entry = markets.entry(order.market_id.clone()).and_modify(|v| {
                    v.push((sender.clone(), order.clone()));
                });

                if let Entry::Vacant(_) = order_entry {
                    return Err(StdError::generic_err("market does not exist").into());
                }

                let markets = self.markets.borrow();
                let (base_denom, quote_denom) = markets.get(&order.market_id).unwrap();
                let need_coin = if BUY_TYPES.contains(&order.order_type) {
                    coin(
                        (order.order_info.quantity * FPDecimal::ONE / order.order_info.price)
                            .into(),
                        base_denom,
                    )
                } else {
                    coin(
                        (order.order_info.quantity * order.order_info.price).into(),
                        quote_denom,
                    )
                };

                // check subaccount has enough coins
                let deposits = self.deposit.borrow();
                let deposit =
                    deposits
                        .get(&order.order_info.subaccount_id)
                        .ok_or(StdError::generic_err(
                            "deposit for subaccount does not exist",
                        ))?;

                let valid = deposit
                    .iter()
                    .any(|coin| coin.denom == need_coin.denom && coin.amount >= need_coin.amount);
                if !valid {
                    return Err(StdError::generic_err(format!(
                        "insufficient balance for subaccount. Deposit: {}, but need: {need_coin}",
                        deposit.iter().join(", "),
                    ))
                    .into());
                }

                // check quantity and price ticks
                let order_size_tick: FPDecimal = 1000000000000000u128.into(); // from the real INJ/USDT market;
                let price_tick = FPDecimal::from_str("0.000000000000001").unwrap();
                if order.order_info.quantity / order_size_tick * order_size_tick
                    != order.order_info.quantity
                {
                    return Err(StdError::generic_err(format!(
                        "Order quantity {} is not a multiple of tick size {}",
                        order.order_info.quantity, order_size_tick
                    ))
                    .into());
                }

                if order.order_info.price / price_tick * price_tick != order.order_info.price {
                    return Err(StdError::generic_err(format!(
                        "Order price {} is not a multiple of tick size {}",
                        order.order_info.price, price_tick
                    ))
                    .into());
                }
            }
            InjectiveMsg::BatchUpdateOrders {
                sender,
                subaccount_id,
                spot_market_ids_to_cancel_all,
                spot_orders_to_create,
                ..
            } => {
                // Cancel all orders
                if !spot_market_ids_to_cancel_all.is_empty() {
                    let subaccount = subaccount_id.ok_or_else(|| {
                        StdError::generic_err("subaccount_id is required to cancell all orders")
                    })?;
                    if get_subaccount(&sender) != subaccount {
                        return Err(StdError::generic_err(
                            "subaccount_id does not belong to sender",
                        )
                        .into());
                    }

                    let mut markets = self.orderbook.borrow_mut();

                    for market_id in spot_market_ids_to_cancel_all {
                        let order_entry = markets.entry(market_id.clone()).and_modify(|v| {
                            v.retain(|(addr, _)| addr != &sender);
                        });

                        if let Entry::Vacant(_) = order_entry {
                            return Err(StdError::generic_err("market does not exist").into());
                        }
                    }
                }

                // Hardcoded ticks from the real INJ/USDT market
                let order_size_tick: FPDecimal = 1000000000000000u128.into();
                let price_tick = FPDecimal::from_str("0.000000000000001").unwrap();

                let mut need_coins: HashMap<String, Coin> = HashMap::default();

                spot_orders_to_create
                    .iter()
                    .try_for_each::<_, AnyResult<()>>(|order| {
                        let mut markets = self.orderbook.borrow_mut();
                        let order_entry = markets.entry(order.market_id.clone()).and_modify(|v| {
                            v.push((sender.clone(), order.clone()));
                        });

                        if let Entry::Vacant(_) = order_entry {
                            return Err(StdError::generic_err("market does not exist").into());
                        }

                        let markets = self.markets.borrow();
                        let (base_denom, quote_denom) = markets.get(&order.market_id).unwrap();
                        let need_coin = if BUY_TYPES.contains(&order.order_type) {
                            coin(
                                (order.order_info.quantity * order.order_info.price).into(),
                                quote_denom,
                            )
                        } else {
                            coin(order.order_info.quantity.into(), base_denom)
                        };
                        need_coins
                            .entry(need_coin.denom.clone())
                            .and_modify(|v| {
                                v.amount += need_coin.amount;
                            })
                            .or_insert(need_coin.clone());

                        // check quantity and price ticks
                        if order.order_info.quantity / order_size_tick * order_size_tick
                            != order.order_info.quantity
                        {
                            return Err(StdError::generic_err(format!(
                                "Order quantity {} is not a multiple of tick size {}",
                                order.order_info.quantity, order_size_tick
                            ))
                            .into());
                        }

                        if order.order_info.price / price_tick * price_tick
                            != order.order_info.price
                        {
                            return Err(StdError::generic_err(format!(
                                "Order price {} is not a multiple of tick size {}",
                                order.order_info.price, price_tick
                            ))
                            .into());
                        }

                        Ok(())
                    })?;

                // check subaccount has enough coins
                if !need_coins.is_empty() {
                    // Consider that all orders are from one subaccount
                    let subaccount = spot_orders_to_create[0].order_info.subaccount_id.clone();

                    let deposits = self.deposit.borrow();
                    let deposit =
                        deposits
                            .get(&subaccount)
                            .ok_or(StdError::generic_err(format!(
                                "deposit for subaccount {} does not exist",
                                subaccount.as_str()
                            )))?;

                    let valid = need_coins.values().all(|coin| {
                        deposit
                            .iter()
                            .any(|d| d.denom == coin.denom && d.amount >= coin.amount)
                    });
                    if !valid {
                        return Err(StdError::generic_err(format!(
                            "insufficient balance for subaccount. Deposit: {}, but need: {}",
                            deposit.iter().join(", "),
                            need_coins.values().join(", ")
                        ))
                        .into());
                    }
                }
            }
            _ => unimplemented!("not implemented"),
        }

        Ok(AppResponse::default())
    }

    fn sudo<ExecC, QueryC>(
        &self,
        _api: &dyn Api,
        _storage: &mut dyn Storage,
        _router: &dyn CosmosRouter<ExecC = ExecC, QueryC = QueryC>,
        _block: &BlockInfo,
        _msg: Self::SudoT,
    ) -> AnyResult<AppResponse>
    where
        ExecC: Debug + Clone + PartialEq + JsonSchema + DeserializeOwned + 'static,
        QueryC: CustomQuery + DeserializeOwned + 'static,
    {
        unimplemented!("not implemented")
    }

    fn query(
        &self,
        _api: &dyn Api,
        _storage: &dyn Storage,
        _querier: &dyn Querier,
        _block: &BlockInfo,
        request: Self::QueryT,
    ) -> AnyResult<Binary> {
        match request.query_data {
            InjectiveQuery::SpotMarket { market_id } => {
                let markets = self.markets.borrow();
                if let Some((base_denom, quote_denom)) = markets.get(&market_id) {
                    // TODO: save min_quantity_tick_size and min_price_tick_size somewhere if needed
                    // as currently they are hardcoded
                    Ok(to_binary(&SpotMarketResponse {
                        market: Some(SpotMarket {
                            ticker: base_denom.to_string() + "/" + quote_denom,
                            market_id,
                            min_quantity_tick_size: f64_to_dec::<Decimal256>(0.001).conv()?, // from the real INJ/USDT market, 0.001 INJ
                            base_denom: base_denom.clone(),
                            quote_denom: quote_denom.clone(),
                            status: 0,
                            min_price_tick_size: f64_to_dec::<Decimal256>(0.000000000000001)
                                .conv()?, // 0.000000000000001
                            maker_fee_rate: Default::default(),
                            taker_fee_rate: Default::default(),
                            relayer_fee_share_rate: Default::default(),
                        }),
                    })?)
                } else {
                    Ok(to_binary(&SpotMarketResponse { market: None })?)
                }
            }
            InjectiveQuery::SubaccountDeposit {
                subaccount_id,
                denom,
            } => {
                let balance = self
                    .deposit
                    .borrow()
                    .get(&subaccount_id)
                    .map(|v| {
                        v.iter()
                            .find_map(|coin| {
                                if coin.denom == denom {
                                    Some(coin.amount)
                                } else {
                                    None
                                }
                            })
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                Ok(to_binary(&SubaccountDepositResponse {
                    deposits: Deposit {
                        available_balance: FPDecimal::from(balance),
                        total_balance: FPDecimal::from(balance),
                    },
                })?)
            }
            InjectiveQuery::TraderSpotOrders {
                market_id,
                subaccount_id,
            } => {
                let orders = self.orderbook.borrow().get(&market_id).map(|v| {
                    v.iter()
                        .filter_map(|(addr, order)| {
                            if get_subaccount(addr) == subaccount_id {
                                Some(order.clone())
                            } else {
                                None
                            }
                        })
                        .map(|order| TrimmedSpotLimitOrder {
                            price: order.get_price(),
                            quantity: order.get_quantity(),
                            fillable: Default::default(),
                            isBuy: BUY_TYPES.contains(&order.order_type),
                            order_hash: "".to_string(),
                        })
                        .collect::<Vec<_>>()
                });
                Ok(to_binary(&TraderSpotOrdersResponse { orders })?)
            }
            InjectiveQuery::WasmxRegisteredContractInfo { contract_address } => {
                let is_executable = self
                    .enabled_contracts
                    .borrow()
                    .get(&Addr::unchecked(contract_address))
                    .cloned()
                    .ok_or(StdError::generic_err("contract not found"))?;
                Ok(to_binary(&QueryContractRegistrationInfoResponse {
                    contract: Some(RegisteredContract {
                        gas_limit: 0,
                        gas_price: 0,
                        is_executable,
                        code_id: 0,
                        admin_address: "".to_string(),
                    }),
                })?)
            }
            _ => unimplemented!("not implemented"),
        }
    }
}

const BUY_TYPES: [OrderType; 3] = [OrderType::Buy, OrderType::BuyPo, OrderType::BuyAtomic];
