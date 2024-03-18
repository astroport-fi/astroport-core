use anyhow::Result as AnyResult;
use cw_utils::Duration;
use std::fmt::Debug;

use crate::{astroport_address, WKApp, ASTROPORT};
use astroport::asset::{Asset, AssetInfo};
use astroport::pair::ExecuteMsg as PairExecuteMsg;
use astroport::shared_multisig::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, PoolType, ProvideParams, QueryMsg,
};

use cosmwasm_std::{Addr, Api, Coin, CosmosMsg, CustomQuery, Decimal, StdResult, Storage, Uint128};
use cw20::{BalanceResponse, Cw20QueryMsg};
use cw3::{ProposalResponse, Vote, VoteListResponse, VoteResponse};
use cw_multi_test::{
    AppResponse, Bank, ContractWrapper, Distribution, Executor, Gov, Ibc, Module, Staking, Stargate,
};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;

pub fn store_code<B, A, S, C, X, D, I, G, T>(app: &WKApp<B, A, S, C, X, D, I, G, T>) -> u64
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    let contract = Box::new(ContractWrapper::new_with_empty(
        astroport_shared_multisig::contract::execute,
        astroport_shared_multisig::contract::instantiate,
        astroport_shared_multisig::contract::query,
    ));

    app.borrow_mut().store_code(contract)
}

pub struct MockSharedMultisigBuilder<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
}

impl<B, A, S, C, X, D, I, G, T> MockSharedMultisigBuilder<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    pub fn new(app: &WKApp<B, A, S, C, X, D, I, G, T>) -> Self {
        Self { app: app.clone() }
    }

    pub fn instantiate(
        self,
        factory_addr: &Addr,
        generator_addr: Option<Addr>,
        target_pool: Option<String>,
    ) -> MockSharedMultisig<B, A, S, C, X, D, I, G, T> {
        let code_id = store_code(&self.app);
        let astroport = astroport_address();

        let address = self
            .app
            .borrow_mut()
            .instantiate_contract(
                code_id,
                astroport,
                &InstantiateMsg {
                    factory_addr: factory_addr.to_string(),
                    generator_addr: generator_addr
                        .unwrap_or(Addr::unchecked("generator_addr"))
                        .to_string(),
                    max_voting_period: Duration::Height(3),
                    manager1: "manager1".to_string(),
                    manager2: "manager2".to_string(),
                    denom1: "untrn".to_string(),
                    denom2: "ibc/astro".to_string(),
                    target_pool,
                },
                &[],
                "Astroport Shared Multisig",
                Some(ASTROPORT.to_owned()),
            )
            .unwrap();

        MockSharedMultisig {
            app: self.app,
            address,
        }
    }
}

pub struct MockSharedMultisig<B, A, S, C: Module, X, D, I, G, T> {
    pub app: WKApp<B, A, S, C, X, D, I, G, T>,
    pub address: Addr,
}

