use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{
    logistics::*,
    progression::{Worker, Workplace},
    resources::*,
};
use std::collections::BTreeMap;
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn sec(n: f64) -> Seconds {
    Seconds::new(n).unwrap()
}
fn p(x: f64) -> Position {
    Position::new(x, 0.).unwrap()
}
fn k(key: &str) -> ResourceKey {
    ResourceKey::parse(key).unwrap()
}
fn stock(items: &[(&str, f64)]) -> Stock {
    Stock::from_entries(items.iter().map(|(key, n)| (k(key), q(*n)))).unwrap()
}
fn owner() -> Owner {
    Owner {
        world: WorldId::new("haulers").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + n).unwrap())
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    trips: LogisticsTrips,
    hauler: Worker,
    producer_worker: Worker,
    producer: Producer,
    initial: BTreeMap<String, ResourceRow>,
    activity: Activity,
}
impl Fixture {
    fn new(supply: bool) -> Self {
        let town = Town::restore(
            owner(),
            [(1, 0), (2, 2), (3, 8), (4, 12)]
                .into_iter()
                .map(|(id, x)| Building {
                    id: b(id),
                    location: Tile { x, y: 0 },
                    construction: ConstructionState::Complete,
                })
                .collect(),
            vec![
                ActorState::new(a(1), p(2.), Cargo::default()),
                ActorState::new(a(2), p(2.), Cargo::default()),
            ],
        )
        .unwrap();
        let mut stores = [(1, "hall"), (2, "storehouse"), (3, "sawpit"), (4, "sawpit")]
            .map(|(id, kind)| Store::new(b(id), StoragePolicy::new(kind)))
            .to_vec();
        stores[1].policy.enabled = Some(BTreeMap::from([
            ("wood".into(), true),
            ("planks".into(), true),
        ]));
        if supply {
            stores[1].stored = stock(&[("wood", 100.)]);
        } else {
            stores[2].output = stock(&[("wood", 20.)]);
        }
        let ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        let initial = ledger.overview().unwrap();
        let activity = ledger.activity().clone();
        let mut producer_worker = Worker::new("woodworking").unwrap();
        producer_worker.sandbox = true;
        Self {
            town,
            ledger,
            trips: LogisticsTrips::default(),
            hauler: Worker::new("hauling").unwrap(),
            producer_worker,
            producer: Producer::recipe(Workplace::new("sawpit")),
            initial,
            activity,
        }
    }
    fn admit(&mut self, stores: Vec<Store>) {
        self.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        self.initial = self.ledger.overview().unwrap();
        self.activity = self.ledger.activity().clone();
    }
    fn start(&mut self, actor: u64, operation: HaulOperation) {
        self.trips
            .start_haul(
                &mut self.town,
                &self.ledger,
                &a(actor),
                &b(2),
                &HaulCandidate {
                    workplace: b(3),
                    operation,
                },
                NavigationRevision(1),
            )
            .unwrap();
        self.conserved();
    }
    fn route(&mut self, actor: u64, points: Option<Vec<Position>>) {
        self.town
            .resume_travel(&a(actor), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(request) = self.town.actor(&a(actor)).unwrap().route().clone()
        else {
            panic!("pending route")
        };
        self.town
            .accept_route(
                &a(actor),
                &request,
                request.navigation(),
                points
                    .map(RouteOutcome::Ready)
                    .unwrap_or(RouteOutcome::Arrived),
            )
            .unwrap();
    }
    fn tick(&mut self, actor: u64, elapsed: f64, blocked: bool) -> Outcome {
        let assignments = [ProducerAssignment {
            building: &b(3),
            worker: &self.producer_worker,
            producer: &self.producer,
        }];
        let result = self
            .trips
            .advance_hauler(
                &mut self.town,
                &mut self.ledger,
                &a(actor),
                &mut self.hauler,
                &assignments,
                &b(1),
                sec(elapsed),
                NavigationRevision(1),
                |_, _, speed| if blocked { None } else { Some(speed) },
            )
            .unwrap();
        self.conserved();
        result
    }
    fn conserved(&self) {
        let rows = self.ledger.overview().unwrap();
        for (key, before) in &self.initial {
            assert!(
                (rows[key].total - before.total).abs() < 1e-9,
                "conservation {key}: {} != {}",
                rows[key].total,
                before.total
            );
        }
        assert_eq!(self.ledger.activity(), &self.activity);
        assert!(self.ledger.produced().entries().is_empty());
    }
    fn reload(&mut self) {
        let json = self.trips.encode(&self.town, &self.ledger).unwrap();
        self.town = Town::restore(
            owner(),
            self.town.buildings().to_vec(),
            self.town
                .actors()
                .iter()
                .map(|a| a.state().clone())
                .collect(),
        )
        .unwrap();
        // Re-admit physical owner facts independently; trip JSON validates its actual
        // cargo against these facts, rather than crediting any goods on reload.
        let caps = self.ledger.caps().clone();
        self.ledger = Ledger::new(
            owner(),
            q(0.),
            self.ledger.stores().to_vec(),
            self.ledger.held().values().cloned().collect(),
        )
        .unwrap();
        for (key, cap) in caps {
            self.ledger.configure_cap(key, Some(cap)).unwrap();
        }
        self.trips = LogisticsTrips::decode(&json, &self.town, &self.ledger).unwrap();
        self.conserved();
    }
    fn pickup(&mut self, operation: HaulOperation) {
        self.start(1, operation);
        self.route(
            1,
            Some(vec![p(if operation == HaulOperation::Collect {
                8.
            } else {
                2.
            })]),
        );
        assert!(matches!(self.tick(1, 10., false), Outcome::PickedUp(_)));
        self.reload();
    }
}
#[test]
fn collection_pickup_delivery_and_second_worker_never_duplicate_goods() {
    let mut f = Fixture::new(false);
    f.start(1, HaulOperation::Collect);
    f.route(1, Some(vec![p(8.)]));
    f.tick(1, 1., false);
    assert!(f.ledger.held().is_empty());
    assert!((f.town.actor(&a(1)).unwrap().state().position.x.get() - 3.8).abs() < 1e-9);
    f.reload();
    f.route(1, Some(vec![p(8.)]));
    assert!(matches!(f.tick(1, 10., false), Outcome::PickedUp(_)));
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 20.)]));
    assert_eq!(f.ledger.free(&k("wood")).unwrap().get(), 0.);
    f.start(2, HaulOperation::Collect);
    f.route(2, None);
    f.tick(2, 0., false);
    assert!(!f.ledger.held().contains_key(&a(2)));
    f.reload();
    f.route(1, None);
    assert!(matches!(f.tick(1, 0., false), Outcome::Completed(_)));
    assert_eq!(
        f.ledger
            .store(&b(2))
            .unwrap()
            .stored
            .quantity(&k("wood"))
            .get(),
        20.
    );
    let practice = f.hauler.practice_value("hauling");
    assert_eq!(practice, 0.08);
    assert_eq!(f.tick(1, 99., false), Outcome::Idle);
    assert_eq!(f.hauler.practice_value("hauling"), practice);
}
#[test]
fn supply_uses_producer_rules_and_hauler_capacity_then_returns_empty_after_delivery() {
    let mut f = Fixture::new(true);
    f.hauler
        .set_talents(BTreeMap::from([("hauling".into(), -1.)]))
        .unwrap();
    f.town.reposition_actor(&a(1), p(0.)).unwrap();
    f.start(1, HaulOperation::Supply);
    f.route(1, Some(vec![p(2.)]));
    f.tick(1, 0.5, false);
    assert!(f.ledger.held().is_empty());
    f.reload();
    f.route(1, Some(vec![p(2.)]));
    assert!(matches!(f.tick(1, 1., false), Outcome::PickedUp(_)));
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 37.5)]));
    assert_eq!(f.ledger.overview().unwrap()["wood"].reserved, 37.5);
    f.reload();
    f.route(1, Some(vec![p(8.)]));
    assert!(matches!(f.tick(1, 10., false), Outcome::Delivered(_)));
    assert!(!f.ledger.held().contains_key(&a(1)));
    assert!(f.trips.trip_id(&a(1)).is_some());
    assert_eq!(f.hauler.practice_value("hauling"), 0.06);
    assert_eq!(f.producer_worker.practice_value("woodworking"), 0.);
    f.reload();
    f.route(1, Some(vec![p(2.)]));
    assert_eq!(f.tick(1, 100., true), Outcome::Travelling);
    f.reload();
    f.route(1, Some(vec![p(2.)]));
    assert!(matches!(f.tick(1, 10., false), Outcome::Completed(_)));
    assert_eq!(
        f.ledger.store(&b(3)).unwrap().inputs,
        stock(&[("wood", 37.5)])
    );
    assert_eq!(f.hauler.practice_value("hauling"), 0.06);
}
#[test]
fn full_disabled_and_blocked_storage_keep_cargo_and_train_once_on_partial_delivery() {
    let mut f = Fixture::new(false);
    let mut stores = f.ledger.stores().to_vec();
    stores[0].stored = stock(&[("wood", 45.)]);
    f.admit(stores);
    f.pickup(HaulOperation::Collect);
    f.route(1, Some(vec![p(2.)]));
    assert_eq!(f.tick(1, 100., true), Outcome::Travelling);
    assert_eq!(f.hauler.practice_value("hauling"), 0.);
    f.reload();
    f.ledger.configure_storage(&b(2), BTreeMap::new()).unwrap();
    f.route(1, None);
    assert_eq!(f.tick(1, 100., false), Outcome::WaitingStorage);
    assert_eq!(f.hauler.practice_value("hauling"), 0.);
    f.reload();
    f.ledger
        .configure_storage(&b(2), BTreeMap::from([("wood".into(), true)]))
        .unwrap();
    f.ledger
        .pickup(&a(2), &b(1), CargoKind::Collect, &stock(&[("wood", 45.)]))
        .unwrap();
    f.ledger.deposit(&a(2), &b(2)).unwrap();
    f.conserved();
    assert_eq!(f.tick(1, 100., false), Outcome::WaitingStorage);
    assert_eq!(f.ledger.held()[&a(1)].goods.quantity(&k("wood")).get(), 15.);
    let trained = f.hauler.practice_value("hauling");
    assert_eq!(trained, 0.08);
    f.reload();
    assert_eq!(f.tick(1, 100., false), Outcome::WaitingStorage);
    assert_eq!(f.hauler.practice_value("hauling"), trained);
    f.ledger
        .pickup(&a(2), &b(2), CargoKind::Collect, &stock(&[("wood", 20.)]))
        .unwrap();
    f.ledger.restore_cargo(&a(2), &b(1)).unwrap();
    assert!(matches!(f.tick(1, 0., false), Outcome::Completed(_)));
    assert_eq!(f.hauler.practice_value("hauling"), trained);
}
#[test]
fn moved_deleted_and_upgrading_endpoints_cancel_before_and_after_pickup_once() {
    for operation in [HaulOperation::Collect, HaulOperation::Supply] {
        for endpoint in [2, 3] {
            for delete in [false, true] {
                for picked in [false, true] {
                    let mut f = Fixture::new(operation == HaulOperation::Supply);
                    if picked {
                        f.pickup(operation);
                    } else {
                        f.start(1, operation);
                    }
                    if delete {
                        f.town.remove_building(&b(endpoint)).unwrap();
                    } else {
                        f.town
                            .relocate_building(&b(endpoint), Tile { x: 99, y: 1 })
                            .unwrap();
                    }
                    f.reload();
                    assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
                    assert!(f.ledger.held().is_empty());
                    assert_eq!(f.tick(1, 999., false), Outcome::Idle);
                    let source = if operation == HaulOperation::Collect {
                        3
                    } else {
                        2
                    };
                    let cargo = if operation == HaulOperation::Collect {
                        20.
                    } else {
                        50.
                    };
                    let expected = if picked && endpoint == source {
                        cargo
                    } else {
                        0.
                    };
                    assert_eq!(
                        f.ledger
                            .store(&b(1))
                            .unwrap()
                            .stored
                            .quantity(&k("wood"))
                            .get(),
                        expected
                    );
                }
            }
        }
    }
    for operation in [HaulOperation::Collect, HaulOperation::Supply] {
        let mut f = Fixture::new(operation == HaulOperation::Supply);
        f.pickup(operation);
        let source = if operation == HaulOperation::Collect {
            3
        } else {
            2
        };
        let mut policy = f.ledger.store(&b(source)).unwrap().policy.clone();
        policy.construction = true;
        f.ledger.update_storage(&b(source), policy).unwrap();
        assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
        assert!(f.ledger.held().is_empty());
    }
}
#[test]
fn collection_keeps_field_and_resource_order_and_cannot_take_worker_pending() {
    let mut f = Fixture::new(false);
    let mut stores = f.ledger.stores().to_vec();
    stores[2].output = stock(&[("planks", 35.), ("wood", 25.)]);
    stores[2].stored = stock(&[("wood", 10.)]);
    stores[2]
        .pending
        .insert(PendingOwner::Orphan, stock(&[("wood", 10.)]));
    stores[2]
        .pending
        .insert(PendingOwner::Worker(a(8)), stock(&[("wood", 70.)]));
    f.admit(stores);
    f.pickup(HaulOperation::Collect);
    assert_eq!(
        f.ledger.held()[&a(1)].goods,
        stock(&[("planks", 35.), ("wood", 15.)])
    );
    assert_eq!(
        f.ledger.store(&b(3)).unwrap().pending[&PendingOwner::Worker(a(8))],
        stock(&[("wood", 70.)])
    );
    f.trips
        .cancel(&mut f.town, &mut f.ledger, &a(1), &b(1))
        .unwrap();
    f.conserved();
    let mut f = Fixture::new(false);
    let mut stores = f.ledger.stores().to_vec();
    stores[2].output = stock(&[("wood", 5.)]);
    stores[2].stored = stock(&[("wood", 7.)]);
    stores[2]
        .pending
        .insert(PendingOwner::Orphan, stock(&[("wood", 9.)]));
    f.admit(stores);
    f.pickup(HaulOperation::Collect);
    assert_eq!(f.ledger.held()[&a(1)].goods.quantity(&k("wood")).get(), 21.);
    assert_eq!(
        f.ledger.store(&b(3)).unwrap().pending[&PendingOwner::Orphan]
            .quantity(&k("wood"))
            .get(),
        0.
    );
}
#[test]
fn candidates_preserve_supply_fullness_distance_and_stable_ties() {
    let mut f = Fixture::new(true);
    let mut stores = f.ledger.stores().to_vec();
    stores[2].output = stock(&[("planks", 5.)]);
    stores[3].output = stock(&[("planks", 10.)]);
    f.admit(stores);
    let id = b(3);
    let assignments = [ProducerAssignment {
        building: &id,
        worker: &f.producer_worker,
        producer: &f.producer,
    }];
    let actual = f
        .trips
        .hauler_candidates(&f.town, &f.ledger, &b(2), &assignments)
        .unwrap();
    assert_eq!(
        actual,
        vec![
            HaulCandidate {
                workplace: b(3),
                operation: HaulOperation::Supply
            },
            HaulCandidate {
                workplace: b(4),
                operation: HaulOperation::Collect
            },
            HaulCandidate {
                workplace: b(3),
                operation: HaulOperation::Collect
            }
        ]
    );
    f.ledger.configure_storage(&b(2), BTreeMap::new()).unwrap();
    let actual = f
        .trips
        .hauler_candidates(&f.town, &f.ledger, &b(2), &assignments)
        .unwrap();
    assert_eq!(
        actual,
        vec![HaulCandidate {
            workplace: b(3),
            operation: HaulOperation::Supply
        }]
    );
    let mut g = Fixture::new(false);
    let mut stores = g.ledger.stores().to_vec();
    stores[3].output = stock(&[("wood", 20.)]);
    g.admit(stores);
    g.town
        .relocate_building(&b(4), Tile { x: -4, y: 0 })
        .unwrap();
    assert_eq!(
        g.trips
            .hauler_candidates(&g.town, &g.ledger, &b(2), &[])
            .unwrap()
            .iter()
            .map(|c| c.workplace.clone())
            .collect::<Vec<_>>(),
        vec![b(3), b(4)]
    );
    g.town
        .relocate_building(&b(4), Tile { x: 1, y: 0 })
        .unwrap();
    assert_eq!(
        g.trips
            .hauler_candidates(&g.town, &g.ledger, &b(2), &[])
            .unwrap()
            .iter()
            .map(|c| c.workplace.clone())
            .collect::<Vec<_>>(),
        vec![b(4), b(3)]
    );
}
fn pairs(value: &serde_json::Value) -> Stock {
    Stock::from_entries(
        value
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| (k(pair[0].as_str().unwrap()), q(pair[1].as_f64().unwrap()))),
    )
    .unwrap()
}
fn same_goods(actual: &Stock, expected: &serde_json::Value, label: &str) {
    let expected = pairs(expected);
    for key in actual.entries().keys().chain(expected.entries().keys()) {
        assert!(
            (actual.quantity(key).get() - expected.quantity(key).get()).abs() < 1e-9,
            "{label} {}: {:?} != {:?}",
            key.as_str(),
            actual,
            expected
        );
    }
    let positive = |s: &Stock| {
        s.ordered_entries()
            .filter(|(_, n)| n.get() > 0.)
            .map(|(k, _)| k.as_str().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        positive(actual),
        positive(&expected),
        "{label} pickup order"
    );
}
#[test]
fn actual_python_hauler_pickups_and_deposits_match_order_capacity_food_and_recipes() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/logistics/haulers.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let supply = case["operation"] == "supply";
        let mut f = Fixture::new(supply);
        let kind = case["kind"].as_str().unwrap();
        let label = case["name"].as_str().unwrap();
        let mut stores = f.ledger.stores().to_vec();
        stores[1].policy.level = 3;
        stores[1].policy.enabled = Some(serde_json::from_value(case["enabled"].clone()).unwrap());
        stores[1].stored = pairs(&case["initial"]["destination"]);
        stores[2].policy = StoragePolicy::new(kind);
        stores[2].output = pairs(&case["initial"]["output"]);
        stores[2].stored = pairs(&case["initial"]["stored"]);
        stores[2]
            .pending
            .insert(PendingOwner::Orphan, pairs(&case["initial"]["orphan"]));
        stores[2]
            .pending
            .insert(PendingOwner::Worker(a(8)), pairs(&case["initial"]["owned"]));
        f.admit(stores);
        f.hauler
            .set_practice(
                "hauling",
                Practice::new(case["hauling"].as_f64().unwrap()).unwrap(),
            )
            .unwrap();
        f.hauler
            .set_talents(BTreeMap::from([(
                "hauling".into(),
                case["talent"].as_f64().unwrap(),
            )]))
            .unwrap();
        f.producer = Producer::recipe(Workplace::new(kind));
        if supply {
            f.producer_worker =
                Worker::new(public_catalog()["buildings"][kind]["job"].as_str().unwrap()).unwrap();
            f.producer_worker.sandbox = true;
        }
        if kind == "workshop" {
            f.producer.workplace.request = Some(lantern_host::progression::Request {
                recipe: "tools".into(),
                remaining: q(100.),
            });
        }
        let operation = if supply {
            HaulOperation::Supply
        } else {
            HaulOperation::Collect
        };
        f.start(1, operation);
        f.reload();
        f.route(1, None);
        assert!(matches!(f.tick(1, 0., false), Outcome::PickedUp(_)));
        let actual = f
            .ledger
            .held()
            .get(&a(1))
            .map(|c| c.goods.clone())
            .unwrap_or_default();
        same_goods(&actual, &case["picked"], label);
        same_goods(&actual, &case["planned"], label);
        f.reload();
        f.route(1, None);
        let outcome = f.tick(1, 0., false);
        assert!(
            if supply {
                matches!(outcome, Outcome::Delivered(_))
            } else {
                matches!(outcome, Outcome::Completed(_))
            },
            "{label} {outcome:?}"
        );
        let actual = f
            .ledger
            .held()
            .get(&a(1))
            .map(|c| c.goods.clone())
            .unwrap_or_default();
        same_goods(&actual, &case["held"], label);
        let target = if supply {
            &f.ledger.store(&b(3)).unwrap().inputs
        } else {
            &f.ledger.store(&b(2)).unwrap().stored
        };
        for pair in case["moved"].as_array().unwrap() {
            let key = k(pair[0].as_str().unwrap());
            assert!(
                target.quantity(&key).get() + 1e-9 >= pair[1].as_f64().unwrap(),
                "{label}"
            );
        }
    }
}
#[test]
fn snapshots_keep_return_completion_and_read_original_producer_v1_without_replaying() {
    let mut f = Fixture::new(true);
    f.pickup(HaulOperation::Supply);
    f.route(1, None);
    assert!(matches!(f.tick(1, 0., false), Outcome::Delivered(_)));
    let before = f.ledger.clone();
    let training = f.hauler.practice_value("hauling");
    f.reload();
    f.route(1, None);
    assert!(matches!(f.tick(1, 0., false), Outcome::Completed(_)));
    assert_eq!(f.ledger, before);
    assert_eq!(f.hauler.practice_value("hauling"), training);
    f.reload();
    assert_eq!(f.tick(1, 0., false), Outcome::Idle);
    let mut f = Fixture::new(true);
    f.trips
        .start(
            &mut f.town,
            &f.ledger,
            &a(1),
            &b(2),
            &b(3),
            NavigationRevision(1),
        )
        .unwrap();
    let mut value: serde_json::Value =
        serde_json::from_str(&f.trips.encode(&f.town, &f.ledger).unwrap()).unwrap();
    value["version"] = 1.into();
    let trip = value["trips"][0][1].as_object_mut().unwrap();
    trip.remove("operation");
    trip.remove("return_to");
    assert!(LogisticsTrips::decode(&value.to_string(), &f.town, &f.ledger).is_ok());
    let mut f = Fixture::new(false);
    f.pickup(HaulOperation::Collect);
    let json = f.trips.encode(&f.town, &f.ledger).unwrap();
    assert!(
        LogisticsTrips::decode(
            &json.replace("\"Collect\"", "\"Supply\""),
            &f.town,
            &f.ledger
        )
        .is_err()
    );
    assert!(
        LogisticsTrips::decode(
            &json.replace("\"version\":2", "\"version\":1"),
            &f.town,
            &f.ledger
        )
        .is_err()
    );
}
#[test]
fn carried_collection_obeys_production_caps_and_reassignment_preserves_the_new_task() {
    let mut f = Fixture::new(false);
    f.ledger.configure_cap(k("wood"), Some(q(20.))).unwrap();
    f.pickup(HaulOperation::Collect);
    assert_eq!(f.ledger.production_room(&k("wood")).unwrap(), 0.);
    let task = Task::Move {
        destination: p(14.),
    };
    f.town.set_task(&a(1), task.clone()).unwrap();
    f.reload();
    assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, task);
    assert!(f.ledger.held().is_empty());
}
#[test]
fn partial_cancellation_returns_only_the_remainder_and_return_interruptions_never_refund_inputs() {
    let mut f = Fixture::new(false);
    let mut stores = f.ledger.stores().to_vec();
    stores[0].stored = stock(&[("wood", 45.)]);
    f.admit(stores);
    f.pickup(HaulOperation::Collect);
    f.ledger
        .pickup(&a(2), &b(1), CargoKind::Collect, &stock(&[("wood", 45.)]))
        .unwrap();
    f.ledger.deposit(&a(2), &b(2)).unwrap();
    f.route(1, None);
    assert_eq!(f.tick(1, 0., false), Outcome::WaitingStorage);
    f.town.remove_building(&b(2)).unwrap();
    f.reload();
    assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
    assert_eq!(
        f.ledger
            .store(&b(3))
            .unwrap()
            .stored
            .quantity(&k("wood"))
            .get(),
        15.
    );
    assert_eq!(
        f.ledger
            .store(&b(2))
            .unwrap()
            .stored
            .quantity(&k("wood"))
            .get(),
        50.
    );
    assert_eq!(f.hauler.practice_value("hauling"), 0.08);
    for endpoint in [2, 3] {
        let mut f = Fixture::new(true);
        f.pickup(HaulOperation::Supply);
        f.route(1, None);
        assert!(matches!(f.tick(1, 0., false), Outcome::Delivered(_)));
        f.town.remove_building(&b(endpoint)).unwrap();
        f.reload();
        assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
        assert_eq!(
            f.ledger
                .store(&b(3))
                .unwrap()
                .inputs
                .quantity(&k("wood"))
                .get(),
            50.
        );
        assert_eq!(
            f.ledger
                .store(&b(1))
                .unwrap()
                .stored
                .quantity(&k("wood"))
                .get(),
            0.
        );
        assert_eq!(f.hauler.practice_value("hauling"), 0.08);
    }
}
#[test]
fn zero_transfers_cannot_preinsert_keys_and_change_later_cargo_order() {
    let mut f = Fixture::new(false);
    let mut stores = f.ledger.stores().to_vec();
    stores[2].output = stock(&[("planks", 35.), ("wood", 25.)]);
    f.admit(stores);
    f.pickup(HaulOperation::Collect);
    assert!(f.ledger.store(&b(3)).unwrap().stored.entries().is_empty());
    f.ledger
        .configure_storage(&b(2), BTreeMap::from([("wood".into(), true)]))
        .unwrap();
    f.route(1, None);
    assert_eq!(f.tick(1, 0., false), Outcome::WaitingStorage);
    assert!(
        !f.ledger
            .store(&b(2))
            .unwrap()
            .stored
            .entries()
            .contains_key(&k("planks"))
    );
    f.trips
        .cancel(&mut f.town, &mut f.ledger, &a(1), &b(1))
        .unwrap();
    f.conserved();
    let restored = &f.ledger.store(&b(3)).unwrap().stored;
    assert_eq!(
        restored
            .ordered_entries()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        vec!["planks"]
    );
}
#[test]
fn full_supply_destination_waits_before_the_empty_return_leg() {
    let mut f = Fixture::new(true);
    f.pickup(HaulOperation::Supply);
    f.ledger
        .reserve(&b(2), &b(3), &stock(&[("wood", 40.)]))
        .unwrap();
    f.route(1, None);
    assert_eq!(f.tick(1, 100., false), Outcome::WaitingStorage);
    assert_eq!(f.ledger.held()[&a(1)].goods.quantity(&k("wood")).get(), 40.);
    f.reload();
    assert_eq!(f.tick(1, 100., false), Outcome::WaitingStorage);
    f.ledger.release(&b(3), &stock(&[("wood", 40.)])).unwrap();
    assert!(matches!(f.tick(1, 0., false), Outcome::Delivered(_)));
    assert_eq!(f.hauler.practice_value("hauling"), 0.08);
    f.reload();
    f.route(1, None);
    assert!(matches!(f.tick(1, 0., false), Outcome::Completed(_)));
    assert_eq!(f.hauler.practice_value("hauling"), 0.08);
}
