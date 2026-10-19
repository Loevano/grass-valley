use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, Owner, TownId, WorldId},
    model::*,
    units::*,
};
use lantern_host::{
    building::{Site, Upgrade},
    logistics::LogisticsTrips,
    population::*,
    progression::Worker,
    resources::*,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn owner(town: &str) -> Owner {
    Owner {
        world: WorldId::new("population").unwrap(),
        town: TownId::new(town).unwrap(),
    }
}
fn a(n: u64) -> ActorId {
    ActorId::from_saved(EntityIdentity::from_saved(owner("a"), n).unwrap())
}
fn b(n: u64) -> BuildingId {
    BuildingId::from_saved(EntityIdentity::from_saved(owner("a"), n).unwrap())
}
fn q(n: f64) -> Quantity {
    Quantity::new(n).unwrap()
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}
struct Fixture {
    town: Town,
    ledger: Ledger,
    population: Population,
    places: Vec<Place>,
    trips: LogisticsTrips,
    next: u64,
}
impl Fixture {
    fn new(v: Value) -> Self {
        let places: Vec<_> = v["buildings"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut site = Site::new(p["kind"].as_str().unwrap(), (1, 1));
                site.level = p["level"].as_u64().unwrap_or(1) as u8;
                site.capacity_level = p["settler_level"].as_u64().unwrap_or(1) as u8;
                if let Some(items) = p["profession_upgrades"].as_array() {
                    site.equipment = items.iter().map(|s| s.as_str().unwrap().into()).collect();
                }
                if p["construction"].get("upgrade").is_some() {
                    site.upgrade = Some(Upgrade::Structure(3));
                }
                Place {
                    id: b(i as u64 + 1),
                    site,
                }
            })
            .collect();
        let villagers = v["villagers"].as_array().unwrap();
        let mut actors: Vec<_> = (0..villagers.len())
            .map(|i| {
                ActorState::new(
                    a(100 + i as u64),
                    Position::new(4., 5.).unwrap(),
                    Cargo::default(),
                )
            })
            .collect();
        // A founder is an actor but never a resident or an eligible departure.
        actors.push(ActorState::new(
            a(900),
            Position::new(10., 11.).unwrap(),
            Cargo::default(),
        ));
        let town = Town::restore(
            owner("a"),
            places
                .iter()
                .enumerate()
                .map(|(i, p)| Building {
                    id: p.id.clone(),
                    location: Tile { x: i as i32, y: 1 },
                    construction: if v["buildings"][i].get("construction").is_some() {
                        ConstructionState::InProgress {
                            remaining: Work::new(30.).unwrap(),
                        }
                    } else {
                        ConstructionState::Complete
                    },
                })
                .collect(),
            actors,
        )
        .unwrap();
        let stores = places
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let mut policy = StoragePolicy::new(p.site.kind.clone());
                policy.level = p.site.level;
                policy.construction = !matches!(
                    town.building(&p.id).unwrap().construction,
                    ConstructionState::Complete
                );
                let mut store = Store::new(p.id.clone(), policy);
                if i == 0 {
                    store.provisions = q(v["food"].as_f64().unwrap_or(0.));
                }
                store
            })
            .collect();
        let ledger = Ledger::new(owner("a"), q(0.), stores, vec![]).unwrap();
        let residents = villagers
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let mut worker = Worker::new(r["job"].as_str().unwrap_or("idle")).unwrap();
                if let Some(m) = r["talents"].as_object() {
                    worker
                        .set_talents(
                            m.iter()
                                .map(|(k, v)| (k.clone(), v.as_f64().unwrap()))
                                .collect(),
                        )
                        .unwrap();
                }
                for job in ["woodcutting", "farming", "baking", "milling"] {
                    if let Some(n) = r[job].as_f64() {
                        worker.set_practice(job, Practice::new(n).unwrap()).unwrap();
                    }
                }
                worker.tool = r["worker_tool"].as_str().map(str::to_string);
                Resident {
                    actor: a(100 + i as u64),
                    name: r["name"].as_str().unwrap_or("").into(),
                    worker,
                    assignment: r["building_index"].as_u64().map(|n| b(n + 1)),
                }
            })
            .collect();
        let history = History {
            food_shortage_seconds: v["food_shortage_seconds"].as_f64().unwrap_or(0.),
            housing_shortage_seconds: v["housing_shortage_seconds"].as_f64().unwrap_or(0.),
            arrival_delay_seconds: v["arrival_delay_seconds"].as_f64().unwrap_or(0.),
            departure_count: 0,
            last_departure: None,
        };
        let population = Population::restore(
            &town,
            v["sandbox"].as_bool().unwrap_or(false),
            residents,
            history,
        )
        .unwrap();
        Self {
            town,
            ledger,
            population,
            places,
            trips: Default::default(),
            next: 1000,
        }
    }
    fn simple() -> Self {
        Self::new(
            json!({"buildings":[{"kind":"hall"},{"kind":"home"},{"kind":"farm"},{"kind":"farm"},{"kind":"bakery"}],"villagers":[{"name":"Ada"},{"name":"Rowan"},{"name":"Wren"},{"name":"Bram"},{"name":"Hazel"}],"food":0}),
        )
    }
    fn tick(&mut self, roll: usize) {
        let next = &mut self.next;
        self.population
            .economic_tick(
                &mut self.town,
                &mut self.ledger,
                &mut self.trips,
                &self.places,
                &mut |n| if n == 10000 { roll } else { 0 },
                &mut || {
                    let id = a(*next);
                    *next += 1;
                    id
                },
            )
            .unwrap();
    }
    fn command(&mut self, c: Command) -> Result<(), Error> {
        self.population.command(
            &owner("a"),
            &mut self.town,
            &mut self.ledger,
            &mut self.trips,
            &self.places,
            c,
        )
    }
}
#[test]
fn paired_python_population_boundaries() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/population/python.json")).unwrap();
    for c in cases.as_array().unwrap() {
        let mut f = Fixture::new(c["before"].clone());
        f.tick(c["roll"].as_u64().unwrap() as usize);
        let after = &c["after"];
        let expected = after["villagers"].as_array().unwrap();
        assert_eq!(
            f.population.residents().len(),
            expected.len(),
            "{}",
            c["name"]
        );
        for (r, e) in f.population.residents().iter().zip(expected) {
            assert_eq!(r.name, e["name"]);
            assert_eq!(r.worker.job(), e["job"]);
            assert_eq!(
                r.assignment.as_ref().map(|b| b.identity().serial() - 1),
                e["building_index"].as_u64()
            );
            let talents: BTreeMap<String, f64> =
                serde_json::from_value(e["talents"].clone()).unwrap();
            assert_eq!(r.worker.talents().unwrap(), &talents);
        }
        close(
            f.ledger.food().unwrap().get(),
            after["food"].as_f64().unwrap(),
        );
        let h = serde_json::to_value(f.population.history()).unwrap();
        for key in [
            "food_shortage_seconds",
            "housing_shortage_seconds",
            "arrival_delay_seconds",
            "departure_count",
        ] {
            close(h[key].as_f64().unwrap(), after[key].as_f64().unwrap_or(0.));
        }
        assert_eq!(h["last_departure"], after["last_departure"]);
        assert_eq!(
            f.population.housing(&f.town, &f.places).unwrap() as u64,
            c["housing"].as_u64().unwrap()
        );
        close(
            f.population.loyalty(&f.town, &f.places),
            c["loyalty"].as_f64().unwrap(),
        );
        assert_eq!(
            f.town.actor(&a(900)).unwrap().state().position,
            Position::new(10., 11.).unwrap()
        );
    }
}
#[test]
fn assignment_capacity_and_preferred_current_then_saved_order() {
    let mut f = Fixture::simple();
    f.command(Command::Job {
        actor: a(100),
        job: "farming".into(),
    })
    .unwrap();
    assert_eq!(f.population.residents()[0].assignment, Some(b(3)));
    f.command(Command::Job {
        actor: a(101),
        job: "farming".into(),
    })
    .unwrap();
    assert_eq!(f.population.residents()[1].assignment, Some(b(4)));
    f.command(Command::Job {
        actor: a(101),
        job: "farming".into(),
    })
    .unwrap();
    assert_eq!(f.population.residents()[1].assignment, Some(b(4)));
    let before = f.population.clone();
    assert_eq!(
        f.command(Command::Assign {
            actor: a(101),
            building: b(3)
        }),
        Err(Error::Full)
    );
    assert_eq!(before, f.population);
    f.command(Command::Remove(a(100))).unwrap();
    assert_eq!(f.population.residents().len(), 5);
    assert_eq!(f.population.residents()[0].worker.job(), "idle");
    f.command(Command::Job {
        actor: a(100),
        job: "building".into(),
    })
    .unwrap();
    f.tick(9999);
    assert_eq!(f.population.residents()[0].worker.job(), "building");
    assert!(f.population.residents()[0].assignment.is_none());
}
#[test]
fn qualification_founders_and_ownership_reject_without_mutation() {
    let mut f = Fixture::simple();
    let before = f.population.clone();
    let goods = f.ledger.clone();
    assert_eq!(
        f.command(Command::Assign {
            actor: a(100),
            building: b(5)
        }),
        Err(Error::Qualification)
    );
    assert_eq!(
        f.command(Command::Remove(a(900))),
        Err(Error::MissingResident)
    );
    assert_eq!(
        f.command(Command::Assign {
            actor: a(100),
            building: b(1)
        }),
        Err(Error::Full)
    );
    assert_eq!(
        f.command(Command::Job {
            actor: a(100),
            job: "unknown".into()
        }),
        Err(Error::Qualification)
    );
    assert_eq!(
        f.population.command(
            &owner("other"),
            &mut f.town,
            &mut f.ledger,
            &mut f.trips,
            &f.places,
            Command::Remove(a(100))
        ),
        Err(Error::Ownership)
    );
    assert_eq!(f.population, before);
    assert_eq!(goods, f.ledger);
}
#[test]
fn reassignment_orphans_output_returns_cargo_and_preserves_position() {
    let mut f = Fixture::simple();
    let key = ResourceKey::parse("wood").unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores[2].pending.insert(
        PendingOwner::Worker(a(100)),
        Stock::from_entries([(key.clone(), q(3.))]).unwrap(),
    );
    f.ledger = Ledger::new(
        owner("a"),
        q(0.),
        stores,
        vec![HeldCargo {
            actor: a(100),
            source: b(3),
            kind: CargoKind::Supply,
            goods: Stock::from_entries([(key.clone(), q(2.))]).unwrap(),
        }],
    )
    .unwrap();
    let position = f.town.actor(&a(100)).unwrap().state().position;
    f.command(Command::Assign {
        actor: a(100),
        building: b(2),
    })
    .unwrap();
    assert_eq!(f.town.actor(&a(100)).unwrap().state().position, position);
    assert!(f.ledger.held().is_empty());
    close(
        f.ledger.store(&b(3)).unwrap().stored.quantity(&key).get(),
        2.,
    );
    assert!(
        f.ledger
            .store(&b(3))
            .unwrap()
            .pending
            .contains_key(&PendingOwner::Orphan)
    );
    let before = f.ledger.clone();
    f.command(Command::Remove(a(100))).unwrap();
    assert_eq!(before, f.ledger);
}
#[test]
fn import_names_and_talents_survive_reorder_and_reload_without_ticks() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Finn","job":"farming","farming":25,"talents":{}},{"name":"Settler 6"},{"name":" "}],"food":100}),
    );
    assert_eq!(f.population.residents()[0].name, "Finn");
    assert_eq!(f.population.residents()[1].name, "Ada");
    assert_eq!(f.population.residents()[2].name, "Rowan");
    assert_eq!(
        f.population.residents()[0].worker.talents(),
        Some(&BTreeMap::new())
    );
    let saved = f.population.encode(&f.town).unwrap();
    let mut states: Vec<_> = f.town.actors().iter().map(|a| a.state().clone()).collect();
    states.reverse();
    f.town = Town::restore(owner("a"), f.town.buildings().to_vec(), states).unwrap();
    for _ in 0..3 {
        let restored = Population::decode(&saved, &f.town).unwrap();
        assert_eq!(restored, f.population);
    }
    close(f.ledger.food().unwrap().get(), 100.);
    let foreign = Town::restore(owner("other"), vec![], vec![]).unwrap();
    assert_eq!(Population::decode(&saved, &foreign), Err(Error::Ownership));
    let absent = Town::restore(owner("a"), vec![], vec![]).unwrap();
    assert!(Population::decode(&saved, &absent).is_err());
    let mut corrupt: Value = serde_json::from_str(&saved).unwrap();
    corrupt["residents"][1]["actor"] = json!(100);
    assert!(Population::decode(&corrupt.to_string(), &f.town).is_err());
}
#[test]
fn departure_returns_tool_once_keeps_specialist_history_and_identity() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"},{"kind":"bakery"}],"villagers":[{"name":"Ada","job":"baking","building_index":1,"baking":80,"milling":20,"worker_tool":"fine_axe"},{"name":"Rowan"}],"food":0,"food_shortage_seconds":57}),
    );
    f.tick(0);
    assert!(f.town.actor(&a(100)).is_err());
    assert!(f.town.actor(&a(101)).is_ok());
    assert_eq!(f.population.returned_tools()["fine_axe"], 1);
    assert_eq!(
        f.population
            .history()
            .last_departure
            .as_ref()
            .unwrap()
            .practice["baking"],
        80.
    );
    assert_eq!(f.population.history().arrival_delay_seconds, 60.);
    let save = f.population.encode(&f.town).unwrap();
    f.population = Population::decode(&save, &f.town).unwrap();
    f.tick(0);
    assert_eq!(f.population.returned_tools()["fine_axe"], 1);
}
#[test]
fn failed_spawn_and_random_draw_are_atomic() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Ada"}],"food":100}),
    );
    let before = f.population.clone();
    let goods = f.ledger.clone();
    let actors = f.town.actors().len();
    assert!(
        f.population
            .economic_tick(
                &mut f.town,
                &mut f.ledger,
                &mut f.trips,
                &f.places,
                &mut |_| panic!("no draw"),
                &mut || a(900)
            )
            .is_err()
    );
    assert_eq!(before, f.population);
    assert_eq!(goods, f.ledger);
    assert_eq!(actors, f.town.actors().len());
}
#[test]
fn unfinished_place_rejects_assignment_and_missing_hall_blocks_tick() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"},{"kind":"home","construction":{}}],"villagers":[{"name":"Ada"}],"food":0}),
    );
    assert_eq!(
        f.command(Command::Assign {
            actor: a(100),
            building: b(2)
        }),
        Err(Error::Unfinished)
    );
    let mut absent = Fixture::new(json!({"buildings":[],"villagers":[],"food":0}));
    assert_eq!(
        absent.population.economic_tick(
            &mut absent.town,
            &mut absent.ledger,
            &mut absent.trips,
            &[],
            &mut |_| panic!("draw"),
            &mut || panic!("spawn")
        ),
        Err(Error::MissingPlace)
    );
}

