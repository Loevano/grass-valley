use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    units::{Practice, Quantity, Seconds, Tile},
};
use lantern_host::{
    progression::*,
    resources::{Ledger, ResourceError, ResourceKey, Stock, StoragePolicy, Store},
};
use serde_json::Value;
use std::collections::BTreeMap;
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/resources/progression.json")).unwrap()
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn skill(n: f64) -> Practice {
    Practice::new(n).unwrap()
}
fn key(k: &str) -> ResourceKey {
    ResourceKey::parse(k).unwrap()
}
fn stock(v: &Value) -> Stock {
    Stock::from_entries(
        v.as_object()
            .into_iter()
            .flatten()
            .map(|(k, n)| (key(k), q(n.as_f64().unwrap()))),
    )
    .unwrap()
}
fn goods(entries: &[(&str, f64)]) -> Stock {
    Stock::from_entries(entries.iter().map(|(k, n)| (key(k), q(*n)))).unwrap()
}
fn worker(v: &Value) -> Worker {
    let mut w = Worker::new(v["job"].as_str().unwrap_or("idle")).unwrap();
    let jobs = lantern_host::resources::public_catalog();
    let extra = [
        "idle",
        "building",
        "woodcutting",
        "farming",
        "fishing",
        "hauling",
        "gathering",
    ];
    for (k, n) in v.as_object().unwrap() {
        if n.is_number()
            && (extra.contains(&k.as_str())
                || jobs["buildings"]
                    .as_object()
                    .unwrap()
                    .values()
                    .any(|b| b["job"].as_str() == Some(k)))
        {
            w.set_practice(k, skill(n.as_f64().unwrap())).unwrap();
        }
    }
    if v["talents"].is_object() {
        w.set_talents(serde_json::from_value(v["talents"].clone()).unwrap())
            .unwrap();
    }
    w.sandbox = v["sandbox"].as_bool().unwrap_or(false);
    w.tool = v["worker_tool"].as_str().map(String::from);
    w
}
fn workplace(v: &Value) -> Workplace {
    let mut b = Workplace::new(v["kind"].as_str().unwrap());
    b.recipe = v["recipe"].as_str().map(String::from);
    b.equipment = v["profession_upgrades"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    b.request = v["craft_queue"]
        .as_array()
        .and_then(|a| a.first())
        .map(|v| Request {
            recipe: v["recipe"].as_str().unwrap().into(),
            remaining: q(v["remaining"].as_f64().unwrap()),
        });
    b
}
#[track_caller]
fn near(a: f64, b: f64) {
    assert!(
        (a - b).abs() <= 1e-10 * a.abs().max(b.abs()).max(1.0),
        "{a} != {b}"
    );
}
fn same_stock(a: &Stock, b: &Value) {
    for (k, n) in a.entries() {
        near(n.get(), b[k.as_str()].as_f64().unwrap_or(0.0));
    }
    for (k, n) in b.as_object().unwrap() {
        near(a.quantity(&key(k)).get(), n.as_f64().unwrap());
    }
}
fn owner() -> Owner {
    Owner {
        world: WorldId::new("progression").unwrap(),
        town: TownId::new("town").unwrap(),
    }
}
fn building(i: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner(), i + 1).unwrap())
}
fn actor(i: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner(), i + 100).unwrap())
}
fn store(i: u64, kind: &str) -> Store {
    Store::new(building(i), StoragePolicy::new(kind))
}
fn ledger(stores: Vec<Store>) -> Ledger {
    Ledger::new(owner(), q(100.0), stores, vec![]).unwrap()
}

