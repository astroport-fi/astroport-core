use cosmwasm_schema::cw_serde;
use cosmwasm_std::testing::MockApi;
use cosmwasm_std::{
    coin, coins, to_json_binary, Addr, Api, Binary, CanonicalAddr, Coin, Deps, DepsMut, Empty, Env,
    MessageInfo, RecoverPubkeyError, Response, StdError, StdResult, Storage, Timestamp, Uint128,
    VerificationError,
};
use cw20::{BalanceResponse, Cw20ReceiveMsg, MarketingInfoResponse};
use cw20_base::msg::InstantiateMarketingInfo;
use cw_storage_plus::Item;

use astroport::asset::{AssetInfo, PairInfo};
use astroport::factory::PairType;
use astroport::incentives::{self as inc, IncentivizationFeeInfo, EPOCHS_START, EPOCH_LENGTH};
use astroport_test::cw_multi_test::{
    AddressGenerator, App, AppBuilder, AppResponse, BankKeeper, BankSudo, Contract,
    ContractWrapper, Executor, WasmKeeper,
};
use astroport_yastro::error::ContractError;
use astroport_yastro::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, RewardState, StakingState};
use astroport_yastro::state::Unbonding;

const ASTRO: &str = "ibc/astro";
const USDC: &str = "ibc/usdc";
const OTHER: &str = "ibc/other";
const WEEK: u64 = EPOCH_LENGTH;
const DAY: u64 = 86400;
/// A Monday 00:00 UTC: an epoch boundary
const B: u64 = EPOCHS_START + 100 * WEEK;
/// Sunday 23:00 UTC, when the Maker deposits
const SUNDAY: u64 = B - 3600;

// Addresses all start with "wasm1", so LP denoms like factory/wasm1_.../astroport/share aren't
// mistaken for CW20 addresses
const PREFIX: &str = "wasm1";

fn a(name: &str) -> Addr {
    Addr::unchecked(format!("{PREFIX}_{name}"))
}

struct TestApi(MockApi);

impl Api for TestApi {
    fn addr_validate(&self, input: &str) -> StdResult<Addr> {
        if input.starts_with(PREFIX) {
            self.0.addr_validate(input)
        } else {
            Err(StdError::generic_err(format!("{input} isn't an address")))
        }
    }
    fn addr_canonicalize(&self, human: &str) -> StdResult<CanonicalAddr> {
        self.0.addr_canonicalize(human)
    }
    fn addr_humanize(&self, canonical: &CanonicalAddr) -> StdResult<Addr> {
        self.0.addr_humanize(canonical)
    }
    fn secp256k1_verify(&self, h: &[u8], s: &[u8], k: &[u8]) -> Result<bool, VerificationError> {
        self.0.secp256k1_verify(h, s, k)
    }
    fn secp256k1_recover_pubkey(
        &self,
        h: &[u8],
        s: &[u8],
        p: u8,
    ) -> Result<Vec<u8>, RecoverPubkeyError> {
        self.0.secp256k1_recover_pubkey(h, s, p)
    }
    fn ed25519_verify(&self, m: &[u8], s: &[u8], k: &[u8]) -> Result<bool, VerificationError> {
        self.0.ed25519_verify(m, s, k)
    }
    fn ed25519_batch_verify(
        &self,
        m: &[&[u8]],
        s: &[&[u8]],
        k: &[&[u8]],
    ) -> Result<bool, VerificationError> {
        self.0.ed25519_batch_verify(m, s, k)
    }
    fn debug(&self, message: &str) {
        self.0.debug(message)
    }
}

struct Addresses;

impl AddressGenerator for Addresses {
    fn contract_address(
        &self,
        _api: &dyn Api,
        storage: &mut dyn Storage,
        _code_id: u64,
        _instance_id: u64,
    ) -> anyhow::Result<Addr> {
        let count = storage
            .get(b"address_count")
            .map(|v| u64::from_be_bytes(v.try_into().unwrap()) + 1)
            .unwrap_or(1);
        storage.set(b"address_count", &count.to_be_bytes());
        Ok(a(&format!("contract{count}")))
    }
}

type TestApp = App<BankKeeper, TestApi>;

fn yastro_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_yastro::contract::execute,
            astroport_yastro::contract::instantiate,
            astroport_yastro::contract::query,
        )
        .with_reply_empty(astroport_yastro::contract::reply)
        .with_migrate_empty(astroport_yastro::contract::migrate),
    )
}

