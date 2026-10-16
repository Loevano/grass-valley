use lantern_core::{
    spatial::*,
    units::{Position, Tile},
};
use lantern_host::legacy::PrivateSpatialCopy;
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn tile(v: &Value) -> Tile {
    Tile {
        x: v[0].as_i64().unwrap() as i32,
        y: v[1].as_i64().unwrap() as i32,
    }
}
fn point(v: &Value) -> Position {
    Position::new(v[0].as_f64().unwrap(), v[1].as_f64().unwrap()).unwrap()
}
fn tiles(v: &Value) -> BTreeSet<Tile> {
    v.as_array().unwrap().iter().map(tile).collect()
}
fn segment_value(s: &Segment) -> Value {
    json!([s.a.x.get(), s.a.y.get(), s.b.x.get(), s.b.y.get()])
}
fn vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/spatial/python.json")).unwrap()
}

#[test]
fn python_occupancy_footprints_profiles_and_roads() {
    for case in vectors()["cases"].as_array().unwrap() {
        let source = serde_json::to_vec(&case["source"]).unwrap();
        let copy = PrivateSpatialCopy::decode(&source).unwrap();
        let index = copy.derive_index().unwrap();
        assert_eq!(index.blocked(), &tiles(&case["blocked"]));
        assert_eq!(index.roads(), &tiles(&case["roads"]));
        assert!(
            !index.blocked().contains(&Tile { x: 6, y: 6 }),
            "moving villagers must not block"
        );
        let empty = BTreeSet::new();
        for access in [LandAccess::Farmer, LandAccess::Construction] {
            let profile = AccessProfile {
                land: access,
                exemptions: &empty,
                only: None,
            };
            let actual = index
                .blocked()
                .iter()
                .copied()
                .filter(|t| index.occupied_for(*t, &profile))
                .collect::<BTreeSet<_>>();
            assert_eq!(actual, tiles(&case["farmer"]));
        }
        for (i, b) in copy.structures().iter().enumerate() {
            assert_eq!(
                b.footprint.tiles().collect::<BTreeSet<_>>(),
                tiles(&case["footprints"][i])
            );
            assert_eq!(b.footprint.perimeter(), tiles(&case["perimeters"][i]));
        }
        let expected: Vec<Value> = case["fences"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                json!(
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|n| n.as_f64().unwrap())
                        .collect::<Vec<_>>()
                )
            })
            .collect();
        assert_eq!(
            index.fences().iter().map(segment_value).collect::<Vec<_>>(),
            expected
        );
        // Access overlays cannot mutate source/index; exemptions remove even
        // overlapping resource occupancy, matching Python's set subtraction.
        let water = Tile { x: 10, y: 10 };
        let permitted = BTreeSet::from([water]);
        let profile = AccessProfile {
            land: LandAccess::Ordinary,
            exemptions: &permitted,
            only: Some(&permitted),
        };
        assert!(!index.occupied_for(water, &profile));
        assert!(index.occupied_for(Tile { x: 0, y: 0 }, &profile));
        assert!(index.blocked().contains(&water));
        assert_eq!(copy.private_bytes(), source);
    }
}

#[test]
fn python_fence_orientation_edges_and_radius() {
    for case in vectors()["pens"].as_array().unwrap() {
        let p = pen(
            tile(&case["origin"]),
            case["size"].as_u64().unwrap() as u32,
            case["requested"].as_i64().unwrap() as i32,
        )
        .unwrap();
        assert_eq!(
            p.orientation,
            case["orientation"].as_u64().unwrap() as usize
        );
        assert_eq!(p.gate, (point(&case["gate"][0]), point(&case["gate"][1])));
        for (s, expected) in p.segments.iter().zip(case["segments"].as_array().unwrap()) {
            let actual = segment_value(s);
            for i in 0..4 {
                assert_eq!(actual[i].as_f64(), expected[i].as_f64());
            }
        }
        for v in case["points"].as_array().unwrap() {
            assert_eq!(
                fence_blocked(point(v), &p.segments, 0.18),
                v[2].as_bool().unwrap(),
                "{case}"
            );
        }
        for v in case["edges"].as_array().unwrap() {
            assert_eq!(
                crosses(point(&v[0]), point(&v[1]), &p.segments),
                v[2].as_bool().unwrap()
            );
        }
    }
}

#[test]
fn python_road_costs_keep_modes_and_world_diagonal_metric() {
    for case in vectors()["costs"].as_array().unwrap() {
        let mode = match case["mode"].as_str().unwrap() {
            "smooth" => RoadMode::Smooth,
            "departure" => RoadMode::Departure,
            _ => RoadMode::Arrival,
        };
        let actual = edge_cost(
            Tile { x: 2, y: 2 },
            tile(&case["end"]),
            &tiles(&case["roads"]),
            mode,
        );
        assert!((actual - case["cost"].as_f64().unwrap()).abs() < 1e-14);
    }
}

#[test]
fn unsupported_import_geometry_rejects_without_replacing_private_source() {
    let base = json!({"map_size":24,"buildings":[{"kind":"hall","x":1,"y":1}],"map_resources":[]});
    for footprint in [
        json!([0, 1]),
        json!([-1, 1]),
        json!([1.0, 1]),
        json!([true, 1]),
        json!([1]),
        json!([1000001, 1]),
        json!(null),
    ] {
        let mut v = base.clone();
        v["buildings"][0]["footprint"] = footprint;
        assert!(PrivateSpatialCopy::decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    for construction in [json!(null), json!(false), json!([])] {
        let mut v = base.clone();
        v["buildings"][0]["construction"] = construction;
        assert!(PrivateSpatialCopy::decode(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    let mut v = base;
    v["buildings"][0]["x"] = json!(i32::MAX);
    assert!(PrivateSpatialCopy::decode(&serde_json::to_vec(&v).unwrap()).is_err());
}