#[test]
fn python_personal_skills_talents_and_exact_decimal_practice_rounding_match() {
    for case in fixture()["practice"].as_array().unwrap() {
        let mut w = worker(&case["worker"]);
        let job = case["job"].as_str().unwrap();
        assert_eq!(w.aptitude(job), case["aptitude"].as_f64().unwrap());
        near(
            w.efficiency(job).unwrap(),
            case["efficiency"].as_f64().unwrap(),
        );
        assert_eq!(
            w.qualification(job).is_ok(),
            case["qualified"].as_bool().unwrap()
        );
        w.practice(
            job,
            Seconds::new(case["seconds"].as_f64().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            w.practice_value(job),
            case["after"].as_f64().unwrap(),
            "{case}"
        );
    }
    for case in fixture()["defaults"].as_array().unwrap() {
        let mut w = worker(&case["initial"]);
        w.ensure_talents(case["index"].as_u64().unwrap() as usize);
        let expected: BTreeMap<String, f64> =
            serde_json::from_value(case["expected"].clone()).unwrap();
        assert_eq!(w.talents(), Some(&expected));
        let before = w.clone();
        w.ensure_talents(999);
        assert_eq!(w, before);
    }
}
#[test]
fn python_every_recipe_qualification_costs_tools_throughput_and_smoked_species_match() {
    for case in fixture()["batches"].as_array().unwrap() {
        let w = worker(&case["worker"]);
        let b = workplace(&case["building"]);
        let r = b.selected_recipe().unwrap();
        same_stock(&r.inputs, &case["recipe"]["inputs"]);
        assert_eq!(
            r.output.as_str(),
            case["recipe"]["output"].as_str().unwrap()
        );
        assert_eq!(r.amount.get(), case["recipe"]["amount"].as_f64().unwrap());
        assert_eq!(r.seconds().get(), case["seconds"].as_f64().unwrap());
        near(
            b.processing_efficiency(&w).unwrap(),
            case["speed"].as_f64().unwrap(),
        );
        let result = evaluate_batch(&w, &b, &stock(&case["inputs"]));
        assert_eq!(
            result.is_ok(),
            case["qualified"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(batch) = result {
            same_stock(&batch.costs, &case["costs"]);
            same_stock(&batch.outputs, &case["outputs"]);
        }
    }
}
#[test]
fn python_recipe_selection_uses_only_qualified_assigned_workers_and_sandbox_rules() {
    for case in fixture()["selections"].as_array().unwrap() {
        let workers: Vec<_> = case["workers"]
            .as_array()
            .unwrap()
            .iter()
            .map(worker)
            .collect();
        assert_eq!(
            choose_recipe(
                case["kind"].as_str().unwrap(),
                case["output"].as_str().unwrap(),
                case["sandbox"].as_bool().unwrap(),
                &workers
            )
            .is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
    }
    assert_eq!(
        choose_recipe("bakery", "tools", true, &[]),
        Err(Error::UnknownRecipe)
    );
}
#[test]
fn python_profession_upgrade_gates_costs_and_instant_boat_match() {
    for case in fixture()["upgrades"].as_array().unwrap() {
        let workers: Vec<_> = case["workers"]
            .as_array()
            .unwrap()
            .iter()
            .map(worker)
            .collect();
        let evidence = construction_evidence(&ledger(vec![]), &workers, None, 0, q(0.0)).unwrap();
        let offer = equipment_offer(
            &workplace(&case["building"]),
            case["key"].as_str().unwrap(),
            case["level"].as_u64().unwrap() as u8,
            &evidence.skills,
            case["sandbox"].as_bool().unwrap(),
        );
        assert_eq!(offer.is_ok(), case["accepted"].as_bool().unwrap(), "{case}");
        if let Ok(offer) = offer {
            same_stock(&offer.costs, &case["costs"]);
            assert_eq!(offer.instant, case["instant"].as_bool().unwrap());
            assert_eq!(offer.seconds.get(), 30.0);
        }
    }
}
#[test]
fn python_crops_boat_species_and_clothing_bonuses_match() {
    let f = fixture();
    for case in f["crops"].as_array().unwrap() {
        assert_eq!(
            crop_unlocked(
                case["kind"].as_str().unwrap(),
                case["crop"].as_str().unwrap(),
                case["level"].as_u64().unwrap() as u8
            )
            .is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
    }
    for case in f["fish"].as_array().unwrap() {
        let equipment = serde_json::from_value(case["equipment"].clone()).unwrap();
        assert_eq!(
            fishing_boat_level(&equipment),
            case["level"].as_u64().unwrap() as u8
        );
        assert_eq!(
            fishing_boat_capacity(&equipment),
            case["capacity"].as_u64().unwrap()
        );
        assert_eq!(
            fishing_catch(&equipment, case["depth"].as_u64().unwrap() as u8)
                .ok()
                .map(|k| k.as_str().to_owned()),
            case["catch"].as_str().map(String::from)
        );
    }
    for case in f["clothes"].as_array().unwrap() {
        near(
            gathering_bonus(
                &serde_json::from_value(case["equipment"].clone()).unwrap(),
                &key(case["good"].as_str().unwrap()),
            ),
            case["bonus"].as_f64().unwrap(),
        );
    }
}
fn context(town: &Value) -> HallContext {
    HallContext {
        buildings: town["buildings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| HallBuilding {
                kind: b["kind"].as_str().unwrap().into(),
                tile: Tile {
                    x: b["x"].as_i64().unwrap() as i32,
                    y: b["y"].as_i64().unwrap() as i32,
                },
                status: if b.get("construction").is_none() {
                    BuildingStatus::Complete
                } else if b["construction"].get("upgrade").is_some() {
                    BuildingStatus::Upgrade
                } else {
                    BuildingStatus::InitialConstruction
                },
            })
            .collect(),
        natural_water: town["map_resources"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"].as_str() == Some("water"))
            .map(|r| Tile {
                x: r["x"].as_i64().unwrap() as i32,
                y: r["y"].as_i64().unwrap() as i32,
            })
            .collect(),
    }
}
#[test]
fn python_hall_milestones_match_water_status_food_and_every_local_output_threshold() {
    for case in fixture()["halls"].as_array().unwrap() {
        let t = &case["town"];
        let e = HallEvidence {
            food: q(t["food"].as_f64().unwrap()),
            produced: stock(&t["produced"]),
            completed_trades: t["completed_trades"].as_u64().unwrap_or(0),
            market_sales: q(t["market_sales"].as_f64().unwrap_or(0.0)),
        };
        assert_eq!(
            hall_milestone(case["level"].as_u64().unwrap() as u8, &e, &context(t)),
            case["ready"].as_bool().unwrap(),
            "{case}"
        );
    }
    for level in 2..=10 {
        let offer = hall_offer(level, false).unwrap();
        let sandbox = hall_offer(level, true).unwrap();
        assert!(
            !offer.costs.entries().contains_key(&key("coins")),
            "hall coins are rewards"
        );
        assert_eq!(offer.reward, sandbox.reward);
        assert!(sandbox.costs.entries().is_empty());
    }
    assert_eq!(hall_offer(11, false), Err(Error::Locked));
}
#[test]
fn completed_batch_is_atomic_with_ledger_capacity_caps_qualification_and_roundoff() {
    let mut s = store(0, "bakery");
    s.inputs = goods(&[("flour", 203.5), ("wood", 4.999999999999972)]);
    let mut l = ledger(vec![s]);
    let mut w = Worker::new("baking").unwrap();
    w.set_practice("milling", skill(2.0)).unwrap();
    w.set_practice("baking", skill(33.1)).unwrap();
    let b = workplace_from_ledger(&l, &building(0), Some("bread".into()), None).unwrap();
    // Skill tier doubles the ordinary batch; a missing prerequisite never consumes.
    w.set_practice("milling", skill(1.999)).unwrap();
    let before = (l.clone(), w.clone());
    assert!(matches!(
        complete_batch(&mut l, &building(0), &actor(0), &mut w, &b),
        Err(Error::Qualification(_))
    ));
    assert_eq!((l.clone(), w.clone()), before);
    w.set_practice("milling", skill(2.0)).unwrap();
    l.configure_cap(key("bread"), Some(q(3.999))).unwrap();
    let before = (l.clone(), w.clone());
    assert_eq!(
        complete_batch(&mut l, &building(0), &actor(0), &mut w, &b),
        Err(Error::Resource(ResourceError::ProductionCap))
    );
    assert_eq!((l.clone(), w.clone()), before);
    l.configure_cap(key("bread"), None).unwrap();
    complete_batch(&mut l, &building(0), &actor(0), &mut w, &b).unwrap();
    assert_eq!(l.produced().quantity(&key("bread")), q(4.0));
    assert_eq!(w.practice_value("baking"), 33.26);
    let mut forged = b.clone();
    forged.equipment.insert("masterwork_oven".into());
    let before = (l.clone(), w.clone());
    assert_eq!(
        complete_batch(&mut l, &building(0), &actor(0), &mut w, &forged),
        Err(Error::WrongWorkplace)
    );
    assert_eq!((l.clone(), w.clone()), before);
    // The saved 5-wood boundary is exercised with a neutral tier-10 batch.
    let mut s = store(0, "bakery");
    s.inputs = goods(&[("flour", 203.5), ("wood", 4.999999999999972)]);
    let mut l = ledger(vec![s]);
    w.set_practice("baking", skill(180.0)).unwrap();
    let batch = complete_batch(&mut l, &building(0), &actor(0), &mut w, &b).unwrap();
    assert_eq!(batch.outputs.quantity(&key("bread")), q(20.0));
    assert_eq!(
        l.store(&building(0)).unwrap().inputs.quantity(&key("wood")),
        q(0.0)
    );
    let before = (l.clone(), w.clone());
    assert_eq!(
        complete_batch(&mut l, &building(0), &actor(0), &mut w, &b),
        Err(Error::Resource(ResourceError::InsufficientStock))
    );
    assert_eq!((l, w.clone()), before);
    let mut full = store(0, "bakery");
    full.inputs = goods(&[("flour", 20.0), ("wood", 5.0)]);
    full.output = goods(&[("bread", 40.0)]);
    let mut l = ledger(vec![full]);
    let before = (l.clone(), w.clone());
    assert_eq!(
        complete_batch(&mut l, &building(0), &actor(0), &mut w, &b),
        Err(Error::Resource(ResourceError::Capacity))
    );
    assert_eq!((l, w), before);
}
#[test]
fn grain_chain_matches_python_active_minute_rates_and_200_worker_food_demand() {
    for expected in fixture()["grain"].as_array().unwrap() {
        let upgraded = expected["upgraded"].as_bool().unwrap();
        let mut mill = store(0, "mill");
        mill.inputs = goods(&[("wheat", if upgraded { 320.0 } else { 40.0 })]);
        if upgraded {
            mill.policy.equipment.insert("masterwork_millstones".into());
        }
        let mut stores = vec![mill];
        for i in 0..if upgraded { 1 } else { 2 } {
            let mut b = store(i + 1, "bakery");
            b.inputs = goods(&[("wood", if upgraded { 80.0 } else { 5.0 })]);
            if upgraded {
                b.policy.equipment.insert("masterwork_oven".into());
            }
            stores.push(b);
        }
        let mut l = ledger(stores);
        let mut miller = Worker::new("milling").unwrap();
        miller.set_practice("farming", skill(10.0)).unwrap();
        let mut bakers = vec![Worker::new("baking").unwrap(); if upgraded { 1 } else { 2 }];
        for w in &mut bakers {
            w.set_practice("milling", skill(2.0)).unwrap();
        }
        let m = workplace_from_ledger(&l, &building(0), None, None).unwrap();
        for _ in 0..10 {
            for _ in 0..2 {
                complete_batch(&mut l, &building(0), &actor(0), &mut miller, &m).unwrap();
            }
            l.drop_off(&building(0), &actor(0)).unwrap();
            for (i, w) in bakers.iter_mut().enumerate() {
                let id = building(i as u64 + 1);
                let b = workplace_from_ledger(&l, &id, None, None).unwrap();
                let costs = evaluate_batch(w, &b, &Stock::default()).unwrap().costs;
                l.reserve(
                    &building(0),
                    &id,
                    &goods(&[("flour", costs.quantity(&key("flour")).get())]),
                )
                .unwrap();
                complete_batch(&mut l, &id, &actor(i as u64 + 1), w, &b).unwrap();
                l.drop_off(&id, &actor(i as u64 + 1)).unwrap();
            }
        }
        same_stock(l.produced(), &expected["produced"]);
        near(
            l.food().unwrap().get(),
            expected["food_before"].as_f64().unwrap(),
        );
        let count = expected["working"].as_u64().unwrap() as usize;
        let meal = food_demand(count, count).unwrap();
        near(meal.get(), if upgraded { 300.0 } else { 60.0 });
        l.eat(meal).unwrap();
        near(
            l.food().unwrap().get(),
            expected["food_after"].as_f64().unwrap(),
        );
        for (w, skills) in std::iter::once(&miller)
            .chain(&bakers)
            .zip(expected["skills"].as_array().unwrap())
        {
            near(w.practice_value(w.job()), skills[w.job()].as_f64().unwrap());
        }
        assert_eq!(l.free(&key("flour")).unwrap(), q(0.0));
    }
}
#[test]
fn imported_goods_cannot_satisfy_production_and_construction_evidence_uses_personal_maxima() {
    let mut s = store(0, "storehouse");
    s.policy.level = 8;
    s.policy.enabled = Some(BTreeMap::from([
        ("tools".into(), true),
        ("bread".into(), true),
    ]));
    let mut l = ledger(vec![s]);
    l.deliver(&key("tools"), q(5.0), false).unwrap();
    l.deliver(&key("bread"), q(10.0), false).unwrap();
    let context = HallContext::default();
    let e = HallEvidence::from_ledger(&l, 0, q(0.0)).unwrap();
    assert!(!hall_milestone(5, &e, &context));
    let mut worker = Worker::new("baking").unwrap();
    worker.set_practice("baking", skill(5.0)).unwrap();
    let mut player = Worker::new("idle").unwrap();
    player.set_practice("baking", skill(10.0)).unwrap();
    let evidence = construction_evidence(&l, &[worker.clone()], Some(&player), 1, q(2.0)).unwrap();
    assert_eq!(evidence.skills["baking"], q(10.0));
    assert!(evidence.produced.is_empty());
    assert_eq!(evidence.food, Some(q(15.0)));
    assert_eq!(evidence.completed_trades, 1);
    assert_eq!(evidence.market_sales, Some(q(2.0)));
    assert_eq!(
        choose_recipe("bakery", "herb_bread", false, &[worker]),
        Err(Error::NoQualifiedWorker)
    );
}
#[test]
fn invalid_facts_and_population_are_rejected_without_mutation() {
    assert_eq!(Worker::new("unknown"), Err(Error::UnknownJob));
    let mut w = Worker::new("farming").unwrap();
    let before = w.clone();
    for n in [f64::NAN, f64::INFINITY, -4.001] {
        assert_eq!(
            w.set_talents(BTreeMap::from([("farming".into(), n)])),
            Err(Error::InvalidTalent)
        );
        assert_eq!(w, before);
    }
    assert_eq!(
        w.set_practice("unknown", skill(1.0)),
        Err(Error::UnknownJob)
    );
    assert_eq!(w, before);
    assert_eq!(food_demand(1, 2), Err(Error::InvalidPopulation));
    near(food_demand(2, 1).unwrap().get(), 2.1);
    w.set_talents(BTreeMap::from([("farming".into(), f64::MAX)]))
        .unwrap();
    let before = w.clone();
    assert_eq!(
        w.practice("farming", Seconds::new(f64::MAX).unwrap()),
        Err(Error::Overflow)
    );
    assert_eq!(w, before);
}

#[test]
fn python_staffing_limits_stay_independent_from_grain_equipment_and_storage() {
    for case in fixture()["staffing"].as_array().unwrap() {
        assert_eq!(
            worker_capacity(
                case["kind"].as_str().unwrap(),
                case["tier"].as_u64().unwrap() as u8
            )
            .unwrap(),
            case["capacity"].as_u64().unwrap()
        );
    }
    for kind in ["mill", "bakery"] {
        assert_eq!(worker_capacity(kind, 3).unwrap(), 1);
    }
    assert!(worker_capacity("mill", 0).is_err());
    assert!(worker_capacity("unknown", 1).is_err());
}