fn incentives_contract() -> Box<dyn Contract<Empty>> {
    Box::new(
        ContractWrapper::new_with_empty(
            astroport_incentives::execute::execute,
            astroport_incentives::instantiate::instantiate,
            astroport_incentives::query::query,
        )
        .with_reply_empty(astroport_incentives::reply::reply),
    )
}

// A pool stand-in: answers Astroport's `pair {}` with its assets and its own tokenfactory LP
const ASSETS: Item<Vec<AssetInfo>> = Item::new("assets");

#[cw_serde]
struct PairInit {
    assets: Vec<AssetInfo>,
}

#[cw_serde]
enum PairQuery {
    Pair {},
}

fn lp_of(pool: &Addr) -> String {
    format!("factory/{pool}/astroport/share")
}

fn mock_pair() -> Box<dyn Contract<Empty>> {
    fn instantiate(deps: DepsMut, _: Env, _: MessageInfo, msg: PairInit) -> StdResult<Response> {
        ASSETS.save(deps.storage, &msg.assets)?;
        Ok(Response::new())
    }
    fn execute(_: DepsMut, _: Env, _: MessageInfo, _: Empty) -> StdResult<Response> {
        Ok(Response::new())
    }
    fn query(deps: Deps, env: Env, _: PairQuery) -> StdResult<Binary> {
        to_json_binary(&PairInfo {
            asset_infos: ASSETS.load(deps.storage)?,
            liquidity_token: lp_of(&env.contract.address),
            contract_addr: env.contract.address,
            pair_type: PairType::Xyk {},
        })
    }
    Box::new(ContractWrapper::new_with_empty(execute, instantiate, query))
}

// A contract that accepts yASTRO sent to it and never claims
#[cw_serde]
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

fn instantiate_msg(incentives: &Addr) -> InstantiateMsg {
    InstantiateMsg {
        owner: a("owner").to_string(),
        astro_denom: ASTRO.to_string(),
        unbonding_period: WEEK,
        reward_denoms: vec![USDC.to_string()],
        incentives: Some(incentives.to_string()),
        name: "Staked ASTRO".to_string(),
        symbol: "yASTRO".to_string(),
        decimals: 6,
        marketing: Some(InstantiateMarketingInfo {
            project: Some("https://astroport.fi".to_string()),
            description: Some("Stake ASTRO, earn protocol fees".to_string()),
            marketing: Some(a("owner").to_string()),
            logo: None,
        }),
    }
}

struct Suite {
    app: TestApp,
    yastro: Addr,
    incentives: Addr,
    yastro_code: u64,
}

impl Suite {
    fn new() -> Self {
        let mut app = AppBuilder::new()
            .with_api(TestApi(MockApi::default()))
            .with_wasm(WasmKeeper::new().with_address_generator(Addresses))
            .build(|_, _, _| {});
        app.update_block(|b| {
            b.time = Timestamp::from_seconds(SUNDAY - WEEK);
            b.height = 1;
        });
        let incentives_code = app.store_code(incentives_contract());
        let incentives = app
            .instantiate_contract(
                incentives_code,
                a("owner"),
                &inc::InstantiateMsg {
                    owner: a("owner").to_string(),
                    factory: a("factory").to_string(),
                    astro_token: AssetInfo::native(ASTRO),
                    vesting_contract: a("vesting").to_string(),
                    incentivization_fee_info: Some(IncentivizationFeeInfo {
                        fee_receiver: a("maker"),
                        fee: coin(1_500_000_000, ASTRO),
                    }),
                    guardian: None,
                },
                &[],
                "incentives",
                None,
            )
            .unwrap();
        let yastro_code = app.store_code(yastro_contract());
        let yastro = app
            .instantiate_contract(
                yastro_code,
                a("owner"),
                &instantiate_msg(&incentives),
                &[],
                "yastro",
                Some(a("owner").to_string()),
            )
            .unwrap();
        Suite {
            app,
            yastro,
            incentives,
            yastro_code,
        }
    }

    fn at(&mut self, ts: u64) {
        self.app.update_block(|b| {
            assert!(ts >= b.time.seconds());
            b.time = Timestamp::from_seconds(ts);
            b.height += 1;
        });
    }

    fn now(&self) -> u64 {
        self.app.block_info().time.seconds()
    }

    fn mint(&mut self, to: &Addr, amount: u128, denom: &str) {
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
        sender: &Addr,
        msg: &ExecuteMsg,
        funds: &[Coin],
    ) -> anyhow::Result<AppResponse> {
        self.app
            .execute_contract(sender.clone(), self.yastro.clone(), msg, funds)
    }

