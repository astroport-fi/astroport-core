use cosmwasm_std::{to_json_binary, Decimal};

use astroport::asset::AssetInfo;
use astroport::incentives::{RewardInfo, RewardType};
use astroport_incentives::state::{PoolInfo, UserInfo};

fn main() {
    // This example shows us a rough estimations of gas costs for storage operations charged by Cosmos SDK.
    // It doesn't include costs charged within Wasm VM to serialize/deserialize data into Rust structures.
    // Given that we allow 5 external rewards and 1 ASTRO reward per pool, we can estimate the following gas costs:

    let reward_info = RewardInfo {
        reward: RewardType::Ext {
            info: AssetInfo::native("test"),
            next_update_ts: 0,
        },
        rps: Default::default(),
        index: Default::default(),
        orphaned: Default::default(),
    };
    let pool_info = PoolInfo::default();

    // https://github.com/cosmos/cosmos-sdk/blob/47f46643affd7ec7978329c42bac47275ac7e1cc/store/types/gas.go#L199
    let reward_info_storage_bytes = to_json_binary(&reward_info).unwrap().len();
    println!("reward info storage bytes {reward_info_storage_bytes}");
    println!("sdk gas cost per read {}", reward_info_storage_bytes * 3);
    println!("sdk gas cost per write {}", reward_info_storage_bytes * 30);

    let pool_info_storage_bytes = to_json_binary(&pool_info).unwrap().len();
    println!("pool info storage bytes {pool_info_storage_bytes}");
    println!("sdk gas cost per read {}", pool_info_storage_bytes * 3);
    println!("sdk gas cost per write {}", pool_info_storage_bytes * 30);

    // Gas costs for a pool with 4 + 1 rewards
    let pool_storage_bytes = pool_info_storage_bytes + 6 * reward_info_storage_bytes;
    println!("pool with 5 + 1 rewards storage bytes {pool_storage_bytes}");
    println!("sdk gas cost per read {}", pool_storage_bytes * 3);
    println!("sdk gas cost per write {}", pool_storage_bytes * 30);

    let user_info = UserInfo {
        amount: Default::default(),
        last_rewards_index: Default::default(),
        last_claim_time: 0,
    };
    let user_info_storage_bytes = to_json_binary(&user_info).unwrap().len();
    println!("user info storage bytes {user_info_storage_bytes}");
    println!("sdk gas cost per read {}", user_info_storage_bytes * 3);
    println!("sdk gas cost per write {}", user_info_storage_bytes * 30);

    // Gas costs for a pool with 5 + 1 rewards
    let reward_index_entry = (
        RewardType::Ext {
            info: AssetInfo::native("test"),
            next_update_ts: 0,
        },
        Decimal::zero(),
    );
    let reward_entry_storage_bytes = to_json_binary(&reward_index_entry).unwrap().len();
    let user_storage_bytes = user_info_storage_bytes + 6 * reward_entry_storage_bytes;
    println!("user with 5 + 1 rewards storage bytes {user_storage_bytes}");
    println!("sdk gas cost per read {}", user_storage_bytes * 3);
    println!("sdk gas cost per write {}", user_storage_bytes * 30);
}
