#![cfg(not(tarpaulin_include))]
use cosmwasm_std::testing::{mock_dependencies, mock_env};
use cosmwasm_std::{Reply, SubMsgResponse, SubMsgResult};

use astroport_liquidity_manager::contract::reply;
use astroport_liquidity_manager::error::ContractError;
use astroport_liquidity_manager::state::{ActionParams, ReplyData, REPLY_DATA};

#[test]
fn test_reply() {
    let mut deps = mock_dependencies();
    let env = mock_env();
    let msg = Reply {
        id: 2000,
        result: SubMsgResult::Ok(SubMsgResponse {
            events: vec![],
            data: None,
        }),
    };

    let err = reply(deps.as_mut(), env.clone(), msg).unwrap_err();
    assert_eq!(err.to_string(), "Generic error: Unsupported reply id 2000");

    let msg = Reply {
        id: 1,
        result: SubMsgResult::Ok(SubMsgResponse {
            events: vec![],
            data: None,
        }),
    };

    // Storing wrong REPLY_DATA which doesn't match reply.id = 1
    REPLY_DATA
        .save(
            deps.as_mut().storage,
            &ReplyData {
                receiver: "".to_string(),
                params: ActionParams::Provide {
                    lp_token_addr: "".to_string(),
                    lp_amount_before: Default::default(),
                    staked_in_generator: false,
                    min_lp_to_receive: Default::default(),
                },
            },
        )
        .unwrap();

    let err = reply(deps.as_mut(), env, msg).unwrap_err();
    assert_eq!(err, ContractError::InvalidReplyData {});
}
