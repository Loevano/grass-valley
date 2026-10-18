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
        world: WorldId::new("boats").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
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
fn key(s: &str) -> ResourceKey {
    ResourceKey::parse(s).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
fn upgrades(tier: u8) -> BTreeSet<String> {
    ["boat", "improved_boat", "master_boat"]
        .iter()
        .take(tier as usize)
        .map(|s| (*s).into())
        .collect()
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    water: Waters,
    boats: Boats,
    land: Fishers,
    workers: Vec<Worker>,
    extra: Vec<Structure>,
}
impl Fixture {
    fn new(tier: u8) -> Self {
        let mut policy = StoragePolicy::new("fishing_hut");
        policy.equipment = upgrades(tier);
        Self {
            town: Town::restore(
                owner(),
                vec![Building {
                    id: b(1),
                    location: t(2, 2),
                    construction: ConstructionState::Complete,
                }],
                (1..=4)
                    .map(|i| ActorState::new(a(i), p(2., 3.), Cargo::default()))
                    .collect(),
            )
            .unwrap(),
            ledger: Ledger::new(owner(), q(200.), vec![Store::new(b(1), policy)], vec![]).unwrap(),
            water: Waters::new(
                (3..=14)
                    .map(|x| Water {
                        tile: t(x, 2),
                        supply: q(80.),
                    })
                    .collect(),
            )
            .unwrap(),
            boats: Boats::default(),
            land: Fishers::default(),
            workers: vec![Worker::new("fishing").unwrap(); 4],
            extra: vec![],
        }
    }
    fn geometry(&self) -> Geometry {
        let mut structures = self.extra.clone();
        structures.extend(self.town.buildings().iter().map(|b| Structure {
            footprint: Footprint::saved(b.location, None).unwrap(),
            kind: "fishing_hut".into(),
            construction: Construction::Complete,
            fence_orientation: 0,
        }));
        let occupied = structures
            .iter()
            .flat_map(|s| s.footprint.tiles())
            .collect();
        let resources = self
            .water
            .water()
            .iter()
            .map(|w| Resource {
                footprint: Footprint::saved(w.tile, None).unwrap(),
                kind: "water".into(),
            })
            .collect::<Vec<_>>();
        Geometry {
            index: Rc::new(SpatialIndex::derive(&structures, &resources, &[], &[], 32).unwrap()),
            terrain: Terrain::default(),
            occupied,
        }
    }
    fn reconcile(&mut self, roster: &[u64]) -> Vec<ActorId> {
        let g = self.geometry();
        self.boats
            .reconcile(
                &mut self.town,
                &mut self.ledger,
                &mut self.land,
                &self.water,
                &g,
                &b(1),
                &roster.iter().map(|&i| a(i)).collect::<Vec<_>>(),
                NavigationRevision(1),
            )
            .unwrap()
    }
    fn resolve(&mut self) {
        let captain = self.boats.captain(&self.town, &b(1)).unwrap();
        self.town
            .resume_travel(&captain, NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&captain).unwrap().route().clone() else {
            return;
        };
        let mut search = Search::new(Rc::new(
            self.boats
                .query(
                    &self.town,
                    &self.ledger,
                    &self.water,
                    &self.geometry(),
                    &b(1),
                )
                .unwrap(),
        ));
        while search.outcome() == &SearchOutcome::Pending {
            search.advance(500);
        }
        let outcome = match search.into_outcome() {
            SearchOutcome::Arrived => RouteOutcome::Arrived,
            SearchOutcome::Unreachable => RouteOutcome::Unreachable,
            SearchOutcome::Completed { path, .. } => RouteOutcome::Ready(
                path.into_iter()
                    .map(|t| p(t.x as f64, t.y as f64))
                    .collect(),
            ),
            _ => panic!("route"),
        };
        self.town
            .accept_route(&captain, &r, r.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, n: f64) -> BoatOutcome {
        let mut remaining = n;
        for _ in 0..8 {
            if matches!(
                self.boats.phase(&b(1)),
                Some(
                    VoyagePhase::Approach
                        | VoyagePhase::Board
                        | VoyagePhase::Sail
                        | VoyagePhase::Return
                )
            ) {
                self.resolve();
            }
            let g = self.geometry();
            let result = self
                .boats
                .advance(
                    &mut self.town,
                    &mut self.ledger,
                    &self.water,
                    &g,
                    &b(1),
                    sec(remaining),
                    NavigationRevision(1),
                    |_, _, s| Some(s),
                )
                .unwrap();
            match result {
                BoatOutcome::RouteRequested(r) => {
                    remaining = r.get();
                    if remaining <= 1e-9 {
                        return BoatOutcome::Advanced;
                    }
                }
                _ => return result,
            }
        }
        panic!("bounded legs");
    }
    fn offshore(&mut self) {
        self.reconcile(&[1, 2, 3, 4]);
        for _ in 0..30 {
            self.advance(1.);
            if self.boats.phase(&b(1)) == Some(VoyagePhase::Fish) {
                return;
            }
        }
        panic!("fish");
    }
    fn attempt(&mut self, i: u64, batch: u64, roll: u8) -> Catch {
        self.boats
            .attempt(
                &self.town,
                &mut self.ledger,
                &mut self.water,
                &b(1),
                &a(i),
                &mut self.workers[i as usize - 1],
                batch,
                roll,
            )
            .unwrap()
    }
    fn load(&self) -> f64 {
        self.boats
            .cargo(&self.town, &self.ledger, &b(1))
            .unwrap()
            .ordered_entries()
            .map(|(_, n)| n.get())
            .sum()
    }
    fn reload(&mut self) {
        let s = self.boats.encode(&self.town).unwrap();
        let shore = self.land.encode(&self.town).unwrap();
        let world = ModelWorld::new(
            owner().world,
            TerrainSeed(0),
            104,
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
        self.land = Fishers::decode(&shore, &self.town, &self.ledger).unwrap();
        self.boats = Boats::decode(&s, &self.town, &self.ledger, &self.land).unwrap();
    }
}
#[test]
fn tiers_own_one_captain_and_leave_excess_workers_on_land() {
    for tier in 1..=3 {
        let mut f = Fixture::new(tier);
        let crew = f.reconcile(&[1, 2, 3, 4]);
        assert_eq!(crew, (1..=u64::from(tier)).map(a).collect::<Vec<_>>());
        assert_eq!(f.boats.source(&b(1)), Some(t(6 + 2 * i32::from(tier), 2)));
        f.offshore();
        for i in 2..=u64::from(tier) {
            assert_eq!(
                f.town.actor(&a(i)).unwrap().state().position,
                f.town.actor(&a(1)).unwrap().state().position
            );
            assert_eq!(
                f.town.actor(&a(i)).unwrap().state().facing,
                f.town.actor(&a(1)).unwrap().state().facing
            );
        }
        assert_eq!(f.town.actor(&a(4)).unwrap().state().task, Task::Idle);
        f.reload();
    }
}
#[test]
fn shared_capacity_species_catches_and_unload_are_exactly_once() {
    for (tier, cap) in [(1, 5.), (2, 10.), (3, 20.)] {
        let mut f = Fixture::new(tier);
        f.offshore();
        for batch in 1..=30 {
            for i in 1..=u64::from(tier) {
                f.attempt(i, batch, 1);
            }
            if f.load() + 1e-9 >= cap {
                break;
            }
        }
        close(f.load(), cap);
        f.reload();
        assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
        let mut unloaded = false;
        for _ in 0..30 {
            if let BoatOutcome::Unloaded(_) = f.advance(1.) {
                unloaded = true;
                break;
            }
        }
        assert!(unloaded);
        close(f.load(), 0.);
        close(f.ledger.free(&key("fish")).unwrap().get(), cap);
        let position = f.town.actor(&a(1)).unwrap().state().position;
        assert_eq!(position, p(4., 2.), "unload remains aboard at the berth");
        f.reload();
        f.advance(0.1);
        close(f.ledger.free(&key("fish")).unwrap().get(), cap);
        close(f.ledger.produced().quantity(&key("fish")).get(), cap);
    }
}
#[test]
fn fractional_offshore_reload_and_blocking_preserve_pose_cargo_and_practice() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.attempt(1, 1, 1);
    f.boats.request_return(&f.town, &f.ledger, &b(1)).unwrap();
    f.advance(3.2);
    f.advance(0.2);
    let before = f.town.actor(&a(1)).unwrap().state().position;
    assert!(before.x.get().fract() != 0.);
    let load = f.load();
    f.reload();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
    f.extra.push(Structure {
        footprint: Footprint::saved(t(5, 2), None).unwrap(),
        kind: "home".into(),
        construction: Construction::Complete,
        fence_orientation: 0,
    });
    f.advance(10.);
    let blocked = f.town.actor(&a(1)).unwrap().state().position;
    f.reload();
    f.advance(10.);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, blocked);
    close(f.load(), load);
    assert_eq!(f.attempt(1, 2, 1), Catch::Skipped);
    f.extra.clear();
    for _ in 0..20 {
        if matches!(f.advance(1.), BoatOutcome::Unloaded(_)) {
            break;
        }
    }
    close(f.ledger.free(&key("fish")).unwrap().get(), load);
}
#[test]
fn partial_hold_stays_offshore_until_empty_supply_or_return_request() {
    for manual in [false, true] {
        let mut f = Fixture::new(1);
        f.offshore();
        f.attempt(1, 1, 1);
        f.advance(10.);
        assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Fish));
        if manual {
            f.boats.request_return(&f.town, &f.ledger, &b(1)).unwrap();
        } else {
            f.water = Waters::new(
                f.water
                    .water()
                    .iter()
                    .map(|w| Water {
                        tile: w.tile,
                        supply: q(0.),
                    })
                    .collect(),
            )
            .unwrap();
        }
        f.reload();
        for _ in 0..20 {
            if matches!(f.advance(1.), BoatOutcome::Unloaded(_)) {
                break;
            }
        }
        close(f.ledger.free(&key("fish")).unwrap().get(), 1.);
    }
}
#[test]
fn crew_promotion_cancellation_and_land_handoffs_do_not_duplicate_rewards() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.attempt(1, 1, 1);
    f.attempt(2, 1, 1);
    let pose = f.town.actor(&a(1)).unwrap().state().position;
    f.reconcile(&[2, 3, 4]);
    assert_eq!(f.boats.captain(&f.town, &b(1)).unwrap(), a(2));
    assert_eq!(f.town.actor(&a(2)).unwrap().state().position, pose);
    f.reload();
    f.boats
        .cancel(&mut f.town, &mut f.ledger, &mut f.land, &b(1))
        .unwrap();
    f.boats
        .cancel(&mut f.town, &mut f.ledger, &mut f.land, &b(1))
        .unwrap();
    let g = f.geometry();
    f.land
        .assign(
            &mut f.town,
            &mut f.ledger,
            &f.water,
            &g,
            &a(2),
            &b(1),
            &f.workers[1],
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(
        f.land
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.water,
                &a(2),
                &mut f.workers[1],
                1,
                1
            )
            .unwrap(),
        Catch::Skipped
    );
    close(f.ledger.overview().unwrap()["fish"].total, 2.5);
}
fn data() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/fishing/boats.json")).unwrap()
}
#[test]
fn python_boat_rates_tools_skills_capacity_supply_and_failed_catches() {
    for c in data()["rates"].as_array().unwrap() {
        let mut f = Fixture::new(c["tier"].as_u64().unwrap() as u8);
        let mut policy = f.ledger.store(&b(1)).unwrap().policy.clone();
        policy.level = c["level"].as_u64().unwrap() as u8;
        policy.equipment.extend(["nets".into(), "wool_nets".into()]);
        f.ledger.update_storage(&b(1), policy).unwrap();
        f.ledger
            .configure_cap(key("fish"), Some(q(c["room"].as_f64().unwrap())))
            .unwrap();
        f.water = Waters::new(
            f.water
                .water()
                .iter()
                .map(|w| Water {
                    tile: w.tile,
                    supply: q(c["supply"].as_f64().unwrap()),
                })
                .collect(),
        )
        .unwrap();
        f.workers[0]
            .set_practice(
                "fishing",
                Practice::new(c["skill"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.workers[0]
            .set_talents(BTreeMap::from([(
                "fishing".into(),
                c["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.workers[0].tool = c["tool"].as_str().map(str::to_owned);
        f.offshore();
        let Catch::Attempt {
            amount, consumed, ..
        } = f.attempt(1, 1, c["roll"].as_u64().unwrap() as u8)
        else {
            panic!("catch");
        };
        close(amount.get(), c["amount"].as_f64().unwrap());
        close(consumed.get(), c["consumed"].as_f64().unwrap());
        close(
            f.workers[0].practice_value("fishing"),
            c["practice"].as_f64().unwrap(),
        );
        f.reload();
        assert_eq!(f.attempt(1, 1, 1), Catch::Skipped);
    }
}
#[test]
fn python_deep_water_selection_species_and_islands() {
    for c in data()["selection"].as_array().unwrap() {
        let mut f = Fixture::new(c["tier"].as_u64().unwrap() as u8);
        f.town.relocate_building(&b(1), t(2, 8)).unwrap();
        for i in 1..=4 {
            f.town.reposition_actor(&a(i), p(2., 9.)).unwrap();
        }
        f.water = Waters::new(
            c["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| Water {
                    tile: t(v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32),
                    supply: q(80.),
                })
                .collect(),
        )
        .unwrap();
        f.offshore();
        let source = f.boats.source(&b(1)).unwrap();
        assert_eq!(
            source,
            t(
                c["source"][0].as_i64().unwrap() as i32,
                c["source"][1].as_i64().unwrap() as i32
            )
        );
        assert_eq!(
            f.water.depths()[&source],
            c["depth"].as_u64().unwrap() as u32
        );
        f.attempt(1, 1, 1);
        let stock = f.boats.cargo(&f.town, &f.ledger, &b(1)).unwrap();
        assert!(stock.quantity(&key(c["species"].as_str().unwrap())).get() > 0.);
        f.reload();
    }
}
#[test]
fn python_exploration_purchases_upgrades_building_and_afloat_bonus() {
    for c in data()["exploration"].as_array().unwrap() {
        let mut f = Fixture::new(0);
        let mut policy = StoragePolicy::new("hall");
        policy.level = 5;
        let mut store = Store::new(b(1), policy);
        store.stored =
            Stock::from_entries([(key("wood"), q(200.)), (key("planks"), q(100.))]).unwrap();
        f.ledger = Ledger::new(owner(), q(200.), vec![store], vec![]).unwrap();
        let mut boat = ExplorationBoat::new(owner());
        for tier in 1..=c["tier"].as_u64().unwrap() {
            boat.purchase(
                &f.town,
                &mut f.ledger,
                if tier > 1 {
                    BoatPurchase::Upgrade
                } else if c["build"] == true {
                    BoatPurchase::Build
                } else {
                    BoatPurchase::Buy
                },
                5,
                q(40.),
                false,
            )
            .unwrap();
            boat = ExplorationBoat::decode(&boat.encode().unwrap(), &f.town).unwrap();
        }
        close(
            f.ledger.free(&key("coins")).unwrap().get(),
            c["coins"].as_f64().unwrap(),
        );
        close(
            f.ledger.free(&key("wood")).unwrap().get(),
            c["wood"].as_f64().unwrap(),
        );
        close(
            f.ledger.free(&key("planks")).unwrap().get(),
            c["planks"].as_f64().unwrap(),
        );
        f.water = Waters::new(vec![Water {
            tile: t(20, 20),
            supply: q(80.),
        }])
        .unwrap();
        let mut founder = lantern_core::founder::FounderState {
            position: p(20., if c["afloat"] == true { 20. } else { 21. }),
            facing: 1,
        };
        let mut angler = LandAngler::new(owner());
        angler.toggle(&f.town, &f.water, &mut founder).unwrap();
        let mut practice = Practice::new(0.).unwrap();
        angler
            .attempt_afloat(
                &f.town,
                &mut f.ledger,
                &mut f.water,
                founder.position,
                &mut practice,
                1,
                1,
                &boat,
            )
            .unwrap();
        close(
            f.ledger.free(&key("fish")).unwrap().get(),
            c["catch"].as_f64().unwrap(),
        );
        close(practice.get(), 6.);
    }
}
#[test]
fn mixed_cargo_returns_with_species_and_original_source_intact() {
    let mut f = Fixture::new(3);
    f.offshore();
    let original = f.boats.source(&b(1));
    for (i, s, n) in [(1, "fish", 2.), (2, "trout", 3.), (3, "salmon", 1.)] {
        f.ledger
            .produce(
                &b(1),
                Some(&a(i)),
                &Stock::default(),
                &Stock::from_entries([(key(s), q(n))]).unwrap(),
            )
            .unwrap();
    }
    f.reload();
    f.reconcile(&[1, 2, 3, 4]);
    assert_eq!(f.boats.source(&b(1)), original);
    f.boats.request_return(&f.town, &f.ledger, &b(1)).unwrap();
    for _ in 0..30 {
        if matches!(f.advance(1.), BoatOutcome::Unloaded(_)) {
            break;
        }
    }
    for (s, n) in [("fish", 2.), ("trout", 3.), ("salmon", 1.)] {
        close(f.ledger.free(&key(s)).unwrap().get(), n);
    }
    close(f.load(), 0.);
}
#[test]
fn curved_water_has_no_land_shortcuts_and_preserves_retarget_position() {
    let mut f = Fixture::new(1);
    f.water = Waters::new(
        [(3, 2), (3, 3), (3, 4), (4, 4), (5, 4), (5, 3), (5, 2)]
            .into_iter()
            .map(|(x, y)| Water {
                tile: t(x, y),
                supply: q(80.),
            })
            .collect(),
    )
    .unwrap();
    f.reconcile(&[1]);
    for _ in 0..5 {
        f.advance(0.3);
    }
    let pose = f.town.actor(&a(1)).unwrap().state().position;
    f.reload();
    f.reconcile(&[1]);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, pose);
    let waters = f.water.tiles();
    f.resolve();
    if let RouteState::Ready { waypoints, .. } = f.town.actor(&a(1)).unwrap().route() {
        for p in waypoints {
            assert!(waters.contains(&t(p.x.get() as i32, p.y.get() as i32)));
        }
    }
    f.advance(0.1);
    let next = f.town.actor(&a(1)).unwrap().state().position;
    assert!((next.x.get() - pose.x.get()).hypot(next.y.get() - pose.y.get()) <= 0.1800001);
}
#[test]
fn python_full_partial_and_depleted_boat_work_clocks() {
    for c in data()["clocks"].as_array().unwrap() {
        let mut f = Fixture::new(1);
        f.offshore();
        f.town
            .set_task(
                &a(1),
                Task::Work {
                    workplace: b(1),
                    target: Destination::Position(p(8., 2.)),
                    phase: WorkPhase::Working,
                    elapsed: sec(c["clock"].as_f64().unwrap()),
                },
            )
            .unwrap();
        f.ledger
            .produce(
                &b(1),
                Some(&a(1)),
                &Stock::default(),
                &Stock::from_entries([(key("fish"), q(c["amount"].as_f64().unwrap()))]).unwrap(),
            )
            .unwrap();
        f.water = Waters::new(
            f.water
                .water()
                .iter()
                .map(|w| Water {
                    tile: w.tile,
                    supply: q(c["supply"].as_f64().unwrap()),
                })
                .collect(),
        )
        .unwrap();
        f.reload();
        f.advance(c["elapsed"].as_f64().unwrap());
        let expected = if c["phase"] == "working" {
            VoyagePhase::Fish
        } else {
            VoyagePhase::Return
        };
        assert_eq!(f.boats.phase(&b(1)), Some(expected));
        let state = f.town.actor(&a(1)).unwrap().state();
        let Task::Work { elapsed, .. } = state.task else {
            panic!("work");
        };
        close(elapsed.get(), c["result_clock"].as_f64().unwrap());
        close(state.position.x.get(), c["x"].as_f64().unwrap());
        close(state.position.y.get(), c["y"].as_f64().unwrap());
    }
}
#[test]
fn moved_hut_keeps_offshore_cargo_and_returns_to_the_new_berth() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.attempt(1, 1, 1);
    let pose = f.town.actor(&a(1)).unwrap().state().position;
    f.town.relocate_building(&b(1), t(3, 3)).unwrap();
    f.reconcile(&[1, 2, 3]);
    assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Return));
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, pose);
    f.reload();
    let berth = f.boats.berth(&b(1)).unwrap();
    for _ in 0..30 {
        if matches!(f.advance(1.), BoatOutcome::Unloaded(_)) {
            break;
        }
    }
    assert_eq!(
        f.town.actor(&a(1)).unwrap().state().position,
        p(berth.x as f64, berth.y as f64)
    );
    close(f.ledger.free(&key("fish")).unwrap().get(), 1.25);
}
#[test]
fn crew_changes_preserve_work_and_deposit_clocks() {
    let mut f = Fixture::new(2);
    f.offshore();
    let source = f.boats.source(&b(1)).unwrap();
    f.town
        .set_task(
            &a(1),
            Task::Work {
                workplace: b(1),
                target: Destination::Position(p(source.x as f64, source.y as f64)),
                phase: WorkPhase::Working,
                elapsed: sec(1.25),
            },
        )
        .unwrap();
    f.town.follow_work_actor(&a(2), &a(1)).unwrap();
    f.reconcile(&[1, 3]);
    let Task::Work { elapsed, .. } = f.town.actor(&a(1)).unwrap().state().task else {
        panic!("work");
    };
    close(elapsed.get(), 1.25);
    f.reload();
    f.attempt(1, 1, 1);
    f.boats.request_return(&f.town, &f.ledger, &b(1)).unwrap();
    for _ in 0..100 {
        f.advance(0.1);
        if f.boats.phase(&b(1)) == Some(VoyagePhase::Deposit) {
            break;
        }
    }
    let Task::Work {
        elapsed: before, ..
    } = f.town.actor(&a(1)).unwrap().state().task
    else {
        panic!("work");
    };
    f.reconcile(&[1, 4]);
    assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Deposit));
    let Task::Work { elapsed: after, .. } = f.town.actor(&a(1)).unwrap().state().task else {
        panic!("work");
    };
    assert_eq!(before, after);
    f.reload();
}
#[test]
fn orphan_adoption_is_bounded_by_the_shared_hold() {
    let mut f = Fixture::new(1);
    f.ledger
        .produce(
            &b(1),
            Some(&a(4)),
            &Stock::default(),
            &Stock::from_entries([(key("trout"), q(8.)), (key("salmon"), q(2.))]).unwrap(),
        )
        .unwrap();
    f.ledger.orphan_pending(&b(1), &a(4)).unwrap();
    f.offshore();
    f.attempt(1, 1, 1);
    close(f.load(), 5.);
    let store = f.ledger.store(&b(1)).unwrap();
    close(
        store.pending[&PendingOwner::Orphan]
            .quantity(&key("trout"))
            .get(),
        4.,
    );
    close(
        store.pending[&PendingOwner::Orphan]
            .quantity(&key("salmon"))
            .get(),
        2.,
    );
    close(f.ledger.produced().quantity(&key("trout")).get(), 8.);
    f.reload();
}
#[test]
fn strict_voyage_reload_rejects_bad_owners_crew_decks_and_clocks() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.attempt(1, 1, 1);
    let json = f.boats.encode(&f.town).unwrap();
    for change in 0..9 {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        match change {
            0 => v["world"] = "other".into(),
            1 => {
                let j = v["voyages"][0].clone();
                v["voyages"].as_array_mut().unwrap().push(j);
            }
            2 => v["voyages"][0][1]["crew"] = serde_json::json!([101, 101]),
            3 => v["voyages"][0][1]["crew"] = serde_json::json!([999]),
            4 => v["voyages"][0][1]["tier"] = 0.into(),
            5 => v["voyages"][0][1]["phase"] = "deposit".into(),
            6 => v["voyages"][0][1]["pier"] = serde_json::json!([[2, 2], [6, 2], [3, 2]]),
            7 => v["voyages"][0][1]["berth"] = serde_json::json!([30, 30]),
            _ => v["history"][0][1]["attempts"] = 3.into(),
        };
        assert!(
            Boats::decode(&v.to_string(), &f.town, &f.ledger, &f.land).is_err(),
            "{change}"
        );
    }
}
#[test]
fn boat_caps_and_failed_attempt_rollback_do_not_award_or_train_extra() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.ledger.configure_cap(key("fish"), Some(q(0.1))).unwrap();
    f.attempt(1, 1, 1);
    let skill = f.workers[1].practice_value("fishing");
    assert_eq!(f.attempt(2, 1, 1), Catch::Skipped);
    close(f.workers[1].practice_value("fishing"), skill);
    close(f.load(), 0.1);
    f.advance(5.);
    assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Return));
    let mut f = Fixture::new(1);
    f.offshore();
    f.workers[0]
        .set_practice("fishing", Practice::new(f64::MAX).unwrap())
        .unwrap();
    f.workers[0]
        .set_talents(BTreeMap::from([("fishing".into(), f64::MAX)]))
        .unwrap();
    let before = (
        f.boats.clone(),
        f.ledger.clone(),
        f.water.clone(),
        f.workers[0].clone(),
    );
    assert!(
        f.boats
            .attempt(
                &f.town,
                &mut f.ledger,
                &mut f.water,
                &b(1),
                &a(1),
                &mut f.workers[0],
                1,
                1
            )
            .is_err()
    );
    assert_eq!((f.boats, f.ledger, f.water, f.workers[0].clone()), before);
}
#[test]
fn exploration_requirements_material_failures_and_sandbox_build_are_atomic() {
    let mut f = Fixture::new(0);
    let mut boat = ExplorationBoat::new(owner());
    let before = f.ledger.clone();
    for (action, hall, xp) in [
        (BoatPurchase::Upgrade, 5, 40.),
        (BoatPurchase::Buy, 2, 40.),
        (BoatPurchase::Build, 2, 40.),
    ] {
        assert!(
            boat.purchase(&f.town, &mut f.ledger, action, hall, q(xp), false)
                .is_err()
        );
        assert_eq!(f.ledger, before);
        assert_eq!(boat.tier(), 0);
    }
    boat.purchase(&f.town, &mut f.ledger, BoatPurchase::Build, 3, q(0.), true)
        .unwrap();
    assert_eq!(f.ledger, before);
    assert_eq!(boat.tier(), 1);
    assert!(
        boat.purchase(&f.town, &mut f.ledger, BoatPurchase::Buy, 5, q(40.), false)
            .is_err()
    );
    assert!(
        boat.purchase(
            &f.town,
            &mut f.ledger,
            BoatPurchase::Upgrade,
            5,
            q(29.),
            false
        )
        .is_err()
    );
    assert!(
        boat.purchase(
            &f.town,
            &mut f.ledger,
            BoatPurchase::Upgrade,
            5,
            q(40.),
            false
        )
        .is_err(),
        "no planks"
    );
    assert_eq!(boat.tier(), 1);
    assert_eq!(f.ledger, before);
    let mut v: serde_json::Value = serde_json::from_str(&boat.encode().unwrap()).unwrap();
    v["tier"] = 4.into();
    assert!(ExplorationBoat::decode(&v.to_string(), &f.town).is_err());
}

