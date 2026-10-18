use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    units::{Quantity, Seconds},
};
use lantern_host::resources::*;
use serde_json::Value;
use std::collections::BTreeMap;

fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn key(k: &str) -> ResourceKey {
    ResourceKey::parse(k).unwrap()
}
fn owner() -> Owner {
    Owner {
        world: WorldId::new("ledger").unwrap(),
        town: TownId::new("one").unwrap(),
    }
}
fn building(i: usize) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), i as u64 + 1).unwrap())
}
fn actor(i: usize) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), 100 + i as u64).unwrap())
}
fn stock(v: &Value) -> Stock {
    Stock::from_entries(
        v.as_object()
            .into_iter()
            .flatten()
            .filter(|(k, _)| k.as_str() != "food")
            .map(|(k, n)| (key(k), q(n.as_f64().unwrap()))),
    )
    .unwrap()
}
fn policy(v: &Value) -> StoragePolicy {
    let mut p = StoragePolicy::new(v["kind"].as_str().unwrap());
    p.level = v["level"].as_u64().unwrap_or(1) as u8;
    p.construction = v.get("construction").is_some();
    p.equipment = v["profession_upgrades"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    p.enabled = v["storage_enabled"].as_object().map(|m| {
        m.iter()
            .map(|(k, v)| (k.clone(), v.as_bool().unwrap()))
            .collect()
    });
    p.legacy_allocations = v["storage_allocations"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), q(v.as_f64().unwrap())))
        .collect();
    p
}
fn store(i: usize, v: &Value) -> Store {
    let mut s = Store::new(building(i), policy(v));
    s.stored = stock(&v["stored_goods"]);
    s.output = stock(&v["output_buffer"]);
    s.inputs = stock(&v["input_buffer"]);
    s.provisions = q(v["stored_goods"]["food"].as_f64().unwrap_or(0.0));
    // Capacity vectors supply only an aggregate pending fact, owned by the orphan pool.
    if v["pending_output"].is_object() {
        s.pending
            .insert(PendingOwner::Orphan, stock(&v["pending_output"]));
    }
    s
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/resources/accounting.json")).unwrap()
}
#[track_caller]
fn near(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-9, "{a} != {b}");
}
fn same_stock(actual: &Stock, expected: &Value) {
    for (k, n) in actual.entries() {
        near(n.get(), expected[k.as_str()].as_f64().unwrap_or(0.0));
    }
    for (k, n) in expected
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| k.as_str() != "food")
    {
        near(actual.quantity(&key(k)).get(), n.as_f64().unwrap());
    }
}
#[test]
fn python_storage_sizes_allocations_and_room_match_every_building() {
    for case in fixture()["storage"].as_array().unwrap() {
        let s = store(0, &case["building"]);
        s.policy.validate().unwrap();
        near(s.policy.size(), case["size"].as_f64().unwrap());
        let expected: BTreeMap<String, f64> =
            serde_json::from_value(case["allocations"].clone()).unwrap();
        let allocations = s.policy.allocations();
        assert_eq!(
            allocations.keys().collect::<Vec<_>>(),
            expected.keys().collect::<Vec<_>>(),
            "{}",
            case["building"]
        );
        for (k, n) in allocations {
            near(n, expected[&k]);
        }
        for (k, n) in case["rooms"].as_object().unwrap() {
            near(s.local_room(k).unwrap(), n.as_f64().unwrap());
        }
    }
}
fn assert_state(l: &Ledger, state: &Value) {
    let town = &state["town"];
    let rows = l.overview().unwrap();
    for (name, row) in &rows {
        let expected = &state["overview"][name];
        for (field, n) in [
            ("free", row.free),
            ("reserved", row.reserved),
            ("pending", row.pending),
            ("total", row.total),
        ] {
            near(n, expected[field].as_f64().unwrap_or(0.0));
        }
    }
    for (i, b) in town["buildings"].as_array().unwrap().iter().enumerate() {
        let s = l.store(&building(i)).unwrap();
        same_stock(&s.stored, &b["stored_goods"]);
        same_stock(&s.output, &b["output_buffer"]);
        same_stock(&s.inputs, &b["input_buffer"]);
        same_stock(&s.pending_stock().unwrap(), &b["pending_output"]);
        near(
            s.provisions.get(),
            b["stored_goods"]["food"].as_f64().unwrap_or(0.0),
        );
    }
    for (name, n) in state["rooms"].as_object().unwrap() {
        near(l.production_room(&key(name)).unwrap(), n.as_f64().unwrap());
    }
    for (name, n) in state["storage_room"].as_object().unwrap() {
        near(l.storage_room(name).unwrap(), n.as_f64().unwrap());
    }
    for (name, n) in state["storage_limit"].as_object().unwrap() {
        near(l.storage_limit(name).unwrap(), n.as_f64().unwrap());
    }
    same_stock(l.produced(), &town["produced"]);
    let history = &town["resource_activity"];
    near(
        l.activity().seconds(),
        history["seconds"].as_f64().unwrap_or(0.0),
    );
    let buckets = history["buckets"].as_array().cloned().unwrap_or_default();
    assert_eq!(l.activity().buckets().len(), buckets.len());
    for (b, expected) in l.activity().buckets().iter().zip(buckets) {
        near(b.at, expected["at"].as_f64().unwrap());
        for (direction, values) in [("in", &b.incoming), ("out", &b.outgoing)] {
            for (k, n) in values {
                near(*n, expected[direction][k].as_f64().unwrap_or(0.0));
            }
            for (k, n) in expected[direction].as_object().unwrap() {
                near(values.get(k).copied().unwrap_or(0.0), n.as_f64().unwrap());
            }
        }
    }
}
#[test]
fn python_accounting_trace_and_browser_overview_match_each_transition() {
    for trace in fixture()["traces"].as_array().unwrap() {
        let initial = &trace["initial"];
        let town = &initial["town"];
        let stores = town["buildings"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, b)| store(i, b))
            .collect();
        let mut l =
            Ledger::new(owner(), q(town["coins"].as_f64().unwrap()), stores, vec![]).unwrap();
        assert_state(&l, initial);
        for step in trace["steps"].as_array().unwrap() {
            let a = &step["action"];
            let i = a["building"].as_u64().unwrap_or(0) as usize;
            let id = building(i);
            let worker = actor(a["actor"].as_u64().unwrap_or(0) as usize);
            let source = building(a["source"].as_u64().unwrap_or(0) as usize);
            match a["op"].as_str().unwrap() {
                "cap" => l
                    .configure_cap(key(a["key"].as_str().unwrap()), a["cap"].as_f64().map(q))
                    .unwrap(),
                "advance" => l
                    .advance(Seconds::new(a["seconds"].as_f64().unwrap()).unwrap())
                    .unwrap(),
                "spend" => l.spend(&stock(&a["goods"])).unwrap(),
                "eat" => l.eat(q(a["amount"].as_f64().unwrap())).unwrap(),
                "reserve" => l.reserve(&source, &id, &stock(&a["goods"])).unwrap(),
                "release" => l.release(&id, &stock(&a["goods"])).unwrap(),
                "produce" => l
                    .produce(
                        &id,
                        if a["pending"].as_bool().unwrap_or(true) {
                            Some(&worker)
                        } else {
                            None
                        },
                        &stock(&a["costs"]),
                        &stock(&a["goods"]),
                    )
                    .unwrap(),
                "drop" => l.drop_off(&id, &worker).unwrap(),
                "orphan" => l.orphan_pending(&id, &worker).unwrap(),
                "pickup" => l
                    .pickup(
                        &worker,
                        &source,
                        match a["kind"].as_str().unwrap() {
                            "collect" => CargoKind::Collect,
                            "supply" => CargoKind::Supply,
                            _ => CargoKind::Sell,
                        },
                        &stock(&a["goods"]),
                    )
                    .unwrap(),
                "deposit" => {
                    l.deposit(&worker, &id).unwrap();
                }
                "cancel" => l.return_cargo(&worker, &building(0)).unwrap(),
                "storage" => l
                    .configure_storage(&id, serde_json::from_value(a["enabled"].clone()).unwrap())
                    .unwrap(),
                "policy" => l
                    .update_storage(&id, policy(&step["state"]["town"]["buildings"][i]))
                    .unwrap(),
                "deliver" => {
                    near(
                        l.deliver(
                            &key(a["key"].as_str().unwrap()),
                            q(a["amount"].as_f64().unwrap()),
                            a["production"].as_bool().unwrap(),
                        )
                        .unwrap()
                        .get(),
                        a["received"].as_f64().unwrap(),
                    );
                }
                _ => panic!("unknown fixture action"),
            }
            std::panic::catch_unwind(|| assert_state(&l, &step["state"]))
                .unwrap_or_else(|_| panic!("accounting mismatch after {a}"));
        }
    }
}

