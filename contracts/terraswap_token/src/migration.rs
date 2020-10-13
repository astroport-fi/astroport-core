use cosmwasm_std::{
    from_binary, log, Api, Env, Extern, HandleResponse, HandleResult, HumanAddr, Querier, StdError,
    StdResult, Storage, Uint128,
};

use crate::state::{balances, token_info, token_info_read, TokenInfo};
use cw20::Cw20ReceiveMsg;
use terraswap::{TokenMigrationResponse, TokenCw20HookMsg};

pub fn receive_cw20<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    env: Env,
    cw20_msg: Cw20ReceiveMsg,
) -> HandleResult {
    // only migration target token contract can execute this message
    let token_info: TokenInfo = token_info_read(&deps.storage).load()?;
    if token_info.migration.is_none()
        || token_info.migration.as_ref().unwrap().token
            != deps.api.canonical_address(&env.message.sender)?
    {
        return Err(StdError::unauthorized());
    }

    if let Some(msg) = cw20_msg.msg {
        match from_binary(&msg)? {
            TokenCw20HookMsg::Migrate {} => migrate(deps, env, cw20_msg.sender, cw20_msg.amount),
        }
    } else {
        Err(StdError::generic_err("data should be given"))
    }
}

/// CONTRACT: always called from receive_cw20 after checking the migration info
fn migrate<S: Storage, A: Api, Q: Querier>(
    deps: &mut Extern<S, A, Q>,
    _env: Env,
    migrator: HumanAddr,
    amount: Uint128,
) -> HandleResult {
    let mut config = token_info_read(&deps.storage).load()?;
    let amount = amount * config.migration.as_ref().unwrap().conversion_rate;

    config.total_supply += amount;
    token_info(&mut deps.storage).save(&config)?;

    // add amount to recipient balance
    let migrator_raw = deps.api.canonical_address(&migrator)?;
    balances(&mut deps.storage).update(migrator_raw.as_slice(), |balance: Option<Uint128>| {
        Ok(balance.unwrap_or_default() + amount)
    })?;

    let res = HandleResponse {
        messages: vec![],
        log: vec![
            log("action", "migrate"),
            log("migrator", migrator),
            log("amount", amount),
        ],
        data: None,
    };
    Ok(res)
}

pub fn query_migration<S: Storage, A: Api, Q: Querier>(
    deps: &Extern<S, A, Q>,
) -> StdResult<Option<TokenMigrationResponse>> {
    let meta = token_info_read(&deps.storage).load()?;
    let migration = match meta.migration {
        Some(m) => Some(TokenMigrationResponse {
            token: deps.api.human_address(&m.token)?,
            conversion_rate: m.conversion_rate,
        }),
        None => None,
    };
    Ok(migration)
}

#[cfg(test)]
mod tests {
    use super::*;

    use cosmwasm_std::testing::{mock_dependencies, mock_env};
    use cosmwasm_std::{coins, to_binary, Decimal, Uint128};
    use cw20::{Cw20CoinHuman, TokenInfoResponse};

    use crate::contract::{handle, init, query_balance, query_token_info};
    use crate::msg::HandleMsg;
    use terraswap::TokenInitMsg;

    // this will set up the init for other tests
    fn do_init<S: Storage, A: Api, Q: Querier>(
        deps: &mut Extern<S, A, Q>,
        addr: &HumanAddr,
        amount: Uint128,
        migration_token: HumanAddr,
        conversion_rate: Decimal,
    ) -> TokenInfoResponse {
        let init_msg = TokenInitMsg {
            name: "Auto Gen".to_string(),
            symbol: "AUTO".to_string(),
            decimals: 3,
            initial_balances: vec![Cw20CoinHuman {
                address: addr.into(),
                amount,
            }],
            mint: None,
            init_hook: None,
            migration: Some(TokenMigrationResponse {
                token: migration_token,
                conversion_rate,
            }),
        };
        let env = mock_env(&HumanAddr("creator".to_string()), &[]);
        init(deps, env, init_msg).unwrap();
        query_token_info(&deps).unwrap()
    }

    #[test]
    fn migrate() {
        let mut deps = mock_dependencies(20, &coins(2, "token"));

        let owner = HumanAddr::from("owner");
        let migrator = HumanAddr::from("addr0001");
        let migration_token = HumanAddr::from("migration0001");
        let conversion_amount = Uint128::from(1000000u128);
        let conversion_rate = Decimal::from_ratio(2u128, 1u128); // x2
        let env = mock_env(migration_token.clone(), &[]);
        do_init(
            &mut deps,
            &owner,
            Uint128::zero(),
            migration_token.clone(),
            conversion_rate,
        );

        // no allowance to start
        let migration = query_migration(&deps).unwrap();
        assert_eq!(
            migration,
            Some(TokenMigrationResponse {
                token: migration_token,
                conversion_rate,
            })
        );

        let msg = HandleMsg::Receive(Cw20ReceiveMsg {
            sender: migrator.clone(),
            amount: conversion_amount,
            msg: Some(to_binary(&TokenCw20HookMsg::Migrate {}).unwrap()),
        });

        handle(&mut deps, env.clone(), msg).unwrap();
        let res = query_balance(&deps, migrator.clone()).unwrap();
        assert_eq!(res.balance, conversion_amount * conversion_rate);
    }
}
