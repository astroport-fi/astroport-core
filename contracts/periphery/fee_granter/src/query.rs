#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_binary, Binary, Deps, Env, Order, StdResult};
use cw_storage_plus::Bound;

use crate::state::{CONFIG, GRANTS};
use astroport::fee_granter::{GrantResponse, QueryMsg};

/// Default pagination limit
const DEFAULT_LIMIT: u32 = 50;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_binary(&CONFIG.load(deps.storage)?),
        QueryMsg::GrantsList { start_after, limit } => {
            to_binary(&list_grants(deps, start_after, limit)?)
        }
        QueryMsg::GrantFor { grantee_contract } => to_binary(&grant_for(deps, grantee_contract)?),
    }
}

fn list_grants(
    deps: Deps,
    start_after: Option<String>,
    limit: Option<u32>,
) -> StdResult<Vec<GrantResponse>> {
    let start_after = start_after
        .map(|addr| deps.api.addr_validate(&addr))
        .transpose()?;
    let start_after = start_after.as_ref().map(Bound::exclusive);
    GRANTS
        .range(deps.storage, start_after, None, Order::Ascending)
        .take(limit.unwrap_or(DEFAULT_LIMIT) as usize)
        .map(|item| {
            let (k, amount) = item?;
            Ok(GrantResponse {
                grantee_contract: k.to_string(),
                amount,
            })
        })
        .collect()
}

fn grant_for(deps: Deps, grantee_contract: String) -> StdResult<GrantResponse> {
    let grantee_contract = deps.api.addr_validate(&grantee_contract)?;
    let amount = GRANTS
        .may_load(deps.storage, &grantee_contract)?
        .unwrap_or_default();
    Ok(GrantResponse {
        grantee_contract: grantee_contract.to_string(),
        amount,
    })
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use crate::contract::{execute, instantiate};
    use astroport::fee_granter::{Config, ExecuteMsg, InstantiateMsg};
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, Addr, Uint128};

    const GAS_DENOM: &str = "inj";

    #[test]
    fn test_queries() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        let info = mock_info("owner", &[]);

        let msg = InstantiateMsg {
            owner: "owner".to_string(),
            admins: vec!["admin".to_string()],
            gas_denom: GAS_DENOM.to_string(),
        };
        instantiate(deps.as_mut(), env.clone(), info.clone(), msg).unwrap();

        let msg = ExecuteMsg::Grant {
            grantee_contract: "contract100".to_string(),
            amount: 100u128.into(),
            bypass_amount_check: false,
        };
        let info = mock_info("owner", &coins(100, GAS_DENOM));
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        let msg = ExecuteMsg::Grant {
            grantee_contract: "contract200".to_string(),
            amount: 200u128.into(),
            bypass_amount_check: false,
        };
        let info = mock_info("admin", &coins(200, GAS_DENOM));
        execute(deps.as_mut(), env.clone(), info, msg).unwrap();

        let resp = query(deps.as_ref(), env.clone(), QueryMsg::Config {}).unwrap();
        let config: Config = from_binary(&resp).unwrap();

        assert_eq!(
            config,
            Config {
                owner: Addr::unchecked("owner".to_string()),
                admins: vec![Addr::unchecked("admin".to_string())],
                gas_denom: GAS_DENOM.to_string(),
            }
        );

        let resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GrantFor {
                grantee_contract: "contract100".to_string(),
            },
        )
        .unwrap();
        let config: GrantResponse = from_binary(&resp).unwrap();
        assert_eq!(
            config,
            GrantResponse {
                grantee_contract: "contract100".to_string(),
                amount: 100u128.into(),
            }
        );

        let resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GrantFor {
                grantee_contract: "random_contract".to_string(),
            },
        )
        .unwrap();
        let config: GrantResponse = from_binary(&resp).unwrap();
        assert_eq!(
            config,
            GrantResponse {
                grantee_contract: "random_contract".to_string(),
                amount: Uint128::zero(),
            }
        );

        let resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GrantsList {
                start_after: None,
                limit: Some(1),
            },
        )
        .unwrap();
        let config: Vec<GrantResponse> = from_binary(&resp).unwrap();
        assert_eq!(
            config,
            [GrantResponse {
                grantee_contract: "contract100".to_string(),
                amount: 100u128.into(),
            }]
        );

        let resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GrantsList {
                start_after: Some("contract100".to_string()),
                limit: None,
            },
        )
        .unwrap();
        let config: Vec<GrantResponse> = from_binary(&resp).unwrap();
        assert_eq!(
            config,
            [GrantResponse {
                grantee_contract: "contract200".to_string(),
                amount: 200u128.into(),
            }]
        );

        let resp = query(
            deps.as_ref(),
            env.clone(),
            QueryMsg::GrantsList {
                start_after: None,
                limit: None,
            },
        )
        .unwrap();
        let config: Vec<GrantResponse> = from_binary(&resp).unwrap();
        assert_eq!(
            config,
            [
                GrantResponse {
                    grantee_contract: "contract100".to_string(),
                    amount: 100u128.into(),
                },
                GrantResponse {
                    grantee_contract: "contract200".to_string(),
                    amount: 200u128.into(),
                }
            ]
        );
    }
}
