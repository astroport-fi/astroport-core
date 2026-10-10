use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, Binary, Coin, Deps, DepsMut, Empty, Env, MessageInfo,
    Response, StdResult, Uint128,
};
use cw20::{BalanceResponse, Cw20ReceiveMsg, TokenInfoResponse};
use cw_storage_plus::Item;

use astroport::asset::AssetInfo;
use astroport::incentives::ExecuteMsg as IncentivesExecuteMsg;
use astroport_test::cw_multi_test::{
    App, AppBuilder, AppResponse, BankSudo, Contract, ContractWrapper, Executor,
};
use astroport_yastro::error::ContractError;
use astroport_yastro::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, StakingState};
use astroport_yastro::state::{Config, Unbonding};

const ASTRO: &str = "ibc/astro";
const USDC: &str = "ibc/usdc";
const FEE: &str = "uluna";
const WEEK: u64 = 7 * 86400;

// Incentives stand-in: records each incentivize call with the funds it came with
const CALLS: Item<Vec<(String, Coin, Vec<Coin>)>> = Item::new("calls");

fn mock_incentives() -> Box<dyn Contract<Empty>> {
    fn execute(
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: IncentivesExecuteMsg,
    ) -> StdResult<Response> {
        if let IncentivesExecuteMsg::Incentivize { lp_token, schedule } = msg {
            let AssetInfo::NativeToken { denom } = schedule.reward.info else {
                panic!("native rewards only");
            };
            let reward = coin(schedule.reward.amount.u128(), denom);
            let mut calls = CALLS.may_load(deps.storage)?.unwrap_or_default();
            calls.push((lp_token, reward, info.funds));
            CALLS.save(deps.storage, &calls)?;
        }
        Ok(Response::new())
    }
    fn instantiate(deps: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        CALLS.save(deps.storage, &vec![])?;
        Ok(Response::new())
    }
    fn query(deps: Deps, _: Env, _: Empty) -> StdResult<Binary> {
        to_json_binary(&CALLS.load(deps.storage)?)
    }
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

// A contract that accepts yASTRO sent to it
#[cosmwasm_schema::cw_serde]
enum ReceiverMsg {
    Receive(Cw20ReceiveMsg),
}

fn mock_receiver() -> Box<dyn Contract<Empty>> {
    fn execute(_: DepsMut, _: Env, _: MessageInfo, _: ReceiverMsg) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn instantiate(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn query(_: Deps, _: Env, _: Empty) -> StdResult<Binary> {
        to_json_binary(&())
    }
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

fn yastro_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_yastro::contract::execute,
            astroport_yastro::contract::instantiate,
            astroport_yastro::contract::query,
        )
        .with_migrate_empty(astroport_yastro::contract::migrate),
    )
}

struct Suite {
    app: App,
    yastro: Addr,
    incentives: Addr,
}

fn owner() -> Addr {
    Addr::unchecked("owner")
}

impl Suite {
    fn new() -> Self {
        let mut app = AppBuilder::new().build(|_, _, _| {});
        let incentives_code = app.store_code(mock_incentives());
        let incentives = app
            .instantiate_contract(incentives_code, owner(), &Empty {}, &[], "incentives", None)
            .unwrap();
        let code = app.store_code(yastro_contract());
        let yastro = app
            .instantiate_contract(
                code,
                owner(),
                &InstantiateMsg {
                    owner: owner().to_string(),
                    astro_denom: ASTRO.to_string(),
                    unbonding_period: WEEK,
                    reward_denoms: vec![USDC.to_string()],
                    incentives: Some(incentives.to_string()),
                    name: "Staked ASTRO".to_string(),
                    symbol: "yASTRO".to_string(),
                    decimals: 6,
                },
                &[],
                "yastro",
                Some(owner().to_string()),
            )
            .unwrap();
        Suite {
            app,
            yastro,
            incentives,
        }
    }

    fn mint(&mut self, to: &str, amount: u128, denom: &str) {
        self.app
            .sudo(
                BankSudo::Mint {
                    to_address: to.to_string(),
                    amount: coins(amount, denom),
                }
                .into(),
            )
            .unwrap();
    }

    fn exec(
        &mut self,
        sender: &str,
        msg: &ExecuteMsg,
        funds: &[Coin],
    ) -> anyhow::Result<AppResponse> {
        self.app
            .execute_contract(Addr::unchecked(sender), self.yastro.clone(), msg, funds)
    }

    fn stake(&mut self, who: &str, amount: u128) {
        self.mint(who, amount, ASTRO);
        self.exec(
            who,
            &ExecuteMsg::Stake { recipient: None },
            &coins(amount, ASTRO),
        )
        .unwrap();
    }

    fn deposit(&mut self, amount: u128) {
        self.mint("maker", amount, USDC);
        self.exec(
            "maker",
            &ExecuteMsg::DepositRewards {},
            &coins(amount, USDC),
        )
        .unwrap();
    }

