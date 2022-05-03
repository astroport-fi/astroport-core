use std::str::FromStr;

use anyhow::Result as AnyResult;
use cosmwasm_std::testing::{mock_env, MockApi, MockStorage};
use cosmwasm_std::{
    coin, to_binary, Addr, Binary, Coin, Decimal, Deps, DepsMut, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20Coin, Cw20ExecuteMsg, Cw20QueryMsg};
use cw_storage_plus::Item;
use terra_multi_test::{
    AppBuilder, AppResponse, BankKeeper, ContractWrapper, Executor, TerraApp, TerraMock,
};

use astroport::asset::{Asset, AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType};
use astroport::pair_reserve::{
    ConfigResponse, Cw20HookMsg, ExecuteMsg, InitParams, QueryMsg, ReverseSimulationResponse,
    SimulationResponse, UpdateFlowParams, UpdateParams,
};

pub const EXCHANGE_RATE_1: &str = "39000"; // 1 BTC -> 39000 USD
pub const EXCHANGE_RATE_2: &str = "41000"; // 1 BTC -> 41000 USD

pub fn mock_app() -> TerraApp {
    let env = mock_env();
    let api = MockApi::default();
    let bank = BankKeeper::new();
    let storage = MockStorage::new();
    let custom = TerraMock::luna_ust_case();

    AppBuilder::new()
        .with_api(api)
        .with_block(env.block)
        .with_bank(bank)
        .with_storage(storage)
        .with_custom(custom)
        .build()
}

pub trait AssetExt {
    fn with_balance(&self, amount: u128) -> Self;
    fn mock_coin_sent(&self, app: &mut TerraApp, user: &Addr, spender: &Addr) -> Vec<Coin>;
}

impl AssetExt for Asset {
    fn with_balance(&self, amount: u128) -> Self {
        Asset {
            amount: Uint128::from(amount),
            ..self.clone()
        }
    }

    fn mock_coin_sent(&self, app: &mut TerraApp, user: &Addr, spender: &Addr) -> Vec<Coin> {
        let mut funds = vec![];
        match &self.info {
            AssetInfo::Token { contract_addr } => {
                let msg = Cw20ExecuteMsg::IncreaseAllowance {
                    spender: spender.to_string(),
                    amount: self.amount,
                    expires: None,
                };
                app.execute_contract(user.clone(), contract_addr.clone(), &msg, &[])
                    .unwrap();
            }
            AssetInfo::NativeToken { denom } => {
                if !self.amount.is_zero() {
                    funds = vec![coin(self.amount.u128(), denom)];
                    // app.send_tokens(user.clone(), spender.clone(), &funds)
                    //     .unwrap();
                }
            }
        }

        funds
    }
}

pub trait AssetsExt {
    fn with_balances(&self, btc: u128, ust: u128) -> Self;
    fn mock_coins_sent(&self, app: &mut TerraApp, user: &Addr, spender: &Addr) -> Vec<Coin>;
}

impl AssetsExt for [Asset; 2] {
    fn with_balances(&self, btc: u128, ust: u128) -> Self {
        let mut assets = self.clone();
        assets[0].amount = Uint128::from(btc);
        assets[1].amount = Uint128::from(ust);

        assets
    }

    fn mock_coins_sent(&self, app: &mut TerraApp, user: &Addr, pair_contract: &Addr) -> Vec<Coin> {
        let mut funds = vec![];
        for asset in self {
            funds.extend(asset.mock_coin_sent(app, user, pair_contract));
        }
        funds
    }
}

struct OracleMockFactory {
    code: u64,
    owner: Addr,
}

impl<'a> OracleMockFactory {
    const EXCHANGE_RATE: Item<'a, Decimal> = Item::new("exchange_rate"); // BTC/USD

    fn new(app: &mut TerraApp, owner: &Addr) -> Self {
        let contract = Box::new(ContractWrapper::new_with_empty(
            Self::execute,
            Self::instantiate,
            Self::query,
        ));
        let code = app.store_code(contract);
        OracleMockFactory {
            code,
            owner: owner.clone(),
        }
    }

    fn init(&self, app: &mut TerraApp, name: &str, exchange_rate: &str) -> Addr {
        app.instantiate_contract(
            self.code,
            self.owner.clone(),
            &Decimal::from_str(exchange_rate).unwrap(),
            &[],
            String::from(name),
            None,
        )
        .unwrap()
    }

    fn instantiate(
        deps: DepsMut,
        _env: Env,
        _info: MessageInfo,
        msg: Decimal,
    ) -> StdResult<Response> {
        Self::EXCHANGE_RATE.save(deps.storage, &msg)?;
        Ok(Response::default())
    }

    fn query(deps: Deps, _env: Env, _msg: ()) -> StdResult<Binary> {
        to_binary(&Self::EXCHANGE_RATE.load(deps.storage)?)
    }

    fn execute(_deps: DepsMut, _env: Env, _info: MessageInfo, _msg: ()) -> StdResult<Response> {
        unimplemented!()
    }
}

