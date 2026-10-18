use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{
    processing::*,
    progression::{Worker, Workplace},
    resources::*,
};
use serde_json::Value;
use std::collections::BTreeMap;
fn owner() -> Owner {
    Owner {
        world: WorldId::new("processing").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn key(k: &str) -> ResourceKey {
    ResourceKey::parse(k).unwrap()
}
fn stock(v: &Value) -> Stock {
    Stock::from_entries(
        v.as_object()
            .unwrap()
            .iter()
            .map(|(k, n)| (key(k), q(n.as_f64().unwrap()))),
    )
    .unwrap()
}
fn goods(entries: &[(&str, f64)]) -> Stock {
    Stock::from_entries(entries.iter().map(|(k, n)| (key(k), q(*n)))).unwrap()
}
#[track_caller]
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[track_caller]
fn same_stock(s: &Stock, v: &Value) {
    for (k, n) in s.entries() {
        close(n.get(), v[k.as_str()].as_f64().unwrap_or(0.));
    }
    for (k, n) in v.as_object().unwrap() {
        close(s.quantity(&key(k)).get(), n.as_f64().unwrap());
    }
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    processors: Processors,
    ore: IronDeposits,
    worker: Worker,
    workplace: Workplace,
}
impl Fixture {
    fn new(kind: &str, inputs: Stock) -> Self {
        let workplace = Workplace::new(kind);
        let mut worker = Worker::new(workplace.job().unwrap()).unwrap();
        worker.sandbox = true;
        let town = Town::restore(
            owner(),
            vec![
                Building {
                    id: b(1),
                    location: Tile { x: 0, y: 0 },
                    construction: ConstructionState::Complete,
                },
                Building {
                    id: b(2),
                    location: Tile { x: 3, y: 0 },
                    construction: ConstructionState::Complete,
                },
            ],
            (1..=3)
                .map(|i| ActorState::new(a(i), p(0., 1.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let mut dest = Store::new(b(1), StoragePolicy::new(kind));
        dest.inputs = inputs.clone();
        let mut source = Store::new(b(2), StoragePolicy::new("storehouse"));
        source.stored =
            Stock::from_entries(inputs.entries().keys().map(|k| (k.clone(), q(100.)))).unwrap();
        Self {
            town,
            ledger: Ledger::new(owner(), q(0.), vec![dest, source], vec![]).unwrap(),
            processors: Processors::default(),
            ore: IronDeposits::new(vec![IronVein {
                tile: Tile { x: 6, y: 0 },
                supply: q(60.),
            }])
            .unwrap(),
            worker,
            workplace,
        }
    }
    fn start(&mut self, i: u64) {
        self.processors
            .start(
                &mut self.town,
                &mut self.ledger,
                &a(i),
                &b(1),
                &self.worker,
                &self.workplace,
                NavigationRevision(1),
            )
            .unwrap();
    }
    fn ready(&mut self, i: u64, outcome: RouteOutcome) {
        self.town
            .resume_travel(&a(i), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(i)).unwrap().route().clone() else {
            panic!("pending route");
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        self.processors
            .advance(
                &mut self.town,
                &mut self.ledger,
                &mut self.ore,
                &a(i),
                &mut self.worker,
                &self.workplace,
                sec(n),
                NavigationRevision(1),
                |_, _, s| Some(s),
            )
            .unwrap()
    }
    fn work(&mut self, n: f64) -> Outcome {
        self.start(1);
        self.ready(1, RouteOutcome::Arrived);
        self.advance(1, n)
    }
    fn unload(&mut self, i: u64, n: f64) -> Outcome {
        self.processors.begin_unload(&mut self.town, &a(i)).unwrap();
        self.advance(i, n)
    }
    fn supply(&mut self, items: &Stock) {
        self.ledger.reserve(&b(2), &b(1), items).unwrap();
    }
}
#[test]
fn real_python_base_processor_rates_and_interruption_snapshots() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/processing/python.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let mut f = Fixture::new(c["kind"].as_str().unwrap(), stock(&c["inputs"]));
        f.worker
            .set_practice(
                f.worker.job().to_owned().as_str(),
                Practice::new(c["skill"].as_f64().unwrap_or(0.)).unwrap(),
            )
            .unwrap();
        let root = lantern_host::resources::public_catalog()["talent_roots"][f.worker.job()]
            .as_str()
            .unwrap_or(f.worker.job())
            .to_owned();
        f.worker
            .set_talents(BTreeMap::from([(root, c["talent"].as_f64().unwrap_or(0.))]))
            .unwrap();
        f.worker.tool = c["tool"].as_str().map(str::to_owned);
        f.workplace.equipment = c["equipment"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let mut stores = f.ledger.stores().to_vec();
        stores[0].policy.equipment = f.workplace.equipment.clone();
        stores[0].output = stock(&c["output"]);
        f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        f.ore = IronDeposits::new(
            c["ore"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| IronVein {
                    tile: Tile {
                        x: v["x"].as_i64().unwrap() as i32,
                        y: v["y"].as_i64().unwrap() as i32,
                    },
                    supply: q(v["supply"].as_f64().unwrap()),
                })
                .collect(),
        )
        .unwrap();
        f.start(1);
        f.ready(1, RouteOutcome::Arrived);
        for (action, state) in c["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(c["states"].as_array().unwrap())
        {
            let outcome = match action["op"].as_str().unwrap() {
                "advance" => Some(f.advance(1, action["seconds"].as_f64().unwrap())),
                "cap" => {
                    f.ledger
                        .configure_cap(
                            key(action["key"].as_str().unwrap()),
                            action["amount"].as_f64().map(q),
                        )
                        .unwrap();
                    None
                }
                "supply" => {
                    f.supply(&stock(&action["goods"]));
                    None
                }
                "cancel" => {
                    f.processors
                        .cancel(&mut f.town, &mut f.ledger, &a(1))
                        .unwrap();
                    None
                }
                "dropoff" => {
                    assert_eq!(f.unload(1, 0.65), Outcome::Unloaded(sec(0.)));
                    None
                }
                _ => panic!("operation"),
            };
            if let Some(status) = state["status"].as_str() {
                let matches = if status == "Working" {
                    matches!(outcome, Some(Outcome::Completed(_)))
                } else if status.starts_with("Producing") {
                    matches!(outcome, Some(Outcome::Working { .. }))
                } else {
                    matches!(outcome, Some(Outcome::Waiting(_)))
                };
                assert!(matches, "{}: {action:?}: {status}, {outcome:?}", c["name"]);
            }
            let store = f.ledger.store(&b(1)).unwrap();
            same_stock(&store.inputs, &state["inputs"]);
            same_stock(&store.output, &state["output"]);
            same_stock(
                store
                    .pending
                    .get(&PendingOwner::Worker(a(1)))
                    .unwrap_or(&Stock::default()),
                &state["pending"],
            );
            same_stock(
                store
                    .pending
                    .get(&PendingOwner::Orphan)
                    .unwrap_or(&Stock::default()),
                &state["orphan"],
            );
            same_stock(f.ledger.produced(), &state["produced"]);
            let flows = f.ledger.activity().totals().unwrap();
            for (actual, expected) in [
                (&flows.incoming, &state["flows"]["incoming"]),
                (&flows.outgoing, &state["flows"]["outgoing"]),
            ] {
                for (k, n) in actual {
                    close(*n, expected[k].as_f64().unwrap_or(0.));
                }
                for (k, n) in expected.as_object().unwrap() {
                    close(actual.get(k).copied().unwrap_or(0.), n.as_f64().unwrap());
                }
            }

            close(
                f.worker.practice_value(f.worker.job()),
                state["practice"].as_f64().unwrap(),
            );
            close(
                f.processors.elapsed(&a(1)).map_or(0., |s| s.get()),
                state["elapsed"].as_f64().unwrap(),
            );
            for (vein, supply) in f.ore.veins().iter().zip(state["ore"].as_array().unwrap()) {
                close(vein.supply.get(), supply.as_f64().unwrap());
            }
        }
    }
}
#[test]
fn pending_blocked_and_partial_arrival_only_advance_productive_time() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 8.)]));
    f.start(1);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 0.);
    f.ready(1, RouteOutcome::Ready(vec![p(0., 2.)]));
    assert_eq!(
        f.processors
            .advance(
                &mut f.town,
                &mut f.ledger,
                &mut f.ore,
                &a(1),
                &mut f.worker,
                &f.workplace,
                sec(10.),
                NavigationRevision(1),
                |_, _, _| None
            )
            .unwrap(),
        Outcome::Waiting(Wait::Travel)
    );
    f.town.resume_travel(&a(1), NavigationRevision(2)).unwrap();
    let RouteState::Pending(r) = f.town.actor(&a(1)).unwrap().route().clone() else {
        panic!("pending");
    };
    f.town
        .accept_route(
            &a(1),
            &r,
            r.navigation(),
            RouteOutcome::Ready(vec![p(0., 2.)]),
        )
        .unwrap();
    let Outcome::Working { elapsed, .. } = f.advance(1, 3.) else {
        panic!("working");
    };
    close(elapsed.get(), 3. - 1. / 1.8);
    close(f.worker.practice_value("milling"), 0.);
    assert!(matches!(f.advance(1, 1. / 1.8), Outcome::Completed(_)));
}
#[test]
fn interrupted_inputs_caps_and_buffers_hold_partial_time_without_practice() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 4.)]));
    f.work(1.);
    let inputs = goods(&[("wheat", 4.)]);
    f.ledger.release(&b(1), &inputs).unwrap();
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Inputs));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 1.);
    f.supply(&goods(&[("wheat", 2.)]));
    f.ledger.configure_cap(key("flour"), Some(q(1.))).unwrap();
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Cap));
    close(f.worker.practice_value("milling"), 0.);
    f.ledger.configure_cap(key("flour"), None).unwrap();
    assert!(matches!(f.advance(1, 2.), Outcome::Completed(_)));
}
#[test]
fn multi_worker_completion_rechecks_shared_reservations_and_cap() {
    let mut f = Fixture::new("sawpit", goods(&[("wood", 2.)]));
    f.start(1);
    f.start(2);
    f.ready(1, RouteOutcome::Arrived);
    f.ready(2, RouteOutcome::Arrived);
    f.advance(1, 3.);
    f.advance(2, 3.);
    let mut second = f.worker.clone();
    assert!(matches!(f.advance(1, 3.), Outcome::Completed(_)));
    std::mem::swap(&mut f.worker, &mut second);
    assert_eq!(f.advance(2, 3.), Outcome::Waiting(Wait::Inputs));
    close(f.worker.practice_value("woodworking"), 0.);
    close(f.ledger.produced().quantity(&key("planks")).get(), 1.);
    f.supply(&goods(&[("wood", 2.)]));
    f.ledger.configure_cap(key("planks"), Some(q(1.))).unwrap();
    assert_eq!(f.advance(2, 3.), Outcome::Waiting(Wait::Cap));
    f.ledger.configure_cap(key("planks"), None).unwrap();
    assert!(matches!(f.advance(2, 3.), Outcome::Completed(_)));
    close(f.ledger.produced().quantity(&key("planks")).get(), 2.);
}
#[test]
fn completed_unloading_preserves_partial_batch_and_remaining_time() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 8.)]));
    f.work(3.);
    f.advance(1, 1.);
    f.processors.begin_unload(&mut f.town, &a(1)).unwrap();
    assert_eq!(f.advance(1, 0.4), Outcome::Unloading);
    close(f.processors.elapsed(&a(1)).unwrap().get(), 1.);
    let skill = f.worker.practice_value("milling");
    let Outcome::Unloaded(remaining) = f.advance(1, 3.) else {
        panic!("unloaded");
    };
    close(remaining.get(), 2.75);
    close(f.worker.practice_value("milling"), skill);
    close(f.ledger.free(&key("flour")).unwrap().get(), 2.);
    assert!(matches!(
        f.advance(1, remaining.get()),
        Outcome::Completed(_)
    ));
}
#[test]
fn supply_travel_pauses_and_resume_keeps_partial_work_and_position() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 8.)]));
    f.work(1.);
    f.town
        .set_task(
            &a(1),
            Task::Transport {
                source: b(2),
                destination: b(1),
                phase: TransportPhase::ToPickup,
            },
        )
        .unwrap();
    assert_eq!(f.advance(1, 100.), Outcome::Paused);
    close(f.processors.elapsed(&a(1)).unwrap().get(), 1.);
    f.town.set_task(&a(1), Task::Idle).unwrap();
    f.start(1);
    f.ready(1, RouteOutcome::Arrived);
    assert!(matches!(f.advance(1, 2.), Outcome::Completed(_)));
}
#[test]
fn reassignment_move_delete_upgrade_and_reload_preserve_finished_goods_once() {
    for event in 0..4 {
        let mut f = Fixture::new("mill", goods(&[("wheat", 8.)]));
        f.work(3.);
        f.advance(1, 1.);
        match event {
            0 => f
                .town
                .relocate_building(&b(1), Tile { x: 5, y: 5 })
                .unwrap(),
            1 => {
                f.town.remove_building(&b(1)).unwrap();
            }
            2 => f
                .town
                .set_task(
                    &a(1),
                    Task::Move {
                        destination: p(8., 8.),
                    },
                )
                .unwrap(),
            _ => {
                let mut policy = StoragePolicy::new("mill");
                policy.construction = true;
                f.ledger.update_storage(&b(1), policy).unwrap();
            }
        }
        f.processors =
            Processors::decode(&f.processors.encode(&f.town).unwrap(), &f.town, &f.ledger).unwrap();
        assert_eq!(f.advance(1, 3.), Outcome::Cancelled);
        f.processors
            .cancel(&mut f.town, &mut f.ledger, &a(1))
            .unwrap();
        close(
            f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Orphan]
                .quantity(&key("flour"))
                .get(),
            2.,
        );
        close(f.ledger.produced().quantity(&key("flour")).get(), 2.);
    }
}
#[test]
fn grain_queue_claims_only_once_after_admission_and_survives_reload() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 8.)]));
    f.processors
        .queue_grain_work(&f.town, &f.ledger, &b(1), sec(0.))
        .unwrap();
    f.processors
        .queue_grain_work(&f.town, &f.ledger, &b(1), sec(2.))
        .unwrap();
    f.work(0.);
    f.processors =
        Processors::decode(&f.processors.encode(&f.town).unwrap(), &f.town, &f.ledger).unwrap();
    assert!(matches!(f.advance(1, 1.), Outcome::Working { .. }));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 1.);
    assert!(matches!(f.advance(1, 2.), Outcome::Completed(_)));
    assert!(matches!(f.advance(1, 1.), Outcome::Completed(_)));
}
#[test]
fn recipe_change_releases_only_obsolete_inputs_resets_work_and_preserves_goods() {
    let mut f = Fixture::new(
        "bakery",
        goods(&[("flour", 8.), ("wood", 2.), ("herbs", 3.)]),
    );
    f.work(6.);
    f.advance(1, 2.);
    let produced = f.ledger.produced().clone();
    let practice = f.worker.practice_value("baking");
    f.processors
        .change_recipe(
            &f.town,
            &mut f.ledger,
            &b(1),
            &mut f.workplace,
            "bread_rolls",
            true,
            &[],
        )
        .unwrap();
    close(f.processors.elapsed(&a(1)).unwrap().get(), 0.);
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .inputs
            .quantity(&key("herbs"))
            .get(),
        0.,
    );
    close(
        f.ledger
            .store(&b(1))
            .unwrap()
            .output
            .quantity(&key("herbs"))
            .get(),
        3.,
    );
    assert_eq!(f.ledger.produced(), &produced);
    close(f.worker.practice_value("baking"), practice);
    assert!(matches!(
        f.processors.advance(
            &mut f.town,
            &mut f.ledger,
            &mut f.ore,
            &a(1),
            &mut f.worker,
            &f.workplace,
            sec(100.),
            NavigationRevision(1),
            |_, _, s| Some(s)
        ),
        Err(Error::Unsupported)
    ));
    // Finished base goods can still unload with an advanced recipe selected.
    assert_eq!(f.unload(1, 0.65), Outcome::Unloaded(sec(0.)));
    close(f.ledger.free(&key("bread")).unwrap().get(), 2.);
}
#[test]
fn invalid_selection_and_malformed_saved_work_fail_without_mutation() {
    let mut f = Fixture::new("bakery", goods(&[("flour", 8.), ("wood", 2.)]));
    f.work(2.);
    let before = (f.processors.clone(), f.ledger.clone(), f.workplace.clone());
    assert!(
        f.processors
            .change_recipe(
                &f.town,
                &mut f.ledger,
                &b(1),
                &mut f.workplace,
                "cakes",
                false,
                &[]
            )
            .is_err()
    );
    assert_eq!(
        (f.processors.clone(), f.ledger.clone(), f.workplace.clone()),
        before
    );
    let json = f.processors.encode(&f.town).unwrap();
    for field in ["elapsed", "output", "building"] {
        let mut bad: Value = serde_json::from_str(&json).unwrap();
        bad["jobs"][0][1][field] = match field {
            "elapsed" => (-1).into(),
            "output" => "tools".into(),
            _ => (999).into(),
        };
        assert!(Processors::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
    }
    let mut bad: Value = serde_json::from_str(&json).unwrap();
    let job = bad["jobs"][0].clone();
    bad["jobs"].as_array_mut().unwrap().push(job);
    assert!(Processors::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
}
#[test]
fn batch_and_practice_overflow_roll_back_every_owner() {
    let mut f = Fixture::new("mine", Stock::default());
    f.start(1);
    f.ready(1, RouteOutcome::Arrived);
    f.worker
        .set_practice("mining", Practice::new(f64::MAX).unwrap())
        .unwrap();
    f.worker
        .set_talents(BTreeMap::from([("woodcutting".into(), f64::MAX)]))
        .unwrap();
    let before = (
        f.processors.clone(),
        f.town.clone(),
        f.ledger.clone(),
        f.ore.clone(),
        f.worker.clone(),
    );
    assert!(
        f.processors
            .advance(
                &mut f.town,
                &mut f.ledger,
                &mut f.ore,
                &a(1),
                &mut f.worker,
                &f.workplace,
                sec(9.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!((f.processors, f.town, f.ledger, f.ore, f.worker), before);
}
#[test]
fn legal_time_subdivisions_preserve_base_rates_and_one_batch_limit() {
    for step in [1., 3.] {
        let mut f = Fixture::new("mill", goods(&[("wheat", 40.)]));
        f.start(1);
        f.ready(1, RouteOutcome::Arrived);
        for _ in 0..(30. / step) as usize {
            f.advance(1, step);
        }
        close(f.ledger.produced().quantity(&key("flour")).get(), 20.);
    }
    let mut f = Fixture::new("mill", goods(&[("wheat", 40.)]));
    f.work(f64::MAX);
    close(f.ledger.produced().quantity(&key("flour")).get(), 2.);
}

#[test]
fn changing_tools_rechecks_batch_costs_without_resetting_partial_work() {
    let mut f = Fixture::new("sawpit", goods(&[("wood", 2.)]));
    f.work(3.);
    f.worker.tool = Some("crafted_saw".into());
    assert_eq!(f.advance(1, 3.), Outcome::Waiting(Wait::Inputs));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 3.);
    f.supply(&goods(&[("wood", 0.5)]));
    let Outcome::Completed(batch) = f.advance(1, 3.) else {
        panic!("completed");
    };
    close(batch.costs.quantity(&key("wood")).get(), 2.5);
    close(batch.outputs.quantity(&key("planks")).get(), 1.25);
}
#[test]
fn ore_selection_has_stable_ties_and_debits_only_completed_batches() {
    let mut f = Fixture::new("mine", Stock::default());
    f.ore = IronDeposits::new(vec![
        IronVein {
            tile: Tile { x: 6, y: 0 },
            supply: q(2.),
        },
        IronVein {
            tile: Tile { x: 8, y: 0 },
            supply: q(2.),
        },
    ])
    .unwrap();
    f.work(3.);
    close(f.ore.veins()[0].supply.get(), 2.);
    assert!(matches!(f.advance(1, 6.), Outcome::Completed(_)));
    close(f.ore.veins()[0].supply.get(), 0.);
    close(f.ore.veins()[1].supply.get(), 2.);
    assert!(matches!(f.advance(1, 9.), Outcome::Completed(_)));
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Ore));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 0.);
    close(f.ore.veins()[1].supply.get(), 0.);
}

#[test]
fn stalled_mill_bakery_chain_resumes_after_unload_reservation_and_consumption() {
    let mut f = Fixture::new("mill", goods(&[("wheat", 4.)]));
    let mut stores = f.ledger.stores().to_vec();
    stores[1] = Store::new(b(2), StoragePolicy::new("bakery"));
    stores[1].inputs = goods(&[("wood", 1.)]);
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    let bakery = Workplace::new("bakery");
    let mut baker = Worker::new("baking").unwrap();
    baker.sandbox = true;
    f.work(3.);
    f.processors
        .start(
            &mut f.town,
            &mut f.ledger,
            &a(2),
            &b(2),
            &baker,
            &bakery,
            NavigationRevision(1),
        )
        .unwrap();
    f.ready(2, RouteOutcome::Arrived);
    let bake = |f: &mut Fixture, w: &mut Worker, n: f64| {
        f.processors
            .advance(
                &mut f.town,
                &mut f.ledger,
                &mut f.ore,
                &a(2),
                w,
                &bakery,
                sec(n),
                NavigationRevision(1),
                |_, _, s| Some(s),
            )
            .unwrap()
    };
    assert_eq!(
        bake(&mut f, &mut baker, 100.),
        Outcome::Waiting(Wait::Inputs)
    );
    close(baker.practice_value("baking"), 0.);
    f.unload(1, 0.65);
    f.ledger
        .reserve(&b(1), &b(2), &goods(&[("flour", 2.)]))
        .unwrap();
    f.ledger.configure_cap(key("bread"), Some(q(2.))).unwrap();
    assert!(matches!(
        bake(&mut f, &mut baker, 6.),
        Outcome::Completed(_)
    ));
    f.advance(1, 3.);
    f.unload(1, 0.65);
    f.ledger
        .reserve(&b(1), &b(2), &goods(&[("flour", 2.)]))
        .unwrap();
    assert_eq!(bake(&mut f, &mut baker, 100.), Outcome::Waiting(Wait::Cap));
    let practice = baker.practice_value("baking");
    f.processors.begin_unload(&mut f.town, &a(2)).unwrap();
    assert_eq!(bake(&mut f, &mut baker, 0.65), Outcome::Unloaded(sec(0.)));
    f.ledger.spend(&goods(&[("bread", 2.)])).unwrap();
    assert!(matches!(
        bake(&mut f, &mut baker, 6.),
        Outcome::Completed(_)
    ));
    assert!(baker.practice_value("baking") > practice);
    close(f.ledger.produced().quantity(&key("flour")).get(), 4.);
    close(f.ledger.produced().quantity(&key("bread")).get(), 4.);
    close(
        f.ledger
            .store(&b(2))
            .unwrap()
            .inputs
            .quantity(&key("wood"))
            .get(),
        0.,
    );
}

#[test]
fn actual_r14_input_trip_preserves_partial_work_and_ledger_reservations() {
    use lantern_host::logistics::{InputTrips, Outcome as TripOutcome, Producer};
    let mut f = Fixture::new("mill", goods(&[("wheat", 2.)]));
    f.work(1.);
    f.ledger.release(&b(1), &goods(&[("wheat", 2.)])).unwrap();
    let mut buildings = f.town.buildings().to_vec();
    buildings.push(Building {
        id: b(3),
        location: Tile { x: 20, y: 20 },
        construction: ConstructionState::Complete,
    });
    f.town = Town::restore(
        owner(),
        buildings,
        f.town.actors().iter().map(|a| a.state().clone()).collect(),
    )
    .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores.push(Store::new(b(3), StoragePolicy::new("hall")));
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    let producer = Producer::recipe(f.workplace.clone());
    let mut trips = InputTrips::default();
    trips
        .start(
            &mut f.town,
            &f.ledger,
            &a(1),
            &b(2),
            &b(1),
            NavigationRevision(1),
        )
        .unwrap();
    assert_eq!(f.advance(1, 100.), Outcome::Paused);
    f.ready(1, RouteOutcome::Arrived);
    assert!(matches!(
        trips
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                &producer,
                &b(3),
                sec(3.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .unwrap(),
        TripOutcome::PickedUp(_)
    ));
    assert_eq!(f.advance(1, 100.), Outcome::Paused);
    let reserved = f.ledger.overview().unwrap()["wheat"].reserved;
    assert!(reserved > 0.);
    f.ready(1, RouteOutcome::Arrived);
    assert!(matches!(
        trips
            .advance(
                &mut f.town,
                &mut f.ledger,
                &a(1),
                &mut f.worker,
                &producer,
                &b(3),
                sec(3.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .unwrap(),
        TripOutcome::Completed(_)
    ));
    close(f.ledger.overview().unwrap()["wheat"].reserved, reserved);
    f.start(1);
    f.ready(1, RouteOutcome::Arrived);
    close(f.processors.elapsed(&a(1)).unwrap().get(), 1.);
    assert!(matches!(f.advance(1, 2.), Outcome::Completed(_)));
}
