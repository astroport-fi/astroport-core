use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use astroport::common::OwnershipProposal;
use astroport::vesting::{OrderBy, VestingInfo};
use cosmwasm_std::{Addr, Deps, StdResult};
use cw_storage_plus::{Bound, Item, Map};

/// ## Description
/// This structure describes the main control config of vesting contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    /// contract address that controls settings
    pub owner: Addr,
    /// The vesting token contract address
    pub token_addr: Addr,
}

/// ## Description
/// Stores config at the given key
pub const CONFIG: Item<Config> = Item::new("config");

/// ## Description
/// The first key part is account contract address, the second key part is an object of type [`VestingInfo`].
pub const VESTING_INFO: Map<&Addr, VestingInfo> = Map::new("vesting_info");

/// ## Description
/// Contains proposal for change ownership.
pub const OWNERSHIP_PROPOSAL: Item<OwnershipProposal> = Item::new("ownership_proposal");

const MAX_LIMIT: u32 = 30;
const DEFAULT_LIMIT: u32 = 10;

/// ## Description
/// Returns the empty vector if does not found data to read, otherwise returns the vector that
/// contains the objects of type [`VESTING_INFO`].
/// ## Params
/// * **deps** is the object of type [`Deps`].
///
/// * **start_after** is an [`Option`] field of type [`Addr`]. Sets the index to start reading.
///
/// * **limit** is an [`Option`] field of type [`u32`]. Sets the limit to reading.
///
/// * **order_by** is an [`Option`] field of type [`OrderBy`].
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