pub struct Helper {
    pub owner: Addr,
    pub pair: Addr,
    pub btc_token: Addr,
    pub assets: [Asset; 2], // (BTC, UST)
    pub lp_token: Addr,
    pub cw20_token: Addr,
    pub oracles: Vec<Addr>,
}

impl Helper {
    const TAX: u128 = 1_390000;

    pub fn init(app: &mut TerraApp, owner: &Addr) -> Self {
        let token_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_token::contract::execute,
            astroport_token::contract::instantiate,
            astroport_token::contract::query,
        ));
        let token_code_id = app.store_code(token_contract);
        let msg = astroport::token::InstantiateMsg {
            name: "BTC".to_string(),
            symbol: "BTC".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: owner.to_string(),
                amount: Uint128::from(1_000_000_000_000000u128),
            }],
            mint: None,
        };
        let btc_token = app
            .instantiate_contract(
                token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("BTC"),
                None,
            )
            .unwrap();

        let msg = astroport::token::InstantiateMsg {
            name: "TOKEN".to_string(),
            symbol: "TOKEN".to_string(),
            decimals: 6,
            initial_balances: vec![Cw20Coin {
                address: owner.to_string(),
                amount: Uint128::from(1_000_000_000_000000u128),
            }],
            mint: None,
        };
        let cw20_token = app
            .instantiate_contract(
                token_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("TOKEN"),
                None,
            )
            .unwrap();

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_reserve_pair::contract::execute,
                astroport_reserve_pair::contract::instantiate,
                astroport_reserve_pair::contract::query,
            )
            .with_reply_empty(astroport_reserve_pair::contract::reply),
        );
        let pair_code = app.store_code(pair_contract);

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );
        let factory_code = app.store_code(factory_contract);
        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![PairConfig {
                code_id: pair_code,
                pair_type: PairType::Reserve {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: true,
            }],
            token_code_id,
            fee_address: Some("fee_address".to_string()),
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 123u64,
        };
        let factory_addr = app
            .instantiate_contract(
                factory_code,
                owner.clone(),
                &msg,
                &[],
                String::from("Astroport Factory"),
                None,
            )
            .unwrap();

        let oracle_factory = OracleMockFactory::new(app, owner);
        let oracle1 = oracle_factory.init(app, "BTC2USD Oracle1", EXCHANGE_RATE_1);
        let oracle2 = oracle_factory.init(app, "BTC2USD Oracle2", EXCHANGE_RATE_2);

        let base_pool = 100_000_000_000000u128;
        let init_params = InitParams {
            pool_params: UpdateParams {
                entry: Some(UpdateFlowParams {
                    base_pool: Uint128::from(base_pool),
                    min_spread: 5,
                    recovery_period: 10,
                }),
                exit: Some(UpdateFlowParams {
                    base_pool: Uint128::from(base_pool),
                    min_spread: 100,
                    recovery_period: 100,
                }),
            },
            oracles: vec![oracle1.to_string(), oracle2.to_string()],
        };
        let asset_infos = [
            AssetInfo::Token {
                contract_addr: btc_token.clone(),
            },
            AssetInfo::NativeToken {
                denom: "uusd".to_string(),
            },
        ];
        let create_pair_msg = astroport::factory::ExecuteMsg::PermissionedCreatePair {
            pair_type: PairType::Reserve {},
            asset_infos: asset_infos.clone(),
            init_params: Some(to_binary(&init_params).unwrap()),
        };
        app.execute_contract(owner.clone(), factory_addr.clone(), &create_pair_msg, &[])
            .unwrap();
        let pair_info: PairInfo = app
            .wrap()
            .query_wasm_smart(
                &factory_addr,
                &astroport::factory::QueryMsg::Pair { asset_infos },
            )
            .unwrap();

        Self {
            owner: owner.clone(),
            pair: pair_info.contract_addr,
            btc_token: btc_token.clone(),
            assets: [
                Asset {
                    info: AssetInfo::Token {
                        contract_addr: btc_token,
                    },
                    amount: Default::default(),
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: "uusd".to_string(),
                    },
                    amount: Default::default(),
                },
            ],
            lp_token: pair_info.liquidity_token,
            cw20_token,
            oracles: vec![oracle1, oracle2],
        }
    }

    pub fn give_coins(&self, app: &mut TerraApp, user: &str, asset: &Asset) {
        match &asset.info {
            AssetInfo::Token { contract_addr } => {
                let msg = Cw20ExecuteMsg::Transfer {
                    recipient: user.to_string(),
                    amount: asset.amount,
                };
                app.execute_contract(self.owner.clone(), contract_addr.clone(), &msg, &[])
                    .unwrap();
            }
            AssetInfo::NativeToken { denom } => {
                app.init_bank_balance(
                    &Addr::unchecked(user),
                    // Giving 1.39 ust to pay tax
                    vec![coin(asset.amount.u128() + Self::TAX, denom)],
                )
                .unwrap();
            }
        }
    }

    pub fn provide_liquidity(
        &self,
        app: &mut TerraApp,
        user: &str,
        assets: [Asset; 2],
        receiver: Option<&str>,
    ) -> AnyResult<AppResponse> {
        let user = Addr::unchecked(user);

        assets.mock_coins_sent(app, &user, &self.pair);
        let msg = ExecuteMsg::ProvideLiquidity {
            assets,
            slippage_tolerance: None,
            receiver: receiver.map(str::to_string),
        };

        app.execute_contract(user, self.pair.clone(), &msg, &[])
    }

    pub fn withdraw_liquidity(
        &self,
        app: &mut TerraApp,
        user: &str,
        amount: u128,
    ) -> AnyResult<AppResponse> {
        let user = Addr::unchecked(user);
        let withdraw_msg = Cw20ExecuteMsg::Send {
            contract: self.pair.to_string(),
            amount: Uint128::from(amount),
            msg: to_binary(&Cw20HookMsg::WithdrawLiquidity {}).unwrap(),
        };
        app.execute_contract(user, self.lp_token.clone(), &withdraw_msg, &[])
    }

    pub fn update_whitelist(
        &self,
        app: &mut TerraApp,
        user: &str,
        add: Vec<&str>,
        remove: Vec<&str>,
    ) -> AnyResult<AppResponse> {
        let user = Addr::unchecked(user);
        let append_addrs = add.iter().map(|addr| addr.to_string()).collect();
        let remove_addrs = remove.iter().map(|addr| addr.to_string()).collect();
        let msg = ExecuteMsg::UpdateProvidersWhitelist {
            append_addrs,
            remove_addrs,
        };
        app.execute_contract(user, self.pair.clone(), &msg, &[])
    }

    pub fn update_oracles(
        &self,
        app: &mut TerraApp,
        user: &str,
        add: Vec<&str>,
        remove: Vec<&str>,
    ) -> AnyResult<AppResponse> {
        let user = Addr::unchecked(user);
        let append_addrs = add.iter().map(|addr| addr.to_string()).collect();
        let remove_addrs = remove.iter().map(|addr| addr.to_string()).collect();
        let msg = ExecuteMsg::UpdateOracles {
            append_addrs,
            remove_addrs,
        };
        app.execute_contract(user, self.pair.clone(), &msg, &[])
    }

    pub fn get_token_balance(
        &self,
        app: &mut TerraApp,
        token: &Addr,
        user: &str,
    ) -> AnyResult<u128> {
        let msg = Cw20QueryMsg::Balance {
            address: user.to_string(),
        };
        let balance: BalanceResponse = app.wrap().query_wasm_smart(token, &msg)?;
        Ok(balance.balance.u128())
    }

    pub fn get_config(&self, app: &mut TerraApp) -> AnyResult<ConfigResponse> {
        let config: ConfigResponse = app
            .wrap()
            .query_wasm_smart(&self.pair, &QueryMsg::Config {})?;
        Ok(config)
    }

    pub fn native_swap(
        &self,
        app: &mut TerraApp,
        user: &str,
        offer_asset: &Asset,
        mock_sent: bool,
    ) -> AnyResult<AppResponse> {
        let user = Addr::unchecked(user);
        let mut funds = vec![];
        if mock_sent {
            funds = offer_asset.mock_coin_sent(app, &user, &self.pair);
        }
        let msg = ExecuteMsg::Swap {
            offer_asset: offer_asset.clone(),
            belief_price: None,
            max_spread: None,
            to: None,
        };
        app.execute_contract(user, self.pair.clone(), &msg, &funds)
    }

    pub fn cw20_swap(
        &self,
        app: &mut TerraApp,
        user: &str,
        offer_asset: &Asset,
    ) -> AnyResult<AppResponse> {
        if let AssetInfo::Token { contract_addr } = &offer_asset.info {
            let cw20_msg = Cw20ExecuteMsg::Send {
                contract: self.pair.to_string(),
                amount: offer_asset.amount,
                msg: to_binary(&Cw20HookMsg::Swap {
                    belief_price: None,
                    max_spread: None,
                    to: None,
                })
                .unwrap(),
            };
            app.execute_contract(Addr::unchecked(user), contract_addr.clone(), &cw20_msg, &[])
        } else {
            unimplemented!()
        }
    }

    pub fn query_simulation(
        &self,
        app: &mut TerraApp,
        offer_asset: &Asset,
    ) -> AnyResult<SimulationResponse> {
        let msg = QueryMsg::Simulation {
            offer_asset: offer_asset.clone(),
        };
        let res: SimulationResponse = app.wrap().query_wasm_smart(&self.pair, &msg)?;
        Ok(res)
    }

    pub fn query_reverse_simulation(
        &self,
        app: &mut TerraApp,
        ask_asset: &Asset,
    ) -> AnyResult<ReverseSimulationResponse> {
        let msg = QueryMsg::ReverseSimulation {
            ask_asset: ask_asset.clone(),
        };
        let res: ReverseSimulationResponse = app.wrap().query_wasm_smart(&self.pair, &msg)?;
        Ok(res)
    }
}