    fn exec_inc(
        &mut self,
        sender: &Addr,
        msg: &inc::ExecuteMsg,
        funds: &[Coin],
    ) -> anyhow::Result<AppResponse> {
        self.app
            .execute_contract(sender.clone(), self.incentives.clone(), msg, funds)
    }

    fn stake(&mut self, who: &Addr, amount: u128) {
        self.mint(who, amount, ASTRO);
        self.exec(
            who,
            &ExecuteMsg::Stake { recipient: None },
            &coins(amount, ASTRO),
        )
        .unwrap();
    }

    fn unstake(&mut self, who: &Addr, amount: u128) -> anyhow::Result<AppResponse> {
        self.exec(
            who,
            &ExecuteMsg::Unstake {
                amount: Uint128::new(amount),
            },
            &[],
        )
    }

    fn deposit(&mut self, amount: u128) {
        self.mint(&a("maker"), amount, USDC);
        self.exec(
            &a("maker"),
            &ExecuteMsg::DepositRewards {},
            &coins(amount, USDC),
        )
        .unwrap();
    }

    fn transfer(&mut self, from: &Addr, to: &Addr, amount: u128) -> anyhow::Result<AppResponse> {
        self.exec(
            from,
            &ExecuteMsg::Transfer {
                recipient: to.to_string(),
                amount: Uint128::new(amount),
            },
            &[],
        )
    }

    fn claim(&mut self, who: &Addr) -> anyhow::Result<AppResponse> {
        self.exec(who, &ExecuteMsg::Claim { recipient: None }, &[])
    }

    fn pending(&self, who: &Addr) -> u128 {
        let coins: Vec<Coin> = self
            .app
            .wrap()
            .query_wasm_smart(
                &self.yastro,
                &QueryMsg::PendingRewards {
                    address: who.to_string(),
                },
            )
            .unwrap();
        coins
            .iter()
            .find(|c| c.denom == USDC)
            .map(|c| c.amount.u128())
            .unwrap_or_default()
    }

    fn balance(&self, who: &Addr) -> u128 {
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

    fn bank(&self, who: &Addr, denom: &str) -> u128 {
        self.app
            .wrap()
            .query_balance(who, denom)
            .unwrap()
            .amount
            .u128()
    }

    fn reward_state(&self) -> Vec<RewardState> {
        self.app
            .wrap()
            .query_wasm_smart(&self.yastro, &QueryMsg::RewardState {})
            .unwrap()
    }

    fn pool(&mut self, assets: Vec<AssetInfo>) -> Addr {
        let code = self.app.store_code(mock_pair());
        self.app
            .instantiate_contract(code, a("owner"), &PairInit { assets }, &[], "pair", None)
            .unwrap()
    }

    fn yastro_pool(&mut self) -> Addr {
        let yastro = AssetInfo::cw20(self.yastro.clone());
        self.pool(vec![yastro, AssetInfo::native(ASTRO)])
    }

    fn stake_lp(&mut self, who: &Addr, pool: &Addr, amount: u128) {
        let lp = lp_of(pool);
        self.mint(who, amount, &lp);
        self.exec_inc(
            who,
            &inc::ExecuteMsg::Deposit { recipient: None },
            &coins(amount, &lp),
        )
        .unwrap();
    }

    fn exempt_yastro(&mut self) {
        let yastro = self.yastro.to_string();
        self.exec_inc(
            &a("owner"),
            &inc::ExecuteMsg::UpdateFeeExemptions {
                add: vec![yastro],
                remove: vec![],
            },
            &[],
        )
        .unwrap();
    }

    fn forward(&mut self, pool: &Addr) -> anyhow::Result<AppResponse> {
        self.exec(
            &a("anyone"),
            &ExecuteMsg::ForwardPoolRewards {
                pool: pool.to_string(),
            },
            &[],
        )
    }
}

fn err(e: anyhow::Error) -> ContractError {
    e.downcast().unwrap()
}

/// `actual` is `expected` less at most `dust` units of rounding
fn assert_close(actual: u128, expected: u128, dust: u128) {
    assert!(
        actual <= expected && actual + dust >= expected,
        "expected {expected} (less up to {dust} dust), got {actual}"
    );
}

#[test]
fn stake_unstake_withdraw() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.stake(&alice, 100);
    assert_eq!(s.balance(&alice), 100);

