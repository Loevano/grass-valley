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
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn key(k: &str) -> ResourceKey {
    ResourceKey::parse(k).unwrap()
}
fn stock(items: &[(&str, f64)]) -> Stock {
    Stock::from_entries(items.iter().map(|(k, n)| (key(k), q(*n)))).unwrap()
}
fn owner() -> Owner {
    Owner {
        world: WorldId::new("input-trips").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), n).unwrap())
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), n + 100).unwrap())
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    trips: InputTrips,
    worker: Worker,
    producer: Producer,
}
impl Fixture {
    fn new() -> Self {
        let buildings = [(1, 0), (2, 10), (3, 20), (4, 30)]
            .map(|(id, x)| Building {
                id: b(id),
                location: Tile { x, y: 0 },
                construction: ConstructionState::Complete,
            })
            .to_vec();
        let town = Town::restore(
            owner(),
            buildings,
            vec![
                ActorState::new(a(1), p(20., 0.), Cargo::default()),
                ActorState::new(a(2), p(20., 0.), Cargo::default()),
            ],
        )
        .unwrap();
        let mut stores = [
            (1, "hall"),
            (2, "storehouse"),
            (3, "sawpit"),
            (4, "storehouse"),
        ]
        .map(|(id, kind)| Store::new(b(id), StoragePolicy::new(kind)))
        .to_vec();
        stores[1].stored = stock(&[("wood", 100.)]);
        stores[3].stored = stock(&[("wood", 100.)]);
        let ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        let mut worker = Worker::new("woodworking").unwrap();
        worker.sandbox = true;
        Self {
            town,
            ledger,
            trips: InputTrips::default(),
            worker,
            producer: Producer::recipe(Workplace::new("sawpit")),
        }
    }
    fn start(&mut self) {
        self.trips
            .start(
                &mut self.town,
                &self.ledger,
                &a(1),
                &b(2),
                &b(3),
                NavigationRevision(1),
            )
            .unwrap();
    }
    fn ready(&mut self, points: Option<Vec<Position>>) {
        self.town
            .resume_travel(&a(1), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(r) = self.town.actor(&a(1)).unwrap().route().clone() else {
            panic!("pending")
        };
        self.town
            .accept_route(
                &a(1),
                &r,
                r.navigation(),
                points
                    .map(RouteOutcome::Ready)
                    .unwrap_or(RouteOutcome::Arrived),
            )
            .unwrap();
    }
    fn advance(&mut self, n: f64, blocked: bool) -> Outcome {
        self.trips
            .advance(
                &mut self.town,
                &mut self.ledger,
                &a(1),
                &mut self.worker,
                &self.producer,
                &b(1),
                sec(n),
                NavigationRevision(1),
                |_, _, speed| if blocked { None } else { Some(speed) },
            )
            .unwrap()
    }
    fn pickup(&mut self) {
        self.start();
        self.ready(Some(vec![p(10., 0.)]));
        assert!(matches!(self.advance(10., false), Outcome::PickedUp(_)));
        assert_eq!(self.ledger.held()[&a(1)].goods, stock(&[("wood", 50.)]));
        self.conserved();
    }
    fn conserved(&self) {
        assert_eq!(self.ledger.overview().unwrap()["wood"].total, 200.);
        assert_eq!(self.ledger.overview().unwrap()["coins"].total, 0.);
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
        self.trips = InputTrips::decode(&json, &self.town, &self.ledger).unwrap();
        self.conserved();
    }
}
#[test]
fn planning_pickup_and_delivery_have_separate_authoritative_events() {
    let mut f = Fixture::new();
    assert_eq!(
        f.trips
            .sources(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap(),
        vec![b(2), b(4)]
    );
    f.start();
    assert_eq!(f.trips.trip_id(&a(1)), Some(1));
    f.conserved();
    assert_eq!(f.ledger.free(&key("wood")).unwrap().get(), 200.);
    f.reload();
    f.ready(Some(vec![p(10., 0.)]));
    f.advance(1., false);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, p(18.2, 0.));
    assert!(f.ledger.held().is_empty());
    f.conserved();
    f.reload();
    f.ready(Some(vec![p(10., 0.)]));
    f.advance(20., false);
    assert_eq!(f.ledger.free(&key("wood")).unwrap().get(), 150.);
    assert_eq!(f.ledger.overview().unwrap()["wood"].reserved, 50.);
    assert!(
        f.trips
            .request(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap()
            .entries()
            .is_empty()
    );
    f.reload();
    f.ready(None);
    assert_eq!(f.advance(0., false), Outcome::Completed(sec(0.)));
    f.conserved();
    assert_eq!(
        f.ledger.store(&b(3)).unwrap().inputs,
        stock(&[("wood", 50.)])
    );
    let trained = f.worker.practice_value("hauling");
    assert!(trained > 0.);
    assert_eq!(f.advance(999., false), Outcome::Idle);
    assert_eq!(f.worker.practice_value("hauling"), trained);
    f.conserved();
    f.start();
    assert_eq!(f.trips.trip_id(&a(1)), Some(2));
}
#[test]
fn blocked_and_full_destinations_hold_actual_cargo_across_reload_and_retries() {
    let mut f = Fixture::new();
    f.pickup();
    f.ready(Some(vec![p(20., 0.)]));
    assert_eq!(f.advance(10., true), Outcome::Travelling);
    f.conserved();
    f.reload();
    f.ledger
        .reserve(&b(4), &b(3), &stock(&[("wood", 40.)]))
        .unwrap();
    f.ready(None);
    assert_eq!(f.advance(10., false), Outcome::WaitingStorage);
    f.conserved();
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 40.)]));
    let training = f.worker.practice_value("hauling");
    f.reload();
    assert_eq!(f.advance(100., false), Outcome::WaitingStorage);
    assert_eq!(f.worker.practice_value("hauling"), training);
    f.conserved();
    f.ledger.release(&b(3), &stock(&[("wood", 40.)])).unwrap();
    assert_eq!(f.advance(0., false), Outcome::Completed(sec(0.)));
    assert_eq!(f.worker.practice_value("hauling"), training);
    f.conserved();
}
#[test]
fn moved_deleted_and_unfinished_endpoints_cancel_once_before_and_after_pickup() {
    for endpoint in [2, 3] {
        for deleted in [false, true] {
            for picked_up in [false, true] {
                let mut f = Fixture::new();
                if picked_up {
                    f.pickup();
                } else {
                    f.start();
                }
                if deleted {
                    f.town.remove_building(&b(endpoint)).unwrap();
                } else {
                    f.town
                        .relocate_building(&b(endpoint), Tile { x: 40, y: 4 })
                        .unwrap();
                }
                f.reload();
                assert_eq!(f.advance(10., false), Outcome::Cancelled);
                f.conserved();
                let expected = if picked_up && endpoint == 2 { 50. } else { 0. };
                assert_eq!(
                    f.ledger
                        .store(&b(1))
                        .unwrap()
                        .stored
                        .quantity(&key("wood"))
                        .get(),
                    expected
                );
                assert!(f.ledger.held().is_empty());
                f.trips
                    .cancel(&mut f.town, &mut f.ledger, &a(1), &b(1))
                    .unwrap();
                assert_eq!(f.advance(10., false), Outcome::Idle);
                f.conserved();
            }
        }
    }
    let mut f = Fixture::new();
    f.pickup();
    let mut policy = f.ledger.store(&b(3)).unwrap().policy.clone();
    policy.construction = true;
    f.ledger.update_storage(&b(3), policy).unwrap();
    assert_eq!(f.advance(0., false), Outcome::Cancelled);
    f.conserved();
}
#[test]
fn pickup_rechecks_stock_rules_and_preserves_filters_and_atomic_errors() {
    let mut f = Fixture::new();
    f.start();
    f.ledger.spend(&stock(&[("wood", 80.)])).unwrap();
    f.ready(None);
    f.advance(0., false);
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 20.)]));
    let mut f = Fixture::new();
    f.pickup();
    f.ledger.configure_storage(&b(2), BTreeMap::new()).unwrap();
    f.trips
        .cancel(&mut f.town, &mut f.ledger, &a(1), &b(1))
        .unwrap();
    f.conserved();
    assert_eq!(
        f.ledger
            .store(&b(2))
            .unwrap()
            .stored
            .quantity(&key("wood"))
            .get(),
        100.
    );
    let mut f = Fixture::new();
    f.pickup();
    let json = f.trips.encode(&f.town, &f.ledger).unwrap();
    for bad in [
        json.replace("input-trips", "foreign"),
        json.replace("50.0", "51.0"),
        json.replace("\"next\":1", "\"next\":0"),
        json.replace("\"Delivery\"", "\"Pickup\""),
    ] {
        assert!(InputTrips::decode(&bad, &f.town, &f.ledger).is_err());
    }
    f.town
        .relocate_building(&b(2), Tile { x: 50, y: 0 })
        .unwrap();
    let before = f.ledger.clone();
    assert!(
        f.trips
            .cancel(&mut f.town, &mut f.ledger, &a(1), &b(4))
            .is_err()
    );
    assert_eq!(f.ledger, before);
    assert!(f.trips.trip_id(&a(1)).is_some());
}
#[test]
fn hauling_skill_changes_speed_without_changing_goods() {
    let mut f = Fixture::new();
    f.worker
        .set_practice("hauling", Practice::new(20.).unwrap())
        .unwrap();
    f.start();
    f.ready(Some(vec![p(10., 0.)]));
    f.advance(1., false);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().position, p(17.75, 0.));
    f.advance(10., false);
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 50.)]));
    f.conserved();
}
fn json_stock(value: &serde_json::Value) -> Stock {
    Stock::from_entries(
        value
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.as_str() != "food")
            .map(|(k, n)| (key(k), q(n.as_f64().unwrap()))),
    )
    .unwrap()
}
#[test]
fn python_recipe_producer_plans_and_pickups_match() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/logistics/python.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mut f = Fixture::new();
        let kind = case["building"]["kind"].as_str().unwrap();
        f.producer = Producer::recipe(Workplace::new(kind));
        if kind == "workshop" {
            f.producer.workplace.request = Some(lantern_host::progression::Request {
                recipe: "tools".into(),
                remaining: q(3.),
            });
        }
        f.worker = Worker::new(case["worker"]["job"].as_str().unwrap()).unwrap();
        f.worker.sandbox = true;
        let mut stores = f.ledger.stores().to_vec();
        stores[1].stored = json_stock(&case["source"]["stored_goods"]);
        stores[2].policy = StoragePolicy::new(kind);
        stores[2].inputs = json_stock(&case["building"]["input_buffer"]);
        stores[3].stored = Stock::default();
        f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        let request = f
            .trips
            .request(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap();
        let expected = json_stock(&case["request"]);
        for k in request.entries().keys().chain(expected.entries().keys()) {
            assert!(
                (request.quantity(k).get() - expected.quantity(k).get()).abs() < 1e-9,
                "{kind} {}",
                k.as_str()
            );
        }
        let before = f.ledger.overview().unwrap();
        f.start();
        f.ready(None);
        f.advance(0., false);
        let actual = f
            .ledger
            .held()
            .get(&a(1))
            .map(|c| c.goods.clone())
            .unwrap_or_default();
        let expected = json_stock(&case["cargo"]);
        for k in actual.entries().keys().chain(expected.entries().keys()) {
            assert!(
                (actual.quantity(k).get() - expected.quantity(k).get()).abs() < 1e-9,
                "pickup {kind} {}: {:?} != {:?}",
                k.as_str(),
                actual,
                expected
            );
        }
        for (k, row) in before {
            assert!((f.ledger.overview().unwrap()[&k].total - row.total).abs() < 1e-9);
        }
    }
}
#[test]
fn livestock_feed_substitution_and_manual_butcher_pause() {
    for kind in ["barn", "stable"] {
        let mut f = Fixture::new();
        let mut stores = f.ledger.stores().to_vec();
        stores[1].stored = stock(&[("vegetables", 60.), ("fodder", 0.5)]);
        stores[2].policy = StoragePolicy::new(kind);
        stores[3].stored = Stock::default();
        f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
        f.producer = Producer {
            workplace: Workplace::new(kind),
            animal_feed: q(1.),
            butchering: false,
        };
        let request = f
            .trips
            .request(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap();
        assert!(request.quantity(&key("vegetables")).get() > 0.);
        assert_eq!(request.quantity(&key("fodder")).get(), 0.);
    }
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores[2].policy = StoragePolicy::new("butcher");
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    f.producer = Producer {
        workplace: Workplace::new("butcher"),
        animal_feed: q(0.),
        butchering: true,
    };
    assert!(
        f.trips
            .request(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap()
            .entries()
            .is_empty()
    );
}
#[test]
fn reassignment_cancels_cargo_once_and_preserves_new_task() {
    let mut f = Fixture::new();
    f.pickup();
    let task = Task::Move {
        destination: p(8., 8.),
    };
    f.town.set_task(&a(1), task.clone()).unwrap();
    f.reload();
    assert_eq!(f.advance(0., false), Outcome::Cancelled);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, task);
    f.conserved();
    assert!(f.ledger.held().is_empty());
}
#[test]
fn recipe_pause_and_partial_delivery_keep_pickup_order() {
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores[2].output = stock(&[("planks", 50.)]);
    stores[2].inputs = stock(&[("wood", 2.)]);
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    assert_eq!(
        f.trips
            .production_gate(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap(),
        ProductionGate::BufferFull
    );
    f.ledger.release(&b(3), &stock(&[("wood", 2.)])).unwrap();
    assert!(matches!(
        f.trips
            .production_gate(&f.town, &f.ledger, &b(3), &f.worker, &f.producer)
            .unwrap(),
        ProductionGate::CollectInputs(_)
    ));
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores[1].stored = stock(&[("leather", 100.), ("iron", 100.)]);
    stores[2].policy = StoragePolicy::new("saddler");
    stores[3].stored = Stock::default();
    f.producer = Producer::recipe(Workplace::new("saddler"));
    f.worker = Worker::new("saddlery").unwrap();
    f.worker.sandbox = true;
    f.ledger = Ledger::new(owner(), q(0.), stores, vec![]).unwrap();
    f.start();
    f.ready(None);
    f.advance(0., false);
    f.ledger
        .reserve(&b(2), &b(3), &stock(&[("iron", 40.)]))
        .unwrap();
    f.ready(None);
    assert_eq!(f.advance(0., false), Outcome::WaitingStorage);
    assert_eq!(
        f.ledger
            .store(&b(3))
            .unwrap()
            .inputs
            .quantity(&key("leather"))
            .get(),
        10.
    );
    f.reload_without_conservation();
}
impl Fixture {
    fn reload_without_conservation(&mut self) {
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
        self.trips = InputTrips::decode(&json, &self.town, &self.ledger).unwrap();
    }
}