fn goods(entries: &[(&str, f64)]) -> Stock {
    Stock::from_entries(entries.iter().map(|(k, n)| (key(k), q(*n)))).unwrap()
}
fn empty_store(i: usize, kind: &str) -> Store {
    Store::new(building(i), StoragePolicy::new(kind))
}
fn ledger(stores: Vec<Store>) -> Ledger {
    Ledger::new(owner(), q(10.0), stores, vec![]).unwrap()
}
fn total(l: &Ledger, k: &str) -> f64 {
    l.overview().unwrap()[k].total
}

#[test]
fn physical_transfers_conserve_stock_and_keep_partial_cargo_unspendable() {
    let mut hall = empty_store(0, "hall");
    hall.stored = goods(&[("wood", 99.875)]);
    let mut source = empty_store(1, "woodcutter");
    source.output = goods(&[("wood", 1.25)]);
    let mut l = ledger(vec![hall, source]);
    let initial = total(&l, "wood");
    l.pickup(
        &actor(0),
        &building(1),
        CargoKind::Collect,
        &goods(&[("wood", 1.25)]),
    )
    .unwrap();
    assert_eq!(l.overview().unwrap()["wood"].pending, 1.25);
    assert!(!l.can_afford(&goods(&[("wood", 100.0)])).unwrap());
    assert_eq!(
        l.deposit(&actor(0), &building(0))
            .unwrap()
            .quantity(&key("wood")),
        q(0.125)
    );
    assert_eq!(l.held()[&actor(0)].goods.quantity(&key("wood")), q(1.125));
    assert_eq!(total(&l, "wood"), initial);
    let before = l.clone();
    assert_eq!(
        l.pickup(
            &actor(0),
            &building(1),
            CargoKind::Collect,
            &Stock::default()
        ),
        Err(ResourceError::CargoExists)
    );
    assert_eq!(l, before);
    l.return_cargo(&actor(0), &building(0)).unwrap();
    assert_eq!(
        l.store(&building(1)).unwrap().stored.quantity(&key("wood")),
        q(1.125)
    );
    assert_eq!(total(&l, "wood"), initial);
    let before = l.clone();
    assert_eq!(
        l.return_cargo(&actor(0), &building(0)),
        Err(ResourceError::MissingCargo)
    );
    assert_eq!(l, before);
    assert!(l.activity().buckets().is_empty());
    assert!(l.produced().entries().is_empty());
}

