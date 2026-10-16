use lantern_core::{
    search::{Access, Outcome, Query, QueryError, Search, Target, Terrain},
    spatial::{Construction, Footprint, RoadMode, SpatialIndex, Structure},
    units::Tile,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
fn t(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn index(structures: &[Structure]) -> Rc<SpatialIndex> {
    Rc::new(SpatialIndex::derive(structures, &[], &[], &[], 8).unwrap())
}
fn query(
    start: Tile,
    target: Target,
    terrain: Terrain,
    access: Access,
    structures: &[Structure],
) -> Rc<Query> {
    Rc::new(
        Query::new(
            index(structures),
            terrain,
            access,
            start,
            target,
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
fn finish(q: Rc<Query>, quantum: usize) -> Outcome {
    let mut search = Search::new(q);
    while search.outcome() == &Outcome::Pending {
        assert!(search.advance(quantum) <= quantum);
    }
    search.into_outcome()
}
fn block(p: Tile) -> Structure {
    Structure {
        footprint: Footprint::saved(p, None).unwrap(),
        kind: "tree".into(),
        construction: Construction::Complete,
        fence_orientation: 0,
    }
}
#[test]
fn quantum_partition_and_zero_are_observationally_equivalent() {
    let q = query(
        t(0, 0),
        Target::Exact(t(7, 7)),
        Terrain::default(),
        Access::default(),
        &[],
    );
    let expected = finish(q.clone(), 10000);
    for quantum in [1, 2, 3, 7, 13] {
        let mut s = Search::new(q.clone());
        while s.outcome() == &Outcome::Pending {
            let before = s.work;
            assert_eq!(s.advance(0), 0);
            assert_eq!(before, s.work);
            assert!(s.advance(quantum) <= quantum);
        }
        assert_eq!(s.into_outcome(), expected);
    }
    if let Outcome::Completed { path, cost } = expected {
        assert_eq!(path.len(), 14);
        assert_eq!(path[0], t(1, 0));
        assert_eq!(path.last(), Some(&t(7, 7)));
        assert_eq!(cost, 84.0);
    } else {
        panic!("expected path")
    }
}
#[test]
fn arrival_precedes_passability_and_raw_anchor_is_distinct() {
    let blocked = [block(t(2, 2))];
    assert_eq!(
        finish(
            query(
                t(2, 2),
                Target::Exact(t(2, 2)),
                Terrain::default(),
                Access::default(),
                &blocked
            ),
            1
        ),
        Outcome::Arrived
    );
    assert_eq!(
        finish(
            query(
                t(2, 2),
                Target::AnchorAdjacent(t(3, 2)),
                Terrain::default(),
                Access::default(),
                &blocked
            ),
            1
        ),
        Outcome::Arrived
    );
    // The raw anchor itself is not adjacent; the solver must leave it.
    assert!(
        matches!(finish(query(t(2,2),Target::AnchorAdjacent(t(2,2)),Terrain::default(),Access::default(),&blocked),1),Outcome::Completed{path,..} if path==vec![t(3,2)])
    );
    let fp = Footprint::saved(t(3, 2), None).unwrap();
    assert_ne!(
        finish(
            query(
                t(2, 2),
                Target::Perimeter(fp),
                Terrain::default(),
                Access::default(),
                &blocked
            ),
            1
        ),
        Outcome::Arrived
    );
}
#[test]
fn sailing_only_and_exemptions_are_owned_and_applied_to_union() {
    let allowed = BTreeSet::from([t(0, 0), t(1, 0), t(2, 0)]);
    let access = Access {
        only: Some(allowed.clone()),
        exemptions: allowed,
        ..Access::default()
    };
    let q = query(
        t(0, 0),
        Target::Exact(t(2, 0)),
        Terrain::default(),
        access,
        &[block(t(1, 0))],
    );
    assert!(matches!(finish(q,1),Outcome::Completed{path,..} if path==vec![t(1,0),t(2,0)]));
    let access = Access {
        only: Some(BTreeSet::from([t(0, 0), t(2, 0)])),
        ..Access::default()
    };
    assert_eq!(
        finish(
            query(
                t(0, 0),
                Target::Exact(t(2, 0)),
                Terrain::default(),
                access,
                &[]
            ),
            1
        ),
        Outcome::Unreachable
    );
}
#[test]
fn terrain_missing_void_bounds_and_validation() {
    let cells = Some(BTreeMap::new());
    // Missing cells are accepted unbounded, rejected within explicit bounds.
    assert!(matches!(
        finish(
            query(
                t(0, 0),
                Target::Exact(t(1, 0)),
                Terrain {
                    cells: cells.clone(),
                    bounds: None
                },
                Access::default(),
                &[]
            ),
            1
        ),
        Outcome::Completed { .. }
    ));
    assert_eq!(
        finish(
            query(
                t(0, 0),
                Target::Exact(t(1, 0)),
                Terrain {
                    cells,
                    bounds: Some((0, 7, 0, 7))
                },
                Access::default(),
                &[]
            ),
            1
        ),
        Outcome::Unreachable
    );
    let terrain = Terrain {
        cells: Some(BTreeMap::from([(t(1, 0), 'v')])),
        bounds: Some((0, 1, 0, 0)),
    };
    assert_eq!(
        finish(
            query(
                t(0, 0),
                Target::Exact(t(1, 0)),
                terrain,
                Access::default(),
                &[]
            ),
            1
        ),
        Outcome::Unreachable
    );
    assert!(matches!(
        Query::new(
            index(&[]),
            Terrain {
                bounds: Some((2, 1, 0, 7)),
                ..Terrain::default()
            },
            Access::default(),
            t(0, 0),
            Target::Multiple(BTreeSet::new()),
            false,
            RoadMode::Smooth
        ),
        Err(QueryError::InvalidBounds)
    ));
}
#[test]
fn extreme_coordinates_empty_goals_and_cancellation_are_safe() {
    for start in [t(i32::MIN, i32::MAX), t(i32::MAX, i32::MIN)] {
        assert_eq!(
            finish(
                query(
                    start,
                    Target::AnchorAdjacent(t(3, 3)),
                    Terrain::default(),
                    Access::default(),
                    &[]
                ),
                1
            ),
            Outcome::Unreachable
        );
    }
    assert_eq!(
        finish(
            query(
                t(0, 0),
                Target::Multiple(BTreeSet::new()),
                Terrain::default(),
                Access::default(),
                &[]
            ),
            1
        ),
        Outcome::Unreachable
    );
    let q = query(
        t(0, 0),
        Target::Exact(t(7, 7)),
        Terrain::default(),
        Access::default(),
        &[],
    );
    let mut s = Search::new(q.clone());
    s.advance(3);
    s.cancel();
    s.cancel();
    assert_eq!(s.advance(usize::MAX), 0);
    assert_eq!(s.outcome(), &Outcome::Cancelled);
    assert!(matches!(finish(q, 1), Outcome::Completed { .. }));
}

#[test]
fn anchor_arrival_and_perimeter_admission_check_the_final_fence() {
    let barn = Structure {
        kind: "barn".into(),
        ..block(t(4, 4))
    };
    let terrain = Terrain {
        bounds: Some((0, 1, 4, 4)),
        ..Terrain::default()
    };
    for target in [
        Target::AnchorAdjacent(t(1, 4)),
        Target::Perimeter(Footprint::saved(t(1, 4), None).unwrap()),
    ] {
        assert_eq!(
            finish(
                query(
                    t(0, 4),
                    target,
                    terrain.clone(),
                    Access::default(),
                    std::slice::from_ref(&barn)
                ),
                1
            ),
            Outcome::Unreachable
        );
    }
}
