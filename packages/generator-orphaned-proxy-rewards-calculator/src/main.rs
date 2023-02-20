use astroport::generator::{QueryMsg, RewardInfoResponse};
use astroport_generator::state::{CompatibleLoader, POOL_INFO, USER_INFO};
use cosmrs::proto::{
    cosmos::base::query::v1beta1::PageRequest,
    cosmwasm::wasm::v1::{
        query_client::QueryClient, QueryAllContractStateRequest, QuerySmartContractStateRequest,
    },
};
use cosmwasm_std::{
    from_slice, to_vec, Addr, Decimal, MemoryStorage, Order, StdError, Storage, Uint128,
};
use cw20::{BalanceResponse, Cw20QueryMsg, TokenInfoResponse};
use cw_storage_plus::KeyDeserialize;
use serde::{de::DeserializeOwned, Serialize};
use tonic::{transport::Channel, Request};

pub trait AtBlockHeight
where
    Self: Sized,
{
    /// height 0 is actually the latest block height
    fn at_block_height(self, height: u64) -> Request<Self> {
        let mut req = Request::new(self);
        req.metadata_mut()
            .append("x-cosmos-block-height", height.into());
        req
    }
}

impl<T> AtBlockHeight for T {}

pub async fn fetch_contract_state(
    query_client: &mut QueryClient<Channel>,
    block_height: u64,
    contract: &str,
) -> Result<MemoryStorage, Box<dyn std::error::Error>> {
    let mut memory = MemoryStorage::new();
    let mut key = vec![];

    loop {
        let query = QueryAllContractStateRequest {
            address: contract.to_owned(),
            pagination: Some(PageRequest {
                key: key.clone(),
                ..Default::default()
            }),
        };

        let res = query_client
            .all_contract_state(query.at_block_height(block_height))
            .await?
            .into_inner();

        for model in res.models {
            memory.set(&model.key, &model.value);
        }

        key = res.pagination.unwrap().next_key;

        if key.is_empty() {
            break;
        }
    }

    Ok(memory)
}

pub async fn query_contract_smart<T: Serialize, R: DeserializeOwned>(
    query_client: &mut QueryClient<Channel>,
    block_height: u64,
    contract: &str,
    msg: &T,
) -> Result<R, Box<dyn std::error::Error>> {
    let query_data = to_vec(msg)?;
    let query = QuerySmartContractStateRequest {
        address: contract.to_owned(),
        query_data,
    };
    let res = query_client
        .smart_contract_state(query.at_block_height(block_height))
        .await?
        .into_inner();
    Ok(from_slice(&res.data)?)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut query_client = QueryClient::connect("https://terra-grpc.polkachu.com:11790").await?;

    const BLOCK_HEIGHT: u64 = 3690036;

    let generator = "terra1ksvlfex49desf4c452j6dewdjs6c48nafemetuwjyj6yexd7x3wqvwa7j9";

    let generator_state = fetch_contract_state(&mut query_client, BLOCK_HEIGHT, generator).await?;

    let pools_with_proxies = POOL_INFO
        .range(&generator_state, None, None, Order::Ascending)
        .filter_map(|res| match res {
            Ok((lp_token, pool_info)) => {
                if let Some(reward_proxy) = pool_info.reward_proxy {
                    let rewards_per_share: Vec<(Addr, Decimal)> = pool_info
                        .accumulated_proxy_rewards_per_share
                        .inner_ref()
                        .iter()
                        .filter(|v| v.0 == reward_proxy)
                        .cloned()
                        .collect();
                    assert!(rewards_per_share.len() == 1);
                    Some(Ok((lp_token, reward_proxy, rewards_per_share[0].1)))
                } else {
                    None
                }
            }
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<(Addr, Addr, Decimal)>, StdError>>()?;

    for (lp_token, reward_proxy, global_index) in &pools_with_proxies {
        let keys = USER_INFO
            .prefix(lp_token)
            .keys_raw(&generator_state, None, None, Order::Ascending)
            .collect::<Vec<Vec<u8>>>();

        let mut amount = Uint128::zero();
        for key in keys {
            let user = <Addr>::from_vec(key)?;
            let user_info = USER_INFO.compatible_load(&generator_state, (lp_token, &user))?;
            let reward_dept_proxy: Vec<(Addr, Uint128)> = user_info
                .reward_debt_proxy
                .inner_ref()
                .iter()
                .filter(|v| v.0 == *reward_proxy)
                .cloned()
                .collect();

            let reward_dept_proxy = match reward_dept_proxy.len() {
                0 => Uint128::zero(),
                1 => reward_dept_proxy[0].1,
                _ => unreachable!(),
            };
            amount += *global_index * user_info.amount - reward_dept_proxy;
        }

        // Proxy reward token
        let reward_info: RewardInfoResponse = query_contract_smart(
            &mut query_client,
            BLOCK_HEIGHT,
            generator,
            &QueryMsg::RewardInfo {
                lp_token: lp_token.to_string(),
            },
        )
        .await?;
        let proxy_reward_token = reward_info.proxy_reward_token.unwrap();

        // Proxy reward token's info
        let token_info: TokenInfoResponse = query_contract_smart(
            &mut query_client,
            BLOCK_HEIGHT,
            proxy_reward_token.as_str(),
            &Cw20QueryMsg::TokenInfo {},
        )
        .await?;

        // Proxy reward token balance
        let br: BalanceResponse = query_contract_smart(
            &mut query_client,
            BLOCK_HEIGHT,
            proxy_reward_token.as_str(),
            &Cw20QueryMsg::Balance {
                address: reward_proxy.to_string(),
            },
        )
        .await?;

        println!(
            "token: {}, orphaned: {}, proxy: {}",
            token_info.symbol,
            br.balance - amount,
            reward_proxy
        )
    }

    Ok(())
}
