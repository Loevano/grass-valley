use lantern_core::{search::*, spatial::*, units::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
fn t(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn q(blocks: &[Tile], roads: &[Tile], diagonal: bool) -> Rc<Query> {
    let resources: Vec<_> = blocks
        .iter()
        .map(|&tile| Resource {
            footprint: Footprint::saved(tile, None).unwrap(),
            kind: "stone".into(),
        })
        .collect();
    let roads: Vec<_> = roads
        .iter()
        .map(|&tile| Road {
            tile,
            construction: Construction::Complete,
        })
        .collect();
    Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&[], &resources, &[], &roads, 16).unwrap()),
            Terrain::default(),
            Access::default(),
            t(0, 0),
            Target::Exact(t(8, 0)),
            diagonal,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
fn finish(s: &mut Search) -> Outcome {
    while s.outcome() == &Outcome::Pending {
        s.advance(64);
    }
    s.outcome().clone()
}
// Independent exhaustive sorted-distance Dijkstra: no heuristic, predecessor,
// frontier serial or resumability. Uses the independently covered spatial metric.
fn reference(blocks: &[Tile], roads: &[Tile], diagonal: bool) -> Option<f64> {
    let mut costs = BTreeMap::from([(t(0, 0), 0f64)]);
    let mut done = BTreeSet::new();
    loop {
        let (&at, &cost) = costs
            .iter()
            .filter(|(p, _)| !done.contains(*p))
            .min_by(|a, b| a.1.total_cmp(b.1))?;
        if at == t(8, 0) {
            return Some(cost);
        }
        done.insert(at);
        let open = |p: Tile| p.x >= 0 && p.y >= 0 && p.x < 16 && p.y < 16 && !blocks.contains(&p);
        for &(dx, dy) in CARDINAL
            .iter()
            .chain(if diagonal { DIAGONAL.iter() } else { [].iter() })
        {
            let next = t(at.x + dx, at.y + dy);
            if !open(next)
                || (dx != 0 && dy != 0 && (!open(t(next.x, at.y)) || !open(t(at.x, next.y))))
            {
                continue;
            }
            let weights: BTreeSet<_> = roads.iter().copied().collect();
            let value = cost + edge_cost(at, next, &weights, RoadMode::Smooth);
            if costs.get(&next).is_none_or(|&old| value < old) {
                costs.insert(next, value);
            }
        }
    }
}
#[test]
fn distant_churn_retains_executed_work_but_refreshes_unseen_graph() {
    let mut s = Search::new(q(&[], &[], false));
    s.advance(3);
    for x in 10..16 {
        let work = s.work;
        assert!(s.refresh(q(&[t(x, 15)], &[], false)));
        assert_eq!(s.work, work);
    }
    assert_eq!(finish(&mut s).cost(), reference(&[t(15, 15)], &[], false));
}
#[test]
fn relevant_addition_removal_and_first_road_invalidate() {
    for diagonal in [false, true] {
        let mut s = Search::new(q(&[], &[], diagonal));
        s.advance(2);
        assert!(!s.refresh(q(&[t(1, 0)], &[], diagonal)));
        assert!(!s.refresh(q(&[], &[t(15, 15)], diagonal)));
        let mut blocked = Search::new(q(&[t(1, 0)], &[], diagonal));
        blocked.advance(2);
        assert!(!blocked.refresh(q(&[], &[], diagonal)));
        if diagonal {
            assert!(!s.refresh(q(&[t(0, 1)], &[], true)));
        }
    }
}
#[test]
fn adversarial_edits_match_current_graph_independent_optimum() {
    for diagonal in [false, true] {
        for seed in 0..48 {
            let blocks: Vec<_> = (0..16)
                .filter(|y| (y + seed) % 5 != 0)
                .map(|y| t(3, y))
                .collect();
            let mut s = Search::new(q(&blocks, &[t(15, 15)], diagonal));
            s.advance(1 + seed as usize);
            let mut changed = blocks.clone();
            if seed % 2 == 0 {
                changed.push(t(1, 0));
            } else {
                changed.retain(|p| p.y != seed % 16);
            }
            let roads = if seed % 3 == 0 {
                vec![t(15, 15), t(2, 0), t(2, 1)]
            } else {
                vec![t(15, 15)]
            };
            let current = q(&changed, &roads, diagonal);
            if !s.refresh(current.clone()) {
                s = Search::new(current.clone());
            }
            let result = finish(&mut s);
            assert_eq!(result, finish(&mut Search::new(current)));
            match (result.cost(), reference(&changed, &roads, diagonal)) {
                (Some(a), Some(b)) => assert!((a - b).abs() < 1e-9, "{seed}: {a} != {b}"),
                (None, None) => {}
                other => panic!("{other:?}"),
            }
        }
    }
}
#[test]
fn negative_certificate_includes_failed_boundary_and_loaded_terrain() {
    let wall: Vec<_> = (0..16).map(|y| t(1, y)).collect();
    let mut s = Search::new(q(&wall, &[], true));
    assert_eq!(finish(&mut s), Outcome::Unreachable);
    assert!(s.dependencies().valid_for(&q(&wall, &[], true)));
    assert!(!s.dependencies().valid_for(&q(&wall[1..], &[], true)));
    let cells = Some(BTreeMap::from([(t(1, 0), 'v'), (t(0, 1), 'v')]));
    let make = |terrain| {
        Rc::new(
            Query::new(
                Rc::new(SpatialIndex::derive(&[], &[], &[], &[], 16).unwrap()),
                terrain,
                Access::default(),
                t(0, 0),
                Target::Exact(t(8, 0)),
                false,
                RoadMode::Smooth,
            )
            .unwrap(),
        )
    };
    let mut s = Search::new(make(Terrain {
        cells,
        bounds: None,
    }));
    assert_eq!(finish(&mut s), Outcome::Unreachable);
    assert!(!s.refresh(make(Terrain::default())));
    assert!(!s.refresh(make(Terrain {
        bounds: Some((0, 8, 0, 8)),
        ..Terrain::default()
    })));
}
#[test]
fn accepted_route_safety_ignores_road_optimality_but_checks_diagonal_clearance() {
    let route = [
        t(1, 1),
        t(2, 1),
        t(3, 1),
        t(4, 1),
        t(5, 1),
        t(6, 1),
        t(7, 1),
        t(8, 0),
    ];
    assert!(q(&[], &[], true).route_safe(t(0, 0), &route));
    assert!(q(&[], &[t(15, 15)], true).route_safe(t(0, 0), &route));
    assert!(!q(&[t(1, 0)], &[], true).route_safe(t(0, 0), &route));
    assert!(!q(&[t(0, 0)], &[], true).route_safe(t(0, 0), &route));
}
#[test]
fn fences_goal_footprints_access_and_arrival_are_dependencies() {
    let make = |barn: bool, orientation, access: Access, target| {
        let structures = if barn {
            vec![Structure {
                footprint: Footprint::saved(t(4, 4), None).unwrap(),
                kind: "barn".into(),
                construction: Construction::Complete,
                fence_orientation: orientation,
            }]
        } else {
            vec![]
        };
        Rc::new(
            Query::new(
                Rc::new(SpatialIndex::derive(&structures, &[], &[], &[], 16).unwrap()),
                Terrain::default(),
                access,
                t(0, 4),
                target,
                true,
                RoadMode::Smooth,
            )
            .unwrap(),
        )
    };
    let mut s = Search::new(make(
        false,
        0,
        Access::default(),
        Target::AnchorAdjacent(t(4, 4)),
    ));
    finish(&mut s);
    assert!(!s.refresh(make(
        true,
        0,
        Access::default(),
        Target::AnchorAdjacent(t(4, 4))
    )));
    assert!(!s.refresh(make(
        false,
        0,
        Access {
            only: Some(BTreeSet::from([t(0, 4)])),
            ..Access::default()
        },
        Target::AnchorAdjacent(t(4, 4))
    )));
    assert!(!s.refresh(make(
        false,
        0,
        Access::default(),
        Target::Perimeter(Footprint::saved(t(4, 4), Some((2, 2))).unwrap())
    )));
    let mut s = Search::new(make(
        true,
        0,
        Access::default(),
        Target::AnchorAdjacent(t(4, 4)),
    ));
    finish(&mut s);
    assert!(!s.refresh(make(
        true,
        1,
        Access::default(),
        Target::AnchorAdjacent(t(4, 4))
    )));
}