#[test]
fn legacy_implicit_workplaces_keep_list_order_and_explicit_overcapacity() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"},{"kind":"farm"},{"kind":"farm"},{"kind":"woodcutter"},{"kind":"bakery"}],"villagers":[{"name":"Ada","job":"farming"},{"name":"Rowan","job":"farming"},{"name":"Wren","job":"farming","building_index":1},{"name":"Bram","job":"woodcutting"},{"name":"Hazel","job":"baking"}],"food":0}),
    );
    let resources = vec![lantern_core::spatial::Resource {
        kind: "tree".into(),
        footprint: lantern_core::spatial::Footprint::saved(Tile { x: 3, y: 2 }, None).unwrap(),
    }];
    f.population
        .resolve_legacy_assignments(&f.town, &f.places, &f.ledger, &resources)
        .unwrap();
    let assignments: Vec<_> = f
        .population
        .residents()
        .iter()
        .map(|r| r.assignment.clone())
        .collect();
    assert_eq!(
        assignments,
        vec![Some(b(2)), Some(b(3)), Some(b(2)), Some(b(4)), None]
    );
    let save = f.population.encode(&f.town).unwrap();
    f.population
        .resolve_legacy_assignments(&f.town, &f.places, &f.ledger, &[])
        .unwrap();
    assert_eq!(save, f.population.encode(&f.town).unwrap());
}
#[test]
fn delayed_replacement_keeps_survivor_identity_and_avoids_last_name() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Ada","woodcutting":100},{"name":"Rowan"}],"food":0,"food_shortage_seconds":57}),
    );
    f.tick(0);
    let mut stores = f.ledger.stores().to_vec();
    stores[0].provisions = q(100.);
    f.ledger = Ledger::new(owner("a"), q(0.), stores, vec![]).unwrap();
    for _ in 0..19 {
        f.tick(9999);
    }
    assert_eq!(f.population.residents().len(), 1);
    let save = f.population.encode(&f.town).unwrap();
    f.population = Population::decode(&save, &f.town).unwrap();
    f.tick(9999);
    assert_eq!(f.population.residents().len(), 2);
    assert_eq!(f.population.residents()[0].actor, a(101));
    let new = &f.population.residents()[1];
    assert_ne!(new.actor, a(100));
    assert_ne!(new.name, "Ada");
    close(new.worker.practice_value("woodcutting"), 0.);
}
#[test]
fn sandbox_qualifies_and_tool_restitution_is_drained_once() {
    let mut f = Fixture::new(
        json!({"sandbox":true,"buildings":[{"kind":"hall"},{"kind":"bakery"}],"villagers":[{"name":"Ada"}],"food":0}),
    );
    f.command(Command::Assign {
        actor: a(100),
        building: b(2),
    })
    .unwrap();
    f.tick(0);
    assert_eq!(f.population.residents()[0].worker.job(), "baking");
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Ada","worker_tool":"crafted_axe"},{"name":"Rowan"}],"food":0,"food_shortage_seconds":57}),
    );
    f.tick(0);
    assert_eq!(
        f.population.take_returned_tools(&owner("other")),
        Err(Error::Ownership)
    );
    assert_eq!(
        f.population.take_returned_tools(&owner("a")).unwrap()["crafted_axe"],
        1
    );
    let save = f.population.encode(&f.town).unwrap();
    f.population = Population::decode(&save, &f.town).unwrap();
    assert!(
        f.population
            .take_returned_tools(&owner("a"))
            .unwrap()
            .is_empty()
    );
}
#[test]
fn working_and_idle_construction_helpers_eat_without_advancing_other_clocks() {
    let mut f = Fixture::simple();
    f.population
        .worker_mut(&a(100))
        .unwrap()
        .set_practice("milling", Practice::new(2.).unwrap())
        .unwrap();
    f.command(Command::Assign {
        actor: a(100),
        building: b(5),
    })
    .unwrap();
    f.command(Command::Assign {
        actor: a(101),
        building: b(2),
    })
    .unwrap();
    f.town
        .set_task(
            &a(101),
            Task::Construction {
                site: b(2),
                phase: ConstructionPhase::Building,
                elapsed: Seconds::new(1.).unwrap(),
            },
        )
        .unwrap();
    f.command(Command::Assign {
        actor: a(102),
        building: b(2),
    })
    .unwrap();
    f.town
        .set_task(
            &a(102),
            Task::RoadConstruction {
                tile: Tile { x: 8, y: 8 },
                phase: ConstructionPhase::Building,
            },
        )
        .unwrap();
    let mut stores = f.ledger.stores().to_vec();
    stores[0].provisions = q(10.);
    f.ledger = Ledger::new(owner("a"), q(0.), stores, vec![]).unwrap();
    f.tick(9999);
    close(f.ledger.food().unwrap().get(), 10. - 5. * 0.03 - 3. * 0.045);
    close(f.ledger.activity().seconds(), 0.);
    assert!(
        matches!(f.town.actor(&a(101)).unwrap().state().task,Task::Construction{elapsed,..} if elapsed.get()==1.)
    );
}
#[test]
fn random_failure_and_world_switch_are_isolated() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Ada"},{"name":"Rowan"}],"food":0,"food_shortage_seconds":57}),
    );
    let before = f.population.clone();
    let goods = f.ledger.clone();
    assert_eq!(
        f.population.economic_tick(
            &mut f.town,
            &mut f.ledger,
            &mut f.trips,
            &f.places,
            &mut |_| 10000,
            &mut || panic!("spawn")
        ),
        Err(Error::Invalid)
    );
    assert_eq!(before, f.population);
    assert_eq!(goods, f.ledger);
    let save = f.population.encode(&f.town).unwrap();
    let other = Town::restore(
        Owner {
            world: WorldId::new("other-world").unwrap(),
            town: owner("a").town,
        },
        vec![],
        vec![],
    )
    .unwrap();
    assert_eq!(Population::decode(&save, &other), Err(Error::Ownership));
    assert_eq!(f.population, before);
}