#[test]
fn supply_reservation_release_and_full_input_buffers_preserve_items() {
    let mut hall = empty_store(0, "hall");
    hall.stored = goods(&[("wood", 2.0), ("flour", 5.0)]);
    let mut bakery = empty_store(1, "bakery");
    bakery.inputs = goods(&[("flour", 99.75)]);
    let mut l = ledger(vec![hall, bakery]);
    let before = l.clone();
    assert_eq!(
        l.reserve(&building(0), &building(1), &goods(&[("wood", 0.5)])),
        Err(ResourceError::Capacity)
    );
    assert_eq!(l, before);
    l.pickup(
        &actor(0),
        &building(0),
        CargoKind::Supply,
        &goods(&[("wood", 0.5)]),
    )
    .unwrap();
    let row = l.overview().unwrap()["wood"];
    assert_eq!((row.free, row.reserved, row.pending), (1.5, 0.5, 0.0));
    l.deposit(&actor(0), &building(1)).unwrap();
    assert_eq!(l.held()[&actor(0)].goods.quantity(&key("wood")), q(0.25));
    assert_eq!(
        l.store(&building(1)).unwrap().inputs.quantity(&key("wood")),
        q(0.25)
    );
    assert_eq!(total(&l, "wood"), 2.0);
    l.release(&building(1), &goods(&[("wood", 0.25)])).unwrap();
    assert_eq!(l.free(&key("wood")).unwrap(), q(1.75));
    l.deposit(&actor(0), &building(1)).unwrap();
    assert!(l.held().is_empty());
    assert_eq!(total(&l, "wood"), 2.0);
    assert!(l.activity().buckets().is_empty());
}

