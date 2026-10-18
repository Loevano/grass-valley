use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{
    logistics::{LogisticsTrips, Outcome as TripOutcome, Producer},
    processing::*,
    progression::{Request, Worker, Workplace},
    resources::*,
};
use serde_json::Value;
use std::collections::BTreeMap;
fn owner() -> Owner {
    Owner {
        world: WorldId::new("workshop").unwrap(),
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
fn goods(v: &[(&str, f64)]) -> Stock {
    Stock::from_entries(v.iter().map(|(k, n)| (key(k), q(*n)))).unwrap()
}
#[track_caller]
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
#[track_caller]
fn same_stock(actual: &Stock, expected: &Value) {
    for (k, n) in actual.entries() {
        close(n.get(), expected[k.as_str()].as_f64().unwrap_or(0.));
    }
    for (k, n) in expected.as_object().unwrap() {
        close(actual.quantity(&key(k)).get(), n.as_f64().unwrap());
    }
}
#[derive(Clone, Debug, PartialEq)]
struct Fixture {
    town: Town,
    ledger: Ledger,
    processors: Processors,
    trips: LogisticsTrips,
    ore: IronDeposits,
    worker: Worker,
    workplace: Workplace,
}
impl Fixture {
    fn new(kind: &str) -> Self {
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
                Building {
                    id: b(3),
                    location: Tile { x: 20, y: 20 },
                    construction: ConstructionState::Complete,
                },
            ],
            (1..=3)
                .map(|i| ActorState::new(a(i), p(0., 1.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        Self {
            town,
            ledger: Ledger::new(
                owner(),
                q(0.),
                vec![
                    Store::new(b(1), StoragePolicy::new(kind)),
                    Store::new(b(2), StoragePolicy::new("storehouse")),
                    Store::new(b(3), StoragePolicy::new("hall")),
                ],
                vec![],
            )
            .unwrap(),
            processors: Processors::default(),
            trips: LogisticsTrips::default(),
            ore: IronDeposits::default(),
            worker,
            workplace,
        }
    }
    fn admit(&mut self, inputs: Stock, source: Stock) {
        let mut stores = self.ledger.stores().to_vec();
        stores[0].inputs = inputs;
        stores[1].stored = source;
        self.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    }
    fn queue(&mut self, recipe: &str, n: u16) -> u64 {
        self.processors
            .queue_workshop(
                &mut self.town,
                &mut self.ledger,
                &mut self.trips,
                &b(3),
                &b(1),
                recipe,
                n,
                true,
                &[],
            )
            .unwrap()
    }
    fn cancel(&mut self, id: u64) {
        self.processors
            .cancel_workshop(
                &mut self.town,
                &mut self.ledger,
                &mut self.trips,
                &b(3),
                &b(1),
                id,
            )
            .unwrap();
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
            panic!("pending");
        };
        self.town
            .accept_route(&a(i), &r, r.navigation(), outcome)
            .unwrap();
    }
    fn advance(&mut self, i: u64, n: f64) -> Outcome {
        self.processors
            .advance_with_trips(
                &mut self.town,
                &mut self.ledger,
                &mut self.ore,
                &mut self.trips,
                &b(3),
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
    fn unload(&mut self, n: f64) -> Outcome {
        self.processors.begin_unload(&mut self.town, &a(1)).unwrap();
        self.advance(1, n)
    }
    fn reload(&mut self) {
        self.processors = Processors::decode(
            &self.processors.encode(&self.town).unwrap(),
            &self.town,
            &self.ledger,
        )
        .unwrap();
        self.trips = LogisticsTrips::decode(
            &self.trips.encode(&self.town, &self.ledger).unwrap(),
            &self.town,
            &self.ledger,
        )
        .unwrap();
    }
    fn pickup(&mut self, i: u64) {
        let effective = self.processors.workplace(&b(1), &self.workplace).unwrap();
        let producer = Producer::recipe(effective);
        self.trips
            .start(
                &mut self.town,
                &self.ledger,
                &a(i),
                &b(2),
                &b(1),
                NavigationRevision(1),
            )
            .unwrap();
        self.ready(i, RouteOutcome::Arrived);
        assert!(matches!(
            self.trips
                .advance(
                    &mut self.town,
                    &mut self.ledger,
                    &a(i),
                    &mut self.worker,
                    &producer,
                    &b(3),
                    sec(3.),
                    NavigationRevision(1),
                    |_, _, s| Some(s)
                )
                .unwrap(),
            TripOutcome::PickedUp(_)
        ));
    }
}
#[test]
fn every_advanced_and_workshop_recipe_matches_real_python_rates_and_orders() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/workshop/python.json")).unwrap();
    let actual: std::collections::BTreeSet<_> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|c| {
            (
                c["kind"].as_str().unwrap().to_owned(),
                c["recipe"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let catalog = lantern_host::resources::public_catalog();
    let expected: std::collections::BTreeSet<_> = catalog["profession_recipes"]
        .as_object()
        .unwrap()
        .iter()
        .flat_map(|(kind, recipes)| {
            recipes
                .as_array()
                .unwrap()
                .iter()
                .filter(move |r| kind == "workshop" || r["level"].as_f64().unwrap() > 1.)
                .map(move |r| (kind.clone(), r["output"].as_str().unwrap().to_owned()))
        })
        .collect();
    assert_eq!(
        actual, expected,
        "every advanced/workshop catalog recipe needs a paired actor trace"
    );
    for c in cases.as_array().unwrap() {
        let mut f = Fixture::new(c["kind"].as_str().unwrap());
        f.workplace.recipe = Some(c["recipe"].as_str().unwrap().into());
        let job = f.worker.job().to_owned();
        f.worker
            .set_practice(
                &job,
                Practice::new(c["skill"].as_f64().unwrap_or(0.)).unwrap(),
            )
            .unwrap();
        let root = lantern_host::resources::public_catalog()["talent_roots"][&job]
            .as_str()
            .unwrap_or(&job)
            .to_owned();
        f.worker
            .set_talents(BTreeMap::from([(root, c["talent"].as_f64().unwrap_or(0.))]))
            .unwrap();
        for order in c["orders"].as_array().into_iter().flatten() {
            f.queue(
                order["recipe"].as_str().unwrap(),
                order["amount"].as_u64().unwrap() as u16,
            );
        }
        f.admit(stock(&c["inputs"]), Stock::default());
        if let Some(cap) = c["cap"].as_f64() {
            f.ledger
                .configure_cap(key(c["recipe"].as_str().unwrap()), Some(q(cap)))
                .unwrap();
        }
        f.start(1);
        f.ready(1, RouteOutcome::Arrived);
        for (action, state) in c["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(c["states"].as_array().unwrap())
        {
            let result = match action["op"].as_str().unwrap() {
                "advance" => Some(f.advance(1, action["seconds"].as_f64().unwrap())),
                "cancel" => {
                    f.cancel(action["id"].as_u64().unwrap());
                    None
                }
                "uncap" => {
                    f.ledger
                        .configure_cap(key(c["recipe"].as_str().unwrap()), None)
                        .unwrap();
                    None
                }
                "dropoff" => {
                    assert_eq!(f.unload(0.65), Outcome::Unloaded(sec(0.)));
                    None
                }
                _ => panic!("operation"),
            };
            if let Some(status) = state["status"].as_str() {
                let ok = if status == "Working" {
                    matches!(result, Some(Outcome::Completed(_)))
                } else if status.starts_with("Producing") {
                    matches!(result, Some(Outcome::Working { .. }))
                } else {
                    matches!(result, Some(Outcome::Waiting(_)))
                };
                assert!(ok, "{} {action:?}: {status}, {result:?}", c["name"]);
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
            same_stock(f.ledger.produced(), &state["produced"]);
            close(
                f.processors.elapsed(&a(1)).unwrap().get(),
                state["elapsed"].as_f64().unwrap(),
            );
            close(
                f.worker.practice_value(&job),
                state["practice"].as_f64().unwrap(),
            );
            let queue = f.processors.workshop_queue(&b(1));
            let expected = state["queue"].as_array().unwrap();
            assert_eq!(queue.len(), expected.len());
            for (o, e) in queue.iter().zip(expected) {
                assert_eq!(o.id, e["id"].as_u64().unwrap());
                assert_eq!(o.recipe, e["recipe"].as_str().unwrap());
                assert_eq!(u64::from(o.amount), e["amount"].as_u64().unwrap());
                close(o.remaining, e["remaining"].as_f64().unwrap());
            }
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
            f.reload();
        }
    }
}
#[test]
fn empty_queue_cannot_be_bypassed_and_finished_last_order_can_unload() {
    let mut f = Fixture::new("workshop");
    f.workplace.recipe = Some("tools".into());
    f.workplace.request = Some(Request {
        recipe: "tools".into(),
        remaining: q(99.),
    });
    f.admit(goods(&[("wood", 1.), ("planks", 2.)]), Stock::default());
    assert_eq!(f.work(100.), Outcome::Waiting(Wait::Requests));
    close(f.worker.practice_value("crafting"), 0.);
    f.queue("tools", 1);
    assert!(matches!(f.advance(1, 9.), Outcome::Completed(_)));
    assert!(f.processors.workshop_queue(&b(1)).is_empty());
    let before = (f.ledger.clone(), f.worker.clone());
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Requests));
    assert_eq!((f.ledger.clone(), f.worker.clone()), before);
    let Outcome::Unloaded(rem) = f.unload(3.) else {
        panic!("unload");
    };
    close(rem.get(), 2.35);
    close(f.ledger.free(&key("tools")).unwrap().get(), 1.);
}
#[test]
fn head_cancellation_restores_transit_supplies_once_and_tail_keeps_work() {
    let mut f = Fixture::new("workshop");
    let head = f.queue("lanterns", 5);
    let tail = f.queue("tools", 2);
    f.admit(
        goods(&[("planks", 2.), ("wood", 1.)]),
        goods(&[("planks", 20.), ("wood", 10.)]),
    );
    f.work(3.);
    f.ledger
        .release(&b(1), &goods(&[("planks", 2.), ("wood", 1.)]))
        .unwrap();
    f.pickup(2);
    let held = f.ledger.held()[&a(2)].goods.clone();
    let source = f.ledger.store(&b(2)).unwrap().stored.clone();
    f.cancel(tail);
    close(f.processors.elapsed(&a(1)).unwrap().get(), 3.);
    assert!(f.trips.trip_id(&a(2)).is_some());
    f.reload();
    f.town
        .set_task(
            &a(2),
            Task::Move {
                destination: p(8., 8.),
            },
        )
        .unwrap();
    f.cancel(head);
    assert!(matches!(
        f.town.actor(&a(2)).unwrap().state().task,
        Task::Move { .. }
    ));
    assert!(f.trips.trip_id(&a(2)).is_none());
    assert!(!f.ledger.held().contains_key(&a(2)));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 0.);
    for (k, n) in held.entries() {
        close(
            f.ledger.store(&b(2)).unwrap().stored.quantity(k).get(),
            source.quantity(k).get() + n.get(),
        );
    }
    let before = f.clone();
    assert!(
        f.processors
            .cancel_workshop(&mut f.town, &mut f.ledger, &mut f.trips, &b(3), &b(1), head)
            .is_err()
    );
    assert_eq!(f, before);
}
#[test]
fn completion_cancels_other_workers_supplies_and_preserves_finished_goods() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 1);
    f.queue("furniture", 1);
    f.admit(Stock::default(), goods(&[("planks", 20.), ("wood", 10.)]));
    f.start(1);
    f.ready(1, RouteOutcome::Arrived);
    f.pickup(2);
    assert!(f.ledger.held().contains_key(&a(2)));
    f.ledger
        .reserve(&b(2), &b(1), &goods(&[("planks", 2.), ("wood", 1.)]))
        .unwrap();
    assert!(matches!(f.advance(1, 9.), Outcome::Completed(_)));
    assert!(f.trips.trip_id(&a(2)).is_none());
    assert_eq!(f.processors.workshop_queue(&b(1))[0].recipe, "furniture");
    close(
        f.ledger.store(&b(1)).unwrap().pending[&PendingOwner::Worker(a(1))]
            .quantity(&key("tools"))
            .get(),
        1.,
    );
}
#[test]
fn competing_workers_clip_final_batch_and_reset_shared_partial_work() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 1);
    f.admit(goods(&[("planks", 4.), ("wood", 2.)]), Stock::default());
    f.start(1);
    f.start(2);
    f.ready(1, RouteOutcome::Arrived);
    f.ready(2, RouteOutcome::Arrived);
    f.advance(1, 3.);
    f.advance(2, 3.);
    assert!(matches!(f.advance(1, 6.), Outcome::Completed(_)));
    close(f.processors.elapsed(&a(2)).unwrap().get(), 0.);
    assert_eq!(f.advance(2, 100.), Outcome::Waiting(Wait::Requests));
    close(f.ledger.produced().quantity(&key("tools")).get(), 1.);
}
#[test]
fn invalid_queue_commands_limits_and_ids_are_atomic_and_survive_reload() {
    let mut f = Fixture::new("workshop");
    for (recipe, amount) in [("tools", 0), ("tools", 1000), ("bread", 1)] {
        let before = f.clone();
        assert!(
            f.processors
                .queue_workshop(
                    &mut f.town,
                    &mut f.ledger,
                    &mut f.trips,
                    &b(3),
                    &b(1),
                    recipe,
                    amount,
                    true,
                    &[]
                )
                .is_err()
        );
        assert_eq!(f, before);
    }
    assert!(
        f.processors
            .queue_workshop(
                &mut f.town,
                &mut f.ledger,
                &mut f.trips,
                &b(3),
                &b(1),
                "lanterns",
                1,
                false,
                &[]
            )
            .is_err()
    );
    let first = f.queue("tools", 1);
    for _ in 1..50 {
        f.queue("tools", 1);
    }
    let before = f.clone();
    assert_eq!(
        f.processors.queue_workshop(
            &mut f.town,
            &mut f.ledger,
            &mut f.trips,
            &b(3),
            &b(1),
            "tools",
            1,
            true,
            &[]
        ),
        Err(Error::QueueFull)
    );
    assert_eq!(f, before);
    f.reload();
    f.cancel(first);
    assert_eq!(f.queue("tools", 1), 51);
}
#[test]
fn failed_hall_restitution_rolls_back_order_cargo_and_progress() {
    let mut f = Fixture::new("workshop");
    let head = f.queue("tools", 1);
    f.admit(Stock::default(), goods(&[("planks", 2.), ("wood", 1.)]));
    f.pickup(2);
    f.town.remove_building(&b(2)).unwrap();
    f.town.remove_building(&b(3)).unwrap();
    let before = f.clone();
    assert!(
        f.processors
            .cancel_workshop(&mut f.town, &mut f.ledger, &mut f.trips, &b(3), &b(1), head)
            .is_err()
    );
    assert_eq!(f, before);
}
#[test]
fn v1_processor_admission_and_v2_queue_validation_reject_corruption() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 2);
    f.admit(goods(&[("planks", 4.), ("wood", 2.)]), Stock::default());
    f.work(3.);
    let saved: Value = serde_json::from_str(&f.processors.encode(&f.town).unwrap()).unwrap();
    for field in ["id", "remaining", "amount", "recipe"] {
        let mut bad = saved.clone();
        bad["workshops"]["queues"][0][1][0][field] = match field {
            "id" => 0.into(),
            "remaining" => (-1).into(),
            "amount" => 1000.into(),
            _ => "bread".into(),
        };
        assert!(Processors::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
    }
    let mut bad = saved.clone();
    bad["version"] = 1.into();
    assert!(Processors::decode(&bad.to_string(), &f.town, &f.ledger).is_err());
    let mut base = Fixture::new("mill");
    base.admit(goods(&[("wheat", 2.)]), Stock::default());
    base.work(1.);
    let mut legacy: Value =
        serde_json::from_str(&base.processors.encode(&base.town).unwrap()).unwrap();
    legacy["version"] = 1.into();
    legacy.as_object_mut().unwrap().remove("workshops");
    base.processors = Processors::decode(&legacy.to_string(), &base.town, &base.ledger).unwrap();
    assert!(matches!(base.advance(1, 2.), Outcome::Completed(_)));
}

#[test]
fn queued_workers_wait_for_routes_without_banking_time_or_practice() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 1);
    f.admit(goods(&[("planks", 2.), ("wood", 1.)]), Stock::default());
    f.start(1);
    let before = (f.ledger.clone(), f.worker.clone());
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    f.ready(1, RouteOutcome::Unreachable);
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Travel));
    assert_eq!((f.ledger.clone(), f.worker.clone()), before);
    close(f.processors.elapsed(&a(1)).unwrap().get(), 0.);
    f.town.resume_travel(&a(1), NavigationRevision(2)).unwrap();
    let RouteState::Pending(r) = f.town.actor(&a(1)).unwrap().route().clone() else {
        panic!("pending");
    };
    f.town
        .accept_route(&a(1), &r, r.navigation(), RouteOutcome::Arrived)
        .unwrap();
    assert!(matches!(f.advance(1, 3.), Outcome::Working { .. }));
    close(f.processors.elapsed(&a(1)).unwrap().get(), 3.);
    close(f.worker.practice_value("crafting"), 0.);
}
#[test]
fn worker_qualification_is_rechecked_during_partial_advanced_work() {
    let mut f = Fixture::new("workshop");
    f.worker.sandbox = false;
    f.worker
        .set_practice("woodworking", Practice::new(2.).unwrap())
        .unwrap();
    f.worker
        .set_practice("crafting", Practice::new(2.).unwrap())
        .unwrap();
    f.processors
        .queue_workshop(
            &mut f.town,
            &mut f.ledger,
            &mut f.trips,
            &b(3),
            &b(1),
            "lanterns",
            1,
            false,
            &[f.worker.clone()],
        )
        .unwrap();
    f.admit(goods(&[("planks", 2.), ("wood", 1.)]), Stock::default());
    f.work(3.);
    f.worker
        .set_practice("crafting", Practice::new(1.).unwrap())
        .unwrap();
    let before = (f.ledger.clone(), f.worker.clone(), f.processors.clone());
    assert_eq!(f.advance(1, 100.), Outcome::Waiting(Wait::Qualification));
    assert_eq!(
        (f.ledger.clone(), f.worker.clone(), f.processors.clone()),
        before
    );
    f.worker
        .set_practice("crafting", Practice::new(2.).unwrap())
        .unwrap();
    assert!(matches!(f.advance(1, 6.), Outcome::Completed(_)));
}
#[test]
fn completion_restitution_failure_rolls_back_finished_batch_and_practice() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 1);
    f.admit(Stock::default(), goods(&[("planks", 6.), ("wood", 3.)]));
    f.start(1);
    f.ready(1, RouteOutcome::Arrived);
    f.pickup(2);
    f.ledger
        .reserve(&b(2), &b(1), &goods(&[("planks", 2.), ("wood", 1.)]))
        .unwrap();
    f.advance(1, 3.);
    f.town.remove_building(&b(2)).unwrap();
    f.town.remove_building(&b(3)).unwrap();
    let before = f.clone();
    assert!(
        f.processors
            .advance_with_trips(
                &mut f.town,
                &mut f.ledger,
                &mut f.ore,
                &mut f.trips,
                &b(3),
                &a(1),
                &mut f.worker,
                &f.workplace,
                sec(6.),
                NavigationRevision(1),
                |_, _, s| Some(s)
            )
            .is_err()
    );
    assert_eq!(f, before);
}
#[test]
fn request_id_exhaustion_and_duplicate_saved_ids_cannot_reset_order_history() {
    let mut f = Fixture::new("workshop");
    f.queue("tools", 999);
    let saved: Value = serde_json::from_str(&f.processors.encode(&f.town).unwrap()).unwrap();
    let mut duplicate = saved.clone();
    let order = duplicate["workshops"]["queues"][0][1][0].clone();
    duplicate["workshops"]["queues"][0][1]
        .as_array_mut()
        .unwrap()
        .push(order);
    assert!(Processors::decode(&duplicate.to_string(), &f.town, &f.ledger).is_err());
    let mut exhausted = saved;
    exhausted["workshops"]["next"] = u64::MAX.into();
    f.processors = Processors::decode(&exhausted.to_string(), &f.town, &f.ledger).unwrap();
    let before = f.clone();
    assert_eq!(
        f.processors.queue_workshop(
            &mut f.town,
            &mut f.ledger,
            &mut f.trips,
            &b(3),
            &b(1),
            "tools",
            1,
            true,
            &[]
        ),
        Err(Error::Exhausted)
    );
    assert_eq!(f, before);
}
