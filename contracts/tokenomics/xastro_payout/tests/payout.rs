use cosmwasm_std::{coins, Addr, Empty, Uint128};
use serde::Deserialize;

use astroport::asset::AssetInfo;
use astroport_test::cw_multi_test::{AppBuilder, BankSudo, Contract, ContractWrapper, Executor};
use astroport_test::modules::stargate::{MockStargate, StargateApp};
use astroport_xastro_payout::error::ContractError;
use astroport_xastro_payout::merkle::{decode_hash, leaf_hash, node_hash, verify};
use astroport_xastro_payout::msg::{Config, ExecuteMsg, InstantiateMsg, Payment, QueryMsg, Status};

const ASTRO: &str = "factory/assembly/astro";

#[derive(Deserialize)]
struct Tree {
    merkle_root: String,
    total: String,
    payments: Vec<Payment>,
}

/// Same construction as merkle.py: leaves sorted by hash, odd node moves up unchanged.
fn build(entries: &[(&str, u128)]) -> (String, Vec<Payment>) {
    let mut leaves: Vec<_> = entries
        .iter()
        .map(|(a, n)| (leaf_hash(a, *n), a.to_string(), *n))
        .collect();
    leaves.sort();
    let mut levels = vec![leaves.iter().map(|l| l.0).collect::<Vec<_>>()];
    while levels.last().unwrap().len() > 1 {
        let cur = levels.last().unwrap();
        let next = cur
            .chunks(2)
            .map(|c| {
                if c.len() == 2 {
                    node_hash(&c[0], &c[1])
                } else {
                    c[0]
                }
            })
            .collect();
        levels.push(next);
    }
    let payments = leaves
        .iter()
        .enumerate()
        .map(|(idx, (_, address, amount))| {
            let mut i = idx;
            let mut proof = vec![];
            for level in &levels[..levels.len() - 1] {
                if let Some(sibling) = level.get(i ^ 1) {
                    proof.push(hex::encode(sibling));
                }
                i /= 2;
            }
            Payment {
                address: address.clone(),
                amount: Uint128::new(*amount),
                proof,
            }
        })
        .collect();
    (hex::encode(levels.last().unwrap()[0]), payments)
}

fn payout_contract() -> Box<dyn Contract<Empty>> {
    Box::new(ContractWrapper::new_with_empty(
        astroport_xastro_payout::contract::execute,
        astroport_xastro_payout::contract::instantiate,
        astroport_xastro_payout::contract::query,
    ))
}

fn setup(root: &str, total: u128, fund: u128) -> (StargateApp, Addr) {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {});
    let code = app.store_code(payout_contract());
    let payout = app
        .instantiate_contract(
            code,
            Addr::unchecked("owner"),
            &InstantiateMsg {
                owner: "owner".to_string(),
                sweep_recipient: "treasury".to_string(),
                asset_info: AssetInfo::native(ASTRO),
                merkle_root: root.to_string(),
                total: Uint128::new(total),
            },
            &[],
            "payout",
            Some("owner".to_string()),
        )
        .unwrap();
    if fund > 0 {
        app.sudo(
            BankSudo::Mint {
                to_address: payout.to_string(),
                amount: coins(fund, ASTRO),
            }
            .into(),
        )
        .unwrap();
    }
    (app, payout)
}

fn balance(app: &StargateApp, addr: &str) -> u128 {
    app.wrap().query_balance(addr, ASTRO).unwrap().amount.u128()
}

fn err(e: anyhow::Error) -> ContractError {
    e.downcast().unwrap()
}

const LIST: [(&str, u128); 5] = [
    ("neutron1alice", 1000),
    ("neutron1bob", 2500),
    ("neutron1carol", 7),
    ("neutron1dave", 123456),
    ("neutron1erin", 1),
];

#[test]
fn python_tree_matches_contract() {
    let tree: Tree = serde_json::from_str(include_str!("fixtures/sample_tree.json")).unwrap();
    let (root, _) = build(&LIST);
    assert_eq!(tree.merkle_root, root);
    assert_eq!(
        tree.total,
        LIST.iter().map(|(_, n)| n).sum::<u128>().to_string()
    );

    let root = decode_hash(&tree.merkle_root).unwrap();
    for p in &tree.payments {
        let proof: Vec<_> = p.proof.iter().map(|h| decode_hash(h).unwrap()).collect();
        assert!(verify(
            &root,
            leaf_hash(&p.address, p.amount.u128()),
            &proof
        ));
        // the amount is part of the leaf
        assert!(!verify(
            &root,
            leaf_hash(&p.address, p.amount.u128() + 1),
            &proof
        ));
    }
}