#[test]
fn production_caps_exclude_inputs_count_pending_and_sales_once_and_block_other_workers() {
    let mut source = empty_store(0, "hall");
    source.stored = goods(&[("wood", 10.0)]);
    let mut a = empty_store(1, "sawpit");
    a.inputs = goods(&[("wood", 4.0)]);
    let mut b = empty_store(2, "sawpit");
    b.inputs = goods(&[("wood", 4.0)]);
    let mut destination = empty_store(3, "storehouse");
    destination.policy.enabled = Some(BTreeMap::from([("planks".into(), true)]));
    let mut l = ledger(vec![source, a, b, destination]);
    l.configure_cap(key("wood"), Some(q(10.0))).unwrap();
    assert_eq!(l.production_room(&key("wood")).unwrap(), 0.0);
    l.configure_cap(key("planks"), Some(q(1.0))).unwrap();
    let costs = goods(&[("wood", 2.0)]);
    let output = goods(&[("planks", 1.0)]);
    l.produce(&building(1), Some(&actor(0)), &costs, &output)
        .unwrap();
    assert_eq!(l.production_room(&key("planks")).unwrap(), 0.0);
    assert!(!l.can_afford(&output).unwrap());
    let before = l.clone();
    assert_eq!(
        l.produce(&building(2), Some(&actor(1)), &costs, &output),
        Err(ResourceError::ProductionCap)
    );
    assert_eq!(l, before);
    l.orphan_pending(&building(1), &actor(0)).unwrap();
    assert_eq!(total(&l, "planks"), 1.0);
    l.pickup(&actor(1), &building(1), CargoKind::Collect, &output)
        .unwrap();
    assert_eq!(l.production_room(&key("planks")).unwrap(), 0.0);
    assert_eq!(total(&l, "planks"), 1.0);
    l.deposit(&actor(1), &building(3)).unwrap();
    l.pickup(&actor(1), &building(3), CargoKind::Sell, &output)
        .unwrap();
    assert_eq!(l.overview().unwrap()["planks"].reserved, 1.0);
    assert_eq!(l.production_room(&key("planks")).unwrap(), 0.0);
    assert_eq!(l.spend(&output), Err(ResourceError::InsufficientStock));
    let before = l.clone();
    assert_eq!(
        l.spend(&goods(&[("coins", 1.0), ("planks", 1.0)])),
        Err(ResourceError::InsufficientStock)
    );
    assert_eq!(l, before);
    l.return_cargo(&actor(1), &building(0)).unwrap();
    l.spend(&output).unwrap();
    assert_eq!(l.production_room(&key("planks")).unwrap(), 1.0);
    l.produce(&building(2), Some(&actor(1)), &costs, &output)
        .unwrap();
    l.drop_off(&building(2), &actor(1)).unwrap();
    let before = l.clone();
    assert_eq!(
        l.drop_off(&building(2), &actor(1)),
        Err(ResourceError::MissingPending)
    );
    assert_eq!(l, before);
    assert_eq!(l.produced().quantity(&key("planks")), q(2.0));
    assert_eq!(l.activity().totals().unwrap().incoming["planks"], 2.0);
    assert_eq!(l.activity().totals().unwrap().outgoing["wood"], 4.0);
}

#[test]
fn food_consumes_only_free_items_in_python_order_and_preserves_reserved_food() {
    let mut hall = empty_store(0, "hall");
    hall.provisions = q(1.0);
    hall.stored = goods(&[("bread", 4.0), ("vegetables", 2.0), ("carrots", 3.0)]);
    let mut bakery = empty_store(1, "bakery");
    bakery.inputs = goods(&[("bread", 2.0)]);
    let mut l = ledger(vec![hall, bakery]);
    assert_eq!(l.overview().unwrap()["food"].total, 15.0);
    l.eat(q(4.0)).unwrap();
    assert_eq!(l.store(&building(0)).unwrap().provisions, q(0.0));
    assert_eq!(l.free(&key("vegetables")).unwrap(), q(0.0));
    assert_eq!(l.free(&key("carrots")).unwrap(), q(2.0));
    assert_eq!(l.free(&key("bread")).unwrap(), q(4.0));
    l.eat(q(100.0)).unwrap();
    assert_eq!(l.food().unwrap(), q(0.0));
    assert_eq!(l.overview().unwrap()["food"].total, 3.0);
    assert_eq!(
        l.store(&building(1))
            .unwrap()
            .inputs
            .quantity(&key("bread")),
        q(2.0)
    );
    assert_eq!(l.activity().totals().unwrap().outgoing["food"], 12.0);
}

