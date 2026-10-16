//! Actual Python paths, terminal phases and independently recomputed costs.
use lantern_core::{
    search::{Access, Outcome, Query, Search, Target, Terrain},
    spatial::{Footprint, LandAccess, RoadMode},
    units::Tile,
};
use lantern_host::legacy::PrivateSpatialCopy;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    process::Command,
    rc::Rc,
};
fn tile(v: &Value) -> Tile {
    Tile {
        x: v[0].as_i64().unwrap().try_into().unwrap(),
        y: v[1].as_i64().unwrap().try_into().unwrap(),
    }
}
fn tiles(v: &Value) -> BTreeSet<Tile> {
    v.as_array().unwrap().iter().map(tile).collect()
}
#[test]
fn frozen_python_routes_match_core_paths_costs_and_quantum_partitions() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/search/python.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let source = serde_json::to_vec(&case["source"]).unwrap();
        let copy = PrivateSpatialCopy::decode(&source).unwrap();
        let index = Rc::new(copy.derive_index().unwrap());
        let p = &case["target"];
        let anchor = || Tile {
            x: p["x"].as_i64().unwrap().try_into().unwrap(),
            y: p["y"].as_i64().unwrap().try_into().unwrap(),
        };
        let target = match case["kind"].as_str().unwrap() {
            "exact" => Target::Exact(anchor()),
            "anchor" => Target::AnchorAdjacent(anchor()),
            "multiple" => Target::Multiple(tiles(p)),
            "perimeter" => Target::Perimeter(
                Footprint::saved(
                    anchor(),
                    Some((
                        p["footprint"][0].as_u64().unwrap().try_into().unwrap(),
                        p["footprint"][1].as_u64().unwrap().try_into().unwrap(),
                    )),
                )
                .unwrap(),
            ),
            _ => panic!("unknown fixture target"),
        };
        let cells = case["terrain"].as_array().map(|rows| {
            let mut cells = BTreeMap::new();
            for (y, row) in rows.iter().enumerate() {
                for (x, c) in row.as_array().unwrap().iter().enumerate() {
                    if let Some(c) = c.as_str().unwrap().chars().next() {
                        cells.insert(
                            Tile {
                                x: x.try_into().unwrap(),
                                y: y.try_into().unwrap(),
                            },
                            c,
                        );
                    }
                }
            }
            cells
        });
        let bounds = case["bounds"].as_array().map(|a| {
            (
                a[0].as_i64().unwrap() as i32,
                a[1].as_i64().unwrap() as i32,
                a[2].as_i64().unwrap() as i32,
                a[3].as_i64().unwrap() as i32,
            )
        });
        let access = Access {
            land: match case["profile"].as_str().unwrap() {
                "ordinary" => LandAccess::Ordinary,
                "farmer" => LandAccess::Farmer,
                "construction" => LandAccess::Construction,
                _ => panic!("profile"),
            },
            exemptions: tiles(&case["exemptions"]),
            only: case["only"].as_array().map(|_| tiles(&case["only"])),
        };
        let mode = match case["mode"].as_str().unwrap() {
            "smooth" => RoadMode::Smooth,
            "departure" => RoadMode::Departure,
            "arrival" => RoadMode::Arrival,
            _ => panic!("mode"),
        };
        let query = Rc::new(
            Query::new(
                index.clone(),
                Terrain { cells, bounds },
                access,
                tile(&case["start"]),
                target,
                case["diagonal"].as_bool().unwrap(),
                mode,
            )
            .unwrap(),
        );
        for quantum in [1, 2, 7, usize::MAX] {
            let mut search = Search::new(query.clone());
            while search.outcome() == &Outcome::Pending {
                let work = search.work;
                assert_eq!(search.advance(0), 0);
                assert_eq!(search.work, work);
                assert!(search.advance(quantum) <= quantum);
                assert!(
                    search.work < 100_000,
                    "{name}: finite fixture exhausted work bound"
                );
            }
            match search.into_outcome() {
                Outcome::Arrived => {
                    assert_eq!(case["state"], "arrived", "{name}");
                    assert_eq!(case["cost"], 0.0);
                }
                Outcome::Unreachable => assert_eq!(case["state"], "unreachable", "{name}"),
                Outcome::Completed { path, cost } => {
                    assert_eq!(case["state"], "completed", "{name}");
                    let expected: Vec<_> =
                        case["path"].as_array().unwrap().iter().map(tile).collect();
                    assert_eq!(path, expected, "{name}: deterministic Python path");
                    assert!(
                        (cost - case["cost"].as_f64().unwrap()).abs() < 1e-9,
                        "{name}: cost"
                    );
                    assert!(
                        (cost - case["oracle_cost"].as_f64().unwrap()).abs() < 1e-9,
                        "{name}: independent optimum"
                    );
                }
                other => panic!("{name}: unexpected terminal {other:?}"),
            }
        }
        let mut cancelled = Search::new(query);
        if cancelled.outcome() == &Outcome::Pending {
            cancelled.advance(1);
            if cancelled.outcome() == &Outcome::Pending {
                cancelled.cancel();
                assert_eq!(cancelled.advance(100), 0);
                assert_eq!(cancelled.outcome(), &Outcome::Cancelled);
            }
        }
        assert_eq!(copy.private_bytes(), source);
        assert_eq!(index.blocked(), copy.derive_index().unwrap().blocked());
    }
}
#[test]
fn python_fixture_check_recomputes_routes_and_independent_oracle_without_updating() {
    let status = Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/search/generate.py"
        ))
        .arg("--check")
        .status()
        .unwrap();
    assert!(status.success());
}