#[test]
fn pays_each_address_once() {
    let total = LIST.iter().map(|(_, n)| n).sum::<u128>();
    let (root, payments) = build(&LIST);
    let (mut app, payout) = setup(&root, total, total);
    let bot = Addr::unchecked("bot");

    // anyone can push a batch; each entry goes to its own address
    app.execute_contract(
        bot.clone(),
        payout.clone(),
        &ExecuteMsg::Pay {
            payments: payments[..3].to_vec(),
        },
        &[],
    )
    .unwrap();
    for p in &payments[..3] {
        assert_eq!(balance(&app, &p.address), p.amount.u128());
    }
    assert_eq!(balance(&app, bot.as_str()), 0);

    // a retried batch, and a repeat inside one batch, are skipped
    let mut retry = payments.clone();
    retry.push(payments[4].clone());
    let res = app
        .execute_contract(
            bot.clone(),
            payout.clone(),
            &ExecuteMsg::Pay { payments: retry },
            &[],
        )
        .unwrap();
    let wasm = res.events.iter().find(|e| e.ty == "wasm").unwrap();
    let get = |k: &str| {
        wasm.attributes
            .iter()
            .find(|a| a.key == k)
            .unwrap()
            .value
            .clone()
    };
    assert_eq!(get("paid"), "2");
    assert_eq!(get("skipped"), "4");

    for (address, amount) in LIST {
        assert_eq!(balance(&app, address), amount);
        let paid: Option<Uint128> = app
            .wrap()
            .query_wasm_smart(
                &payout,
                &QueryMsg::Paid {
                    address: address.to_string(),
                },
            )
            .unwrap();
        assert_eq!(paid, Some(Uint128::new(amount)));
    }
    assert_eq!(balance(&app, payout.as_str()), 0);

    let status: Status = app
        .wrap()
        .query_wasm_smart(&payout, &QueryMsg::Status {})
        .unwrap();
    assert_eq!(
        status,
        Status {
            paid_total: Uint128::new(total),
            paid_count: 5,
            closed: false
        }
    );
    let list: Vec<(Addr, Uint128)> = app
        .wrap()
        .query_wasm_smart(
            &payout,
            &QueryMsg::PaidList {
                start_after: Some("neutron1bob".to_string()),
                limit: Some(2),
            },
        )
        .unwrap();
    assert_eq!(
        list,
        vec![
            (Addr::unchecked("neutron1carol"), Uint128::new(7)),
            (Addr::unchecked("neutron1dave"), Uint128::new(123456)),
        ]
    );
}

#[test]
fn rejects_forged_entries() {
    let total = LIST.iter().map(|(_, n)| n).sum::<u128>();
    let (root, payments) = build(&LIST);
    let (mut app, payout) = setup(&root, total, total);
    let bot = Addr::unchecked("bot");

    let pay = |app: &mut StargateApp, p: Payment| {
        app.execute_contract(
            bot.clone(),
            payout.clone(),
            &ExecuteMsg::Pay { payments: vec![p] },
            &[],
        )
        .map_err(err)
    };

    // more than the listed amount
    let mut p = payments[0].clone();
    p.amount += Uint128::one();
    assert_eq!(
        pay(&mut app, p.clone()).unwrap_err(),
        ContractError::InvalidProof { address: p.address }
    );

    // someone else's proof for a different address
    let mut p = payments[0].clone();
    p.address = "neutron1mallory".to_string();
    assert_eq!(
        pay(&mut app, p).unwrap_err(),
        ContractError::InvalidProof {
            address: "neutron1mallory".to_string()
        }
    );

    // a missing proof
    let mut p = payments[0].clone();
    p.proof.clear();
    assert!(pay(&mut app, p).is_err());

    // garbage proof
    let mut p = payments[0].clone();
    p.proof = vec!["zz".to_string()];
    assert!(pay(&mut app, p).is_err());

    // one bad entry fails the whole batch
    let mut bad = payments[1].clone();
    bad.amount = Uint128::new(1);
    app.execute_contract(
        bot.clone(),
        payout.clone(),
        &ExecuteMsg::Pay {
            payments: vec![payments[0].clone(), bad],
        },
        &[],
    )
    .unwrap_err();
    assert_eq!(balance(&app, &payments[0].address), 0);

    // funds can't be attached
    app.sudo(
        BankSudo::Mint {
            to_address: bot.to_string(),
            amount: coins(5, ASTRO),
        }
        .into(),
    )
    .unwrap();
    let e = app
        .execute_contract(
            bot.clone(),
            payout.clone(),
            &ExecuteMsg::Pay {
                payments: vec![payments[0].clone()],
            },
            &coins(5, ASTRO),
        )
        .unwrap_err();
    assert!(matches!(err(e), ContractError::Payment(_)));
}