    s.unstake(&alice, 40).unwrap();
    assert_eq!(s.balance(&alice), 60);
    let state: StakingState = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::StakingState {})
        .unwrap();
    assert_eq!(state.total_staked.u128(), 60);
    assert_eq!(state.total_unbonding.u128(), 40);
    let unbondings: Vec<Unbonding> = s
        .app
        .wrap()
        .query_wasm_smart(
            &s.yastro,
            &QueryMsg::Unbondings {
                address: alice.to_string(),
            },
        )
        .unwrap();
    assert_eq!(unbondings[0].release_at, s.now() + WEEK);

    let e = s.exec(&alice, &ExecuteMsg::Withdraw {}, &[]).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToWithdraw {});
    let t = s.now() + WEEK;
    s.at(t);
    s.exec(&alice, &ExecuteMsg::Withdraw {}, &[]).unwrap();
    assert_eq!(s.bank(&alice, ASTRO), 40);
    assert_eq!(s.bank(&s.yastro.clone(), ASTRO), 60);

    // Only ASTRO, and something of it
    let e = s
        .exec(&alice, &ExecuteMsg::Stake { recipient: None }, &[])
        .unwrap_err();
    assert!(matches!(err(e), ContractError::Payment(_)));
    let e = s.unstake(&alice, 0).unwrap_err();
    assert_eq!(err(e), ContractError::ZeroAmount {});
    s.unstake(&alice, 61).unwrap_err();
}

#[test]
fn deposits_stream_over_the_next_epoch() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.stake(&alice, 1_000_000);

    s.at(SUNDAY);
    s.deposit(7_000_000);
    let state = &s.reward_state()[0];
    assert_eq!(state.epoch_end, B);
    assert_eq!(state.queued.to_string(), "7000000");
    assert_eq!(state.total_deposited.u128(), 7_000_000);

    // Nothing until Monday 00:00
    s.at(B);
    assert_eq!(s.pending(&alice), 0);

    // Then evenly over the week
    s.at(B + WEEK / 2);
    assert_close(s.pending(&alice), 3_500_000, 1);
    let state = &s.reward_state()[0];
    assert_eq!(state.epoch_end, B + WEEK);
    assert!(state.queued.is_zero());

    s.at(B + WEEK);
    assert_close(s.pending(&alice), 7_000_000, 1);
    s.at(B + 3 * WEEK);
    assert_close(s.pending(&alice), 7_000_000, 1);

    s.claim(&alice).unwrap();
    assert_close(s.bank(&alice, USDC), 7_000_000, 1);
    let e = s.claim(&alice).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToClaim {});
}

#[test]
fn rewards_follow_balance_and_time_held() {
    let mut s = Suite::new();
    let (alice, bob, carol) = (a("alice"), a("bob"), a("carol"));
    s.stake(&alice, 100);
    s.at(SUNDAY);
    s.deposit(8_000_000);

    // Bob holds as much as alice for the second half of the epoch
    s.at(B + WEEK / 2);
    s.stake(&bob, 100);
    // Alice sends carol half hers three quarters through
    s.at(B + 3 * WEEK / 4);
    s.transfer(&alice, &carol, 50).unwrap();

    // First half: alice alone, 4M. Third quarter: alice and bob split 2M. Last quarter: alice
    // 50, carol 50 and bob 100 split 2M.
    s.at(B + WEEK);
    assert_close(s.pending(&alice), 4_000_000 + 1_000_000 + 500_000, 2);
    assert_close(s.pending(&bob), 1_000_000 + 1_000_000, 2);
    assert_close(s.pending(&carol), 500_000, 2);
}

#[test]
fn buying_just_before_the_epoch_earns_next_to_nothing() {
    let mut s = Suite::new();
    let (alice, sniper) = (a("alice"), a("sniper"));
    s.stake(&alice, 1_000_000);
    s.at(SUNDAY);
    s.deposit(10_000_000_000);

    // A thousand times alice's stake, held for one minute around the epoch boundary
    s.at(B - 30);
    s.stake(&sniper, 1_000_000_000);
    s.at(B + 30);
    s.transfer(&sniper, &alice, 1_000_000_000).unwrap();

    // 30 seconds of a week's stream
    s.at(B + WEEK);
    let sniped = s.pending(&sniper);
    assert!(sniped <= 10_000_000_000 * 30 / WEEK as u128, "{sniped}");
    assert_close(s.pending(&alice) + sniped, 10_000_000_000, 2);
}

