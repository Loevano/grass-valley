use lantern_core::{spatial::*, units::Tile};
use std::collections::BTreeSet;
fn tile(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn structure(
    kind: &str,
    x: i32,
    y: i32,
    dimensions: Option<(u32, u32)>,
    construction: Construction,
) -> Structure {
    Structure {
        footprint: Footprint::saved(tile(x, y), dimensions).unwrap(),
        kind: kind.into(),
        construction,
        fence_orientation: 0,
    }
}
#[test]
fn saved_dimensions_are_not_rotated_or_replaced_by_catalog() {
    assert_eq!(
        Footprint::saved(tile(0, 0), None).unwrap().dimensions(),
        (1, 1)
    );
    assert_eq!(placement_dimensions("sawpit", 1), (2, 3));
    assert_eq!(placement_dimensions("sawpit", 0), (3, 2));
    let fp = Footprint::saved(tile(-3, -2), Some((2, 3))).unwrap();
    let inside = fp.tiles().collect::<BTreeSet<_>>();
    assert_eq!(inside.len(), 6);
    assert_eq!(fp.perimeter().len(), 10);
    assert!(inside.is_disjoint(&fp.perimeter()));
    assert!(Footprint::saved(tile(i32::MAX, 0), None).is_err());
}
#[test]
fn diagonal_edges_require_both_side_tiles_and_never_cross_fences() {
    let blocks = [structure("hall", 2, 1, None, Construction::Complete)];
    let index = SpatialIndex::derive(&blocks, &[], &[], &[], 20).unwrap();
    let open = |t| !index.blocked().contains(&t);
    assert!(!index.edge_open(tile(1, 1), tile(2, 2), true, open));
    assert!(index.edge_open(tile(1, 1), tile(0, 2), true, open));
    assert!(!index.edge_open(tile(1, 1), tile(0, 2), false, open));
    assert!(!index.edge_open(tile(1, 1), tile(3, 1), true, open));
    let index = SpatialIndex::derive(
        &[structure("barn", 10, 10, None, Construction::Complete)],
        &[],
        &[],
        &[],
        24,
    )
    .unwrap();
    assert!(!index.edge_open(tile(6, 10), tile(7, 10), true, |_| true));
    assert!(
        index.edge_open(tile(13, 11), tile(14, 11), true, |_| true),
        "two-tile gate remains accessible"
    );
}
#[test]
fn pen_protection_uses_whole_footprints_and_unfinished_barns() {
    let barn = structure("barn", 10, 10, None, Construction::Site);
    let touching = Footprint::saved(tile(5, 10), Some((3, 1))).unwrap();
    assert!(violates_pen(touching, false, std::slice::from_ref(&barn)));
    let outside = Footprint::saved(tile(5, 10), Some((2, 1))).unwrap();
    assert!(!violates_pen(outside, false, &[barn]));
    assert!(violates_pen(
        Footprint::saved(tile(10, 10), None).unwrap(),
        true,
        &[structure(
            "hall",
            5,
            10,
            Some((3, 1)),
            Construction::Complete
        )]
    ));
}
#[test]
fn task_access_is_derived_and_does_not_change_shared_obstacles() {
    let buildings = [
        structure("acre", 3, 3, Some((2, 1)), Construction::Complete),
        structure("acre", 5, 3, None, Construction::Upgrade),
    ];
    let index = SpatialIndex::derive(&buildings, &[], &[], &[], 12).unwrap();
    let empty = BTreeSet::new();
    let farmer = AccessProfile {
        land: LandAccess::Farmer,
        exemptions: &empty,
        only: None,
    };
    assert!(!index.occupied_for(tile(3, 3), &farmer));
    assert!(
        index.occupied_for(tile(4, 3), &farmer),
        "only acre anchors are removed in Python"
    );
    assert!(
        index.occupied_for(tile(5, 3), &farmer),
        "upgrading acres remain blocked"
    );
    assert!(index.blocked().contains(&tile(3, 3)));
}
