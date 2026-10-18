//! Experimental route-service/travel traces; no public host or accounting port.
use lantern_core::{
    ids::{ActorId, IdAllocator, Owner, TownId, WorldId},
    model::*,
    navigation::{Limits, Priority},
    search::*,
    spatial::*,
    travel::{Speed, Status as TravelStatus},
    units::*,
};
use lantern_host::navigation::*;
use lantern_host::{
    dto::{decode_save, encode_save},
    state::{ModelWorld, PrivateCredentials},
};
use std::{cell::Cell, rc::Rc, time::Duration};

#[derive(Default)]
struct Fake(Cell<Duration>);
impl Clock for Fake {
    fn now(&self) -> Duration {
        self.0.get()
    }
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn fixture() -> (Vec<Town>, Vec<(usize, ActorId)>) {
    let world = WorldId::new("travel-recovery").unwrap();
    let mut ids = IdAllocator::new(world.clone());
    let mut keys = vec![];
    let towns = (0..4)
        .map(|t| {
            let owner = Owner {
                world: world.clone(),
                town: TownId::new(format!("town-{t}")).unwrap(),
            };
            let workplace = ids.allocate(&owner).unwrap();
            let actors = (0..4)
                .map(|_| {
                    let id = ids.allocate(&owner).unwrap();
                    keys.push((t, id.clone()));
                    ActorState::new(
                        id,
                        p(1.25, 2.0),
                        Cargo::from_entries([("grain".into(), Quantity::new(3.125).unwrap())])
                            .unwrap(),
                    )
                })
                .collect();
            let mut town = Town::restore(
                owner,
                vec![Building {
                    id: workplace.clone(),
                    location: Tile { x: 10, y: 2 },
                    construction: ConstructionState::Complete,
                }],
                actors,
            )
            .unwrap();
            for (_, id) in keys.iter().filter(|(i, _)| *i == t) {
                town.set_task(
                    id,
                    Task::Work {
                        workplace: workplace.clone(),
                        target: Destination::Position(p(1.0, 2.0)),
                        phase: WorkPhase::Returning,
                        elapsed: Seconds::new(0.375).unwrap(),
                    },
                )
                .unwrap();
                town.request_route(
                    id,
                    Destination::Building(workplace.clone()),
                    NavigationRevision(1),
                )
                .unwrap();
            }
            town
        })
        .collect();
    (towns, keys)
}
fn reload(towns: Vec<Town>) -> Vec<Town> {
    let world_id = towns[0].owner().world.clone();
    let high_water = towns
        .iter()
        .flat_map(|t| t.actors().iter().map(|a| a.state().id.identity().serial()))
        .max()
        .unwrap();
    let world = ModelWorld::new(
        world_id,
        TerrainSeed(0),
        high_water,
        towns,
        PrivateCredentials::default(),
    )
    .unwrap();
    let encoded = encode_save(&world).unwrap();
    let world = decode_save(&encoded).unwrap();
    world
        .towns()
        .iter()
        .map(|t| {
            Town::restore(
                t.owner().clone(),
                t.buildings().to_vec(),
                t.actors().iter().map(|a| a.state().clone()).collect(),
            )
            .unwrap()
        })
        .collect()
}
struct Scene(Rc<SpatialIndex>);
impl Scene {
    fn new(sealed: bool) -> Self {
        let blocked: Vec<_> = if sealed {
            (0..12)
                .map(|y| Tile { x: 6, y })
                .chain([Tile { x: 2, y: 2 }])
                .collect()
        } else {
            vec![]
        };
        let resources: Vec<_> = blocked
            .into_iter()
            .map(|t| Resource {
                footprint: Footprint::saved(t, None).unwrap(),
                kind: "stone".into(),
            })
            .collect();
        Self(Rc::new(
            SpatialIndex::derive(&[], &resources, &[], &[], 12).unwrap(),
        ))
    }
    fn query(&self, request: &RouteRequest) -> Rc<Query> {
        Rc::new(
            Query::new(
                self.0.clone(),
                Terrain::default(),
                Access::default(),
                Tile {
                    x: request.origin().x.get().round_ties_even() as i32,
                    y: request.origin().y.get().round_ties_even() as i32,
                },
                Target::AnchorAdjacent(Tile {
                    x: request.goal().x.get() as i32,
                    y: request.goal().y.get() as i32,
                }),
                true,
                RoadMode::Smooth,
            )
            .unwrap(),
        )
    }
}
fn settle(
    pump: &mut NavigationPump,
    towns: &mut [Town],
    keys: &[(usize, ActorId)],
    scene: &Scene,
    revision: u64,
    target: TravelStatus,
) {
    let clock = Fake::default();
    for _ in 0..400 {
        let requests: Vec<_> = keys
            .iter()
            .filter_map(|(t, id)| {
                towns[*t]
                    .resume_travel(id, NavigationRevision(revision))
                    .unwrap()
                    .map(|r| (*t, r))
            })
            .collect();
        let intents: Vec<_> = requests
            .iter()
            .map(|(town, request)| Intent {
                town: *town,
                request,
                query: scene.query(request),
                priority: Priority::Blocked,
            })
            .collect();
        clock.0.set(pump.next_due());
        pump.poll(&clock, towns, &intents, |e| {
            if let Some((_, result)) = e.adopted {
                result.unwrap();
            }
        })
        .unwrap();
        if keys
            .iter()
            .all(|(t, id)| towns[*t].actor(id).unwrap().travel_status() == target)
        {
            return;
        }
    }
    panic!("finite workload must settle");
}

#[test]
fn blocked_return_legs_wait_then_reopen_under_saturated_navigation_without_losing_cargo() {
    let (mut towns, keys) = fixture();
    let mut pump = NavigationPump::new(
        Limits {
            jobs: 2,
            ..Limits::default()
        },
        Cadence {
            allowance: 128,
            ..Cadence::default()
        },
        Duration::ZERO,
    );
    let open = Scene::new(false);
    settle(
        &mut pump,
        &mut towns,
        &keys,
        &open,
        1,
        TravelStatus::Walking,
    );
    let mut saved = vec![];
    for (t, id) in &keys {
        let town = &mut towns[*t];
        let RouteState::Ready { request, .. } = town.actor(id).unwrap().route() else {
            panic!("ready")
        };
        let q = open.query(request);
        town.advance_travel(id, Seconds::new(0.125).unwrap(), 10, |a, b| {
            q.segment_safe(a, b).then(|| Speed::new(1.0).unwrap())
        })
        .unwrap();
        assert_eq!(town.actor(id).unwrap().state().position, p(1.375, 2.0));
        saved.push(town.actor(id).unwrap().state().clone());
    }
    let sealed = Scene::new(true);
    for ((t, id), before) in keys.iter().zip(&saved) {
        let town = &mut towns[*t];
        let RouteState::Ready { request, .. } = town.actor(id).unwrap().route() else {
            panic!("ready")
        };
        let q = sealed.query(request);
        let a = town
            .advance_travel(id, Seconds::new(10.0).unwrap(), 10, |a, b| {
                q.segment_safe(a, b).then(|| Speed::new(1.0).unwrap())
            })
            .unwrap();
        assert_eq!(a.status, TravelStatus::Blocked);
        assert!(!a.arrived_now);
        assert_eq!(town.actor(id).unwrap().state(), before);
    }
    settle(
        &mut pump,
        &mut towns,
        &keys,
        &sealed,
        2,
        TravelStatus::Unreachable,
    );
    for ((t, id), before) in keys.iter().zip(&saved) {
        let town = &mut towns[*t];
        for _ in 0..4 {
            assert_eq!(town.resume_travel(id, NavigationRevision(2)).unwrap(), None);
        }
        let after = town.actor(id).unwrap().state();
        assert_eq!(after.position, before.position);
        assert_eq!(after.cargo, before.cargo);
        assert_eq!(after.task, before.task);
    }
    towns = reload(towns);
    // Retire the old navigation instance: saved facts reconstruct fresh searches.
    pump = NavigationPump::new(
        Limits {
            jobs: 2,
            ..Limits::default()
        },
        Cadence {
            allowance: 128,
            ..Cadence::default()
        },
        Duration::ZERO,
    );
    settle(
        &mut pump,
        &mut towns,
        &keys,
        &open,
        3,
        TravelStatus::Walking,
    );
    let mut arrivals = 0;
    for ((t, id), before) in keys.iter().zip(&saved) {
        let town = &mut towns[*t];
        let RouteState::Ready { request, .. } = town.actor(id).unwrap().route() else {
            panic!("ready")
        };
        assert_eq!(request.origin(), before.position);
        assert_eq!(
            Some(request.destination()),
            town.actor(id).unwrap().travel_destination()
        );
        let q = open.query(request);
        town.advance_travel(id, Seconds::new(0.05).unwrap(), 10, |a, b| {
            q.segment_safe(a, b).then(|| Speed::new(1.0).unwrap())
        })
        .unwrap();
        assert!(
            (town.actor(id).unwrap().state().position.x.get() - before.position.x.get() - 0.05)
                .abs()
                < 1e-12
        );
        for _ in 0..2 {
            let a = town
                .advance_travel(id, Seconds::new(100.0).unwrap(), 64, |a, b| {
                    q.segment_safe(a, b).then(|| Speed::new(1.0).unwrap())
                })
                .unwrap();
            arrivals += usize::from(a.arrived_now);
            assert_eq!(a.status, TravelStatus::Arrived);
        }
        assert_eq!(town.actor(id).unwrap().state().cargo, before.cargo);
        assert_eq!(town.actor(id).unwrap().state().task, before.task);
        assert_eq!(town.resume_travel(id, NavigationRevision(4)).unwrap(), None);
    }
    assert_eq!(arrivals, 16);
    assert!(pump.diagnostics().max_jobs <= 2);
    assert_eq!(pump.diagnostics().rejected, 0);
    assert_eq!(pump.service().resident_jobs(), 0);
    towns = reload(towns);
    for ((t, id), before) in keys.iter().zip(&saved) {
        let town = &mut towns[*t];
        assert_eq!(
            town.actor(id).unwrap().travel_status(),
            TravelStatus::Arrived
        );
        assert_eq!(town.resume_travel(id, NavigationRevision(5)).unwrap(), None);
        assert!(
            !town
                .advance_travel(id, Seconds::new(100.0).unwrap(), 64, |_, _| panic!(
                    "completed leg must not restart"
                ))
                .unwrap()
                .arrived_now
        );
        assert_eq!(town.actor(id).unwrap().state().cargo, before.cargo);
        assert_eq!(town.actor(id).unwrap().state().task, before.task);
    }
}