#[test]
fn never_pays_more_than_total() {
    let (root, payments) = build(&LIST);
    // a list total set too low stops payments once it is reached
    let (mut app, payout) = setup(&root, 3000, 1_000_000);
    let big = payments
        .iter()
        .find(|p| p.address == "neutron1dave")
        .unwrap()
        .clone();
    let e = app
        .execute_contract(
            Addr::unchecked("bot"),
            payout,
            &ExecuteMsg::Pay {
                payments: vec![big],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::TotalExceeded {});
}

#[test]
fn owner_closes_and_sweeps() {
    let total = LIST.iter().map(|(_, n)| n).sum::<u128>();
    let (root, payments) = build(&LIST);
    let (mut app, payout) = setup(&root, total, total);

    app.execute_contract(
        Addr::unchecked("bot"),
        payout.clone(),
        &ExecuteMsg::Pay {
            payments: payments[..2].to_vec(),
        },
        &[],
    )
    .unwrap();
    let left = total - payments[..2].iter().map(|p| p.amount.u128()).sum::<u128>();

    let close = ExecuteMsg::Close {};
    let e = app
        .execute_contract(Addr::unchecked("bot"), payout.clone(), &close, &[])
        .unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});

    app.execute_contract(Addr::unchecked("owner"), payout.clone(), &close, &[])
        .unwrap();
    assert_eq!(balance(&app, "treasury"), left);
    assert_eq!(balance(&app, payout.as_str()), 0);

    let e = app
        .execute_contract(
            Addr::unchecked("bot"),
            payout.clone(),
            &ExecuteMsg::Pay {
                payments: payments[2..].to_vec(),
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::Closed {});

    // closing again with nothing left is fine
    app.execute_contract(Addr::unchecked("owner"), payout.clone(), &close, &[])
        .unwrap();
}

#[test]
fn ownership_transfer() {
    let (root, _) = build(&LIST);
    let (mut app, payout) = setup(&root, 1, 0);

    app.execute_contract(
        Addr::unchecked("owner"),
        payout.clone(),
        &ExecuteMsg::ProposeNewOwner {
            owner: "dao".to_string(),
            expires_in: 100,
        },
        &[],
    )
    .unwrap();
    app.execute_contract(
        Addr::unchecked("dao"),
        payout.clone(),
        &ExecuteMsg::ClaimOwnership {},
        &[],
    )
    .unwrap();
    let config: Config = app
        .wrap()
        .query_wasm_smart(&payout, &QueryMsg::Config {})
        .unwrap();
    assert_eq!(config.owner, Addr::unchecked("dao"));
    assert_eq!(config.merkle_root, root);
    assert_eq!(config.asset_info, AssetInfo::native(ASTRO));

    // the old owner lost Close
    let e = app
        .execute_contract(Addr::unchecked("owner"), payout, &ExecuteMsg::Close {}, &[])
        .unwrap_err();
    assert_eq!(err(e), ContractError::Unauthorized {});
}

#[test]
fn rejects_bad_root() {
    let mut app = AppBuilder::new_custom()
        .with_stargate(MockStargate::default())
        .build(|_, _, _| {});
    let code = app.store_code(payout_contract());
    let e = app
        .instantiate_contract(
            code,
            Addr::unchecked("owner"),
            &InstantiateMsg {
                owner: "owner".to_string(),
                sweep_recipient: "treasury".to_string(),
                asset_info: AssetInfo::native(ASTRO),
                merkle_root: "abcd".to_string(),
                total: Uint128::new(1),
            },
            &[],
            "payout",
            None,
        )
        .unwrap_err();
    assert_eq!(err(e), ContractError::InvalidRoot {});
}

#[test]
fn one_leaf_tree_has_an_empty_proof() {
    let (root, payments) = build(&[("neutron1alice", 1000)]);
    assert!(payments[0].proof.is_empty());
    let (mut app, payout) = setup(&root, 1000, 1000);
    app.execute_contract(
        Addr::unchecked("bot"),
        payout,
        &ExecuteMsg::Pay { payments },
        &[],
    )
    .unwrap();
    assert_eq!(balance(&app, "neutron1alice"), 1000);
}

#[test]
fn inner_node_is_not_a_leaf() {
    let list = [("neutron1alice", 1000), ("neutron1bob", 2500)];
    let (root, payments) = build(&list);
    let (mut app, payout) = setup(&root, 3500, 3500);

    // a leaf can't stand in for the root with an empty proof
    let mut p = payments[0].clone();
    p.proof.clear();
    let e = app
        .execute_contract(
            Addr::unchecked("bot"),
            payout.clone(),
            &ExecuteMsg::Pay { payments: vec![p] },
            &[],
        )
        .unwrap_err();
    assert!(matches!(err(e), ContractError::InvalidProof { .. }));

    // the root's two children, re-read as "address:amount" bytes, can't be a leaf either: leaves
    // and nodes hash under different prefixes, so no string reproduces a node hash
    let a = leaf_hash(list[0].0, list[0].1);
    let b = leaf_hash(list[1].0, list[1].1);
    assert_eq!(hex::encode(node_hash(&a, &b)), root);
    assert_ne!(leaf_hash(&hex::encode(a), 0), a);
}

#[test]
fn zero_amount_is_refused() {
    let (root, payments) = build(&[("neutron1alice", 0), ("neutron1bob", 5)]);
    let (mut app, payout) = setup(&root, 5, 5);
    let zero = payments
        .iter()
        .find(|p| p.address == "neutron1alice")
        .unwrap()
        .clone();
    let e = app
        .execute_contract(
            Addr::unchecked("bot"),
            payout,
            &ExecuteMsg::Pay {
                payments: vec![zero],
            },
            &[],
        )
        .unwrap_err();
    assert_eq!(
        err(e),
        ContractError::ZeroAmount {
            address: "neutron1alice".to_string()
        }
    );
}