#[test]
fn deposits_with_nobody_staked_stream_again_instead_of_being_swept() {
    let mut s = Suite::new();
    let (alice, bob) = (a("alice"), a("bob"));
    s.at(SUNDAY);
    s.deposit(7_000_000);

    // Alice stakes dust halfway through the epoch: there's nothing to sweep
    s.at(B + WEEK / 2);
    s.stake(&alice, 1);
    assert_eq!(s.pending(&alice), 0);
    s.at(B + WEEK / 2 + 60);
    assert!(s.pending(&alice) <= 7_000_000 * 60 / WEEK as u128 + 1);

    // She gets the half streamed while she held; the half streamed to nobody comes back next
    // epoch, where bob shares it
    s.at(B + WEEK);
    assert_close(s.pending(&alice), 3_500_000, 1);
    s.stake(&bob, 1);
    s.at(B + 2 * WEEK);
    assert_close(s.pending(&alice), 3_500_000 + 1_750_000, 2);
    assert_close(s.pending(&bob), 1_750_000, 2);
}

#[test]
fn long_idle_periods_resume_with_the_epoch() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.at(SUNDAY);
    s.deposit(7_000_000);

    // Ten weeks with nobody staked
    s.at(B + 10 * WEEK + WEEK / 2);
    s.stake(&alice, 5);
    assert_eq!(s.pending(&alice), 0);
    s.at(B + 12 * WEEK);
    assert_close(s.pending(&alice), 7_000_000, 2);
}

#[test]
fn fractions_add_up_across_settlements() {
    let mut s = Suite::new();
    let (alice, bob) = (a("alice"), a("bob"));
    s.stake(&alice, 1);
    s.stake(&bob, 2);

    // Alice is owed two thirds of a unit per epoch. Settling her every day keeps the fractions.
    for week in 0..2u64 {
        s.at(SUNDAY + week * WEEK);
        s.deposit(2);
        for day in 0..7u64 {
            s.at(B + week * WEEK + day * DAY + 1);
            s.transfer(&alice, &bob, 1).unwrap();
            s.transfer(&bob, &alice, 1).unwrap();
        }
    }
    s.at(B + 2 * WEEK);
    assert_eq!(s.pending(&alice), 1);
    assert_eq!(s.pending(&bob), 2);
    s.claim(&alice).unwrap();
    assert_eq!(s.bank(&alice, USDC), 1);
}

#[test]
fn transfers_and_contract_holders() {
    let mut s = Suite::new();
    let (alice, bob, spender) = (a("alice"), a("bob"), a("spender"));
    let receiver_code = s.app.store_code(mock_receiver());
    let receiver = s
        .app
        .instantiate_contract(receiver_code, a("owner"), &Empty {}, &[], "receiver", None)
        .unwrap();

    s.stake(&alice, 300);
    s.at(SUNDAY);
    s.deposit(3_000_000);
    s.at(B);
    // To bob through an allowance, to a contract with send
    s.exec(
        &alice,
        &ExecuteMsg::IncreaseAllowance {
            spender: spender.to_string(),
            amount: Uint128::new(100),
            expires: None,
        },
        &[],
    )
    .unwrap();
    s.exec(
        &spender,
        &ExecuteMsg::TransferFrom {
            owner: alice.to_string(),
            recipient: bob.to_string(),
            amount: Uint128::new(100),
        },
        &[],
    )
    .unwrap();
    s.exec(
        &alice,
        &ExecuteMsg::Send {
            contract: receiver.to_string(),
            amount: Uint128::new(100),
            msg: Binary::default(),
        },
        &[],
    )
    .unwrap();

    s.at(B + WEEK);
    for who in [&alice, &bob, &receiver] {
        assert_close(s.pending(who), 1_000_000, 1);
    }

    // The receiver can't claim, so anyone can pay it its own rewards
    s.exec(
        &a("anyone"),
        &ExecuteMsg::ClaimFor {
            address: receiver.to_string(),
        },
        &[],
    )
    .unwrap();
    assert_close(s.bank(&receiver, USDC), 1_000_000, 1);
    assert_eq!(s.pending(&receiver), 0);

    // Nothing to this contract, nothing of zero
    let me = s.yastro.clone();
    let e = s.transfer(&alice, &me, 1).unwrap_err();
    assert_eq!(err(e), ContractError::SelfRecipient {});
    let e = s.transfer(&alice, &bob, 0).unwrap_err();
    assert_eq!(err(e), ContractError::ZeroAmount {});
    s.mint(&alice, 1, ASTRO);
    let e = s
        .exec(
            &alice,
            &ExecuteMsg::Stake {
                recipient: Some(me.to_string()),
            },
            &coins(1, ASTRO),
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::SelfRecipient {});
}