    fn pending(&self, who: &str) -> Vec<Coin> {
        self.app
            .wrap()
            .query_wasm_smart(
                &self.yastro,
                &QueryMsg::PendingRewards {
                    address: who.to_string(),
                },
            )
            .unwrap()
    }

    fn pending_usdc(&self, who: &str) -> u128 {
        self.pending(who)
            .iter()
            .find(|c| c.denom == USDC)
            .map(|c| c.amount.u128())
            .unwrap_or(0)
    }

    fn balance(&self, who: &str) -> u128 {
        let res: BalanceResponse = self
            .app
            .wrap()
            .query_wasm_smart(
                &self.yastro,
                &QueryMsg::Balance {
                    address: who.to_string(),
                },
            )
            .unwrap();
        res.balance.u128()
    }

    fn bank(&self, who: &str, denom: &str) -> u128 {
        self.app
            .wrap()
            .query_balance(who, denom)
            .unwrap()
            .amount
            .u128()
    }

    fn staking_state(&self) -> StakingState {
        self.app
            .wrap()
            .query_wasm_smart(&self.yastro, &QueryMsg::StakingState {})
            .unwrap()
    }

    /// The contract always holds exactly the staked plus unbonding ASTRO
    fn assert_backed(&self) {
        let s = self.staking_state();
        assert_eq!(
            self.bank(self.yastro.as_str(), ASTRO),
            s.total_staked.u128() + s.total_unbonding.u128()
        );
    }

    fn skip(&mut self, seconds: u64) {
        self.app.update_block(|b| {
            b.time = b.time.plus_seconds(seconds);
            b.height += 1;
        });
    }
}

fn err(e: anyhow::Error) -> ContractError {
    e.downcast().unwrap()
}

#[test]
fn staking_mints_yastro_one_to_one() {
    let mut s = Suite::new();
    s.stake("alice", 1_000);
    s.mint("bob", 500, ASTRO);
    s.exec(
        "bob",
        &ExecuteMsg::Stake {
            recipient: Some("carol".to_string()),
        },
        &coins(500, ASTRO),
    )
    .unwrap();

    assert_eq!(s.balance("alice"), 1_000);
    assert_eq!(s.balance("bob"), 0);
    assert_eq!(s.balance("carol"), 500);
    let info: TokenInfoResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::TokenInfo {})
        .unwrap();
    assert_eq!(info.symbol, "yASTRO");
    assert_eq!(info.total_supply.u128(), 1_500);
    s.assert_backed();

    // only ASTRO stakes
    s.mint("bob", 10, USDC);
    let e = s
        .exec(
            "bob",
            &ExecuteMsg::Stake { recipient: None },
            &coins(10, USDC),
        )
        .unwrap_err();
    assert!(matches!(err(e), ContractError::Payment(_)));
}

#[test]
fn rewards_split_pro_rata_and_are_claimed() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    s.stake("bob", 300);
    s.deposit(400);

    assert_eq!(s.pending_usdc("alice"), 100);
    assert_eq!(s.pending_usdc("bob"), 300);

    s.exec("alice", &ExecuteMsg::Claim { recipient: None }, &[])
        .unwrap();
    assert_eq!(s.bank("alice", USDC), 100);
    assert_eq!(s.pending_usdc("alice"), 0);

    // claiming again with nothing accrued
    let e = s
        .exec("alice", &ExecuteMsg::Claim { recipient: None }, &[])
        .unwrap_err();
    assert_eq!(err(e), ContractError::NothingToClaim {});

    // to another address
    s.exec(
        "bob",
        &ExecuteMsg::Claim {
            recipient: Some("bob-cold".to_string()),
        },
        &[],
    )
    .unwrap();
    assert_eq!(s.bank("bob-cold", USDC), 300);
}

#[test]
fn transfers_settle_both_sides_first() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    s.stake("bob", 300);
    s.deposit(400);

    // alice moves half her yASTRO; what she earned before stays hers
    s.exec(
        "alice",
        &ExecuteMsg::Transfer {
            recipient: "carol".to_string(),
            amount: Uint128::new(50),
        },
        &[],
    )
    .unwrap();
    assert_eq!(s.pending_usdc("alice"), 100);
    assert_eq!(s.pending_usdc("carol"), 0);

    // the next deposit follows the new balances: 50 / 300 / 50
    s.deposit(400);
    assert_eq!(s.pending_usdc("alice"), 150);
    assert_eq!(s.pending_usdc("bob"), 600);
    assert_eq!(s.pending_usdc("carol"), 50);

    // transfer_from through an allowance settles the owner and the recipient too
    s.exec(
        "bob",
        &ExecuteMsg::IncreaseAllowance {
            spender: "dex".to_string(),
            amount: Uint128::new(300),
            expires: None,
        },
        &[],
    )
    .unwrap();
    s.exec(
        "dex",
        &ExecuteMsg::TransferFrom {
            owner: "bob".to_string(),
            recipient: "dave".to_string(),
            amount: Uint128::new(300),
        },
        &[],
    )
    .unwrap();
    s.deposit(100);
    assert_eq!(s.pending_usdc("bob"), 600);
    assert_eq!(s.pending_usdc("dave"), 75);
    assert_eq!(s.pending_usdc("alice"), 162);
    assert_eq!(s.pending_usdc("carol"), 62);
}

