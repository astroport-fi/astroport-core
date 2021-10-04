use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::vesting::{OrderBy, VestingInfo};
use cosmwasm_std::{Addr, Deps, StdResult};
use cw_storage_plus::{Bound, Item, Map};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub owner: Addr,
    pub token_addr: Addr,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const VESTING_INFO: Map<&Addr, VestingInfo> = Map::new("vesting_info");

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

pub fn read_vesting_infos(
    deps: Deps,
    start_after: Option<Addr>,
    limit: Option<u32>,
    order_by: Option<OrderBy>,
) -> StdResult<Vec<(Addr, VestingInfo)>> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start_after = start_after.map(|v| Bound::Exclusive(v.as_bytes().to_vec()));
    let (start, end) = match &order_by {
        Some(OrderBy::Asc) => (start_after, None),
        _ => (None, start_after),
    };

    let info: Vec<(Addr, VestingInfo)> = VESTING_INFO
        .range(
            deps.storage,
            start,
            end,
            order_by.unwrap_or(OrderBy::Desc).into(),
        )
        .take(limit)
        .filter_map(|v| v.ok())
        .map(|(k, v)| (Addr::unchecked(String::from_utf8(k).unwrap()), v))
        .collect();

    Ok(info)
}

#[test]
fn read_vesting_infos_as_expected() {
    use cosmwasm_std::{testing::mock_dependencies, Uint128};

    let mut deps = mock_dependencies(&[]);

    let vi_mock = VestingInfo {
        released_amount: Uint128::zero(),
        schedules: vec![],
    };

    for i in 1..5 {
        let key = Addr::unchecked(format! {"address{}", i});

        VESTING_INFO
            .save(&mut deps.storage, &key, &vi_mock)
            .unwrap();
    }

    let res = read_vesting_infos(
        deps.as_ref(),
        Some(Addr::unchecked("address2")),
        None,
        Some(OrderBy::Asc),
    )
    .unwrap();
    assert_eq!(
        res,
        vec![
            (Addr::unchecked("address3"), vi_mock.clone()),
            (Addr::unchecked("address4"), vi_mock.clone())
        ]
    );

    let res = read_vesting_infos(
        deps.as_ref(),
        Some(Addr::unchecked("address2")),
        Some(1),
        Some(OrderBy::Asc),
    )
    .unwrap();
    assert_eq!(res, vec![(Addr::unchecked("address3"), vi_mock.clone())]);

    let res = read_vesting_infos(
        deps.as_ref(),
        Some(Addr::unchecked("address3")),
        None,
        Some(OrderBy::Desc),
    )
    .unwrap();
    assert_eq!(
        res,
        vec![
            (Addr::unchecked("address2"), vi_mock.clone()),
            (Addr::unchecked("address1"), vi_mock.clone())
        ]
    );

    let res = read_vesting_infos(
        deps.as_ref(),
        Some(Addr::unchecked("address3")),
        Some(1),
        Some(OrderBy::Desc),
    )
    .unwrap();
    assert_eq!(res, vec![(Addr::unchecked("address2"), vi_mock.clone())]);
}
