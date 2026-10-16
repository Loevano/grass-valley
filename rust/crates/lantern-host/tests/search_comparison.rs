#[path = "../examples/search_support/mod.rs"]
mod search_support;
use lantern_core::spatial::{Construction, LandAccess, Road, RoadMode};
use search_support::*;
use std::{cell::Cell, collections::BTreeSet};

fn compare(f: &Fixture) {
    let expected = reference(f);
    let graph = Graph::prepare(f);
    for backend in Backend::ALL {
        let answer = solve(&graph, backend);
        assert!(answer.work >= answer.expansions);
        match (expected, answer.outcome.cost()) {
            (None, None) => assert_eq!(answer.outcome, Outcome::Unreachable),
            (Some(a), Some(b)) => assert!(
                (a - b).abs() < 1e-9,
                "{} {}: {a} != {b}",
                f.name,
                backend.name()
            ),
            other => panic!("{} {}: {other:?}", f.name, backend.name()),
        }
        if let Outcome::Completed { path, cost } = answer.outcome {
            assert!(!path.is_empty());
            assert_ne!(path[0], f.start);
            let mut at = f.start;
            let mut actual = 0.0;
            for next in &path {
                actual += graph
                    .successors(at)
                    .into_iter()
                    .find(|(t, _)| t == next)
                    .expect("legal edge")
                    .1
                    .0;
                at = *next;
            }
            assert!(graph.goals.contains(&at));
            assert!((cost - actual).abs() < 1e-9);
        }
    }
}

#[test]
fn representative_backends_agree_with_independent_dijkstra() {
    for fixture in fixtures() {
        compare(&fixture);
    }
}

#[test]
fn varied_small_graphs_cover_corners_access_bounds_and_modes() {
    // Deterministic generated fixtures, no dependency on production RNG.
    for seed in 0..48u32 {
        let mut f = Fixture::open("generated", 9, tile(0, 0), tile(8, 8));
        f.diagonal = seed % 2 == 0;
        f.mode = match seed % 3 {
            0 => RoadMode::Smooth,
            1 => RoadMode::Departure,
            _ => RoadMode::Arrival,
        };
        let mut bits = seed + 17;
        for y in 0..9 {
            for x in 0..9 {
                bits = bits.wrapping_mul(1664525).wrapping_add(1013904223);
                let t = tile(x, y);
                if t == f.start || t == tile(8, 8) {
                    continue;
                }
                if bits % 7 == 0 {
                    f.block(t, if bits % 2 == 0 { "acre" } else { "tree" });
                }
                if bits % 11 == 0 {
                    f.roads.push(Road {
                        tile: t,
                        construction: Construction::Complete,
                    });
                }
                if bits % 29 == 0 {
                    f.void.insert(t);
                }
                if bits % 31 == 0 {
                    f.unavailable.insert(t);
                }
            }
        }
        f.land = if seed % 3 == 0 {
            LandAccess::Farmer
        } else {
            LandAccess::Ordinary
        };
        if seed % 4 == 0 {
            f.bounds = Some((0, 8, 0, 7));
        }
        if seed % 5 == 0 {
            f.target = Target::Multiple(BTreeSet::from([tile(8, 8), tile(7, 6)]));
        }
        f.exemptions.insert(tile(1, 0));
        compare(&f);
    }
}

#[test]
fn incremental_quantum_counts_pops_and_reconstruction_and_cancellation() {
    let f = Fixture::open("quantum", 20, tile(0, 0), tile(19, 19));
    let graph = Graph::prepare(&f);
    let mut job = Incremental::new(&graph);
    assert_eq!(job.advance(0), 0);
    assert_eq!(job.outcome, Outcome::Pending);
    while job.outcome == Outcome::Pending {
        let before = job.work;
        assert_eq!(job.advance(1), 1);
        assert_eq!(job.work, before + 1);
    }
    assert!(job.work > job.expansions + 38); // reconstruction is charged
    assert_eq!(job.outcome, solve(&graph, Backend::Custom).outcome);
    let mut cancelled = Incremental::new(&graph);
    cancelled.advance(3);
    cancelled.cancel();
    let work = cancelled.work;
    assert_eq!(cancelled.advance(100), 0);
    assert_eq!(cancelled.work, work);
    assert_eq!(cancelled.outcome, Outcome::Cancelled);
}

#[test]
fn actual_library_reach_iterator_resumes_without_restarting() {
    let f = Fixture::open("iterator", 9, tile(0, 0), tile(8, 8));
    let graph = Graph::prepare(&f);
    let expansions = Cell::new(0);
    let mut iterator = reach(&graph, &expansions);
    assert_eq!(expansions.get(), 0);
    let first: Vec<_> = iterator.by_ref().take(3).collect();
    assert_eq!(first.len(), 3);
    assert_eq!(expansions.get(), 3);
    let rest: Vec<_> = iterator.collect();
    assert_eq!(expansions.get(), 81);
    let nodes: BTreeSet<_> = first.iter().chain(&rest).map(|x| x.node).collect();
    assert_eq!(nodes.len(), 81);
}