#[test]
fn claim_to_a_recipient() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.stake(&alice, 10);
    s.at(SUNDAY);
    s.deposit(1_000_000);
    s.at(B + WEEK);
    s.exec(
        &alice,
        &ExecuteMsg::Claim {
            recipient: Some(a("wallet").to_string()),
        },
        &[],
    )
    .unwrap();
    assert_close(s.bank(&a("wallet"), USDC), 1_000_000, 1);
}

#[test]
fn unbonding_earns_nothing() {
    let mut s = Suite::new();
    let (alice, bob) = (a("alice"), a("bob"));
    s.stake(&alice, 100);
    s.stake(&bob, 100);
    s.at(SUNDAY);
    s.deposit(2_000_000);
    s.at(B);
    s.unstake(&bob, 100).unwrap();
    s.at(B + WEEK);
    assert_close(s.pending(&alice), 2_000_000, 1);
    assert_eq!(s.pending(&bob), 0);
}

#[test]
fn unbonding_limits() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.stake(&alice, 100);
    for _ in 0..astroport_yastro::contract::MAX_UNBONDINGS {
        s.unstake(&alice, 1).unwrap();
    }
    let e = s.unstake(&alice, 1).unwrap_err();
    assert_eq!(err(e), ContractError::TooManyUnbondings {});

    let max = astroport_yastro::contract::MAX_UNBONDING_PERIOD;
    for period in [0, max + 1, u64::MAX] {
        let e = s
            .exec(
                &a("owner"),
                &ExecuteMsg::UpdateConfig {
                    add_reward_denoms: None,
                    remove_reward_denoms: None,
                    unbonding_period: Some(period),
                    incentives: None,
                },
                &[],
            )
            .unwrap_err();
        assert_eq!(err(e), ContractError::InvalidUnbondingPeriod { max });

        let mut msg = instantiate_msg(&s.incentives);
        msg.unbonding_period = period;
        let code = s.yastro_code;
        let e = s
            .app
            .instantiate_contract(code, a("owner"), &msg, &[], "y", None)
            .unwrap_err();
        assert_eq!(err(e), ContractError::InvalidUnbondingPeriod { max });
    }
}

#[test]
fn reward_denoms() {
    let mut s = Suite::new();
    let alice = a("alice");
    s.stake(&alice, 10);

    for denom in [ASTRO, OTHER] {
        s.mint(&alice, 10, denom);
        let e = s
            .exec(&alice, &ExecuteMsg::DepositRewards {}, &coins(10, denom))
            .unwrap_err();
        assert_eq!(
            err(e),
            ContractError::NotARewardDenom {
                denom: denom.to_string()
            }
        );
    }

    let update = |add: Vec<&str>, remove: Vec<&str>| ExecuteMsg::UpdateConfig {
        add_reward_denoms: Some(add.into_iter().map(String::from).collect()),
        remove_reward_denoms: Some(remove.into_iter().map(String::from).collect()),
        unbonding_period: None,
        incentives: None,
    };
    let e = s
        .exec(&a("owner"), &update(vec![ASTRO], vec![]), &[])
        .unwrap_err();
    assert_eq!(
        err(e),
        ContractError::StakedDenomAsReward {
            denom: ASTRO.to_string()
        }
    );
    let e = s
        .exec(&alice, &update(vec![OTHER], vec![]), &[])
        .unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});

    // At most five denoms, ever
    let e = s
        .exec(
            &a("owner"),
            &update(vec!["d1", "d2", "d3", "d4", "d5"], vec![]),
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::TooManyRewardDenoms { max: 5 });

    // Removing a denom stops deposits; what's owed stays claimable
    s.at(SUNDAY);
    s.deposit(1_000_000);
    s.exec(&a("owner"), &update(vec![], vec![USDC]), &[])
        .unwrap();
    s.mint(&a("maker"), 1, USDC);
    s.exec(&a("maker"), &ExecuteMsg::DepositRewards {}, &coins(1, USDC))
        .unwrap_err();
    s.at(B + WEEK);
    s.claim(&alice).unwrap();
    assert_close(s.bank(&alice, USDC), 1_000_000, 1);
    // A removed denom that was streamed still counts towards the five
    let e = s
        .exec(
            &a("owner"),
            &update(vec!["d1", "d2", "d3", "d4", "d5"], vec![]),
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::TooManyRewardDenoms { max: 5 });
    s.exec(
        &a("owner"),
        &update(vec!["d1", "d2", "d3", "d4"], vec![]),
        &[],
    )
    .unwrap();
}