#[test]
fn independent_huts_cannot_claim_the_same_actor_or_each_others_cargo() {
    let mut f = Fixture::new(1);
    f.town
        .insert_building(Building {
            id: b(2),
            location: t(2, 6),
            construction: ConstructionState::Complete,
        })
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    let mut policy = StoragePolicy::new("fishing_hut");
    policy.equipment = upgrades(2);
    stores.push(Store::new(b(2), policy));
    f.ledger = Ledger::new(owner(), q(200.), stores, vec![]).unwrap();
    let mut water = f.water.water().to_vec();
    water.extend((3..=14).map(|x| Water {
        tile: t(x, 6),
        supply: q(80.),
    }));
    f.water = Waters::new(water).unwrap();
    f.reconcile(&[1, 4]);
    let g = f.geometry();
    let crew = f
        .boats
        .reconcile(
            &mut f.town,
            &mut f.ledger,
            &mut f.land,
            &f.water,
            &g,
            &b(2),
            &[a(2), a(3)],
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(crew, vec![a(2), a(3)]);
    assert_eq!(f.boats.captain(&f.town, &b(1)).unwrap(), a(1));
    assert_eq!(f.boats.captain(&f.town, &b(2)).unwrap(), a(2));
    let before = (f.boats.clone(), f.town.clone(), f.ledger.clone());
    assert!(
        f.boats
            .reconcile(
                &mut f.town,
                &mut f.ledger,
                &mut f.land,
                &f.water,
                &g,
                &b(2),
                &[a(1)],
                NavigationRevision(1)
            )
            .is_err()
    );
    assert_eq!((f.boats.clone(), f.town.clone(), f.ledger.clone()), before);
    f.ledger
        .produce(
            &b(2),
            Some(&a(2)),
            &Stock::default(),
            &Stock::from_entries([(key("salmon"), q(3.))]).unwrap(),
        )
        .unwrap();
    close(
        f.boats
            .cargo(&f.town, &f.ledger, &b(1))
            .unwrap()
            .quantity(&key("salmon"))
            .get(),
        0.,
    );
    close(
        f.boats
            .cargo(&f.town, &f.ledger, &b(2))
            .unwrap()
            .quantity(&key("salmon"))
            .get(),
        3.,
    );
    f.reload();
}
#[test]
fn completed_hut_upgrades_change_crew_and_capacity_without_discarding_partial_cargo() {
    let mut f = Fixture::new(1);
    f.offshore();
    f.attempt(1, 1, 1);
    let source = f.boats.source(&b(1));
    let pose = f.town.actor(&a(1)).unwrap().state().position;
    let mut policy = f.ledger.store(&b(1)).unwrap().policy.clone();
    policy.construction = true;
    f.ledger.update_storage(&b(1), policy.clone()).unwrap();
    assert_eq!(f.attempt(1, 2, 1), Catch::Skipped);
    assert_eq!(f.boats.crew(&f.town, &b(1)).unwrap(), vec![a(1)]);
    policy.construction = false;
    policy.equipment = upgrades(3);
    f.ledger.update_storage(&b(1), policy).unwrap();
    f.reconcile(&[1, 2, 3, 4]);
    assert_eq!(
        f.boats.crew(&f.town, &b(1)).unwrap(),
        vec![a(1), a(2), a(3)]
    );
    assert_eq!(f.boats.source(&b(1)), source);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, pose);
    close(f.load(), 1.);
    f.reload();
    for batch in 3..20 {
        for i in 1..=3 {
            f.attempt(i, batch, 1);
        }
        if f.load() + 1e-9 >= 20. {
            break;
        }
    }
    close(f.load(), 20.);
}