#[test]
fn fractional_fuel_batch_completes_and_real_shortage_or_full_output_is_atomic() {
    let mut bakery = empty_store(0, "bakery");
    bakery.inputs = goods(&[("flour", 203.5), ("wood", 4.999999999999972)]);
    let mut l = ledger(vec![bakery]);
    let costs = goods(&[("flour", 20.0), ("wood", 5.0)]);
    let output = goods(&[("bread", 20.0)]);
    l.produce(&building(0), Some(&actor(0)), &costs, &output)
        .unwrap();
    assert_eq!(
        l.store(&building(0)).unwrap().inputs.quantity(&key("wood")),
        q(0.0)
    );
    assert_eq!(
        l.store(&building(0))
            .unwrap()
            .inputs
            .quantity(&key("flour")),
        q(183.5)
    );
    assert_eq!(
        l.activity().totals().unwrap().outgoing["wood"],
        4.999999999999972
    );
    let before = l.clone();
    assert_eq!(
        l.produce(&building(0), Some(&actor(0)), &costs, &output),
        Err(ResourceError::InsufficientStock)
    );
    assert_eq!(l, before);
    let mut workshop = empty_store(0, "workshop");
    workshop.stored = goods(&[("stone", 49.0)]);
    workshop.inputs = goods(&[("wood", 2.0)]);
    let mut l = ledger(vec![workshop]);
    let before = l.clone();
    assert_eq!(
        l.produce(
            &building(0),
            None,
            &goods(&[("wood", 1.0)]),
            &goods(&[("wood", 1.0), ("planks", 1.0)])
        ),
        Err(ResourceError::Capacity)
    );
    assert_eq!(l, before);
}

#[test]
fn filters_upgrades_and_imports_preserve_excess_goods_and_milestones() {
    let mut s = empty_store(0, "storehouse");
    s.stored = goods(&[("bread", 100.0), ("wood", 50.0)]);
    s.policy.enabled = Some(BTreeMap::from([
        ("food".into(), true),
        ("bread".into(), false),
    ]));
    let mut l = ledger(vec![s]);
    assert_eq!(
        l.store(&building(0)).unwrap().local_room("bread").unwrap(),
        0.0
    );
    assert_eq!(
        l.store(&building(0)).unwrap().local_room("salmon").unwrap(),
        50.0
    );
    l.configure_storage(&building(0), BTreeMap::from([("food".into(), true)]))
        .unwrap();
    assert_eq!(
        l.storage_room("salmon").unwrap(),
        0.0,
        "command switches missing keys off instead of legacy expansion"
    );
    assert_eq!(total(&l, "bread"), 100.0);
    assert_eq!(total(&l, "wood"), 50.0);
    let mut policy = l.store(&building(0)).unwrap().policy.clone();
    policy.level = 2;
    policy.enabled = Some(BTreeMap::from([("wood".into(), true)]));
    l.update_storage(&building(0), policy).unwrap();
    assert_eq!(l.storage_room("wood").unwrap(), 25.0);
    l.configure_cap(key("wood"), Some(q(51.0))).unwrap();
    assert_eq!(l.deliver(&key("wood"), q(10.0), true).unwrap(), q(1.0));
    assert_eq!(l.deliver(&key("wood"), q(10.0), false).unwrap(), q(10.0));
    assert_eq!(l.produced().quantity(&key("wood")), q(1.0));
    assert_eq!(l.activity().totals().unwrap().incoming["wood"], 11.0);
    let before = l.clone();
    assert_eq!(
        l.configure_cap(key("coins"), Some(q(0.0))),
        Err(ResourceError::InvalidCap)
    );
    assert_eq!(l, before);
}