#[test]
fn pool_rewards_go_to_lp_stakers_in_incentives() {
    let mut s = Suite::new();
    let (alice, lp_staker) = (a("alice"), a("lp_staker"));
    let pool = s.yastro_pool();
    s.stake(&alice, 2_000);
    s.transfer(&alice, &pool, 1_000).unwrap();

    s.at(SUNDAY);
    s.deposit(14_000_000);
    // Forwarded on Sunday night, the stream still has an hour to run
    s.at(SUNDAY + WEEK);
    let streamed = 7_000_000 * (WEEK - 3600) as u128 / WEEK as u128;
    assert_close(s.pending(&pool), streamed, 1);

    // Only pools: contracts answering `pair {}` with yASTRO in them
    let e = s.forward(&alice).unwrap_err();
    assert_eq!(
        err(e),
        ContractError::NotAPool {
            address: alice.to_string()
        }
    );
    let other_pool = s.pool(vec![AssetInfo::native(ASTRO), AssetInfo::native(USDC)]);
    let e = s.forward(&other_pool).unwrap_err();
    assert_eq!(
        err(e),
        ContractError::NotAPool {
            address: other_pool.to_string()
        }
    );
    // Pools get theirs through forwarding, not claim_for
    let e = s
        .exec(
            &a("anyone"),
            &ExecuteMsg::ClaimFor {
                address: pool.to_string(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err(e),
        ContractError::IsAPool {
            address: pool.to_string()
        }
    );

    // Nobody has staked the LP token yet
    let e = s.forward(&pool).unwrap_err();
    assert_eq!(
        err(e),
        ContractError::NoLpStakers {
            lp_token: lp_of(&pool)
        }
    );
    s.stake_lp(&lp_staker, &pool, 100);

    // Not exempt from the fee yet: Incentives rejects it and the pool keeps its rewards
    let held = s.bank(&s.yastro.clone(), USDC);
    let res = s.forward(&pool).unwrap();
    assert!(res.events.iter().any(|e| e
        .attributes
        .iter()
        .any(|attr| attr.key == "action" && attr.value == "forward_failed")));
    assert_close(s.pending(&pool), streamed, 1);
    assert_eq!(s.bank(&s.yastro.clone(), USDC), held);

    // Attached funds are refused
    s.mint(&a("anyone"), 1_500_000_000, ASTRO);
    let e = s
        .exec(
            &a("anyone"),
            &ExecuteMsg::ForwardPoolRewards {
                pool: pool.to_string(),
            },
            &coins(1_500_000_000, ASTRO),
        )
        .unwrap_err();
    assert!(matches!(err(e), ContractError::Payment(_)));

    s.exempt_yastro();
    let forwarded = s.pending(&pool);
    s.forward(&pool).unwrap();
    assert_eq!(s.pending(&pool), 0);
    assert_eq!(s.bank(&s.incentives.clone(), USDC), forwarded);
    assert_eq!(s.bank(&a("maker"), ASTRO), 0);

    // The LP staker earns it over the schedule (until next Monday plus a week)
    s.at(B + 2 * WEEK);
    s.exec_inc(
        &lp_staker,
        &inc::ExecuteMsg::ClaimRewards {
            lp_tokens: vec![lp_of(&pool)],
        },
        &[],
    )
    .unwrap();
    assert_close(s.bank(&lp_staker, USDC), forwarded, 2);
}

#[test]
fn pool_rewards_too_small_to_schedule_stay_with_the_pool() {
    let mut s = Suite::new();
    let alice = a("alice");
    let pool = s.yastro_pool();
    s.stake(&alice, 1_000_000);
    s.transfer(&alice, &pool, 1).unwrap();
    s.stake_lp(&a("lp_staker"), &pool, 100);
    s.exempt_yastro();

    s.at(SUNDAY);
    s.deposit(1_000_000_000);
    s.at(SUNDAY + WEEK);
    // About 1,000 units: under Incentives' 1 unit per second
    let owed = s.pending(&pool);
    assert!(owed > 0 && owed < 600_000, "{owed}");
    let e = s.forward(&pool).unwrap_err();
    assert_eq!(err(e), ContractError::NothingToForward {});
    assert_eq!(s.pending(&pool), owed);
}

#[test]
fn owner_and_ownership() {
    let mut s = Suite::new();
    let (owner, new_owner) = (a("owner"), a("new_owner"));
    let propose = ExecuteMsg::ProposeNewOwner {
        owner: new_owner.to_string(),
        expires_in: 100,
    };
    let e = s.exec(&new_owner, &propose, &[]).unwrap_err();
    assert!(e.root_cause().to_string().contains("Unauthorized"));
    s.exec(&owner, &propose, &[]).unwrap();
    s.exec(&new_owner, &ExecuteMsg::ClaimOwnership {}, &[])
        .unwrap();
    let config: astroport_yastro::state::Config = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner, new_owner);
}

#[test]
fn token_metadata_and_marketing() {
    let mut s = Suite::new();
    let info: MarketingInfoResponse = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::MarketingInfo {})
        .unwrap();
    assert_eq!(info.project.as_deref(), Some("https://astroport.fi"));
    assert_eq!(info.marketing, Some(a("owner")));

    let update = |description: &str| ExecuteMsg::UpdateMarketing {
        project: None,
        description: Some(description.to_string()),
        marketing: None,
    };
    s.exec(&a("owner"), &update("yASTRO"), &[]).unwrap();
    s.exec(&a("alice"), &update("x"), &[]).unwrap_err();

    // cw20-base's validation applies
    let mut msg = instantiate_msg(&s.incentives);
    msg.symbol = "y".to_string();
    let code = s.yastro_code;
    s.app
        .instantiate_contract(code, a("owner"), &msg, &[], "y", None)
        .unwrap_err();

    let minter: Option<cw20::MinterResponse> = s
        .app
        .wrap()
        .query_wasm_smart(&s.yastro, &QueryMsg::Minter {})
        .unwrap();
    assert!(minter.is_none());
}