#[test]
fn contracts_holding_yastro_earn_like_anyone() {
    let mut s = Suite::new();
    let code = s.app.store_code(mock_receiver());
    let pool = s
        .app
        .instantiate_contract(code, owner(), &Empty {}, &[], "pool", None)
        .unwrap();
    s.stake("alice", 200);
    s.exec(
        "alice",
        &ExecuteMsg::Send {
            contract: pool.to_string(),
            amount: Uint128::new(100),
            msg: Binary::default(),
        },
        &[],
    )
    .unwrap();
    s.deposit(200);
    assert_eq!(s.pending_usdc("alice"), 100);
    assert_eq!(s.pending_usdc(pool.as_str()), 100);
}

#[test]
fn deposits_with_no_holders_carry_over() {
    let mut s = Suite::new();
    s.deposit(50);
    s.stake("alice", 100);
    assert_eq!(s.pending_usdc("alice"), 0);
    // the held 50 goes out with the next deposit
    s.deposit(10);
    assert_eq!(s.pending_usdc("alice"), 60);
}

#[test]
fn only_reward_denoms_are_accepted() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    s.mint("maker", 10, FEE);
    let e = s
        .exec("maker", &ExecuteMsg::DepositRewards {}, &coins(10, FEE))
        .unwrap_err();
    assert_eq!(
        err(e),
        ContractError::NotARewardDenom {
            denom: FEE.to_string()
        }
    );
    let e = s
        .exec("maker", &ExecuteMsg::DepositRewards {}, &[])
        .unwrap_err();
    assert!(matches!(err(e), ContractError::Payment(_)));

    // ASTRO is the staked asset and can't be a reward
    let e = s
        .exec(
            "owner",
            &ExecuteMsg::UpdateConfig {
                add_reward_denoms: Some(vec![ASTRO.to_string()]),
                remove_reward_denoms: None,
                unbonding_period: None,
                incentives: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err(e),
        ContractError::StakedDenomAsReward {
            denom: ASTRO.to_string()
        }
    );

    // removing a denom stops deposits but keeps what's owed claimable
    s.deposit(100);
    s.exec(
        "owner",
        &ExecuteMsg::UpdateConfig {
            add_reward_denoms: None,
            remove_reward_denoms: Some(vec![USDC.to_string()]),
            unbonding_period: None,
            incentives: None,
        },
        &[],
    )
    .unwrap();
    s.mint("maker", 10, USDC);
    assert!(s
        .exec("maker", &ExecuteMsg::DepositRewards {}, &coins(10, USDC))
        .is_err());
    s.exec("alice", &ExecuteMsg::Claim { recipient: None }, &[])
        .unwrap();
    assert_eq!(s.bank("alice", USDC), 100);
}

#[test]
fn unstaking_unbonds_for_a_week_without_rewards() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    s.stake("bob", 100);

    s.exec(
        "alice",
        &ExecuteMsg::Unstake {
            amount: Uint128::new(60),
        },
        &[],
    )
    .unwrap();
    assert_eq!(s.balance("alice"), 40);
    s.assert_backed();

    // unbonding ASTRO earns nothing: the deposit splits 40 / 100
    s.deposit(140);
    assert_eq!(s.pending_usdc("alice"), 40);
    assert_eq!(s.pending_usdc("bob"), 100);

    let e = s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToWithdraw {});

    s.skip(WEEK - 1);
    assert!(s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).is_err());
    s.skip(1);
    s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).unwrap();
    assert_eq!(s.bank("alice", ASTRO), 60);
    let unbondings: Vec<Unbonding> = s
        .app
        .wrap()
        .query_wasm_smart(
            &s.yastro,
            &QueryMsg::Unbondings {
                address: "alice".to_string(),
            },
        )
        .unwrap();
    assert!(unbondings.is_empty());
    s.assert_backed();

    // can't unstake more than held
    assert!(s
        .exec(
            "alice",
            &ExecuteMsg::Unstake {
                amount: Uint128::new(41)
            },
            &[]
        )
        .is_err());
}

