use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{orchards::*, progression::Worker, resources::*};
use std::collections::BTreeMap;
fn owner() -> Owner {
    Owner {
        world: WorldId::new("orchards").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn fruit() -> ResourceKey {
    ResourceKey::parse("fruit").unwrap()
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}")
}
fn stage(s: &str) -> Stage {
    match s {
        "sapling" => Stage::Sapling,
        "blossom" => Stage::Blossom,
        "fruit" => Stage::Fruit,
        "ripe" => Stage::Ripe,
        "harvested" => Stage::Harvested,
        _ => panic!("stage"),
    }
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    orchards: Orchards,
    worker: Worker,
    nav: u64,
}
impl Fixture {
    fn new(stages: &[Stage]) -> Self {
        let mut buildings = vec![Building {
            id: b(1),
            location: Tile { x: 3, y: 3 },
            construction: ConstructionState::Complete,
        }];
        let mut stores = vec![Store::new(b(1), StoragePolicy::new("orchard"))];
        let mut trees = Vec::new();
        for (i, s) in stages.iter().enumerate() {
            let id = b(i as u64 + 2);
            buildings.push(Building {
                id: id.clone(),
                location: Tile {
                    x: 4 + i as i32,
                    y: 4,
                },
                construction: ConstructionState::Complete,
            });
            stores.push(Store::new(id.clone(), StoragePolicy::new("orchard_tree")));
            trees.push(Tree {
                id,
                orchard: b(1),
                stage: *s,
                elapsed: sec(0.),
                fruit: q(if *s == Stage::Harvested { 0. } else { 8. }),
            });
        }
        let town = Town::restore(
            owner(),
            buildings,
            (1..=3)
                .map(|i| ActorState::new(a(i), p(4., 3.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        let orchards = Orchards::new(
            &town,
            &ledger,
            trees,
            (1..=3).map(|i| (a(i), b(1))).collect(),
        )
        .unwrap();
        let mut worker = Worker::new("orcharding").unwrap();
        worker
            .set_practice("farming", Practice::new(20.).unwrap())
            .unwrap();
        Self {
            town,
            ledger,
            orchards,
            worker,
            nav: 1,
        }
    }
    fn assign(&mut self, i: u64) -> Option<BuildingId> {
        self.orchards
            .assign(
                &mut self.town,
                &self.ledger,
                &a(i),
                &b(1),
                NavigationRevision(self.nav),
            )
            .unwrap()
    }
    fn route(&mut self, i: u64, outcome: RouteOutcome) {
        self.nav += 1;
        self.town
            .resume_travel(&a(i), NavigationRevision(self.nav))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            panic!("route")
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), outcome)
            .unwrap();
    }
    fn ready(&mut self, i: u64) {
        self.assign(i).unwrap();
        self.route(i, RouteOutcome::Arrived);
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        self.orchards
            .advance(
                &mut self.town,
                &mut self.ledger,
                &a(i),
                &mut self.worker,
                sec(n),
                NavigationRevision(self.nav),
                |_, _, s| Some(s),
            )
            .unwrap()
    }
    fn reload(&mut self) {
        let s = self.orchards.encode(&self.town, &self.ledger).unwrap();
        let mut bs = self.town.buildings().to_vec();
        bs.reverse();
        self.town = Town::restore(
            owner(),
            bs,
            self.town
                .actors()
                .iter()
                .map(|a| a.state().clone())
                .collect(),
        )
        .unwrap();
        self.orchards = Orchards::decode(&s, &self.town, &self.ledger).unwrap();
    }
    fn moved(&mut self) {
        let before = self.town.clone();
        let mut after = self.town.clone();
        for b in before.buildings() {
            after
                .relocate_building(
                    &b.id,
                    Tile {
                        x: b.location.x + 2,
                        y: b.location.y + 1,
                    },
                )
                .unwrap();
        }
        self.orchards
            .reconcile_relocation(&before, &mut after, &self.ledger, NavigationRevision(10))
            .unwrap();
        self.town = after;
    }
}
#[test]
fn paired_python_growth_restocks_and_stops_at_ripe() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/farming/orchard_cases.json")).unwrap();
    for c in data["growth"].as_array().unwrap() {
        let mut f = Fixture::new(&[stage(c["stage"].as_str().unwrap())]);
        for n in c["parts"].as_array().unwrap() {
            f.orchards
                .grow(&f.town, &f.ledger, sec(n.as_f64().unwrap()));
            f.reload();
        }
        let t = f.orchards.tree(&b(2)).unwrap();
        assert_eq!(t.stage, stage(c["state"]["stage"].as_str().unwrap()));
        close(t.elapsed.get(), c["state"]["elapsed"].as_f64().unwrap());
        close(t.fruit.get(), c["state"]["fruit"].as_f64().unwrap());
        assert_eq!(f.ledger.produced().quantity(&fruit()), q(0.));
    }
}
#[test]
fn paired_python_pick_duration_levels_talents_caps_and_partial_fruit() {
    let data: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/farming/orchard_cases.json")).unwrap();
    for c in data["picks"].as_array().unwrap() {
        let mut f = Fixture::new(&[Stage::Ripe]);
        let mut policy = StoragePolicy::new("orchard");
        policy.level = c["level"].as_u64().unwrap() as u8;
        f.ledger.update_storage(&b(1), policy).unwrap();
        f.worker
            .set_practice(
                "orcharding",
                Practice::new(c["skill"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.worker
            .set_talents(BTreeMap::from([(
                "farming".into(),
                c["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.ledger
            .configure_cap(fruit(), c["cap"].as_f64().map(q))
            .unwrap();
        if c["cap"] == 0 {
            assert_eq!(f.assign(1), None);
        } else {
            f.ready(1);
            f.reload();
            let Outcome::Picked {
                amount, remaining, ..
            } = f.advance(1, c["duration"].as_f64().unwrap())
            else {
                panic!("pick {c}")
            };
            close(amount.get(), c["produced"].as_f64().unwrap());
            close(remaining.get(), 0.);
            f.reload();
        }
        let t = f.orchards.tree(&b(2)).unwrap();
        assert_eq!(t.stage, stage(c["state"]["stage"].as_str().unwrap()));
        close(t.fruit.get(), c["state"]["fruit"].as_f64().unwrap());
        close(
            f.worker.practice_value("orcharding"),
            c["practice"].as_f64().unwrap(),
        );
        close(f.ledger.free(&fruit()).unwrap().get(), 0.);
        close(
            f.ledger.produced().quantity(&fruit()).get(),
            c["produced"].as_f64().unwrap(),
        );
    }
}
#[test]
fn exclusive_claims_cargo_return_survives_removed_tree_and_blockage() {
    let mut f = Fixture::new(&[Stage::Ripe, Stage::Ripe]);
    assert_eq!(f.assign(1), Some(b(2)));
    assert_eq!(f.assign(2), Some(b(3)));
    assert_eq!(f.assign(3), None);
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 3.2), Outcome::Picked { .. }));
    f.orchards.remove_tree(&mut f.town, &b(2)).unwrap();
    f.town.remove_building(&b(2)).unwrap();
    f.reload();
    f.route(1, RouteOutcome::Ready(vec![p(3., 3.)]));
    assert_eq!(
        f.orchards
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(100.),
                NavigationRevision(f.nav),
                |_, _, _| None
            )
            .unwrap(),
        Outcome::Waiting
    );
    f.reload();
    f.route(1, RouteOutcome::Unreachable);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting);
    f.route(1, RouteOutcome::Arrived);
    f.advance(1, 0.25);
    f.reload();
    assert!(matches!(f.advance(1, 0.4), Outcome::Unloaded { .. }));
    close(f.ledger.free(&fruit()).unwrap().get(), 8.);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting);
}
#[test]
fn linked_move_retains_identity_and_return_cargo_but_reapproaches_work() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 1.);
    f.moved();
    f.reload();
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, p(4., 3.));
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1,3.2),Outcome::Picked{amount,..}if amount==q(8.)));
    f.moved();
    f.reload();
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    close(f.ledger.free(&fruit()).unwrap().get(), 8.);
}
#[test]
fn subdivision_pick_clocks_at_skill_boundary_are_stable_across_reload() {
    let mut results = Vec::new();
    for parts in [vec![3.2], vec![0.1; 32], vec![0.8; 4]] {
        let mut f = Fixture::new(&[Stage::Ripe]);
        f.worker
            .set_practice("orcharding", Practice::new(19.98).unwrap())
            .unwrap();
        f.ready(1);
        let mut picked = 0;
        for n in parts {
            if matches!(f.advance(1, n), Outcome::Picked { .. }) {
                picked += 1;
            }
            f.reload();
        }
        assert_eq!(picked, 1);
        results.push((
            f.ledger.produced().quantity(&fruit()).get(),
            f.worker.practice_value("orcharding"),
        ));
    }
    for r in &results {
        close(r.0, results[0].0);
        close(r.1, results[0].1);
    }
}
#[test]
fn malformed_reload_and_legacy_tree_facts_are_checked() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    let json = f.orchards.encode(&f.town, &f.ledger).unwrap();
    for mode in 0..6 {
        let mut v: serde_json::Value = serde_json::from_str(&json).unwrap();
        match mode {
            0 => v["version"] = 2.into(),
            1 => v["world"] = "foreign".into(),
            2 => {
                let t = v["trees"][0].clone();
                v["trees"].as_array_mut().unwrap().push(t);
            }
            3 => v["jobs"][0]["clock"] = 100.into(),
            4 => v["trees"][0]["fruit"] = 9.into(),
            5 => v["trees"][0]["stage"] = "unknown".into(),
            _ => unreachable!(),
        };
        assert!(Orchards::decode(&v.to_string(), &f.town, &f.ledger).is_err());
    }
    let raw = vec![
        serde_json::json!({"kind":"orchard"}),
        serde_json::json!({"kind":"orchard_tree","orchard_index":0,"tree":{"stage":"ripe","elapsed":0,"fruit":3.5}}),
    ];
    let o =
        Orchards::admit_legacy(&f.town, &f.ledger, &raw, &[b(1), b(2)], BTreeMap::new()).unwrap();
    assert_eq!(o.tree(&b(2)).unwrap().fruit, q(3.5));
}

