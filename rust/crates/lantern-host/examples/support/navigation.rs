use lantern_core::{ids::*, model::*, search::*, spatial::*, units::*};
use std::rc::Rc;
pub fn position(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
pub fn towns(n: usize) -> (Vec<Town>, Vec<(usize, ActorId)>) {
    towns_at(n, position(0., 0.), position(63., 0.))
}
pub fn towns_at(n: usize, start: Position, goal: Position) -> (Vec<Town>, Vec<(usize, ActorId)>) {
    let world = WorldId::new("synthetic-service").unwrap();
    let mut ids = IdAllocator::new(world.clone());
    let mut keys = Vec::new();
    let towns = (0..4)
        .map(|i| {
            let owner = Owner {
                world: world.clone(),
                town: TownId::new(format!("town-{i}")).unwrap(),
            };
            let actors = (0..n / 4)
                .map(|_| {
                    let id = ids.allocate(&owner).unwrap();
                    keys.push((i, id.clone()));
                    ActorState::new(id, start, Cargo::default())
                })
                .collect();
            let mut town = Town::restore(owner, vec![], actors).unwrap();
            for (_, id) in keys.iter().filter(|(town, _)| *town == i) {
                town.set_task(id, Task::Move { destination: goal }).unwrap();
            }
            town
        })
        .collect();
    (towns, keys)
}
pub fn query(scenario: &str, edited: bool) -> Rc<Query> {
    let mut resources: Vec<_> = if scenario == "negative" || scenario == "opened-negative" {
        (0..64)
            .filter(|&y| !(edited && scenario == "opened-negative" && y == 0))
            .map(|y| Resource {
                footprint: Footprint::saved(Tile { x: 32, y }, None).unwrap(),
                kind: "stone".into(),
            })
            .collect()
    } else {
        (0..63)
            .map(|y| Resource {
                footprint: Footprint::saved(Tile { x: 32, y }, None).unwrap(),
                kind: "stone".into(),
            })
            .collect()
    };
    if edited && scenario == "distant-obstacle" {
        resources.push(Resource {
            footprint: Footprint::saved(Tile { x: 63, y: 63 }, None).unwrap(),
            kind: "stone".into(),
        });
    }
    if edited && scenario == "relevant-obstacle" {
        resources.push(Resource {
            footprint: Footprint::saved(Tile { x: 1, y: 0 }, None).unwrap(),
            kind: "stone".into(),
        });
    }
    let mut roads = vec![];
    if scenario == "distant-road" {
        roads.push(Road {
            tile: Tile { x: 63, y: 62 },
            construction: Construction::Complete,
        });
    }
    if edited
        && (scenario == "first-road" || scenario == "distant-road" || scenario == "relevant-road")
    {
        roads.push(Road {
            tile: if scenario == "relevant-road" {
                Tile { x: 1, y: 0 }
            } else {
                Tile { x: 63, y: 63 }
            },
            construction: Construction::Complete,
        });
    }
    let structures = if edited && scenario == "fence" {
        vec![Structure {
            footprint: Footprint::saved(Tile { x: 4, y: 0 }, None).unwrap(),
            kind: "barn".into(),
            construction: Construction::Complete,
            fence_orientation: 1,
        }]
    } else {
        vec![]
    };
    let terrain = if edited && scenario == "terrain" {
        Terrain {
            cells: Some([(Tile { x: 1, y: 0 }, 'v')].into()),
            bounds: None,
        }
    } else {
        Terrain::default()
    };
    Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&structures, &resources, &[], &roads, 64).unwrap()),
            terrain,
            Access::default(),
            Tile { x: 0, y: 0 },
            Target::Exact(Tile { x: 63, y: 0 }),
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
