use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{DepsMut, Env, MessageInfo, Response, StdError, Uint128};
use cw2::{get_contract_version, set_contract_version};

use astroport::asset::validate_native_denom;
use astroport::tokenfactory_tracker::{InstantiateMsg, SudoMsg};

use crate::error::ContractError;
use crate::state::{Config, BALANCES, CONFIG, TOTAL_SUPPLY_HISTORY};

const CONTRACT_NAME: &str = env!("CARGO_PKG_NAME");
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    deps.api.addr_validate(&msg.tokenfactory_module_address)?;

    validate_native_denom(&msg.tracked_denom)?;

    let config = Config {
        d: msg.tracked_denom.clone(),
        m: msg.tokenfactory_module_address,
        t: msg.track_over_seconds,
    };
    CONFIG.save(deps.storage, &config)?;

    Ok(Response::default()
        .add_attribute("action", "instantiate")
        .add_attribute("contract", CONTRACT_NAME)
        .add_attribute("tracked_denom", config.d)
        .add_attribute("tokenfactory_module_address", config.m))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn sudo(deps: DepsMut, env: Env, msg: SudoMsg) -> Result<Response, ContractError> {
    match msg {
        // BlockBeforeSend is called before a send - if an error is returned the send is cancelled.
        // This call doesn't have gas limitations but the gas used due to calling this contract contributes to the total tx gas.
        // Extended bank module calls BlockBeforeSend and TrackBeforeSend sequentially on mint, send and burn actions.
        // Ref: https://github.com/neutron-org/cosmos-sdk/blob/28f3db48a7ae038e9ccdd2bae632cb21c1c9de86/x/bank/keeper/send.go#L207-L223
        SudoMsg::BlockBeforeSend { from, to, amount } => {
            let config = CONFIG.load(deps.storage)?;

            // Ensure the denom being sent is the tracked denom
            // If this isn't checked, another token could be tracked with the same
            // contract and that will skew the real numbers
            if amount.denom != config.d {
                Err(ContractError::InvalidDenom {
                    expected_denom: config.d,
                })
            } else {
                let tracking_unit = if config.t {
                    env.block.time.seconds()
                } else {
                    env.block.height
                };
                // If this function throws error all send, mint and burn actions will be blocked.
                // However, balances query will still work, hence governance will be able to recover the contract.
                track_balances(deps, tracking_unit, &config, from, to, amount.amount)
            }
        }
        // tokenfactory enforces hard gas limit 100k on TrackBeforeSend of which 60k is a flat contract initialization.
        // Hence, we have only up to 40k gas to handle our logic. If TrackBeforeSend hits the limit it is silently ignored on chain level,
        // making balance tracking broken with no way to recover.
        // Balance tracking feature is crucial for Astroport and Neutron DAOs thus we deliberately abuse SudoMsg::BlockBeforeSend
        // because it is not gas metered and we can do all the logic we need.
        // Ref: https://github.com/neutron-org/neutron/blob/57a25eb719eb0db973543f9d54ace484ac098721/x/tokenfactory/keeper/before_send.go#L143-L150
        SudoMsg::TrackBeforeSend { .. } => Ok(Response::default()),
    }
}

/// Track balance and total supply changes over specified tracking unit.
/// Only tokenfactory module itself can change supply by minting and burning tokens.
/// Only denom admin can dispatch mint/burn messages to the module.
/// Sending tokens to the tokenfactory module address isn't allowed by the chain.
/// Thus,
/// - if from == module_address -> mint
/// - if to == module_address -> burn
/// - other scenarios are simple transfers between addresses
/// Possible errors:
/// - serialization/deserialization errors.
/// It should never happen if both BALANCES and TOTAL_SUPPLY_HISTORY storage keys and data layout are not changed.
pub fn track_balances(
    deps: DepsMut,
    tracking_unit: u64,
    config: &Config,
    from: String,
    to: String,
    amount: Uint128,
) -> Result<Response, ContractError> {
    if from != to {
        if from != config.m {
            let from_balance = deps.querier.query_balance(&from, &config.d)?.amount;
            BALANCES.save(
                deps.storage,
                &from,
                &from_balance.checked_sub(amount)?,
                tracking_unit,
            )?;
        }

        if to != config.m {
            let to_balance = deps.querier.query_balance(&to, &config.d)?.amount;
            BALANCES.save(
                deps.storage,
                &to,
                &to_balance.checked_add(amount)?,
                tracking_unit,
            )?;
        }
    }

    let total_supply = deps.querier.query_supply(&config.d)?.amount;
    TOTAL_SUPPLY_HISTORY.save(deps.storage, &total_supply, tracking_unit)?;

    Ok(Response::default())
}

