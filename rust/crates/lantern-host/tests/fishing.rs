use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    search::{Outcome as SearchOutcome, Search, Terrain},
    spatial::*,
    units::*,
};
use lantern_host::{
    dto::{decode_save, encode_save},
    fishing::*,
    progression::Worker,
    resources::*,
    state::{ModelWorld, PrivateCredentials},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
fn owner() -> Owner {
    Owner {
        world: WorldId::new("fishing").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn b() -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), 1).unwrap())
}
fn t(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn fish() -> ResourceKey {
    ResourceKey::parse("fish").unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/fishing/python.json")).unwrap()
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    waters: Waters,
    fishers: Fishers,
    worker: Worker,
}
impl Fixture {
    fn new() -> Self {
        Self {
            town: Town::restore(
                owner(),
                vec![Building {
                    id: b(),
                    location: t(5, 3),
                    construction: ConstructionState::Complete,
                }],
                (1..=2)
                    .map(|i| ActorState::new(a(i), p(4., 3.), Cargo::default()))
                    .collect(),
            )
            .unwrap(),
            ledger: Ledger::new(
                owner(),
                q(0.),
                vec![Store::new(b(), StoragePolicy::new("fishing_hut"))],
                vec![],
            )
            .unwrap(),
            waters: Waters::new(vec![
                Water {
                    tile: t(5, 2),
                    supply: q(80.),
                },
                Water {
                    tile: t(6, 2),
                    supply: q(80.),
                },
            ])
            .unwrap(),
            fishers: Fishers::default(),
            worker: Worker::new("fishing").unwrap(),
        }
    }
    fn geometry(&self, extra: Vec<Structure>) -> Geometry {
        let mut buildings = vec![Structure {
            footprint: Footprint::saved(self.town.building(&b()).unwrap().location, None).unwrap(),
            kind: "fishing_hut".into(),
            construction: Construction::Complete,
            fence_orientation: 0,
        }];
        buildings.extend(extra);
        let occupied = buildings.iter().flat_map(|s| s.footprint.tiles()).collect();
        let resources = self
            .waters
            .water()
            .iter()
            .map(|w| Resource {
                footprint: Footprint::saved(w.tile, None).unwrap(),
                kind: "water".into(),
            })
            .collect::<Vec<_>>();
        Geometry {
            index: Rc::new(SpatialIndex::derive(&buildings, &resources, &[], &[], 20).unwrap()),
            terrain: Terrain::default(),
            occupied,
        }
    }
    fn assign(&mut self, i: u64) {
        let g = self.geometry(vec![]);
        self.fishers
            .assign(
                &mut self.town,
                &mut self.ledger,
                &self.waters,
                &g,
                &a(i),
                &b(),
                &self.worker,
                NavigationRevision(1),
            )
            .unwrap();
    }
    fn resolve(&mut self, i: u64) {
        self.town
            .resume_travel(&a(i), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(request) = self.town.actor(&a(i)).unwrap().route().clone() else {
            return;
        };
        let mut search = Search::new(Rc::new(
            self.fishers
                .query(&self.town, &self.geometry(vec![]), &a(i))
                .unwrap(),
        ));
        while search.outcome() == &SearchOutcome::Pending {
            search.advance(100);
        }
        let outcome = match search.into_outcome() {
            SearchOutcome::Arrived => RouteOutcome::Arrived,
            SearchOutcome::Completed { path, .. } => RouteOutcome::Ready(
                path.into_iter()
                    .map(|t| p(t.x as f64, t.y as f64))
                    .collect(),
            ),
            SearchOutcome::Unreachable => RouteOutcome::Unreachable,
            _ => panic!("route"),
        };
        self.town
            .accept_route(&a(i), &request, request.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        let query = self
            .fishers
            .query(&self.town, &self.geometry(vec![]), &a(i))
            .ok();
        self.fishers
            .advance(
                &mut self.town,
                &mut self.ledger,
                &a(i),
                sec(n),
                NavigationRevision(1),
                |x, y, s| {
                    query
                        .as_ref()
                        .is_some_and(|q| q.segment_safe(x, y))
                        .then_some(s)
                },
            )
            .unwrap()
    }
    fn attempt(&mut self, i: u64, batch: u64, roll: u8) -> Catch {
        self.fishers
            .attempt(
                &self.town,
                &mut self.ledger,
                &mut self.waters,
                &a(i),
                &mut self.worker,
                batch,
                roll,
            )
            .unwrap()
    }
    fn phase(&self) -> (WorkPhase, f64) {
        let Task::Work { phase, elapsed, .. } = self.town.actor(&a(1)).unwrap().state().task else {
            panic!("work");
        };
        (phase, elapsed.get())
    }
    fn set_phase(&mut self, phase: WorkPhase, n: f64) {
        self.town
            .set_task(
                &a(1),
                Task::Work {
                    workplace: b(),
                    target: Destination::Position(p(5., 2.)),
                    phase,
                    elapsed: sec(n),
                },
            )
            .unwrap();
        let states = self
            .town
            .actors()
            .iter()
            .map(|actor| {
                let mut state = actor.state().clone();
                if state.id == a(1) {
                    state.position = p(5., 2.);
                }
                state
            })
            .collect();
        self.town = Town::restore(owner(), self.town.buildings().to_vec(), states).unwrap();
        let target = match phase {
            WorkPhase::Outbound => Some(Destination::Position(p(5., 2.))),
            WorkPhase::Returning => Some(Destination::Building(b())),
            _ => None,
        };
        if let Some(target) = target {
            self.town
                .request_route(&a(1), target, NavigationRevision(1))
                .unwrap();
        }
    }
    fn total(&self) -> f64 {
        self.ledger.overview().unwrap()["fish"].total
    }
    fn reload(&mut self) {
        let json = self.fishers.encode(&self.town).unwrap();
        let world = ModelWorld::new(
            owner().world,
            TerrainSeed(0),
            102,
            vec![self.town.clone()],
            PrivateCredentials::default(),
        )
        .unwrap();
        let loaded = decode_save(&encode_save(&world).unwrap()).unwrap();
        let town = &loaded.towns()[0];
        self.town = Town::restore(
            town.owner().clone(),
            town.buildings().to_vec(),
            town.actors().iter().map(|a| a.state().clone()).collect(),
        )
        .unwrap();
        self.fishers = Fishers::decode(&json, &self.town, &self.ledger).unwrap();
    }
}
#[test]
fn python_catch_rates_chance_tools_nets_levels_shared_supply_and_practice() {
    for c in data()["rates"].as_array().unwrap() {
        let mut f = Fixture::new();
        let mut policy = StoragePolicy::new("fishing_hut");
        policy.level = c["level"].as_u64().unwrap() as u8;
        policy.equipment = c["upgrades"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().into())
            .collect();
        f.ledger.update_storage(&b(), policy).unwrap();
        f.ledger
            .configure_cap(fish(), Some(q(c["room"].as_f64().unwrap())))
            .unwrap();
        f.worker
            .set_practice(
                "fishing",
                Practice::new(c["skill"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.worker
            .set_talents(BTreeMap::from([(
                "fishing".into(),
                c["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.worker.tool = c["tool"].as_str().map(str::to_owned);
        f.waters = Waters::new(vec![
            Water {
                tile: t(5, 2),
                supply: q(c["supply"].as_f64().unwrap()),
            },
            Water {
                tile: t(6, 2),
                supply: q(c["supply"].as_f64().unwrap()),
            },
        ])
        .unwrap();
        f.assign(1);
        f.resolve(1);
        let Catch::Attempt {
            caught,
            amount,
            consumed,
        } = f.attempt(1, 1, c["roll"].as_u64().unwrap() as u8)
        else {
            panic!("attempt");
        };
        assert_eq!(caught, c["roll"] != 0);
        close(amount.get(), c["amount"].as_f64().unwrap());
        close(consumed.get(), c["consumed"].as_f64().unwrap());
        close(
            f.worker.practice_value("fishing"),
            c["practice"].as_f64().unwrap(),
        );
        close(f.total(), amount.get());
        for w in f.waters.water() {
            close(
                w.supply.get(),
                c["supply"].as_f64().unwrap() - consumed.get(),
            );
        }
        f.reload();
        assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
        close(f.total(), amount.get());
    }
}
#[test]
fn python_depth_nearest_bank_connectivity_and_reach() {
    for c in data()["access"].as_array().unwrap() {
        let point =
            |v: &serde_json::Value| t(v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32);
        let water = Waters::new(
            c["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Water {
                    tile: point(v),
                    supply: q(80.),
                })
                .collect(),
        )
        .unwrap();
        let depths = water.depths();
        for v in c["depths"].as_array().unwrap() {
            assert_eq!(depths[&point(v)], v[2].as_u64().unwrap() as u32);
        }
        let occupied: BTreeSet<_> = c["occupied"]
            .as_array()
            .unwrap()
            .iter()
            .map(point)
            .collect();
        let actual = water.connected(
            point(&c["origin"]),
            c["reach"].as_u64().unwrap() as u32,
            c["tier"].as_u64().unwrap() as u8,
            |t| !occupied.contains(&t),
        );
        assert_eq!(
            actual,
            c["selected"]
                .as_array()
                .unwrap()
                .iter()
                .map(point)
                .collect::<Vec<_>>()
        );
    }
}
#[test]
fn python_independent_work_and_unload_clocks() {
    for c in data()["clocks"].as_array().unwrap() {
        let mut f = Fixture::new();
        f.assign(1);
        f.resolve(1);
        if c["pending"] == true {
            f.attempt(1, 1, 1);
        }
        let phase = if c["phase"] == "working" {
            WorkPhase::Working
        } else {
            WorkPhase::Depositing
        };
        f.set_phase(phase, c["clock"].as_f64().unwrap());
        f.reload();
        let result = f.advance(1, c["elapsed"].as_f64().unwrap());
        if c["finished"] == true {
            let Outcome::Unloaded(remaining) = result else {
                panic!("unloaded");
            };
            close(remaining.get(), c["remaining"].as_f64().unwrap());
            assert_eq!(f.phase(), (WorkPhase::Outbound, 0.));
        } else {
            let expected = match c["result_phase"].as_str().unwrap() {
                "working" => WorkPhase::Working,
                "returning" => WorkPhase::Returning,
                "depositing" => WorkPhase::Depositing,
                _ => panic!("phase"),
            };
            if expected == WorkPhase::Depositing && f.phase().0 == WorkPhase::Returning {
                f.resolve(1);
                f.advance(1, 0.);
            }
            assert_eq!(f.phase().0, expected);
            close(f.phase().1, c["result_clock"].as_f64().unwrap());
        }
        let actual = f.fishers.pier(&a(1)).unwrap();
        assert_eq!(actual, vec![t(5, 3), t(5, 2)]);
    }
}
#[test]
fn saved_pier_is_retained_and_private_to_the_fisher() {
    let mut f = Fixture::new();
    f.assign(1);
    let pier = f.fishers.pier(&a(1));
    let g = f.geometry(vec![]);
    assert!(g.index.blocked().contains(&t(5, 2)));
    f.reload();
    f.assign(1);
    assert_eq!(f.fishers.pier(&a(1)), pier);
    assert!(g.index.blocked().contains(&t(5, 2)));
    f.resolve(1);
    f.advance(1, 1.);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, p(5., 2.));
}
fn structure(at: Tile, kind: &str) -> Structure {
    Structure {
        footprint: Footprint::saved(at, None).unwrap(),
        kind: kind.into(),
        construction: Construction::Complete,
        fence_orientation: 0,
    }
}
#[test]
fn pier_obstacles_fences_void_and_far_water_use_bank_access() {
    for obstacle in ["home", "barn", "void"] {
        let mut f = Fixture::new();
        let mut g = f.geometry(if obstacle == "void" {
            vec![]
        } else {
            vec![structure(t(5, 2), obstacle)]
        });
        if obstacle == "void" {
            g.terrain.cells = Some(BTreeMap::from([(t(5, 2), 'v')]));
        }
        f.fishers
            .assign(
                &mut f.town,
                &mut f.ledger,
                &f.waters,
                &g,
                &a(1),
                &b(),
                &f.worker,
                NavigationRevision(1),
            )
            .unwrap();
        assert!(f.fishers.pier(&a(1)).unwrap().is_empty(), "{obstacle}");
    }
    let mut f = Fixture::new();
    let mut policy = StoragePolicy::new("fishing_hut");
    policy.level = 3;
    f.ledger.update_storage(&b(), policy).unwrap();
    f.waters = Waters::new(vec![Water {
        tile: t(9, 3),
        supply: q(80.),
    }])
    .unwrap();
    f.assign(1);
    assert!(f.fishers.pier(&a(1)).unwrap().is_empty());
    f.resolve(1);
    f.advance(1, 10.);
    let position = f.town.actor(&a(1)).unwrap().state().position;
    close(
        (position.x.get() - 9.).abs() + (position.y.get() - 3.).abs(),
        1.,
    );
}
#[test]
fn catches_during_confirmed_land_travel_match_python_but_pending_and_blocked_do_not() {
    let mut f = Fixture::new();
    f.assign(1);
    assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
    f.resolve(1);
    assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
    assert!(matches!(f.attempt(1, 2, 1), Catch::Attempt { .. }));
    close(f.worker.practice_value("fishing"), 0.24);
    let position = f.town.actor(&a(1)).unwrap().state().position;
    f.fishers
        .advance(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            sec(2.),
            NavigationRevision(1),
            |_, _, _| None,
        )
        .unwrap();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, position);
    assert_eq!(f.attempt(1, 3, 1), Catch::Skipped);
    close(f.total(), 1.);
}
#[test]
fn blocked_return_reload_and_dropoff_deliver_exactly_once() {
    let mut f = Fixture::new();
    f.waters = Waters::new(vec![Water {
        tile: t(5, 0),
        supply: q(80.),
    }])
    .unwrap();
    f.assign(1);
    f.resolve(1);
    f.advance(1, 2.);
    f.attempt(1, 1, 1);
    f.advance(1, 3.2);
    assert_eq!(f.phase().0, WorkPhase::Returning);
    f.reload();
    f.resolve(1);
    let before = f.town.actor(&a(1)).unwrap().state().position;
    f.fishers
        .advance(
            &mut f.town,
            &mut f.ledger,
            &a(1),
            sec(2.),
            NavigationRevision(1),
            |_, _, _| None,
        )
        .unwrap();
    f.reload();
    close(f.ledger.free(&fish()).unwrap().get(), 0.);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
    f.resolve(1);
    let result = f.advance(1, 3.);
    assert!(matches!(result, Outcome::Unloaded(_)));
    close(f.ledger.free(&fish()).unwrap().get(), 1.);
    f.reload();
    f.resolve(1);
    f.advance(1, 0.1);
    close(f.total(), 1.);
    close(f.ledger.produced().quantity(&fish()).get(), 1.);
}
#[test]
fn moved_hut_preserves_position_and_cargo_and_rebuilds_pier() {
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    f.advance(1, 0.2);
    f.attempt(1, 1, 1);
    let before = f.town.actor(&a(1)).unwrap().state().position;
    f.town.relocate_building(&b(), t(8, 3)).unwrap();
    f.assign(1);
    assert_eq!(f.phase().0, WorkPhase::Returning);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
    f.reload();
    f.resolve(1);
    assert!(matches!(f.advance(1, 10.), Outcome::Unloaded(_)));
    close(f.total(), 1.);
    close(f.ledger.free(&fish()).unwrap().get(), 1.);
}
#[test]
fn cancellation_and_replacement_keep_goods_and_attempt_history() {
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    f.attempt(1, 1, 1);
    f.town
        .set_task(
            &a(1),
            Task::Move {
                destination: p(0., 0.),
            },
        )
        .unwrap();
    assert_eq!(f.advance(1, 1.), Outcome::Cancelled);
    f.fishers.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
    close(f.total(), 1.);
    assert!(matches!(
        f.town.actor(&a(1)).unwrap().state().task,
        Task::Move { .. }
    ));
    f.assign(2);
    f.resolve(2);
    f.attempt(2, 2, 1);
    close(f.total(), 2.);
    assert!(
        !f.ledger
            .store(&b())
            .unwrap()
            .pending
            .contains_key(&PendingOwner::Orphan)
    );
    f.town.set_task(&a(1), Task::Idle).unwrap();
    f.assign(1);
    f.resolve(1);
    f.reload();
    assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
    close(f.total(), 2.);
}
#[test]
fn production_caps_full_buffer_and_competing_workers_do_not_overfill() {
    for cap in [true, false] {
        let mut f = Fixture::new();
        if cap {
            f.ledger.configure_cap(fish(), Some(q(1.5))).unwrap();
        } else {
            f.ledger
                .produce(
                    &b(),
                    None,
                    &Stock::default(),
                    &Stock::from_entries([(fish(), q(48.5))]).unwrap(),
                )
                .unwrap();
        }
        f.assign(1);
        f.assign(2);
        f.resolve(1);
        f.resolve(2);
        f.attempt(1, 1, 1);
        f.attempt(2, 1, 1);
        let skill = f.worker.practice_value("fishing");
        assert_eq!(f.attempt(1, 2, 1), Catch::Skipped);
        close(f.worker.practice_value("fishing"), skill);
        close(f.total(), if cap { 1.5 } else { 50. });
    }
}
#[test]
fn strict_reload_rejects_bad_owners_ids_decks_clocks_and_cargo() {
    let mut f = Fixture::new();
    f.assign(1);
    let json = f.fishers.encode(&f.town).unwrap();
    for mutation in 0..8 {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        match mutation {
            0 => v["world"] = "other".into(),
            1 => v["version"] = 2.into(),
            2 => {
                let j = v["jobs"][0].clone();
                v["jobs"].as_array_mut().unwrap().push(j);
            }
            3 => v["jobs"][0][0] = 999.into(),
            4 => v["jobs"][0][1]["pier"] = serde_json::json!([[5, 3], [5, 0], [5, 2]]),
            5 => v["jobs"][0][1]["hut"] = 900.into(),
            6 => v["jobs"][0][1]["source"] = serde_json::json!([15, 15]),
            _ => v["unexpected"] = true.into(),
        };
        assert!(
            Fishers::decode(&v.to_string(), &f.town, &f.ledger).is_err(),
            "{mutation}"
        );
    }
    f.set_phase(WorkPhase::Working, 3.2);
    assert!(Fishers::decode(&json, &f.town, &f.ledger).is_err());
}
#[test]
fn python_founder_land_fishing_has_distinct_chance_practice_and_direct_rewards() {
    for c in data()["founder"].as_array().unwrap() {
        let mut f = Fixture::new();
        let mut angler = LandAngler::new(owner());
        let mut founder = lantern_core::founder::FounderState {
            position: p(4., 2.),
            facing: -1,
        };
        let mut skill = Practice::new(c["skill"].as_f64().unwrap()).unwrap();
        angler.toggle(&f.town, &f.waters, &mut founder).unwrap();
        assert_eq!(founder.facing, 1);
        angler
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.waters,
                founder.position,
                &mut skill,
                1,
                c["roll"].as_u64().unwrap() as u8,
            )
            .unwrap();
        close(
            f.ledger.free(&fish()).unwrap().get(),
            c["amount"].as_f64().unwrap(),
        );
        close(skill.get(), c["practice"].as_f64().unwrap());
        close(
            f.waters.water()[0].supply.get(),
            c["supply"].as_f64().unwrap(),
        );
        angler = LandAngler::decode(&angler.encode().unwrap(), &f.town).unwrap();
        assert_eq!(
            angler
                .attempt(
                    &f.town,
                    &mut f.ledger,
                    &mut f.waters,
                    founder.position,
                    &mut skill,
                    1,
                    1
                )
                .unwrap(),
            Catch::Skipped
        );
        angler.toggle(&f.town, &f.waters, &mut founder).unwrap();
        assert_eq!(angler.status(), AnglerStatus::Idle);
        assert_eq!(angler.source(), None);
    }
}
#[test]
fn founder_range_stop_capacity_and_invalid_reload() {
    let mut f = Fixture::new();
    let mut angler = LandAngler::new(owner());
    let mut founder = lantern_core::founder::FounderState {
        position: p(4., 2.),
        facing: -1,
    };
    let mut skill = Practice::new(0.).unwrap();
    angler.toggle(&f.town, &f.waters, &mut founder).unwrap();
    let saved = angler.encode().unwrap();
    let mut bad: serde_json::Value = serde_json::from_str(&saved).unwrap();
    bad["world"] = "other".into();
    assert!(LandAngler::decode(&bad.to_string(), &f.town).is_err());
    bad = serde_json::from_str(&saved).unwrap();
    bad.as_object_mut().unwrap().remove("source");
    assert!(LandAngler::decode(&bad.to_string(), &f.town).is_err());
    founder.position = p(1., 1.);
    assert!(angler.toggle(&f.town, &f.waters, &mut founder).is_err());
    assert_eq!(angler.encode().unwrap(), saved);
    angler
        .attempt(
            &f.town,
            &mut f.ledger,
            &mut f.waters,
            founder.position,
            &mut skill,
            1,
            1,
        )
        .unwrap();
    assert_eq!(angler.status(), AnglerStatus::Idle);
    close(skill.get(), 0.);
    founder.position = p(4., 2.);
    angler.toggle(&f.town, &f.waters, &mut founder).unwrap();
    f.ledger.configure_cap(fish(), Some(q(0.))).unwrap();
    assert_eq!(
        angler
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.waters,
                founder.position,
                &mut skill,
                2,
                1
            )
            .unwrap(),
        Catch::Skipped
    );
    assert_eq!(angler.status(), AnglerStatus::Cap);
    close(skill.get(), 0.);
    angler.stop();
    assert_eq!(angler.status(), AnglerStatus::Idle);
}
#[test]
fn all_nine_villager_success_rolls_and_invalid_rolls() {
    for roll in 1..10 {
        let mut f = Fixture::new();
        f.assign(1);
        f.resolve(1);
        assert!(matches!(
            f.attempt(1, 1, roll),
            Catch::Attempt { caught: true, .. }
        ));
        close(f.total(), 1.);
    }
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    let before = (
        f.fishers.clone(),
        f.ledger.clone(),
        f.waters.clone(),
        f.worker.clone(),
    );
    assert!(
        f.fishers
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.waters,
                &a(1),
                &mut f.worker,
                1,
                10
            )
            .is_err()
    );
    assert_eq!(
        (
            f.fishers.clone(),
            f.ledger.clone(),
            f.waters.clone(),
            f.worker.clone()
        ),
        before
    );
}
#[test]
fn failed_completion_rolls_back_supply_practice_history_and_goods() {
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    f.worker
        .set_talents(BTreeMap::from([("fishing".into(), f64::MAX)]))
        .unwrap();
    let before = (
        f.fishers.clone(),
        f.ledger.clone(),
        f.waters.clone(),
        f.worker.clone(),
    );
    // Practice overflows after goods and supply have been calculated privately.
    f.worker
        .set_practice("fishing", Practice::new(f64::MAX).unwrap())
        .unwrap();
    let worker = f.worker.clone();
    assert!(
        f.fishers
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.waters,
                &a(1),
                &mut f.worker,
                1,
                1
            )
            .is_err()
    );
    assert_eq!(
        (f.fishers, f.ledger, f.waters, f.worker),
        (before.0, before.1, before.2, worker)
    );
}
#[test]
fn live_source_preference_range_and_independent_water_bodies() {
    let mut f = Fixture::new();
    f.waters = Waters::new(vec![
        Water {
            tile: t(5, 2),
            supply: q(0.),
        },
        Water {
            tile: t(7, 3),
            supply: q(10.),
        },
        Water {
            tile: t(15, 15),
            supply: q(50.),
        },
    ])
    .unwrap();
    f.assign(1);
    assert_eq!(f.fishers.source(&a(1)), Some(t(7, 3)));
    f.resolve(1);
    f.attempt(1, 1, 1);
    close(f.waters.water()[0].supply.get(), 0.);
    close(f.waters.water()[2].supply.get(), 50.);
    f.fishers.cancel(&mut f.town, &mut f.ledger, &a(1)).unwrap();
    f.waters = Waters::new(vec![Water {
        tile: t(9, 3),
        supply: q(80.),
    }])
    .unwrap();
    f.assign(1);
    assert_eq!(f.fishers.source(&a(1)), None);
    let mut policy = StoragePolicy::new("fishing_hut");
    policy.level = 2;
    f.ledger.update_storage(&b(), policy).unwrap();
    f.assign(1);
    assert_eq!(f.fishers.source(&a(1)), Some(t(9, 3)));
}
#[test]
fn invalidated_pier_requires_revalidation_without_editing_shared_geometry() {
    let mut f = Fixture::new();
    f.assign(1);
    let mut g = f.geometry(vec![]);
    g.terrain.cells = Some(BTreeMap::from([(t(5, 2), 'v')]));
    assert!(f.fishers.query(&f.town, &g, &a(1)).is_err());
    f.fishers
        .assign(
            &mut f.town,
            &mut f.ledger,
            &f.waters,
            &g,
            &a(1),
            &b(),
            &f.worker,
            NavigationRevision(2),
        )
        .unwrap();
    assert!(f.fishers.pier(&a(1)).unwrap().is_empty());
    assert!(g.index.blocked().contains(&t(5, 2)));
    let mut g = f.geometry(vec![]);
    g.terrain.bounds = Some((0, 4, 0, 19));
    f.fishers
        .assign(
            &mut f.town,
            &mut f.ledger,
            &f.waters,
            &g,
            &a(1),
            &b(),
            &f.worker,
            NavigationRevision(3),
        )
        .unwrap();
    assert!(f.fishers.pier(&a(1)).unwrap().is_empty());
    f.assign(1);
    assert_eq!(f.fishers.pier(&a(1)).unwrap(), vec![t(5, 3), t(5, 2)]);
}

#[test]
fn barn_fence_blocks_a_short_pier_without_occupying_its_tiles() {
    let mut f = Fixture::new();
    // Barn's north fence is y=2.5; its gate faces east, away from this deck.
    let g = f.geometry(vec![structure(t(5, 6), "barn")]);
    assert!(!g.occupied.contains(&t(5, 2)));
    assert!(lantern_core::spatial::crosses(
        p(5., 3.),
        p(5., 2.),
        g.index.fences()
    ));
    f.fishers
        .assign(
            &mut f.town,
            &mut f.ledger,
            &f.waters,
            &g,
            &a(1),
            &b(),
            &f.worker,
            NavigationRevision(1),
        )
        .unwrap();
    assert!(f.fishers.pier(&a(1)).unwrap().is_empty());
}

#[test]
fn reload_rejects_duplicate_foreign_and_impossible_catch_history() {
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    f.attempt(1, 1, 1);
    let json = f.fishers.encode(&f.town).unwrap();
    for change in 0..4 {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        match change {
            0 => {
                let h = v["history"][0].clone();
                v["history"].as_array_mut().unwrap().push(h);
            }
            1 => v["history"][0][0] = 999.into(),
            2 => v["history"][0][1]["attempts"] = 2.into(),
            _ => v["history"][0][1]["last_batch"] = 0.into(),
        }
        assert!(Fishers::decode(&v.to_string(), &f.town, &f.ledger).is_err());
    }
}

#[test]
fn removed_water_clears_the_deck_and_finishes_saved_cargo() {
    let mut f = Fixture::new();
    f.assign(1);
    f.resolve(1);
    f.advance(1, 1.);
    f.attempt(1, 1, 1);
    let mut founder = lantern_core::founder::FounderState {
        position: p(4., 2.),
        facing: 1,
    };
    let mut angler = LandAngler::new(owner());
    angler.toggle(&f.town, &f.waters, &mut founder).unwrap();
    f.waters = Waters::new(vec![]).unwrap();
    f.assign(1);
    assert!(f.fishers.pier(&a(1)).unwrap().is_empty());
    assert_eq!(f.phase().0, WorkPhase::Returning);
    f.reload();
    f.resolve(1);
    f.advance(1, 1.);
    close(f.ledger.free(&fish()).unwrap().get(), 1.);
    let mut skill = Practice::new(0.).unwrap();
    angler
        .attempt(
            &f.town,
            &mut f.ledger,
            &mut f.waters,
            founder.position,
            &mut skill,
            1,
            1,
        )
        .unwrap();
    assert_eq!(angler.status(), AnglerStatus::Idle);
    close(skill.get(), 0.);
}

#[test]
fn an_owner_cannot_accumulate_jobs_from_another_town() {
    let mut f = Fixture::new();
    f.assign(1);
    let before = f.fishers.clone();
    let other = Owner {
        world: owner().world,
        town: TownId::new("other").unwrap(),
    };
    let actor = ActorId::from_saved(EntityIdentity::from_saved(other.clone(), 101).unwrap());
    let hut = BuildingId::from_saved(EntityIdentity::from_saved(other.clone(), 1).unwrap());
    let mut town = Town::restore(
        other.clone(),
        vec![Building {
            id: hut.clone(),
            location: t(5, 3),
            construction: ConstructionState::Complete,
        }],
        vec![ActorState::new(actor.clone(), p(4., 3.), Cargo::default())],
    )
    .unwrap();
    let mut ledger = Ledger::new(
        other,
        q(0.),
        vec![Store::new(hut.clone(), StoragePolicy::new("fishing_hut"))],
        vec![],
    )
    .unwrap();
    assert!(
        f.fishers
            .assign(
                &mut town,
                &mut ledger,
                &f.waters,
                &f.geometry(vec![]),
                &actor,
                &hut,
                &f.worker,
                NavigationRevision(1)
            )
            .is_err()
    );
    assert_eq!(f.fishers, before);
    assert_eq!(town.actor(&actor).unwrap().state().task, Task::Idle);
}
