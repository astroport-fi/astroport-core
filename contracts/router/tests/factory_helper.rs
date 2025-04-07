#![cfg(not(tarpaulin_include))]

use anyhow::Result as AnyResult;
use cosmwasm_std::{coins, Addr, Binary};
use cw20::MinterResponse;

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType, QueryMsg};
use astroport_test::cw_multi_test::{AppResponse, ContractWrapper, Executor};
use astroport_test::modules::stargate::StargateApp as App;

pub struct FactoryHelper {
    pub owner: Addr,
    pub factory: Addr,
    pub coin_registry: Addr,
    pub cw20_token_code_id: u64,
}

impl FactoryHelper {
    pub fn init(router: &mut App, owner: &Addr) -> Self {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        ));

        let cw20_token_code_id = router.store_code(astro_token_contract);

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair::contract::execute,
                astroport_pair::contract::instantiate,
                astroport_pair::contract::query,
            )
            .with_reply_empty(astroport_pair::contract::reply),
        );

        let pair_code_id = router.store_code(pair_contract);

        let pcl_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair_concentrated::contract::execute,
                astroport_pair_concentrated::contract::instantiate,
                astroport_pair_concentrated::queries::query,
            )
            .with_reply_empty(astroport_pair_concentrated::contract::reply),
        );
        let pcl_code_id = router.store_code(pcl_contract);

        let coin_registry_contract = Box::new(ContractWrapper::new_with_empty(
            astroport_native_coin_registry::contract::execute,
            astroport_native_coin_registry::contract::instantiate,
            astroport_native_coin_registry::contract::query,
        ));
        let coin_registry_code_id = router.store_code(coin_registry_contract);

        let coin_registry = router
            .instantiate_contract(
                coin_registry_code_id,
                owner.clone(),
                &astroport::native_coin_registry::InstantiateMsg {
                    owner: owner.to_string(),
                },
                &[],
                "coin_registry",
                None,
            )
            .unwrap();

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );

        let factory_code_id = router.store_code(factory_contract);

        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![
                PairConfig {
                    code_id: pair_code_id,
                    pair_type: PairType::Xyk {},
                    total_fee_bps: 0,
                    maker_fee_bps: 0,
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                },
                PairConfig {
                    code_id: pair_code_id,
                    pair_type: PairType::Stable {},
                    total_fee_bps: 0,
                    maker_fee_bps: 0,
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                },
                PairConfig {
                    code_id: pcl_code_id,
                    maker_fee_bps: 5000,
                    total_fee_bps: 0u16, // Concentrated pair does not use this field,
                    pair_type: PairType::Custom("concentrated".to_string()),
                    is_disabled: false,
                    is_generator_disabled: false,
                    permissioned: false,
                },
            ],
            token_code_id: cw20_token_code_id,
            fee_address: None,
            generator_address: None,
            owner: owner.to_string(),
            whitelist_code_id: 0,
            coin_registry_address: coin_registry.to_string(),
            tracker_config: None,
        };

        let factory = router
            .instantiate_contract(
                factory_code_id,
                owner.clone(),
                &msg,
                &[],
                String::from("ASTRO"),
                None,
            )
            .unwrap();

        Self {
            owner: owner.clone(),
            factory,
            coin_registry,
            cw20_token_code_id,
        }
    }

    pub fn create_pair(
        &mut self,
        app: &mut App,
        sender: &Addr,
        pair_type: PairType,
        asset_infos: [AssetInfo; 2],
        init_params: Option<Binary>,
    ) -> AnyResult<Addr> {
        let msg = astroport::factory::ExecuteMsg::CreatePair {
            pair_type,
            asset_infos: asset_infos.to_vec(),
            init_params,
        };

        for asset_info in &asset_infos {
            match &asset_info {
                AssetInfo::Token { .. } => {}
                AssetInfo::NativeToken { denom } => {
                    app.execute_contract(
                        self.owner.clone(),
                        self.coin_registry.clone(),
                        &astroport::native_coin_registry::ExecuteMsg::Add {
                            native_coins: vec![(denom.clone(), 6)],
                        },
                        &[],
                    )?;
                }
            }
        }

        app.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])?;

        let res: PairInfo = app.wrap().query_wasm_smart(
            self.factory.clone(),
            &QueryMsg::Pair {
                asset_infos: asset_infos.to_vec(),
            },
        )?;

        Ok(res.contract_addr)
    }
}

pub fn instantiate_token(
    app: &mut App,
    token_code_id: u64,
    owner: &Addr,
    token_name: &str,
    decimals: Option<u8>,
) -> Addr {
    let init_msg = astroport::token::InstantiateMsg {
        name: token_name.to_string(),
        symbol: token_name.to_string(),
        decimals: decimals.unwrap_or(6),
        initial_balances: vec![],
        mint: Some(MinterResponse {
            minter: owner.to_string(),
            cap: None,
        }),
        marketing: None,
    };

    app.instantiate_contract(
        token_code_id,
        owner.clone(),
        &init_msg,
        &[],
        token_name,
        None,
    )
    .unwrap()
}

pub fn mint(
    app: &mut App,
    owner: &Addr,
    token: &Addr,
    amount: u128,
    receiver: &Addr,
) -> AnyResult<AppResponse> {
    app.execute_contract(
        owner.clone(),
        token.clone(),
        &cw20::Cw20ExecuteMsg::Mint {
            recipient: receiver.to_string(),
            amount: amount.into(),
        },
        &[],
    )
}

pub fn mint_native(
    app: &mut App,
    denom: &str,
    amount: u128,
    receiver: &Addr,
) -> AnyResult<AppResponse> {
    // .init_balance() erases previous balance thus we use such hack and create intermediate "denom admin"
    let denom_admin = Addr::unchecked(format!("{denom}_admin"));
    let coins_vec = coins(amount, denom);
    app.init_modules(|router, _, storage| {
        router
            .bank
            .init_balance(storage, &denom_admin, coins_vec.clone())
    })
    .unwrap();

    app.send_tokens(denom_admin, receiver.clone(), &coins_vec)
}