impl<B, A, S, C, X, D, I, G, T> MockSharedMultisig<B, A, S, C, X, D, I, G, T>
where
    B: Bank,
    A: Api,
    S: Storage,
    C: Module,
    X: Staking,
    D: Distribution,
    I: Ibc,
    G: Gov,
    C::ExecT: Clone + Debug + PartialEq + JsonSchema + DeserializeOwned + 'static,
    C::QueryT: CustomQuery + DeserializeOwned + 'static,
    T: Stargate,
{
    pub fn propose(&self, sender: &Addr, msgs: Vec<CosmosMsg>) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::Propose {
                title: "Create a new proposal".to_string(),
                description: "Create a new proposal".to_string(),
                msgs,
                latest: None,
            },
            &[],
        )
    }

    pub fn vote(&self, sender: &Addr, proposal_id: u64, vote: Vote) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::Vote { proposal_id, vote },
            &[],
        )
    }

    pub fn execute(&self, sender: &Addr, proposal_id: u64) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::Execute { proposal_id },
            &[],
        )
    }

    pub fn transfer(
        &self,
        sender: &Addr,
        asset: Asset,
        recipient: Option<String>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::Transfer { asset, recipient },
            &[],
        )
    }

    pub fn setup_max_voting_period(
        &self,
        sender: &Addr,
        max_voting_period: Duration,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::SetupMaxVotingPeriod { max_voting_period },
            &[],
        )
    }

    pub fn start_rage_quit(&self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::StartRageQuit {},
            &[],
        )
    }

    pub fn complete_target_pool_migration(&self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::CompleteTargetPoolMigration {},
            &[],
        )
    }

    pub fn setup_pools(
        &self,
        sender: &Addr,
        target_pool: Option<String>,
        migration_pool: Option<String>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::SetupPools {
                target_pool,
                migration_pool,
            },
            &[],
        )
    }

    pub fn provide(
        &self,
        sender: &Addr,
        pool_type: PoolType,
        assets: Option<Vec<Asset>>,
        slippage_tolerance: Option<Decimal>,
        auto_stake: Option<bool>,
        receiver: Option<String>,
    ) -> AnyResult<AppResponse> {
        let assets = if let Some(assets) = assets {
            assets
        } else {
            vec![
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: "untrn".to_string(),
                    },
                    amount: Uint128::new(100_000_000),
                },
                Asset {
                    info: AssetInfo::NativeToken {
                        denom: "ibc/astro".to_string(),
                    },
                    amount: Uint128::new(100_000_000),
                },
            ]
        };

        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::ProvideLiquidity {
                pool_type,
                assets,
                slippage_tolerance,
                auto_stake,
                receiver,
            },
            &[],
        )
    }

    pub fn withdraw(
        &self,
        sender: &Addr,
        withdraw_amount: Option<Uint128>,
        provide_params: Option<ProvideParams>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::WithdrawTargetPoolLP {
                withdraw_amount,
                provide_params,
            },
            &[],
        )
    }

    pub fn withdraw_ragequit(
        &self,
        sender: &Addr,
        pool_type: PoolType,
        withdraw_amount: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::WithdrawRageQuitLP {
                pool_type,
                withdraw_amount,
            },
            &[],
        )
    }

    pub fn deposit_generator(
        &self,
        sender: &Addr,
        amount: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::DepositGenerator { amount },
            &[],
        )
    }

    pub fn withdraw_generator(
        &self,
        sender: &Addr,
        amount: Option<Uint128>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::WithdrawGenerator { amount },
            &[],
        )
    }

    pub fn claim_generator_rewards(&self, sender: &Addr) -> AnyResult<AppResponse> {
        self.app.borrow_mut().execute_contract(
            sender.clone(),
            self.address.clone(),
            &ExecuteMsg::ClaimGeneratorRewards {},
            &[],
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn swap(
        &self,
        sender: &Addr,
        pair: &Addr,
        denom: &String,
        amount: u64,
        ask_asset_info: Option<AssetInfo>,
        belief_price: Option<Decimal>,
        max_spread: Option<Decimal>,
        to: Option<String>,
    ) -> AnyResult<AppResponse> {
        let msg = PairExecuteMsg::Swap {
            offer_asset: Asset {
                info: AssetInfo::NativeToken {
                    denom: denom.clone(),
                },
                amount: Uint128::from(amount),
            },
            ask_asset_info,
            belief_price,
            max_spread,
            to,
        };

        let send_funds = vec![Coin {
            denom: denom.to_owned(),
            amount: Uint128::from(amount),
        }];

        self.app
            .borrow_mut()
            .execute_contract(sender.clone(), pair.clone(), &msg, &send_funds)
    }

    pub fn query_config(&self) -> StdResult<ConfigResponse> {
        self.app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Config {})
    }

    pub fn query_vote(&self, proposal_id: u64, voter: &Addr) -> StdResult<VoteResponse> {
        self.app.borrow().wrap().query_wasm_smart(
            self.address.clone(),
            &QueryMsg::Vote {
                proposal_id,
                voter: voter.to_string(),
            },
        )
    }

    pub fn query_votes(&self, proposal_id: u64) -> StdResult<VoteListResponse> {
        self.app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::ListVotes { proposal_id })
    }

    pub fn query_proposal(&self, proposal_id: u64) -> StdResult<ProposalResponse> {
        self.app
            .borrow()
            .wrap()
            .query_wasm_smart(self.address.clone(), &QueryMsg::Proposal { proposal_id })
    }

    pub fn query_native_balance(&self, account: Option<&str>, denom: &str) -> StdResult<Coin> {
        self.app
            .borrow()
            .wrap()
            .query_balance(account.unwrap_or(self.address.as_str()), denom.to_owned())
    }

    pub fn query_cw20_balance(
        &self,
        lp_token: &Addr,
        account: Option<Addr>,
    ) -> StdResult<BalanceResponse> {
        self.app
            .borrow()
            .wrap()
            .query_wasm_smart::<BalanceResponse>(
                lp_token.as_str(),
                &Cw20QueryMsg::Balance {
                    address: account.unwrap_or(self.address.clone()).to_string(),
                },
            )
    }

    pub fn send_tokens(
        &self,
        owner: &Addr,
        denoms: Option<Vec<Coin>>,
        recipient: Option<Addr>,
    ) -> AnyResult<AppResponse> {
        self.app.borrow_mut().send_tokens(
            owner.clone(),
            recipient.unwrap_or(self.address.clone()),
            &denoms.unwrap_or(vec![
                Coin {
                    denom: String::from("untrn"),
                    amount: Uint128::new(900_000_000u128),
                },
                Coin {
                    denom: String::from("ibc/astro"),
                    amount: Uint128::new(900_000_000u128),
                },
                Coin {
                    denom: String::from("usdt"),
                    amount: Uint128::new(900_000_000u128),
                },
            ]),
        )
    }
}
