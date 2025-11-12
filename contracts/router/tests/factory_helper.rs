use anyhow::Result as AnyResult;
use cosmwasm_std::{coins, Addr, Binary};
use cw20::MinterResponse;

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::{PairConfig, PairType, QueryMsg};
use astroport_test::cw_multi_test::{AppResponse, ContractWrapper, Executor};
use astroport_test::modules::stargate::StargateApp;

pub type App = StargateApp;

pub struct FactoryHelper {
    pub factory: Addr,
    pub cw20_token_code_id: u64,
    pub owner: Addr,
}

impl FactoryHelper {
    pub fn init(app: &mut App) -> Self {
        let astro_token_contract = Box::new(ContractWrapper::new_with_empty(
            cw20_base::contract::execute,
            cw20_base::contract::instantiate,
            cw20_base::contract::query,
        ));

        let cw20_token_code_id = app.store_code(astro_token_contract);

        let pair_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_pair::contract::execute,
                astroport_pair::contract::instantiate,
                astroport_pair::contract::query,
            )
            .with_reply_empty(astroport_pair::contract::reply),
        );

        let pair_code_id = app.store_code(pair_contract);

        let factory_contract = Box::new(
            ContractWrapper::new_with_empty(
                astroport_factory::contract::execute,
                astroport_factory::contract::instantiate,
                astroport_factory::contract::query,
            )
            .with_reply_empty(astroport_factory::contract::reply),
        );

        let factory_code_id = app.store_code(factory_contract);

        let owner = app.api().addr_make("owner");

        let msg = astroport::factory::InstantiateMsg {
            pair_configs: vec![PairConfig {
                code_id: pair_code_id,
                pair_type: PairType::Xyk {},
                total_fee_bps: 0,
                maker_fee_bps: 0,
                is_disabled: false,
                is_generator_disabled: false,
                permissioned: false,
                whitelist: None,
            }],
            token_code_id: cw20_token_code_id,
            fee_address: None,
            owner: owner.to_string(),
            coin_registry_address: app.api().addr_make("coin_registry").to_string(),
            generator_address: None,
            creation_fee: None,
        };

        let factory = app
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
            factory,
            cw20_token_code_id,
            owner,
        }
    }

    pub fn create_pair(
        &mut self,
        router: &mut App,
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

        router.execute_contract(sender.clone(), self.factory.clone(), &msg, &[])?;

        self.query_pair_by_asset_infos(router, &asset_infos)
    }

    pub fn query_pair_by_asset_infos(
        &self,
        app: &App,
        asset_infos: &[AssetInfo],
    ) -> AnyResult<Addr> {
        let res: Vec<PairInfo> = app.wrap().query_wasm_smart(
            &self.factory,
            &QueryMsg::PairsByAssetInfos {
                asset_infos: asset_infos.to_vec(),
                start_after: None,
                limit: None,
            },
        )?;

        Ok(res[0].contract_addr.clone())
    }
}

pub fn instantiate_token(
    app: &mut App,
    token_code_id: u64,
    owner: &Addr,
    token_name: &str,
    decimals: Option<u8>,
) -> Addr {
    let init_msg = cw20_base::msg::InstantiateMsg {
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
