use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{logistics::*, progression::Worker, resources::*};
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
        world: WorldId::new("merchants").unwrap(),
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
    merchant: Worker,
}
impl Fixture {
    fn new() -> Self {
        let town = Town::restore(
            owner(),
            [(1, 0), (2, 2), (3, 8), (4, 10), (5, 14)]
                .into_iter()
                .map(|(id, x)| Building {
                    id: b(id),
                    location: Tile { x, y: 0 },
                    construction: ConstructionState::Complete,
                })
                .collect(),
            (1..=3)
                .map(|id| ActorState::new(a(id), p(10.), Cargo::default()))
                .collect(),
        )
        .unwrap();
        let mut stores = [
            (1, "hall"),
            (2, "storehouse"),
            (3, "sawpit"),
            (4, "market"),
            (5, "market"),
        ]
        .map(|(id, kind)| Store::new(b(id), StoragePolicy::new(kind)))
        .to_vec();
        stores[1].policy.enabled = Some(BTreeMap::from([
            ("planks".into(), true),
            ("wood".into(), true),
            ("food".into(), true),
        ]));
        stores[2].stored = stock(&[("planks", 13.)]);
        Self {
            town,
            ledger: Ledger::new(owner(), q(1000.), stores, vec![]).unwrap(),
            trips: LogisticsTrips::default(),
            merchant: Worker::new("trading").unwrap(),
        }
    }
    fn configure(&mut self, key: &str, buy: Option<f64>, sell: Option<f64>) {
        self.trips
            .configure_trade(&self.town, &self.ledger, k(key), buy.map(q), sell.map(q))
            .unwrap();
    }
    fn start(&mut self, actor: u64) {
        self.trips
            .start_sale(
                &mut self.town,
                &self.ledger,
                &a(actor),
                &self.merchant,
                &b(3),
                &b(4),
                NavigationRevision(1),
            )
            .unwrap();
    }
    fn route(&mut self, actor: u64, points: Option<Vec<Position>>) {
        self.town
            .resume_travel(&a(actor), NavigationRevision(1))
            .unwrap();
        let RouteState::Pending(request) = self.town.actor(&a(actor)).unwrap().route().clone()
        else {
            panic!("route pending")
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
        self.trips
            .advance_merchant(
                &mut self.town,
                &mut self.ledger,
                &a(actor),
                &mut self.merchant,
                &b(1),
                sec(elapsed),
                NavigationRevision(1),
                |_, _, speed| if blocked { None } else { Some(speed) },
            )
            .unwrap()
    }
    fn buy(&mut self, actor: u64, update: u64, elapsed: f64) -> PurchaseOutcome {
        self.trips
            .purchase_batch(
                &self.town,
                &mut self.ledger,
                &a(actor),
                &mut self.merchant,
                &b(4),
                update,
                sec(elapsed),
            )
            .unwrap()
    }
    fn pickup(&mut self) {
        self.start(1);
        self.route(1, Some(vec![p(8.)]));
        assert!(matches!(self.tick(1, 10., false), Outcome::PickedUp(_)));
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
        self.trips = LogisticsTrips::decode(&json, &self.town, &self.ledger).unwrap();
        // Ledger and personal facts are independent admitted owners (R19 persists
        // them together); trip reload must not change either accounting or practice.
    }
    fn conserved_sales(&self, total: f64) {
        let count = self.ledger.market_sales().get();
        assert!((self.ledger.overview().unwrap()["planks"].total + count - total).abs() < 1e-9);
        assert!(
            (self.ledger.free(&k("coins")).unwrap().get()
                - 1000.
                - count * k("planks").sale_price())
            .abs()
                < 1e-9
        );
        assert!(self.ledger.produced().entries().is_empty());
    }
}
#[test]
fn sale_transfers_only_at_pickup_and_market_arrival_and_retries_do_not_sell_twice() {
    let mut f = Fixture::new();
    f.configure("planks", None, Some(10.));
    f.start(1);
    f.conserved_sales(13.);
    f.reload();
    f.route(1, Some(vec![p(8.)]));
    f.tick(1, 0.5, false);
    assert!(f.ledger.held().is_empty());
    f.conserved_sales(13.);
    f.reload();
    f.route(1, Some(vec![p(8.)]));
    assert!(matches!(f.tick(1, 1., false), Outcome::PickedUp(_)));
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("planks", 3.)]));
    assert_eq!(f.ledger.free(&k("planks")).unwrap().get(), 10.);
    f.conserved_sales(13.);
    f.reload();
    f.route(1, Some(vec![p(10.)]));
    assert_eq!(f.tick(1, 100., true), Outcome::Travelling);
    assert_eq!(f.ledger.market_sales().get(), 0.);
    f.reload();
    f.route(1, Some(vec![p(10.)]));
    assert!(matches!(f.tick(1, 10., false), Outcome::Completed(_)));
    f.conserved_sales(13.);
    assert_eq!(f.ledger.market_sales().get(), 3.);
    assert_eq!(f.merchant.practice_value("trading"), 0.08);
    assert_eq!(f.merchant.practice_value("hauling"), 0.08);
    let ledger = f.ledger.clone();
    let worker = f.merchant.clone();
    f.reload();
    assert_eq!(f.tick(1, 999., false), Outcome::Idle);
    assert_eq!(f.ledger, ledger);
    assert_eq!(f.merchant, worker);
    let activity = f.ledger.activity().totals().unwrap();
    assert_eq!(activity.outgoing["planks"], 3.);
    assert_eq!(activity.incoming["coins"], 3. * k("planks").sale_price());
}
#[test]
fn arrival_rechecks_disabled_threshold_and_batch_rules_and_returns_unsold_cargo() {
    for (threshold, batch, enabled, expected) in [
        (12., 10, true, 1.),
        (10., 1, true, 1.),
        (100., 10, true, 0.),
        (10., 10, false, 0.),
    ] {
        let mut f = Fixture::new();
        f.configure("planks", None, Some(10.));
        f.pickup();
        f.trips
            .configure_market_sale(
                &f.town,
                &f.ledger,
                k("planks"),
                Some(q(threshold)),
                enabled,
                Some(batch),
            )
            .unwrap();
        f.reload();
        f.route(1, None);
        f.tick(1, 0., false);
        assert_eq!(f.ledger.market_sales().get(), expected);
        assert_eq!(
            f.ledger
                .store(&b(3))
                .unwrap()
                .stored
                .quantity(&k("planks"))
                .get(),
            13. - expected
        );
        assert!(f.ledger.held().is_empty());
        f.conserved_sales(13.);
        assert_eq!(
            f.merchant.practice_value("trading"),
            if expected > 0. { 0.08 } else { 0. }
        );
    }
}
#[test]
fn moved_deleted_and_upgrading_sale_endpoints_cancel_once_without_sales() {
    for endpoint in [3, 4] {
        for deleted in [false, true] {
            for picked in [false, true] {
                let mut f = Fixture::new();
                f.configure("planks", None, Some(10.));
                if picked {
                    f.pickup();
                } else {
                    f.start(1);
                }
                if deleted {
                    f.town.remove_building(&b(endpoint)).unwrap();
                } else {
                    f.town
                        .relocate_building(&b(endpoint), Tile { x: 40, y: 1 })
                        .unwrap();
                }
                f.reload();
                assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
                f.conserved_sales(13.);
                assert_eq!(f.ledger.market_sales().get(), 0.);
                assert!(f.ledger.held().is_empty());
                assert_eq!(
                    f.ledger
                        .store(&b(1))
                        .unwrap()
                        .stored
                        .quantity(&k("planks"))
                        .get(),
                    if endpoint == 3 && picked { 3. } else { 0. }
                );
                assert_eq!(f.tick(1, 999., false), Outcome::Idle);
            }
        }
    }
    let mut f = Fixture::new();
    f.configure("planks", None, Some(10.));
    f.pickup();
    let mut policy = f.ledger.store(&b(4)).unwrap().policy.clone();
    policy.construction = true;
    f.ledger.update_storage(&b(4), policy).unwrap();
    assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
    f.conserved_sales(13.);
}
#[test]
fn competing_sales_recheck_actual_stock_and_exclude_reserved_and_pending_goods() {
    let mut f = Fixture::new();
    f.trips
        .configure_market_sale(
            &f.town,
            &f.ledger,
            k("planks"),
            Some(q(10.)),
            true,
            Some(10),
        )
        .unwrap();
    f.start(1);
    f.start(2);
    f.route(1, None);
    f.tick(1, 0., false);
    assert_eq!(
        f.ledger.held()[&a(1)].goods.quantity(&k("planks")).get(),
        10.
    );
    f.route(2, None);
    f.tick(2, 0., false);
    assert!(!f.ledger.held().contains_key(&a(2)));
    f.conserved_sales(13.);
    f.reload();
    f.route(1, None);
    f.tick(1, 0., false);
    f.conserved_sales(13.);
    assert_eq!(f.ledger.market_sales().get(), 10.);
    let mut f = Fixture::new();
    f.ledger
        .reserve(&b(3), &b(3), &stock(&[("planks", 13.)]))
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores[2]
        .pending
        .insert(PendingOwner::Orphan, stock(&[("planks", 5.)]));
    stores[2]
        .pending
        .insert(PendingOwner::Worker(a(8)), stock(&[("planks", 10.)]));
    f.ledger = Ledger::new(owner(), q(1000.), stores, vec![]).unwrap();
    assert_eq!(f.ledger.overview().unwrap()["planks"].reserved, 13.);
    assert_eq!(f.ledger.overview().unwrap()["planks"].pending, 15.);
    f.configure("planks", None, Some(0.));
    assert!(
        f.trips
            .sale_sources(&f.town, &f.ledger, &b(4), &f.merchant)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn purchases_save_fractional_work_rotate_and_ignore_replayed_or_older_update_ids() {
    let mut f = Fixture::new();
    f.configure("wood", Some(25.), None);
    f.configure("vegetables", Some(25.), None);
    assert_eq!(f.buy(1, 1, 1.), PurchaseOutcome::Waiting);
    let before = f.ledger.clone();
    f.reload();
    assert_eq!(f.buy(1, 1, 999.), PurchaseOutcome::Replayed);
    assert_eq!(f.ledger, before);
    assert_eq!(
        f.buy(1, 2, 2.),
        PurchaseOutcome::Bought {
            resource: k("wood"),
            amount: q(10.)
        }
    );
    f.reload();
    assert_eq!(f.buy(1, 2, 3.), PurchaseOutcome::Replayed);
    assert_eq!(f.buy(1, 1, 3.), PurchaseOutcome::Replayed);
    assert_eq!(
        f.buy(1, 3, 100.),
        PurchaseOutcome::Bought {
            resource: k("vegetables"),
            amount: q(10.)
        }
    );
    assert_eq!(
        f.buy(1, 4, 3.),
        PurchaseOutcome::Bought {
            resource: k("wood"),
            amount: q(10.)
        }
    );
    assert_eq!(f.ledger.free(&k("wood")).unwrap().get(), 20.);
    assert_eq!(f.ledger.free(&k("vegetables")).unwrap().get(), 10.);
    assert_eq!(f.merchant.practice_value("trading"), 0.24);
    assert_eq!(f.merchant.practice_value("hauling"), 0.);
    assert!(f.ledger.produced().entries().is_empty());
    assert_eq!(f.ledger.market_sales().get(), 0.);
    let totals = f.ledger.activity().totals().unwrap();
    assert_eq!(totals.incoming["wood"], 20.);
    assert_eq!(totals.incoming["food"], 10.);
    assert_eq!(totals.outgoing["coins"], 60.);
}
#[test]
fn purchases_recheck_shared_stock_room_coins_and_reset_work_when_no_candidates_remain() {
    let mut f = Fixture::new();
    f.configure("wood", Some(12.), None);
    assert!(matches!(f.buy(1,1,3.),PurchaseOutcome::Bought{amount,..} if amount.get()==10.));
    assert!(matches!(f.buy(2,1,3.),PurchaseOutcome::Bought{amount,..} if amount.get()==2.));
    assert_eq!(f.buy(1, 2, 3.), PurchaseOutcome::Idle);
    assert_eq!(f.ledger.free(&k("wood")).unwrap().get(), 12.);
    let mut f = Fixture::new();
    f.configure("wood", Some(100.), None);
    assert_eq!(f.buy(1, 1, 2.), PurchaseOutcome::Waiting);
    f.ledger.configure_storage(&b(2), BTreeMap::new()).unwrap();
    let mut hall = f.ledger.store(&b(1)).unwrap().policy.clone();
    hall.construction = true;
    f.ledger.update_storage(&b(1), hall).unwrap();
    let mut source = f.ledger.store(&b(3)).unwrap().policy.clone();
    source.construction = true;
    f.ledger.update_storage(&b(3), source).unwrap();
    assert_eq!(f.buy(1, 2, 10.), PurchaseOutcome::Idle);
    f.ledger
        .configure_storage(&b(2), BTreeMap::from([("wood".into(), true)]))
        .unwrap();
    assert_eq!(f.buy(1, 3, 1.), PurchaseOutcome::Waiting);
    let mut f = Fixture::new();
    f.configure("wood", Some(100.), None);
    f.ledger.spend(&stock(&[("coins", 998.)])).unwrap();
    assert_eq!(
        f.buy(1, 1, 100.),
        PurchaseOutcome::Bought {
            resource: k("wood"),
            amount: q(1.)
        }
    );
    assert_eq!(f.ledger.free(&k("coins")).unwrap().get(), 0.);
    assert_eq!(f.buy(1, 2, 3.), PurchaseOutcome::Idle);
}
#[test]
fn market_rules_validate_atomically_preserve_order_and_prioritize_buying() {
    let mut f = Fixture::new();
    f.configure("planks", Some(5.), Some(10.));
    let before = f.trips.encode(&f.town, &f.ledger).unwrap();
    assert!(
        f.trips
            .configure_trade(&f.town, &f.ledger, k("planks"), Some(q(10.)), Some(q(5.)))
            .is_err()
    );
    assert!(
        f.trips
            .configure_market_sale(&f.town, &f.ledger, k("planks"), Some(q(4.)), true, None)
            .is_err()
    );
    assert_eq!(f.trips.encode(&f.town, &f.ledger).unwrap(), before);
    f.configure("wood", Some(5.), None);
    assert!(
        f.trips
            .sale_sources(&f.town, &f.ledger, &b(4), &f.merchant)
            .unwrap()
            .is_empty()
    );
    f.configure("wood", None, None);
    assert_eq!(
        f.trips
            .sale_sources(&f.town, &f.ledger, &b(4), &f.merchant)
            .unwrap(),
        vec![b(3)]
    );
    f.configure("planks", None, None);
    assert!(!f.trips.sale_rules()[0].enabled);
    assert!(f.trips.purchase_rules().is_empty());
}
#[test]
fn quick_sales_are_atomic_count_actual_goods_and_keep_saved_command_receipts() {
    let mut f = Fixture::new();
    assert!(
        f.trips
            .quick_sale(&f.town, &mut f.ledger, 1, k("planks"), 5)
            .unwrap()
    );
    f.conserved_sales(13.);
    f.reload();
    let before = f.ledger.clone();
    assert!(
        !f.trips
            .quick_sale(&f.town, &mut f.ledger, 1, k("planks"), 5)
            .unwrap()
    );
    assert_eq!(f.ledger, before);
    assert!(
        f.trips
            .quick_sale(&f.town, &mut f.ledger, 2, k("planks"), 200)
            .is_err()
    );
    assert_eq!(f.ledger, before);
    assert!(
        f.trips
            .quick_sale(&f.town, &mut f.ledger, 2, k("planks"), 8)
            .unwrap()
    );
    f.conserved_sales(13.);
    assert_eq!(f.ledger.market_sales().get(), 13.);
    assert!(f.merchant.skills().is_empty());
}
fn near(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{label}: {actual} != {expected}"
    );
}
fn map_near(actual: &BTreeMap<String, f64>, expected: &serde_json::Value, label: &str) {
    for (key, n) in actual {
        near(
            *n,
            expected[key].as_f64().unwrap_or(0.),
            &format!("{label} {key}"),
        );
    }
    for (key, n) in expected.as_object().unwrap() {
        near(
            actual.get(key).copied().unwrap_or(0.),
            n.as_f64().unwrap(),
            &format!("{label} {key}"),
        );
    }
}
#[test]
fn actual_python_market_traces_match_thresholds_prices_activity_food_and_practice() {
    let cases: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/logistics/markets.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let mut f = Fixture::new();
        let mut stores = f.ledger.stores().to_vec();
        stores[2].stored = Stock::from_entries(
            case["goods"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(key, n)| (k(key), q(n.as_f64().unwrap()))),
        )
        .unwrap();
        f.ledger = Ledger::new(owner(), q(1000.), stores, vec![]).unwrap();
        for (index, (action, expected)) in case["actions"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["states"].as_array().unwrap())
            .enumerate()
        {
            let optional = |name: &str| action[name].as_f64();
            match action["op"].as_str().unwrap() {
                "trade" => f.configure(
                    action["key"].as_str().unwrap(),
                    optional("buy"),
                    optional("sell"),
                ),
                "sale" => f
                    .trips
                    .configure_market_sale(
                        &f.town,
                        &f.ledger,
                        k(action["key"].as_str().unwrap()),
                        optional("cap").map(q),
                        action["enabled"].as_bool().unwrap(),
                        Some(action["amount"].as_u64().unwrap() as u8),
                    )
                    .unwrap(),
                "pickup" => {
                    f.start(1);
                    f.route(1, None);
                    assert!(matches!(f.tick(1, 0., false), Outcome::PickedUp(_)));
                }
                "sell" => {
                    f.route(1, None);
                    assert!(matches!(f.tick(1, 0., false), Outcome::Completed(_)));
                }
                "purchase" => {
                    f.buy(1, index as u64 + 1, action["elapsed"].as_f64().unwrap());
                }
                "quick" => {
                    f.trips
                        .quick_sale(
                            &f.town,
                            &mut f.ledger,
                            index as u64 + 1,
                            k(action["key"].as_str().unwrap()),
                            action["amount"].as_u64().unwrap() as u16,
                        )
                        .unwrap();
                }
                op => panic!("unknown action {op}"),
            }
            let label = format!("{} {index}", case["name"].as_str().unwrap());
            let rows = f.ledger.overview().unwrap();
            let actual = rows
                .iter()
                .filter(|(k, _)| k.as_str() != "coins" && k.as_str() != "food")
                .map(|(k, r)| (k.clone(), r.free))
                .collect();
            map_near(&actual, &expected["free"], &label);
            near(
                rows["coins"].total,
                expected["coins"].as_f64().unwrap(),
                &label,
            );
            near(
                rows["food"].free,
                expected["food"].as_f64().unwrap(),
                &label,
            );
            near(
                f.ledger.market_sales().get(),
                expected["sales"].as_f64().unwrap(),
                &label,
            );
            near(
                f.merchant.practice_value("trading"),
                expected["trading"].as_f64().unwrap(),
                &label,
            );
            near(
                f.merchant.practice_value("hauling"),
                expected["hauling"].as_f64().unwrap(),
                &label,
            );
            let totals = f.ledger.activity().totals().unwrap();
            map_near(&totals.incoming, &expected["incoming"], &label);
            map_near(&totals.outgoing, &expected["outgoing"], &label);
            let cargo = f
                .ledger
                .held()
                .get(&a(1))
                .map(|c| {
                    c.goods
                        .entries()
                        .iter()
                        .map(|(k, n)| (k.as_str().into(), n.get()))
                        .collect()
                })
                .unwrap_or_default();
            map_near(&cargo, &expected["held"], &label);
            assert!(f.ledger.produced().entries().is_empty());
            f.reload();
        }
    }
}
#[test]
fn purchase_market_interruptions_clear_work_and_saved_rules_reject_corruption() {
    for deleted in [false, true] {
        let mut f = Fixture::new();
        f.configure("wood", Some(10.), None);
        assert_eq!(f.buy(1, 1, 2.), PurchaseOutcome::Waiting);
        if deleted {
            f.town.remove_building(&b(4)).unwrap();
        } else {
            f.town
                .relocate_building(&b(4), Tile { x: 20, y: 1 })
                .unwrap();
        }
        f.reload();
        let before = f.ledger.clone();
        let outcome = f.buy(1, 2, 0.);
        assert_eq!(
            outcome,
            if deleted {
                PurchaseOutcome::Unavailable
            } else {
                PurchaseOutcome::Waiting
            }
        );
        assert_eq!(f.ledger, before);
    }
    let mut f = Fixture::new();
    f.configure("wood", Some(10.), None);
    f.buy(1, 1, 1.);
    let json = f.trips.encode(&f.town, &f.ledger).unwrap();
    for bad in [
        json.replace("\"work\":1.0", "\"work\":3.0"),
        json.replace("\"update\":1", "\"update\":0"),
        json.replace("merchants", "foreign"),
    ] {
        assert!(LogisticsTrips::decode(&bad, &f.town, &f.ledger).is_err());
    }
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["market"]["rules"]["purchases"][0][1] = serde_json::json!(-1);
    assert!(LogisticsTrips::decode(&value.to_string(), &f.town, &f.ledger).is_err());
}
#[test]
fn overflow_or_failed_restitution_leaves_trip_ledger_and_training_unchanged() {
    let mut f = Fixture::new();
    f.configure("planks", None, Some(10.));
    f.pickup();
    f.town
        .relocate_building(&b(3), Tile { x: 50, y: 1 })
        .unwrap();
    let state = f.trips.encode(&f.town, &f.ledger).unwrap();
    let before = f.ledger.clone();
    let worker = f.merchant.clone();
    assert!(
        f.trips
            .cancel(&mut f.town, &mut f.ledger, &a(1), &b(2))
            .is_err()
    );
    assert_eq!(f.ledger, before);
    assert_eq!(f.merchant, worker);
    assert_eq!(f.trips.encode(&f.town, &f.ledger).unwrap(), state);
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores[2].stored = stock(&[("trade_crates", f64::MAX)]);
    f.ledger = Ledger::new(owner(), q(1000.), stores, vec![]).unwrap();
    let before = f.ledger.clone();
    assert_eq!(
        f.ledger.sell_free(&k("trade_crates"), q(f64::MAX)),
        Err(ResourceError::Overflow)
    );
    assert_eq!(f.ledger, before);
}
#[test]
fn moving_assignment_and_new_purchase_candidates_cannot_sell_or_buy_carried_goods_twice() {
    let mut f = Fixture::new();
    f.configure("planks", None, Some(10.));
    f.pickup();
    f.configure("wood", Some(10.), None);
    assert_eq!(f.buy(1, 1, 100.), PurchaseOutcome::OnTrip);
    let task = Task::Move { destination: p(0.) };
    f.town.set_task(&a(1), task.clone()).unwrap();
    f.reload();
    assert_eq!(f.tick(1, 0., false), Outcome::Cancelled);
    assert_eq!(f.town.actor(&a(1)).unwrap().state().task, task);
    assert_eq!(f.buy(1, 1, 100.), PurchaseOutcome::Replayed);
    f.conserved_sales(13.);
}
#[test]
fn pickup_uses_current_ordered_rules_and_never_withdraws_more_than_actual_fractional_stock() {
    let mut f = Fixture::new();
    let mut stores = f.ledger.stores().to_vec();
    stores[2].stored = stock(&[("planks", 13.), ("wood", 13.)]);
    f.ledger = Ledger::new(owner(), q(1000.), stores, vec![]).unwrap();
    f.configure("planks", None, Some(10.));
    f.configure("wood", None, Some(10.));
    f.start(1);
    f.ledger.spend(&stock(&[("planks", 4.)])).unwrap();
    f.route(1, None);
    f.tick(1, 0., false);
    assert_eq!(f.ledger.held()[&a(1)].goods, stock(&[("wood", 3.)]));
    assert_eq!(
        f.trips
            .sale_rules()
            .iter()
            .map(|r| r.resource.as_str())
            .collect::<Vec<_>>(),
        vec!["planks", "wood"]
    );
    let mut f = Fixture::new();
    let actual = 3. - 1e-10;
    let mut stores = f.ledger.stores().to_vec();
    stores[2].stored = stock(&[("planks", actual)]);
    f.ledger = Ledger::new(owner(), q(1000.), stores, vec![]).unwrap();
    f.trips
        .configure_market_sale(&f.town, &f.ledger, k("planks"), Some(q(1.)), true, Some(10))
        .unwrap();
    f.pickup();
    assert_eq!(
        f.ledger.held()[&a(1)].goods.quantity(&k("planks")).get(),
        actual
    );
    f.route(1, None);
    f.tick(1, 0., false);
    f.conserved_sales(actual);
}
#[test]
fn legacy_trip_versions_remain_readable_but_cannot_smuggle_market_state() {
    let mut f = Fixture::new();
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
    let original: serde_json::Value =
        serde_json::from_str(&f.trips.encode(&f.town, &f.ledger).unwrap()).unwrap();
    for version in [1, 2] {
        let mut value = original.clone();
        value["version"] = version.into();
        assert!(LogisticsTrips::decode(&value.to_string(), &f.town, &f.ledger).is_err());
        value.as_object_mut().unwrap().remove("market");
        if version == 1 {
            let t = value["trips"][0][1].as_object_mut().unwrap();
            t.remove("operation");
            t.remove("return_to");
        }
        assert!(LogisticsTrips::decode(&value.to_string(), &f.town, &f.ledger).is_ok());
    }
}