#[test]
fn activity_uses_simulation_time_ignores_roundoff_and_preview_does_not_leak() {
    let mut activity = Activity::default();
    activity
        .record(&key("bread"), q(1e-9), Direction::In)
        .unwrap();
    assert!(activity.buckets().is_empty());
    activity.advance(Seconds::new(3.0).unwrap()).unwrap();
    activity
        .record(&key("bread"), q(2.0), Direction::In)
        .unwrap();
    activity
        .record(&key("wood"), q(1.0), Direction::In)
        .unwrap();
    assert_eq!(activity.buckets().len(), 1);
    assert_eq!(activity.totals().unwrap().incoming["food"], 3.0);
    activity.advance(Seconds::new(57.0).unwrap()).unwrap();
    assert_eq!(activity.buckets().len(), 1);
    activity.advance(Seconds::new(3.0).unwrap()).unwrap();
    assert!(activity.buckets().is_empty());
    for _ in 0..100 {
        activity
            .record(&key("wood"), q(1.0), Direction::In)
            .unwrap();
        activity.advance(Seconds::new(3.0).unwrap()).unwrap();
    }
    assert_eq!(activity.buckets().len(), 19);
    let before = activity.clone();
    let mut preview = activity.clone();
    preview
        .record(&key("wood"), q(5.0), Direction::Out)
        .unwrap();
    assert_eq!(activity, before);
    let mut l = ledger(vec![empty_store(0, "hall")]);
    let before = l.clone();
    let mut preview = l.clone();
    preview.deliver(&key("wood"), q(5.0), true).unwrap();
    assert_eq!(l, before);
    l.advance(Seconds::new(0.0).unwrap()).unwrap();
    assert_eq!(l, before);
}

#[test]
fn ownership_duplicate_facts_overflow_and_fallback_are_explicit_and_atomic() {
    let hall = empty_store(0, "hall");
    assert_eq!(
        Ledger::new(owner(), q(0.0), vec![hall.clone(), hall.clone()], vec![]),
        Err(ResourceError::DuplicateIdentity)
    );
    let mut invalid = hall.clone();
    invalid.policy.level = 0;
    assert_eq!(
        Ledger::new(owner(), q(0.0), vec![invalid], vec![]),
        Err(ResourceError::InvalidStorage)
    );
    let mut invalid = hall.clone();
    invalid.stored = goods(&[("coins", 1.0)]);
    assert_eq!(
        Ledger::new(owner(), q(0.0), vec![invalid], vec![]),
        Err(ResourceError::InvalidStorage)
    );
    let foreign = ActorId::from_saved(
        EntityIdentity::from_saved(
            Owner {
                town: TownId::new("foreign").unwrap(),
                ..owner()
            },
            100,
        )
        .unwrap(),
    );
    let mut l = ledger(vec![hall.clone()]);
    let before = l.clone();
    assert_eq!(
        l.pickup(
            &foreign,
            &building(0),
            CargoKind::Collect,
            &Stock::default()
        ),
        Err(ResourceError::Ownership)
    );
    assert_eq!(l, before);
    let mut a = empty_store(0, "sawpit");
    let mut b = empty_store(1, "sawpit");
    a.pending
        .insert(PendingOwner::Worker(actor(0)), goods(&[("planks", 1.0)]));
    b.pending = a.pending.clone();
    assert_eq!(
        Ledger::new(owner(), q(0.0), vec![a, b], vec![]),
        Err(ResourceError::DuplicateIdentity)
    );
    let mut huge = hall.clone();
    huge.stored = goods(&[("wood", f64::MAX)]);
    huge.output = huge.stored.clone();
    assert_eq!(
        Ledger::new(owner(), q(0.0), vec![huge], vec![]),
        Err(ResourceError::Overflow)
    );
    let cargo = HeldCargo {
        actor: actor(0),
        source: building(5),
        kind: CargoKind::Supply,
        goods: goods(&[("bread", 1.25)]),
    };
    let mut l = Ledger::new(owner(), q(0.0), vec![hall], vec![cargo]).unwrap();
    l.return_cargo(&actor(0), &building(0)).unwrap();
    assert_eq!(l.free(&key("bread")).unwrap(), q(1.25));
    assert!(l.activity().buckets().is_empty());
    let mut activity = Activity::default();
    activity
        .record(&key("wood"), q(f64::MAX), Direction::In)
        .unwrap();
    let before = activity.clone();
    assert_eq!(
        activity.record(&key("wood"), q(f64::MAX), Direction::In),
        Err(ResourceError::Overflow)
    );
    assert_eq!(activity, before);
}