#[cw_serde]
pub struct MigrateMsg {
    pub track_over_seconds: bool,
}

/// Manages the contract migration.
#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut, _env: Env, msg: MigrateMsg) -> Result<Response, ContractError> {
    let contract_version = get_contract_version(deps.storage)?;

    match (
        contract_version.contract.as_ref(),
        contract_version.version.as_ref(),
    ) {
        ("astroport-tokenfactory-tracker", "1.0.0") => {
            let config = CONFIG.load(deps.storage)?;
            let new_config = Config {
                d: config.d,
                m: config.m,
                t: msg.track_over_seconds,
            };
            CONFIG.save(deps.storage, &new_config)?;
        }
        _ => {
            return Err(StdError::generic_err(format!(
                "Unsupported contract version: {} {}",
                contract_version.contract, contract_version.version
            ))
            .into())
        }
    }

    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default().add_attributes([
        ("previous_contract_name", contract_version.contract.as_str()),
        (
            "previous_contract_version",
            contract_version.version.as_str(),
        ),
        ("new_contract_name", CONTRACT_NAME),
        ("new_contract_version", CONTRACT_VERSION),
    ]))
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::mock_dependencies;
    use cosmwasm_std::{
        coins,
        testing::{mock_env, mock_info},
        to_json_binary, Addr, BankMsg, Coin, Uint128,
    };
    use cw_multi_test::{App, BankSudo, ContractWrapper, Executor};

    use astroport::tokenfactory_tracker::QueryMsg;

    use crate::query::query;

    use super::*;

    const OWNER: &str = "owner";
    const DENOM: &str = "factory/contract0/token";
    const MODULE_ADDRESS: &str = "tokenfactory_module";

    // Basic operations for testing calculations
    struct TestOperation {
        from: String,
        to: String,
        amount: Uint128,
    }

    #[test]
    fn track_token_balances() {
        let mut app = App::new(|router, _, store| {
            router
                .bank
                .init_balance(store, &Addr::unchecked(MODULE_ADDRESS), coins(200, DENOM))
                .unwrap();
        });

        let operations = vec![
            // Simulate a mint
            TestOperation {
                from: MODULE_ADDRESS.to_string(),
                to: "user1".to_string(),
                amount: Uint128::from(100u128),
            },
            TestOperation {
                from: "user1".to_string(),
                to: "user2".to_string(),
                amount: Uint128::from(50u128),
            },
            TestOperation {
                from: "user1".to_string(),
                to: "user3".to_string(),
                amount: Uint128::from(50u128),
            },
            TestOperation {
                from: "user2".to_string(),
                to: "user3".to_string(),
                amount: Uint128::from(50u128),
            },
            // Simulate a mint
            TestOperation {
                from: MODULE_ADDRESS.to_string(),
                to: "user4".to_string(),
                amount: Uint128::from(100u128),
            },
            // Simulate a burn
            TestOperation {
                from: "user4".to_string(),
                to: MODULE_ADDRESS.to_string(),
                amount: Uint128::from(99u128),
            },
        ];

        let expected_user1_balance = Uint128::zero();
        let expected_user2_balance = Uint128::zero();
        let expected_user3_balance = Uint128::from(100u128);
        let expected_user4_balance = Uint128::from(1u128);
        let expected_total_supply = Uint128::from(101u128);

        // setup tracker contract
        let tracker_code_id = app.store_code(Box::new(
            ContractWrapper::new_with_empty(instantiate, instantiate, query).with_sudo_empty(sudo),
        ));
        let tracker_contract = app
            .instantiate_contract(
                tracker_code_id,
                Addr::unchecked(OWNER),
                &InstantiateMsg {
                    tokenfactory_module_address: MODULE_ADDRESS.to_string(),
                    tracked_denom: DENOM.to_string(),
                    track_over_seconds: true,
                },
                &[],
                "label",
                None,
            )
            .unwrap();
        app.sudo(
            BankSudo::SetHook {
                denom: DENOM.to_string(),
                contract_addr: tracker_contract.to_string(),
            }
            .into(),
        )
        .unwrap();

        for TestOperation { from, to, amount } in operations {
            app.send_tokens(
                Addr::unchecked(&from),
                Addr::unchecked(&to),
                &coins(amount.u128(), DENOM),
            )
            .unwrap();
        }

        // burn everything from module balance
        let amount = app.wrap().query_all_balances(MODULE_ADDRESS).unwrap();
        app.execute(
            Addr::unchecked(MODULE_ADDRESS),
            BankMsg::Burn { amount }.into(),
        )
        .unwrap();

        // send coin to trigger total supply update
        let user = Addr::unchecked("user4");
        app.send_tokens(user.clone(), user, &coins(1, DENOM))
            .unwrap();

        let query_at_ts = app.block_info().time.seconds() + 10;

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::BalanceAt {
                    address: "user1".to_string(),
                    unit: Some(query_at_ts),
                },
            )
            .unwrap();
        assert_eq!(balance, expected_user1_balance);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::BalanceAt {
                    address: "user2".to_string(),
                    unit: Some(query_at_ts),
                },
            )
            .unwrap();
        assert_eq!(balance, expected_user2_balance);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::BalanceAt {
                    address: "user3".to_string(),
                    unit: Some(query_at_ts),
                },
            )
            .unwrap();
        assert_eq!(balance, expected_user3_balance);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::BalanceAt {
                    address: "user3".to_string(),
                    unit: None,
                },
            )
            .unwrap();
        assert_eq!(balance, expected_user3_balance);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::BalanceAt {
                    address: "user4".to_string(),
                    unit: None,
                },
            )
            .unwrap();
        assert_eq!(balance, expected_user4_balance);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(
                &tracker_contract,
                &QueryMsg::TotalSupplyAt {
                    unit: Some(query_at_ts),
                },
            )
            .unwrap();
        assert_eq!(balance, expected_total_supply);

        let balance: Uint128 = app
            .wrap()
            .query_wasm_smart(&tracker_contract, &QueryMsg::TotalSupplyAt { unit: None })
            .unwrap();
        assert_eq!(balance, expected_total_supply);
    }

    #[test]
    fn no_track_other_token() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info(OWNER, &[]);

        instantiate(
            deps.as_mut(),
            env.clone(),
            info,
            InstantiateMsg {
                tokenfactory_module_address: MODULE_ADDRESS.to_string(),
                tracked_denom: DENOM.to_string(),
                track_over_seconds: true,
            },
        )
        .unwrap();

        // The contract only tracks a specific denom, this should result in
        // an error
        let err = sudo(
            deps.as_mut(),
            env.clone(),
            SudoMsg::BlockBeforeSend {
                from: MODULE_ADDRESS.to_string(),
                to: "user1".to_string(),
                amount: Coin {
                    denom: "OTHER_DENOM".to_string(),
                    amount: Uint128::from(100u128),
                },
            },
        )
        .unwrap_err();

        assert_eq!(
            err,
            ContractError::InvalidDenom {
                expected_denom: DENOM.to_string()
            }
        );

        // Verify that it was not tracked
        let balance = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::BalanceAt {
                address: "user1".to_string(),
                unit: Some(env.block.time.seconds()),
            },
        )
        .unwrap();
        assert_eq!(balance, to_json_binary(&Uint128::zero()).unwrap());
    }
}