#[test]
fn withdraw_pays_only_matured_unbondings() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    s.exec(
        "alice",
        &ExecuteMsg::Unstake {
            amount: Uint128::new(10),
        },
        &[],
    )
    .unwrap();
    s.skip(3 * 86400);
    s.exec(
        "alice",
        &ExecuteMsg::Unstake {
            amount: Uint128::new(20),
        },
        &[],
    )
    .unwrap();
    s.skip(4 * 86400);
    s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).unwrap();
    assert_eq!(s.bank("alice", ASTRO), 10);
    s.skip(3 * 86400);
    s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).unwrap();
    assert_eq!(s.bank("alice", ASTRO), 30);
    s.assert_backed();
}

#[test]
fn open_unbondings_are_capped() {
    let mut s = Suite::new();
    s.stake("alice", 100);
    for _ in 0..astroport_yastro::contract::MAX_UNBONDINGS {
        s.exec(
            "alice",
            &ExecuteMsg::Unstake {
                amount: Uint128::new(1),
            },
            &[],
        )
        .unwrap();
    }
    let e = s
        .exec(
            "alice",
            &ExecuteMsg::Unstake {
                amount: Uint128::new(1),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::TooManyUnbondings {});
    s.skip(WEEK);
    s.exec("alice", &ExecuteMsg::Withdraw {}, &[]).unwrap();
    s.exec(
        "alice",
        &ExecuteMsg::Unstake {
            amount: Uint128::new(1),
        },
        &[],
    )
    .unwrap();
}

#[test]
fn pool_rewards_go_to_incentives() {
    let mut s = Suite::new();
    let pool = "pool";
    s.stake("alice", 100);
    s.exec(
        "alice",
        &ExecuteMsg::Transfer {
            recipient: pool.to_string(),
            amount: Uint128::new(50),
        },
        &[],
    )
    .unwrap();
    s.deposit(100);

    // only registered pools, and only by the owner
    let forward = ExecuteMsg::ForwardPoolRewards {
        pool: pool.to_string(),
    };
    let e = s.exec("bot", &forward, &[]).unwrap_err();
    assert_eq!(
        err(e),
        ContractError::PoolNotRegistered {
            pool: pool.to_string()
        }
    );
    let set = ExecuteMsg::SetPool {
        pool: pool.to_string(),
        lp_token: Some("factory/pool/astroport/share".to_string()),
    };
    let e = s.exec("bot", &set, &[]).unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});
    s.exec("owner", &set, &[]).unwrap();

    // anyone forwards; the caller's funds pay Incentives' new-reward fee
    s.mint("bot", 1_500, FEE);
    s.exec("bot", &forward, &coins(1_500, FEE)).unwrap();
    let calls: Vec<(String, Coin, Vec<Coin>)> = s
        .app
        .wrap()
        .query_wasm_smart(&s.incentives, &Empty {})
        .unwrap();
    assert_eq!(
        calls,
        vec![(
            "factory/pool/astroport/share".to_string(),
            coin(50, USDC),
            vec![coin(50, USDC), coin(1_500, FEE)]
        )]
    );
    assert_eq!(s.pending_usdc(pool), 0);
    assert_eq!(s.pending_usdc("alice"), 50);

    // nothing new to forward
    let e = s.exec("bot", &forward, &[]).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToClaim {});
}

#[test]
fn owner_controls_config_and_ownership() {
    let mut s = Suite::new();
    let update = ExecuteMsg::UpdateConfig {
        add_reward_denoms: None,
        remove_reward_denoms: None,
        unbonding_period: Some(3 * 86400),
        incentives: None,
    };
    let e = s.exec("alice", &update, &[]).unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});
    s.exec("owner", &update, &[]).unwrap();

    let e = s
        .exec(
            "owner",
            &ExecuteMsg::UpdateConfig {
                add_reward_denoms: None,
                remove_reward_denoms: None,
                unbonding_period: Some(0),
                incentives: None,
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::ZeroUnbondingPeriod {});

    s.exec(
        "owner",
        &ExecuteMsg::ProposeNewOwner {
            owner: "dao".to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();
    s.exec("dao", &ExecuteMsg::ClaimOwnership {}, &[]).unwrap();
    let config: Config = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner, Addr::unchecked("dao"));
    assert_eq!(config.unbonding_period, 3 * 86400);
}

#[test]
fn rounding_never_pays_out_more_than_deposited() {
    let mut s = Suite::new();
    s.stake("aaa", 3);
    s.stake("bbb", 3);
    s.stake("ccc", 3);
    for _ in 0..5 {
        s.deposit(10);
    }
    let owed: u128 = ["aaa", "bbb", "ccc"]
        .iter()
        .map(|w| s.pending_usdc(w))
        .sum();
    assert!(owed <= 50);
    for w in ["aaa", "bbb", "ccc"] {
        s.exec(w, &ExecuteMsg::Claim { recipient: None }, &[])
            .unwrap();
    }
    assert_eq!(s.bank(s.yastro.as_str(), USDC), 50 - owed);
}