#[test]
fn reassigned_crew_is_not_overwritten_by_captain_movement() {
    let mut f = Fixture::new(2);
    f.offshore();
    f.attempt(2, 1, 1);
    let task = Task::Move {
        destination: p(20., 20.),
    };
    f.town.set_task(&a(2), task.clone()).unwrap();
    let g = f.geometry();
    assert_eq!(
        f.boats
            .advance(
                &mut f.town,
                &mut f.ledger,
                &f.water,
                &g,
                &b(1),
                sec(3.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .unwrap(),
        BoatOutcome::NeedsReconcile
    );
    assert_eq!(f.town.actor(&a(2)).unwrap().state().task, task);
    f.reconcile(&[1, 3]);
    assert_eq!(f.town.actor(&a(2)).unwrap().state().task, task);
    close(
        f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Orphan]
            .quantity(&key("fish"))
            .get(),
        1.25,
    );
    f.reload();
}

#[test]
fn python_crew_slots_and_capacities_match_reviewed_vectors() {
    for c in data()["crews"].as_array().unwrap() {
        let mut f = Fixture::new(c["tier"].as_u64().unwrap() as u8);
        let crew = f.reconcile(&[1, 2, 3, 4]);
        assert_eq!(
            crew,
            c["crew"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| a(n.as_u64().unwrap() + 1))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            lantern_host::progression::fishing_boat_capacity(
                &f.ledger.store(&b(1)).unwrap().policy.equipment
            ),
            c["capacity"].as_u64().unwrap()
        );
    }
}

#[test]
fn exploration_boat_uses_existing_water_speed_and_preserves_land_speed() {
    use lantern_core::founder::{Direction, FounderScene, FounderState, screen_distance};
    let f = Fixture::new(0); // Typed saved tier stands for a completed, paid purchase; cost paths are paired above.
    let json = serde_json::json!({"version":1,"world":"boats","town":"one","tier":3});
    let boat = ExplorationBoat::decode(&json.to_string(), &f.town).unwrap();
    for afloat in [false, true] {
        let resources = if afloat {
            (8..=12)
                .flat_map(|x| {
                    (8..=12).map(move |y| Resource {
                        footprint: Footprint::saved(t(x, y), None).unwrap(),
                        kind: "water".into(),
                    })
                })
                .collect()
        } else {
            vec![]
        };
        let index = SpatialIndex::derive(&[], &resources, &[], &[], 32).unwrap();
        let scene = FounderScene {
            spatial: &index,
            terrain: |_| Some('g'),
        };
        let result = scene
            .walk(
                FounderState {
                    position: p(10., 10.),
                    facing: 1,
                },
                Direction::new(1, -1).unwrap(),
                boat.movement_access(false, false).unwrap(),
            )
            .unwrap();
        close(
            screen_distance(
                result.state.position.x.get() - 10.,
                result.state.position.y.get() - 10.,
            ),
            if afloat { 0.45 } else { 0.3 },
        );
        assert!(result.clear_activity);
    }
}

#[test]
fn a_wholly_new_crew_approaches_the_boat_before_fishing() {
    let mut f = Fixture::new(1);
    f.offshore();
    f.attempt(1, 1, 1);
    let before = f.town.actor(&a(3)).unwrap().state().position;
    f.reconcile(&[3]);
    assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Approach));
    assert_eq!(f.town.actor(&a(3)).unwrap().state().position, before);
    assert_eq!(f.attempt(3, 2, 1), Catch::Skipped);
    f.reload();
    for _ in 0..30 {
        f.advance(1.);
        if f.boats.phase(&b(1)) == Some(VoyagePhase::Fish) {
            break;
        }
    }
    assert_eq!(f.boats.phase(&b(1)), Some(VoyagePhase::Fish));
    assert!(matches!(f.attempt(3, 3, 1), Catch::Attempt { .. }));
}

#[test]
fn missing_water_releases_empty_voyage_without_losing_failed_catch_history() {
    let mut f = Fixture::new(1);
    f.offshore();
    f.attempt(1, 1, 0);
    f.water = Waters::new(vec![]).unwrap();
    assert!(f.reconcile(&[1]).is_empty());
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, Task::Idle);
    assert_eq!(f.land.history(&a(1)).unwrap().last_batch, 1);
    assert_eq!(f.land.history(&a(1)).unwrap().attempts, 1);
}