#[test]
fn outcomes_and_ties_have_explicit_contracts() {
    let mut f = Fixture::open("arrived", 4, tile(1, 1), tile(1, 1));
    for b in Backend::ALL {
        assert_eq!(solve(&Graph::prepare(&f), b).outcome, Outcome::Arrived);
    }
    f.target = Target::Multiple(BTreeSet::new());
    for b in Backend::ALL {
        assert_eq!(solve(&Graph::prepare(&f), b).outcome, Outcome::Unreachable);
    }
    f.start = tile(0, 0);
    f.target = Target::Exact(tile(1, 1));
    let expected = Outcome::Completed {
        path: vec![tile(1, 0), tile(1, 1)],
        cost: 12.0,
    };
    assert_eq!(
        solve(&Graph::prepare(&f), Backend::Custom).outcome,
        expected
    );
    // Weighted equal-f choices preserve earlier discovery too.
    f.roads.push(Road {
        tile: tile(3, 3),
        construction: Construction::Complete,
    });
    assert_eq!(
        solve(&Graph::prepare(&f), Backend::Custom).outcome,
        expected
    );
}

#[test]
fn footprint_goals_and_diagonal_side_clearance_are_explicit() {
    let mut f = Fixture::open("corner", 3, tile(0, 0), tile(1, 1));
    f.diagonal = true;
    f.block(tile(1, 0), "tree");
    f.block(tile(0, 1), "tree");
    compare(&f);
    assert_eq!(reference(&f), None);
    let fp = lantern_core::spatial::Footprint::saved(tile(2, 2), Some((2, 2))).unwrap();
    let mut f = Fixture::open("perimeter", 8, tile(0, 0), tile(7, 7));
    f.target = Target::Adjacent(fp);
    let graph = Graph::prepare(&f);
    assert_eq!(
        graph.goals,
        BTreeSet::from([
            tile(2, 1),
            tile(3, 1),
            tile(2, 4),
            tile(3, 4),
            tile(1, 2),
            tile(1, 3),
            tile(4, 2),
            tile(4, 3)
        ])
    );
    compare(&f);
}

#[test]
fn unavailable_chunk_and_obstacle_edits_rebuild_only_private_graph() {
    let f = Fixture::open("edit", 8, tile(0, 0), tile(7, 0));
    let before = Graph::prepare(&f);
    let baseline = solve(&before, Backend::Custom).outcome;
    let mut changed = f.clone();
    changed.block(tile(3, 0), "tree");
    changed.unavailable.insert(tile(3, 1));
    compare(&changed);
    assert!(reference(&changed) > reference(&f));
    assert_eq!(solve(&before, Backend::Custom).outcome, baseline);
    assert!(f.structures.is_empty());
}

#[test]
fn acre_exceptions_subtract_only_anchors_from_the_occupancy_union() {
    use lantern_core::spatial::{Footprint, Structure};
    let mut f = Fixture::open("saved_large_acre", 6, tile(0, 2), tile(5, 2));
    f.structures.push(Structure {
        footprint: Footprint::saved(tile(2, 2), Some((2, 2))).unwrap(),
        kind: "acre".into(),
        construction: Construction::Complete,
        fence_orientation: 0,
    });
    // Overlap does not undo the actor's subtraction from the whole blocked set.
    f.block(tile(2, 2), "tree");
    f.land = LandAccess::Farmer;
    let graph = Graph::prepare(&f);
    assert!(graph.open(tile(2, 2)));
    assert!(!graph.open(tile(3, 2)));
    compare(&f);
    f.land = LandAccess::Ordinary;
    assert!(!Graph::prepare(&f).open(tile(2, 2)));
    compare(&f);
    f.land = LandAccess::Construction;
    compare(&f);
}

#[test]
fn evaluated_library_ties_differ_from_the_stable_discovery_contract() {
    // Cost equality alone is insufficient for adoption. This pinned candidate
    // actually chooses a different equal-cost path on the evaluation workload.
    assert!(fixtures().iter().any(|f| {
        let graph = Graph::prepare(f);
        let custom = solve(&graph, Backend::Custom).outcome;
        let library = solve(&graph, Backend::LibraryAstar).outcome;
        custom.cost() == library.cost() && custom != library
    }));
}

#[test]
fn footprint_goal_filter_uses_actor_access_before_search() {
    let mut f = Fixture::open("perimeter_access", 8, tile(0, 0), tile(7, 7));
    let fp = lantern_core::spatial::Footprint::saved(tile(2, 2), Some((2, 2))).unwrap();
    f.target = Target::Adjacent(fp);
    f.block(tile(2, 1), "tree");
    f.block(tile(1, 2), "acre");
    let ordinary = Graph::prepare(&f);
    assert!(!ordinary.goals.contains(&tile(2, 1)));
    assert!(!ordinary.goals.contains(&tile(1, 2)));
    f.land = LandAccess::Farmer;
    f.exemptions.insert(tile(2, 1));
    let permitted = Graph::prepare(&f);
    assert!(permitted.goals.contains(&tile(2, 1)));
    assert!(permitted.goals.contains(&tile(1, 2)));
    compare(&f);
}
