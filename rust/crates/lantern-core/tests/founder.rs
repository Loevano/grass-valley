use lantern_core::{
    founder::*,
    spatial::*,
    units::{Position, Tile},
};
fn state(x: f64, y: f64) -> FounderState {
    FounderState {
        position: Position::new(x, y).unwrap(),
        facing: -1,
    }
}
fn access() -> FounderAccess {
    FounderAccess::new(false, 1., false, false).unwrap()
}
fn building(kind: &str, x: i32, y: i32) -> Structure {
    Structure {
        kind: kind.into(),
        footprint: Footprint::saved(Tile { x, y }, None).unwrap(),
        construction: Construction::Complete,
        fence_orientation: 0,
    }
}
#[test]
fn all_directions_cover_equal_screen_distance_and_zero_preserves_facing_activity() {
    let index = SpatialIndex::derive(&[], &[], &[], &[], 24).unwrap();
    let scene = FounderScene {
        spatial: &index,
        terrain: |_| None,
    };
    for dx in -1..=1 {
        for dy in -1..=1 {
            let start = state(5.1, 7.2);
            let result = scene
                .walk(start, Direction::new(dx, dy).unwrap(), access())
                .unwrap();
            if (dx, dy) == (0, 0) {
                assert_eq!(result.state, start);
                assert!(!result.clear_activity);
            } else {
                assert!(
                    (screen_distance(
                        result.state.position.x.get() - 5.1,
                        result.state.position.y.get() - 7.2
                    ) - 0.3)
                        .abs()
                        < 1e-12
                );
                assert!(result.clear_activity);
            }
        }
    }
}
#[test]
fn radius_slide_corner_and_substeps_stop_at_obstacles() {
    let index = SpatialIndex::derive(&[building("hall", 4, 4)], &[], &[], &[], 24).unwrap();
    let scene = FounderScene {
        spatial: &index,
        terrain: |_| Some('g'),
    };
    let start = state(3.3, 4.);
    let result = scene
        .walk(start, Direction::new(1, 1).unwrap(), access())
        .unwrap();
    assert_eq!(result.state.position.x, start.position.x);
    assert!(result.state.position.y > start.position.y);
    assert!(scene.walkable(result.state.position, access()));
    let mut current = state(2., 4.);
    for _ in 0..20 {
        current = scene
            .walk(current, Direction::new(1, 0).unwrap(), access())
            .unwrap()
            .state;
    }
    assert!(current.position.x.get() <= 3.32);
    let corner = scene
        .walk(state(3.3, 3.3), Direction::new(1, 1).unwrap(), access())
        .unwrap();
    assert!(scene.walkable(corner.state.position, access()));
    assert!(corner.state.position.x.get() < 3.32 || corner.state.position.y.get() < 3.32);
}
#[test]
fn batches_validate_before_queries_and_preserve_order_without_mutation() {
    let index = SpatialIndex::derive(&[], &[], &[], &[], 24).unwrap();
    let queries = std::cell::Cell::new(0);
    let scene = FounderScene {
        spatial: &index,
        terrain: |_| {
            queries.set(queries.get() + 1);
            None
        },
    };
    let initial = state(5.4, 7.);
    assert_eq!(
        scene.walk_batch(initial, &[(1, 0), (2, 0)], access()),
        Err(MovementError::InvalidDirection)
    );
    assert_eq!(queries.get(), 0);
    assert_eq!(
        scene.walk_batch(initial, &[], access()),
        Err(MovementError::InvalidBatch)
    );
    assert_eq!(
        scene.walk_batch(initial, &[(0, 0); 6], access()),
        Err(MovementError::InvalidBatch)
    );
    let inputs = [(1, 0), (0, 1), (-1, 1), (0, 0)];
    let batch = scene.walk_batch(initial, &inputs, access()).unwrap();
    let mut single = initial;
    for (dx, dy) in inputs {
        single = scene
            .walk(single, Direction::new(dx, dy).unwrap(), access())
            .unwrap()
            .state;
    }
    assert_eq!(batch.state, single);
    assert!(index.blocked().is_empty());
    assert_eq!(initial, state(5.4, 7.));
    assert_eq!(
        scene.walk_batch(state(-1., 0.), &[(0, 0)], access()),
        Err(MovementError::NeedsRecovery)
    );
}
#[test]
fn water_overlay_does_not_remove_overlapping_buildings_and_npcs_do_not_block() {
    let tile = Tile { x: 5, y: 7 };
    let water = Resource {
        kind: "water".into(),
        footprint: Footprint::saved(tile, Some((2, 1))).unwrap(),
    };
    let index = SpatialIndex::derive(
        &[building("hall", 5, 7)],
        std::slice::from_ref(&water),
        &[Footprint::saved(Tile { x: 3, y: 3 }, None).unwrap()],
        &[],
        24,
    )
    .unwrap();
    assert!(index.founder_occupied(tile, true));
    assert!(!index.founder_occupied(Tile { x: 6, y: 7 }, true));
    assert!(!index.founder_occupied(Tile { x: 3, y: 3 }, false));
    assert!(index.blocked().contains(&Tile { x: 3, y: 3 }));
    assert!(index.water().contains(&tile));
    assert!(!index.water().contains(&Tile { x: 6, y: 7 }));
    assert_eq!(water.footprint.dimensions(), (2, 1));
}
#[test]
fn finite_access_and_max_map_coordinates_do_not_overflow() {
    assert!(FounderAccess::new(true, f64::NAN, false, false).is_err());
    assert!(FounderAccess::new(true, f64::INFINITY, false, false).is_err());
    let index = SpatialIndex::derive(&[], &[], &[], &[], i32::MAX as u32).unwrap();
    let scene = FounderScene {
        spatial: &index,
        terrain: |_| Some('b'),
    };
    let boots = FounderAccess::new(false, 1., false, true).unwrap();
    assert!(scene.walkable(
        state(f64::from(i32::MAX) - 2., f64::from(i32::MAX) - 2.).position,
        boots
    ));
}