#[test]
fn unicode_placeholders_do_not_rename_real_numeric_names() {
    let f = Fixture::new(
        json!({"buildings":[{"kind":"hall"}],"villagers":[{"name":"Settler ⅓"},{"name":"Settler ²"},{"name":"Settler ١"}],"food":0}),
    );
    assert_eq!(f.population.residents()[0].name, "Settler ⅓");
    assert_eq!(f.population.residents()[1].name, "Ada");
    assert_eq!(f.population.residents()[2].name, "Rowan");
}
#[test]
fn independent_towns_and_founders_keep_their_own_resident_clocks() {
    let mut first = Fixture::simple();
    let other_owner = owner("b");
    let identity = |n| EntityIdentity::from_saved(other_owner.clone(), n).unwrap();
    let buildings: Vec<_> = first
        .town
        .buildings()
        .iter()
        .map(|b| Building {
            id: BuildingId::from_saved(identity(b.id.identity().serial())),
            location: b.location,
            construction: b.construction.clone(),
        })
        .collect();
    let actors = first
        .town
        .actors()
        .iter()
        .map(|a| {
            let mut s = a.state().clone();
            s.id = ActorId::from_saved(identity(s.id.identity().serial()));
            s
        })
        .collect();
    let mut town = Town::restore(other_owner.clone(), buildings, actors).unwrap();
    let residents = first
        .population
        .residents()
        .iter()
        .map(|r| Resident {
            actor: ActorId::from_saved(identity(r.actor.identity().serial())),
            name: r.name.clone(),
            worker: r.worker.clone(),
            assignment: None,
        })
        .collect();
    let second = Population::restore(&town, false, residents, History::default()).unwrap();
    let second_save = second.encode(&town).unwrap();
    first.tick(9999);
    assert_eq!(second.encode(&town).unwrap(), second_save);
    assert_eq!(first.population.history().food_shortage_seconds, 3.);
    let foreign_actor = ActorId::from_saved(identity(100));
    assert_eq!(
        first.command(Command::Remove(foreign_actor)),
        Err(Error::Ownership)
    );
    let before = second.clone();
    let mut second = second;
    assert_eq!(
        second.command(
            &owner("a"),
            &mut town,
            &mut first.ledger,
            &mut first.trips,
            &first.places,
            Command::Remove(a(100))
        ),
        Err(Error::Ownership)
    );
    assert_eq!(second, before);
}

#[test]
fn farming_job_selects_a_farm_while_explicit_assignment_accepts_an_acre() {
    let mut f = Fixture::new(
        json!({"buildings":[{"kind":"hall"},{"kind":"acre"},{"kind":"farm"}],"villagers":[{"name":"Ada"}],"food":0}),
    );
    f.command(Command::Job {
        actor: a(100),
        job: "farming".into(),
    })
    .unwrap();
    assert_eq!(f.population.residents()[0].assignment, Some(b(3)));
    f.command(Command::Assign {
        actor: a(100),
        building: b(2),
    })
    .unwrap();
    assert_eq!(f.population.residents()[0].assignment, Some(b(2)));
}