#[test]
fn empty_unfinished_full_and_unqualified_orchards_do_not_train_or_produce() {
    let mut f = Fixture::new(&[]);
    assert_eq!(f.assign(1), None);
    let mut f = Fixture::new(&[Stage::Sapling]);
    assert_eq!(f.assign(1), None);
    let mut policy = StoragePolicy::new("orchard_tree");
    policy.construction = true;
    f.ledger.update_storage(&b(2), policy).unwrap();
    f.orchards.grow(&f.town, &f.ledger, sec(1000.));
    assert_eq!(f.orchards.tree(&b(2)).unwrap().stage, Stage::Sapling);
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ledger
        .produce(
            &b(1),
            None,
            &Stock::default(),
            &Stock::from_entries([(fruit(), q(50.))]).unwrap(),
        )
        .unwrap();
    assert_eq!(f.assign(1), None);
    close(f.worker.practice_value("orcharding"), 0.);
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.worker
        .set_practice("farming", Practice::new(0.).unwrap())
        .unwrap();
    f.ready(1);
    let before = (
        f.orchards.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    assert!(
        f.orchards
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(10.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.orchards, f.town, f.ledger, f.worker), before);
}
#[test]
fn assignment_change_preserves_old_orchard_cargo_until_its_delivery() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 3.2);
    f.town
        .insert_building(Building {
            id: b(10),
            location: Tile { x: 7, y: 7 },
            construction: ConstructionState::Complete,
        })
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(10), StoragePolicy::new("orchard")));
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    f.orchards
        .set_assignment(&mut f.town, &f.ledger, &a(1), Some(&b(10)))
        .unwrap();
    f.orchards
        .assign(&mut f.town, &f.ledger, &a(1), &b(10), NavigationRevision(1))
        .unwrap();
    f.reload();
    f.route(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 0.65), Outcome::Unloaded { .. }));
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .output
            .quantity(&fruit())
            .get(),
        8.,
    );
    close(
        f.ledger
            .store(&b(10))
            .unwrap()
            .output
            .quantity(&fruit())
            .get(),
        0.,
    );
}
#[test]
fn partial_pick_retries_require_another_pick_and_restock_is_exact() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ledger.configure_cap(fruit(), Some(q(0.5))).unwrap();
    f.ready(1);
    let Outcome::Picked { amount, .. } = f.advance(1, 3.2) else {
        panic!("partial")
    };
    assert_eq!(amount, q(0.5));
    f.route(1, RouteOutcome::Arrived);
    f.advance(1, 0.65);
    assert_eq!(f.assign(1), None);
    f.ledger.configure_cap(fruit(), None).unwrap();
    f.ready(1);
    assert_eq!(f.advance(1, 0.1), Outcome::Progress);
    f.reload();
    assert!(matches!(f.advance(1,3.1),Outcome::Picked{amount,..}if amount==q(7.5)));
    f.orchards.grow(&f.town, &f.ledger, sec(120.));
    assert_eq!(f.orchards.tree(&b(2)).unwrap().stage, Stage::Ripe);
    assert_eq!(f.orchards.tree(&b(2)).unwrap().fruit, q(8.));
}
#[test]
fn pick_completion_generation_failure_rolls_back_tree_goods_and_practice() {
    let mut f = Fixture::new(&[Stage::Ripe]);
    f.ready(1);
    f.advance(1, 0.5);
    let mut actors: Vec<_> = f.town.actors().iter().map(|a| a.state().clone()).collect();
    actors[0].route_generation = RouteGeneration::from_saved(u64::MAX);
    f.town = Town::restore(owner(), f.town.buildings().to_vec(), actors).unwrap();
    let before = (
        f.orchards.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.worker.clone(),
    );
    assert!(
        f.orchards
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                sec(2.7),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.orchards, f.town, f.ledger, f.worker), before);
}

#[test]
fn partial_tree_selection_is_retained_after_delivery_and_a_new_cap_stops_approach() {
    let mut f = Fixture::new(&[Stage::Ripe, Stage::Ripe]);
    f.town.reposition_actor(&a(1), p(5., 4.)).unwrap();
    assert_eq!(f.assign(1), Some(b(3)));
    f.route(1, RouteOutcome::Arrived);
    f.ledger.configure_cap(fruit(), Some(q(0.5))).unwrap();
    f.advance(1, 3.2);
    f.route(1, RouteOutcome::Ready(vec![p(3., 3.)]));
    assert!(matches!(f.advance(1, 10.), Outcome::Unloaded { .. }));
    f.ledger.configure_cap(fruit(), None).unwrap();
    f.reload();
    assert_eq!(f.assign(1), Some(b(3)));
    f.route(1, RouteOutcome::Ready(vec![p(5., 3.)]));
    f.ledger.configure_cap(fruit(), Some(q(0.5))).unwrap();
    let before = f.town.actor(&a(1)).unwrap().state().position;
    assert_eq!(f.advance(1, 100.), Outcome::Waiting);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, before);
}