#[test]
fn migrate_refuses_unknown_versions() {
    let mut s = Suite::new();
    let (yastro, code) = (s.yastro.clone(), s.yastro_code);
    let e = s
        .app
        .migrate_contract(
            a("owner"),
            yastro,
            &astroport_yastro::msg::MigrateMsg {},
            code,
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::MigrationError {});
}

#[test]
fn precision_at_launch_scale() {
    let mut s = Suite::new();
    let (whale, small) = (a("whale"), a("small"));
    s.stake(&whale, 600_000_000_000_000);
    s.stake(&small, 1_000_000);
    s.at(SUNDAY);
    s.deposit(1_000_000);
    s.at(B + WEEK);
    assert_close(s.pending(&whale), 1_000_000, 2);
}

#[test]
fn fuzz_never_owes_more_than_it_holds() {
    let mut s = Suite::new();
    let users: Vec<Addr> = (0..7).map(|i| a(&format!("user{i}"))).collect();
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rnd = |m: u64| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed % m
    };
    let mut deposited = 0u128;
    let mut claimed = 0u128;
    for _ in 0..500 {
        let t = s.now() + rnd(2 * DAY);
        s.at(t);
        match rnd(6) {
            0 | 1 => {
                let u = users[rnd(7) as usize].clone();
                s.stake(&u, 1 + rnd(1_000_000_000_000) as u128);
            }
            2 => {
                let from = users[rnd(7) as usize].clone();
                let to = users[rnd(7) as usize].clone();
                let balance = s.balance(&from);
                if balance > 0 {
                    s.transfer(&from, &to, 1 + rnd(balance as u64) as u128)
                        .unwrap();
                }
            }
            3 => {
                let u = users[rnd(7) as usize].clone();
                let balance = s.balance(&u);
                if balance > 0 {
                    let _ = s.unstake(&u, 1 + rnd(balance as u64) as u128);
                }
            }
            4 => {
                let amount = 1 + rnd(10_000_000_000) as u128;
                s.deposit(amount);
                deposited += amount;
            }
            _ => {
                let u = users[rnd(7) as usize].clone();
                let owed = s.pending(&u);
                if owed > 0 {
                    s.claim(&u).unwrap();
                    claimed += owed;
                }
            }
        }
        let owed: u128 = users.iter().map(|u| s.pending(u)).sum();
        let held = s.bank(&s.yastro.clone(), USDC);
        assert!(held >= owed, "shortfall: holds {held}, owes {owed}");
        assert_eq!(held + claimed, deposited);
    }

    // Once every stream has run out, nearly everything deposited is claimable
    let t = s.now() + 3 * WEEK;
    s.at(t);
    for u in &users {
        let owed = s.pending(u);
        if owed > 0 {
            s.claim(u).unwrap();
            claimed += owed;
        }
    }
    let left = s.bank(&s.yastro.clone(), USDC);
    assert_eq!(left + claimed, deposited);
    assert!(left <= 1_000, "{left} left as dust");
}
