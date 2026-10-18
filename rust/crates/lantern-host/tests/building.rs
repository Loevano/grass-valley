use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, IdAllocator, Owner, TownId, WorldId},
    model::*,
    spatial::{Footprint, Resource},
    units::*,
};
use lantern_host::{
    building::*,
    state::{ModelWorld, PrivateCredentials},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn tile(x: i32, y: i32) -> Tile {
    Tile { x, y }
}
fn p(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn q(x: f64) -> Quantity {
    Quantity::new(x).unwrap()
}
fn pair(v: &Value) -> Tile {
    tile(v[0].as_i64().unwrap() as i32, v[1].as_i64().unwrap() as i32)
}
fn pos(v: &Value) -> Position {
    p(v[0].as_f64().unwrap(), v[1].as_f64().unwrap())
}
fn goods(v: &Value) -> BTreeMap<String, Quantity> {
    v.as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| (k.clone(), q(v.as_f64().unwrap())))
                .collect()
        })
        .unwrap_or_default()
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/building/python.json")).unwrap()
}
fn owner() -> Owner {
    Owner {
        world: WorldId::new("building-tests").unwrap(),
        town: TownId::new("own").unwrap(),
    }
}
fn source_world(source: &Value, actors: Vec<ActorState>) -> BuildingWorld {
    let owner = owner();
    let mut ids = IdAllocator::new(owner.world.clone());
    let mut sites = BTreeMap::new();
    let buildings = source["buildings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| {
            let id = ids.allocate(&owner).unwrap();
            let dimensions = b
                .get("footprint")
                .map(pair)
                .map(|p| (p.x as u32, p.y as u32))
                .unwrap_or((1, 1));
            let mut site = Site::new(b["kind"].as_str().unwrap(), dimensions);
            site.orientation = b["orientation"].as_u64().unwrap_or(0) as u8;
            site.fence_orientation = b["fence_orientation"].as_u64().unwrap_or(0) as u8;
            site.stored = goods(&b["stored_goods"]);
            site.pending = goods(&b["pending_output"]);
            site.level = b["level"].as_u64().unwrap_or(1) as u8;
            site.capacity_level = b["settler_level"].as_u64().unwrap_or(1) as u8;
            site.equipment = b["profession_upgrades"]
                .as_array()
                .map(|v| v.iter().map(|v| v.as_str().unwrap().to_string()).collect())
                .unwrap_or_default();
            sites.insert(id.clone(), site);
            Building {
                id,
                location: tile(
                    b["x"].as_i64().unwrap() as i32,
                    b["y"].as_i64().unwrap() as i32,
                ),
                construction: b
                    .get("construction")
                    .map(|c| ConstructionState::InProgress {
                        remaining: Work::new(
                            c["total"].as_f64().unwrap() - c["progress"].as_f64().unwrap_or(0.),
                        )
                        .unwrap(),
                    })
                    .unwrap_or(ConstructionState::Complete),
            }
        })
        .collect();
    let high = actors
        .iter()
        .map(|a| a.id.identity().serial())
        .max()
        .unwrap_or(0)
        .max(ids.high_water());
    let town = Town::restore(owner.clone(), buildings, actors).unwrap();
    let model = ModelWorld::new(
        owner.world.clone(),
        TerrainSeed(1),
        high,
        vec![town],
        PrivateCredentials::default(),
    )
    .unwrap();
    let rules = Rules {
        progression: Progression {
            skills: goods(&source["skills"]),
            produced: goods(&source["produced"]),
            food: source["food"].as_f64().map(q),
            completed_trades: source["completed_trades"].as_u64().unwrap_or(0),
            market_sales: source["market_sales"].as_f64().map(q),
        },
        level: source["level"].as_u64().unwrap() as u8,
        sandbox: source["sandbox"].as_bool().unwrap(),
        coins: q(source["coins"].as_f64().unwrap()),
        origin: pair(&source["origin"]),
        player: pos(&source["player"]),
        sites,
        roads: source["roads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| Road {
                tile: tile(
                    r["x"].as_i64().unwrap() as i32,
                    r["y"].as_i64().unwrap() as i32,
                ),
                remaining: r.get("construction").map(|c| {
                    Work::new(c["total"].as_f64().unwrap() - c["progress"].as_f64().unwrap_or(0.))
                        .unwrap()
                }),
            })
            .collect(),
    };
    let landscape = Landscape {
        size: 120,
        unavailable: source["unavailable"]
            .as_array()
            .unwrap()
            .iter()
            .map(pair)
            .collect(),
        resources: source["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| Resource {
                kind: r["kind"].as_str().unwrap().into(),
                footprint: Footprint::saved(
                    tile(
                        r["x"].as_i64().unwrap() as i32,
                        r["y"].as_i64().unwrap() as i32,
                    ),
                    None,
                )
                .unwrap(),
            })
            .collect(),
        locals: source["locals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                Footprint::saved(
                    tile(
                        r["x"].as_i64().unwrap() as i32,
                        r["y"].as_i64().unwrap() as i32,
                    ),
                    None,
                )
                .unwrap()
            })
            .collect(),
        local_positions: vec![],
    };
    BuildingWorld::restore(model, [(owner.town, rules)].into(), landscape, false).unwrap()
}
fn command(world: &BuildingWorld, value: &Value) -> Command {
    let orientation = value["orientation"].as_u64().unwrap_or(0) as u8;
    let location = || {
        tile(
            value["x"].as_i64().unwrap() as i32,
            value["y"].as_i64().unwrap() as i32,
        )
    };
    let id = || {
        world.model().towns()[0].buildings()[value["index"].as_u64().unwrap() as usize]
            .id
            .clone()
    };
    match value["action"].as_str().unwrap() {
        "build" => Command::Build {
            kind: value["kind"].as_str().unwrap().into(),
            tile: location(),
            orientation,
        },
        "move" => Command::Move {
            id: id(),
            tile: location(),
        },
        "destroy" => Command::Destroy { id: id() },
        "set_building_orientation" | "set_decoration_orientation" => Command::Rotate {
            id: id(),
            orientation,
            decoration: value["action"] == "set_decoration_orientation",
        },
        "set_fence_orientation" => Command::RotateFence {
            id: id(),
            orientation,
        },
        "build_road" => Command::BuildRoad(location()),
        "remove_road" => Command::RemoveRoad(location()),
        _ => panic!("unsupported vector"),
    }
}
fn values(goods: &BTreeMap<String, Quantity>) -> Value {
    json!(
        goods
            .iter()
            .map(|(k, v)| (k, v.get()))
            .collect::<BTreeMap<_, _>>()
    )
}
fn snapshot(w: &BuildingWorld) -> Value {
    let town = &w.model().towns()[0];
    let r = &w.rules()[&town.owner().town];
    json!({"coins":r.coins.get(),"buildings":town.buildings().iter().map(|b| {
        let s=&r.sites[&b.id];
        json!({"kind":s.kind,"tile":[b.location.x,b.location.y],"dimensions":[s.dimensions.0,s.dimensions.1],
            "orientation":s.orientation,"fence_orientation":s.fence_orientation,
            "remaining":match b.construction {ConstructionState::Complete=>None,ConstructionState::InProgress{remaining}=>Some(remaining.get())},
            "stored":values(&s.stored),"pending":values(&s.pending)})
    }).collect::<Vec<_>>(),"roads":r.roads.iter().map(|r| json!({"tile":[r.tile.x,r.tile.y],"remaining":r.remaining.map(|w| w.get())})).collect::<Vec<_>>(),
        "resources":w.landscape().resources.iter().map(|r| json!({"kind":r.kind,"x":r.footprint.origin().x,"y":r.footprint.origin().y})).collect::<Vec<_>>()})
}
// JSON integer and binary64 representations intentionally compare numerically.
fn equal_numbers(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64(), "{path}"),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                equal_numbers(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (k, b) in b {
                equal_numbers(&a[k], b, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
#[test]
fn paired_python_building_commands_and_catalog() {
    let vectors = fixture();
    let catalog_value: Value =
        serde_json::from_str(include_str!("../src/building_catalog.json")).unwrap();
    assert_eq!(catalog_value, vectors["catalog"]);
    for case in vectors["cases"].as_array().unwrap() {
        let mut world = source_world(&case["source"], vec![]);
        for (index, (input, outcome)) in case["commands"]
            .as_array()
            .unwrap()
            .iter()
            .zip(case["outcomes"].as_array().unwrap())
            .enumerate()
        {
            let cmd = command(&world, input);
            let before = snapshot(&world);
            let high = world.model().high_water();
            let result = world.execute(&owner(), cmd);
            assert_eq!(
                result.is_ok(),
                outcome["ok"].as_bool().unwrap(),
                "{} command {index}: {input} {result:?}",
                case["name"]
            );
            if result.is_err() {
                assert_eq!(snapshot(&world), before);
                assert_eq!(world.model().high_water(), high);
            }
            equal_numbers(
                &snapshot(&world),
                &outcome["state"],
                &format!("{}[{index}]", case["name"]),
            );
            // Rebuild from explicit authoritative facts between every command.
            // Generic v2 saves cannot carry these construction-specific records.
            let (model, rules, landscape, shared, builders) = world.into_parts();
            world = BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders)
                .unwrap();
        }
    }
}
fn worker(serial: u64) -> ActorState {
    ActorState::new(
        ActorId::from_saved(EntityIdentity::from_saved(owner(), serial).unwrap()),
        p(35.25, 28.5),
        Cargo::from_entries([("wood".into(), q(2.125))]).unwrap(),
    )
}
fn with_actors(states: Vec<ActorState>) -> BuildingWorld {
    let mut source = fixture()["cases"][0]["source"].clone();
    source["buildings"].as_array_mut().unwrap().push(json!({"kind":"woodcutter","x":40,"y":30,"stored_goods":{"wood":1.25},"pending_output":{"planks":0.375}}));
    source_world(&source, states)
}
fn replace_town(world: BuildingWorld, town: Town) -> BuildingWorld {
    let (model, rules, landscape, shared, builders) = world.into_parts();
    let model = ModelWorld::new(
        model.id().clone(),
        model.seed(),
        model.high_water(),
        vec![town],
        PrivateCredentials::default(),
    )
    .unwrap();
    if builders.is_empty() {
        // R11 generic task fixtures do not imply R12 builder enrolment.
        BuildingWorld::restore(model, rules, landscape, shared).unwrap()
    } else {
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap()
    }
}
#[test]
fn move_during_return_travel_then_demolition_preserves_actor_and_salvages_once() {
    let actor = worker(10);
    let actor_id = actor.id.clone();
    let mut world = with_actors(vec![actor]);
    let mut town = world.model().towns()[0].clone();
    let id = town.buildings()[1].id.clone();
    town.set_task(
        &actor_id,
        Task::Work {
            workplace: id.clone(),
            target: Destination::Building(id.clone()),
            phase: WorkPhase::Returning,
            elapsed: Seconds::new(0.375).unwrap(),
        },
    )
    .unwrap();
    let stale = town
        .request_route(
            &actor_id,
            Destination::Building(id.clone()),
            NavigationRevision(1),
        )
        .unwrap();
    town.accept_route(
        &actor_id,
        &stale,
        NavigationRevision(1),
        RouteOutcome::Ready(vec![p(39., 29.)]),
    )
    .unwrap();
    let before = town.actor(&actor_id).unwrap().state().clone();
    world = replace_town(world, town);
    world
        .execute(
            &owner(),
            Command::Move {
                id: id.clone(),
                tile: tile(48, 30),
            },
        )
        .unwrap();
    let mut town = world.model().towns()[0].clone();
    let actor = town.actor(&actor_id).unwrap();
    assert_eq!(actor.state(), &before);
    assert_eq!(actor.route(), &RouteState::NotRequested);
    assert!(
        town.accept_route(
            &actor_id,
            &stale,
            NavigationRevision(1),
            RouteOutcome::Arrived
        )
        .is_err()
    );
    let resumed = town
        .resume_travel(&actor_id, NavigationRevision(2))
        .unwrap()
        .unwrap();
    assert_eq!(resumed.origin(), before.position);
    assert_eq!(resumed.goal(), p(48., 30.));
    world = replace_town(world, town);
    world
        .execute(&owner(), Command::Destroy { id: id.clone() })
        .unwrap();
    let actor = world.model().towns()[0].actor(&actor_id).unwrap();
    assert_eq!(actor.state().position, before.position);
    assert_eq!(actor.state().cargo, before.cargo);
    assert_eq!(actor.state().task, Task::Idle);
    assert_eq!(actor.state().travel, TravelIntent::None);
    let r = &world.rules()[&owner().town];
    let hall = &world.model().towns()[0].buildings()[0].id;
    assert_eq!(r.sites[hall].stored["wood"].get(), 101.25);
    assert_eq!(r.sites[hall].stored["planks"].get(), 30.375);
    let before = snapshot(&world);
    assert_eq!(
        world.execute(&owner(), Command::Destroy { id }),
        Err(Error::MissingBuilding)
    );
    assert_eq!(snapshot(&world), before);
}
#[test]
fn demolition_generation_exhaustion_is_atomic_for_all_workers_and_inventory() {
    let world = with_actors(vec![worker(10), worker(11)]);
    let town = &world.model().towns()[0];
    let id = town.buildings()[1].id.clone();
    let mut states = town
        .actors()
        .iter()
        .map(|a| a.state().clone())
        .collect::<Vec<_>>();
    for a in &mut states {
        a.task = Task::Construction {
            site: id.clone(),
            phase: ConstructionPhase::Building,
            elapsed: Seconds::new(3.).unwrap(),
        };
    }
    states[1].task_generation = TaskGeneration::from_saved(u64::MAX);
    let town = Town::restore(owner(), town.buildings().to_vec(), states).unwrap();
    let mut world = replace_town(world, town);
    let before = snapshot(&world);
    let actors = world.model().towns()[0].clone();
    assert_eq!(
        world.execute(&owner(), Command::Destroy { id }),
        Err(Error::Model)
    );
    assert_eq!(snapshot(&world), before);
    assert_eq!(&world.model().towns()[0], &actors);
}
#[test]
fn owner_checks_unsupported_children_and_pending_stock_fail_without_consuming_ids() {
    let mut world = with_actors(vec![worker(10)]);
    let before = snapshot(&world);
    let high = world.model().high_water();
    let foreign = Owner {
        town: TownId::new("foreign").unwrap(),
        ..owner()
    };
    assert_eq!(
        world.execute(&foreign, Command::BuildRoad(tile(40, 40))),
        Err(Error::MissingTown)
    );
    for (kind, orientation, error) in [
        ("acre", 0, Error::Unsupported),
        ("orchard_tree", 0, Error::Unsupported),
        ("home", 4, Error::InvalidOrientation),
    ] {
        assert_eq!(
            world.execute(
                &owner(),
                Command::Build {
                    kind: kind.into(),
                    tile: tile(40, 40),
                    orientation
                }
            ),
            Err(error)
        );
    }
    assert_eq!(snapshot(&world), before);
    assert_eq!(world.model().high_water(), high);
    let (model, mut rules, landscape, shared, builders) = world.into_parts();
    for site in rules.get_mut(&owner().town).unwrap().sites.values_mut() {
        site.stored.clear();
        site.pending.insert("wood".into(), q(100.));
    }
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    assert_eq!(
        world.execute(
            &owner(),
            Command::Build {
                kind: "home".into(),
                tile: tile(40, 40),
                orientation: 0
            }
        ),
        Err(Error::Materials)
    );
}
#[test]
fn shared_territory_foreign_footprints_players_roads_and_hall_separation() {
    let world = with_actors(vec![]);
    let (model, mut rules, mut landscape, _, _builders) = world.into_parts();
    let foreign = Owner {
        town: TownId::new("other").unwrap(),
        ..owner()
    };
    let hall = BuildingId::from_saved(EntityIdentity::from_saved(foreign.clone(), 20).unwrap());
    let mut foreign_rules = rules[&owner().town].clone();
    foreign_rules.origin = tile(80, 30);
    foreign_rules.player = p(40., 40.);
    foreign_rules.sites.clear();
    foreign_rules
        .sites
        .insert(hall.clone(), Site::new("hall", (3, 3)));
    foreign_rules.roads = vec![Road {
        tile: tile(45, 40),
        remaining: None,
    }];
    rules.insert(foreign.town.clone(), foreign_rules);
    landscape.resources.clear();
    let towns = vec![
        model.towns()[0].clone(),
        Town::restore(
            foreign,
            vec![Building {
                id: hall,
                location: tile(80, 30),
                construction: ConstructionState::Complete,
            }],
            vec![],
        )
        .unwrap(),
    ];
    let model = ModelWorld::new(
        model.id().clone(),
        model.seed(),
        20,
        towns,
        PrivateCredentials::default(),
    )
    .unwrap();
    let mut world = BuildingWorld::restore(model, rules, landscape, true).unwrap();
    for (tile, error) in [
        (tile(40, 40), Error::Player),
        (tile(80, 30), Error::Occupied),
        (tile(45, 40), Error::Occupied),
    ] {
        assert_eq!(
            world.execute(
                &owner(),
                Command::Build {
                    kind: "sawpit".into(),
                    tile,
                    orientation: 0
                }
            ),
            Err(error)
        );
    }
    let hall = world.model().towns()[0].buildings()[0].id.clone();
    assert_eq!(
        world.execute(
            &owner(),
            Command::Move {
                id: hall.clone(),
                tile: tile(50, 30)
            }
        ),
        Err(Error::HallSeparation)
    );
    world
        .execute(
            &owner(),
            Command::Move {
                id: hall,
                tile: tile(10, 30),
            },
        )
        .unwrap();
    // Current hall range reaches x=0, but original territory excludes this tile.
    let (model, mut rules, landscape, shared, builders) = world.into_parts();
    rules.get_mut(&owner().town).unwrap().level = 1;
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    assert_eq!(
        world.execute(&owner(), Command::BuildRoad(tile(0, 29))),
        Err(Error::Territory)
    );
}
#[test]
fn gate_rotation_checks_actual_worker_and_local_positions_and_map_edge() {
    let mut source = fixture()["cases"][3]["source"].clone();
    source["buildings"][1]["x"] = json!(0);
    source["buildings"][1]["y"] = json!(10);
    let mut actor = worker(10);
    actor.position = p(3.5, 10.);
    let mut world = source_world(&source, vec![actor]);
    let id = world.model().towns()[0].buildings()[1].id.clone();
    assert_eq!(
        world.execute(
            &owner(),
            Command::RotateFence {
                id: id.clone(),
                orientation: 2
            }
        ),
        Err(Error::Boundary)
    );
    assert_eq!(
        world.execute(
            &owner(),
            Command::RotateFence {
                id: id.clone(),
                orientation: 1
            }
        ),
        Err(Error::Player)
    );
    let (model, rules, mut landscape, shared, builders) = world.into_parts();
    let town = Town::restore(owner(), model.towns()[0].buildings().to_vec(), vec![]).unwrap();
    let model = ModelWorld::new(
        model.id().clone(),
        model.seed(),
        model.high_water(),
        vec![town],
        PrivateCredentials::default(),
    )
    .unwrap();
    landscape.local_positions.push(p(3.5, 10.));
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    assert_eq!(
        world.execute(&owner(), Command::RotateFence { id, orientation: 1 }),
        Err(Error::Player)
    );
}
#[test]
fn restore_rejects_missing_or_unsupported_records() {
    let world = with_actors(vec![]);
    let (model, mut rules, landscape, shared, builders) = world.into_parts();
    let id = model.towns()[0].buildings()[1].id.clone();
    rules.get_mut(&owner().town).unwrap().sites.remove(&id);
    assert!(matches!(
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders),
        Err(Error::InvalidFacts)
    ));
}

#[test]
fn structural_removal_preflights_generations_and_releases_only_related_tasks() {
    let world = with_actors(vec![worker(10), worker(11), worker(12), worker(13)]);
    let original = &world.model().towns()[0];
    let workplace = original.buildings()[1].id.clone();
    let hall = original.buildings()[0].id.clone();
    let mut actors = original
        .actors()
        .iter()
        .map(|a| a.state().clone())
        .collect::<Vec<_>>();
    actors[0].task = Task::Transport {
        source: hall.clone(),
        destination: workplace.clone(),
        phase: TransportPhase::ToDropoff,
    };
    actors[1].task = Task::Work {
        workplace: workplace.clone(),
        target: Destination::Position(p(42., 30.)),
        phase: WorkPhase::Working,
        elapsed: Seconds::new(2.125).unwrap(),
    };
    actors[2].task = Task::Construction {
        site: workplace.clone(),
        phase: ConstructionPhase::Outbound,
        elapsed: Seconds::new(0.).unwrap(),
    };
    actors[3].task = Task::Move {
        destination: p(20., 20.),
    };
    actors[2].task_generation = TaskGeneration::from_saved(u64::MAX);
    let mut town = Town::restore(owner(), original.buildings().to_vec(), actors.clone()).unwrap();
    let before = town.clone();
    assert_eq!(
        town.remove_building(&workplace),
        Err(ModelError::GenerationExhausted)
    );
    assert_eq!(town, before);
    actors[2].task_generation = TaskGeneration::default();
    let mut town = Town::restore(owner(), original.buildings().to_vec(), actors.clone()).unwrap();
    town.remove_building(&workplace).unwrap();
    for state in &actors[..3] {
        let actor = town.actor(&state.id).unwrap();
        assert_eq!(actor.state().task, Task::Idle);
        assert_eq!(actor.state().position, state.position);
        assert_eq!(actor.state().cargo, state.cargo);
    }
    assert_eq!(town.actor(&actors[3].id).unwrap().state(), &actors[3]);
    assert_eq!(town.building(&hall).unwrap().id, hall);
}

#[test]
fn overflowing_salvage_and_exhausted_allocator_leave_all_facts_unchanged() {
    let world = with_actors(vec![]);
    let (model, mut rules, landscape, shared, builders) = world.into_parts();
    let id = model.towns()[0].buildings()[1].id.clone();
    for site in rules.get_mut(&owner().town).unwrap().sites.values_mut() {
        site.stored.insert("wood".into(), q(f64::MAX));
    }
    let model = ModelWorld::new(
        model.id().clone(),
        model.seed(),
        u64::MAX,
        vec![model.towns()[0].clone()],
        PrivateCredentials::default(),
    )
    .unwrap();
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    let before = snapshot(&world);
    assert_eq!(
        world.execute(&owner(), Command::Destroy { id }),
        Err(Error::Quantity)
    );
    assert_eq!(snapshot(&world), before);
    assert_eq!(
        world.execute(
            &owner(),
            Command::Build {
                kind: "home".into(),
                tile: tile(50, 40),
                orientation: 0
            }
        ),
        Err(Error::Exhausted)
    );
    assert_eq!(snapshot(&world), before);
    assert_eq!(world.model().high_water(), u64::MAX);
}

fn route(plan: Plan) -> PlanResult {
    use lantern_core::search::{Outcome, Search};
    let mut search = Search::new(plan.query);
    search.advance(1_000_000);
    match search.into_outcome() {
        Outcome::Arrived => PlanResult::Ready(vec![]),
        Outcome::Completed { path, .. } => PlanResult::Ready(path),
        Outcome::Unreachable => PlanResult::Unreachable,
        other => panic!("unexpected test planner outcome {other:?}"),
    }
}
fn construction_vectors() -> Value {
    serde_json::from_str(include_str!("fixtures/building/builder_ticks.json")).unwrap()
}
fn construction_world(case: &Value) -> BuildingWorld {
    let actors = case["roles"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let mut a = worker(10 + i as u64);
            a.position = p(35., 30.);
            a
        })
        .collect();
    let mut world = source_world(&case["source"], actors);
    for (i, role) in case["roles"].as_array().unwrap().iter().enumerate() {
        let id = world.model().towns()[0].actors()[i].state().id.clone();
        let role = match role.as_str().unwrap() {
            "building" => Role::Dedicated,
            "idle" => Role::Idle,
            _ => Role::Busy,
        };
        world.set_builder_role(&owner(), &id, role).unwrap();
    }
    world
}
fn edit_landscape(world: BuildingWorld, edit: impl FnOnce(&mut Landscape)) -> BuildingWorld {
    let (model, rules, mut landscape, shared, builders) = world.into_parts();
    edit(&mut landscape);
    BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap()
}
fn construction_snapshot(world: &BuildingWorld) -> Value {
    let town = &world.model().towns()[0];
    let rules = &world.rules()[&owner().town];
    json!({"remaining":town.buildings().iter().map(|b| match b.construction {ConstructionState::Complete=>None,ConstructionState::InProgress{remaining}=>Some(remaining.get())}).collect::<Vec<_>>(),
        "roads":rules.roads.iter().map(|r|r.remaining.map(|w|w.get())).collect::<Vec<_>>(),
        "workers":town.actors().iter().map(|a| {
            let builder=&world.builders()[&a.state().id];
            let claim=builder.claim.as_ref().map(|c|match c {
                Claim::Building(id)=>{let b=town.building(id).unwrap();json!([rules.sites[id].kind,b.location.x,b.location.y])},
                Claim::Road(t)=>json!(["road",t.x,t.y]),
            });
            json!({"position":[a.state().position.x.get(),a.state().position.y.get()],"claim":claim,
                "path":builder.walk.as_ref().map(|w|w.path.iter().map(|p|json!([p.x.get(),p.y.get()])).collect::<Vec<_>>()),
                "step":builder.walk.as_ref().map(|w|w.step),
                "working":matches!(a.state().task,Task::Construction{phase:ConstructionPhase::Building,..}|Task::RoadConstruction{phase:ConstructionPhase::Building,..})})
        }).collect::<Vec<_>>()})
}
#[test]
fn paired_python_builder_claims_progress_walks_and_reload() {
    for case in construction_vectors().as_array().unwrap() {
        let mut world = construction_world(case);
        for (i, dt) in case["steps"].as_array().unwrap().iter().enumerate() {
            let event = &case["events"][i];
            match event["kind"].as_str() {
                Some("block") => {
                    world = edit_landscape(world, |l| {
                        l.resources.push(Resource {
                            kind: "rock".into(),
                            footprint: Footprint::saved(
                                tile(
                                    event["x"].as_i64().unwrap() as i32,
                                    event["y"].as_i64().unwrap() as i32,
                                ),
                                None,
                            )
                            .unwrap(),
                        })
                    });
                }
                Some("open") => {
                    world = edit_landscape(world, |l| l.resources.clear());
                }
                Some("move") => {
                    let id = world.model().towns()[0].buildings()[1].id.clone();
                    world
                        .execute(
                            &owner(),
                            Command::Move {
                                id,
                                tile: tile(44, 30),
                            },
                        )
                        .unwrap();
                }
                Some("busy") | Some("dedicated") => {
                    let id = world.model().towns()[0].actors()[0].state().id.clone();
                    world
                        .set_builder_role(
                            &owner(),
                            &id,
                            if event["kind"] == "busy" {
                                Role::Busy
                            } else {
                                Role::Dedicated
                            },
                        )
                        .unwrap();
                }
                None => {}
                other => panic!("unexpected event {other:?}"),
            }
            world
                .advance_construction(&owner(), Seconds::new(dt.as_f64().unwrap()).unwrap(), route)
                .unwrap();
            equal_numbers(
                &construction_snapshot(&world),
                &case["outcomes"][i],
                &format!("{}[{i}]", case["name"]),
            );
            let saved = encode_building_save(&world).unwrap();
            let loaded = decode_building_save(&saved).unwrap();
            assert_eq!(world.builders(), loaded.builders());
            assert_eq!(world.model().towns(), loaded.model().towns());
            assert_eq!(encode_building_save(&loaded).unwrap(), saved);
            world = loaded;
        }
    }
}
#[test]
fn pending_searches_do_not_work_and_invalid_routes_are_atomic() {
    let mut world = construction_world(&construction_vectors()[0]);
    let mut calls = 0;
    world
        .advance_construction(&owner(), Seconds::new(30.).unwrap(), |_| {
            calls += 1;
            PlanResult::Pending
        })
        .unwrap();
    assert_eq!(calls, 2); // dedicated and one idle claim; third helper waits
    assert_eq!(
        world
            .builders()
            .values()
            .filter(|b| b.claim.is_some())
            .count(),
        2
    );
    assert!(world.model().towns()[0].buildings().iter().skip(1).all(|b| matches!(b.construction,ConstructionState::InProgress{remaining} if remaining.get()==30.)));
    let before = encode_building_save(&world).unwrap();
    assert_eq!(
        world.advance_construction(&owner(), Seconds::new(3.).unwrap(), |_| PlanResult::Ready(
            vec![tile(119, 119)]
        )),
        Err(Error::InvalidFacts)
    );
    assert_eq!(encode_building_save(&world).unwrap(), before);
    world
        .advance_construction(&owner(), Seconds::new(3.).unwrap(), route)
        .unwrap();
    assert!(world.builders().values().any(|b| b.walk.is_some()));
}
#[test]
fn saved_routes_survive_unrelated_edits_and_only_replan_a_blocked_edge() {
    let mut world = construction_world(&construction_vectors()[1]);
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    let old = world.builders()[&id].walk.clone().unwrap();
    world = edit_landscape(world, |l| {
        l.resources.push(Resource {
            kind: "rock".into(),
            footprint: Footprint::saved(tile(100, 100), None).unwrap(),
        })
    });
    world = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), |_| {
            panic!("valid saved walk was discarded")
        })
        .unwrap();
    assert_eq!(world.builders()[&id].walk.as_ref().unwrap().path, old.path);
    let next = old.path[old.step + 1];
    world = edit_landscape(world, |l| {
        l.resources.push(Resource {
            kind: "rock".into(),
            footprint: Footprint::saved(tile(next.x.get() as i32, next.y.get() as i32), None)
                .unwrap(),
        })
    });
    let mut planned = 0;
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), |p| {
            planned += 1;
            route(p)
        })
        .unwrap();
    assert_eq!(planned, 1);
    assert_ne!(world.builders()[&id].walk.as_ref().unwrap().path, old.path);
}
#[test]
fn removal_releases_building_and_road_claims_once_preserving_pose_and_cargo() {
    let mut world = construction_world(&construction_vectors()[6]);
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    let road_worker = world
        .builders()
        .iter()
        .find(|(_, b)| matches!(b.claim, Some(Claim::Road(_))))
        .unwrap()
        .0
        .clone();
    let before = world.model().towns()[0]
        .actor(&road_worker)
        .unwrap()
        .state()
        .clone();
    world
        .execute(&owner(), Command::RemoveRoad(tile(36, 30)))
        .unwrap();
    let after = world.model().towns()[0]
        .actor(&road_worker)
        .unwrap()
        .state();
    assert_eq!(after.position, before.position);
    assert_eq!(after.cargo, before.cargo);
    assert_eq!(after.task, Task::Idle);
    assert!(world.builders()[&road_worker].claim.is_none());
    let id = world.model().towns()[0].buildings()[1].id.clone();
    world.execute(&owner(), Command::Destroy { id }).unwrap();
    let saved = encode_building_save(&world).unwrap();
    let mut world = decode_building_save(&saved).unwrap();
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    assert!(
        world.model().towns()[0]
            .actors()
            .iter()
            .all(|a| a.state().cargo.goods()["wood"].get() == 2.125)
    );
}
#[test]
fn construction_save_rejects_invalid_fields_numbers_claims_and_walks() {
    let mut world = construction_world(&construction_vectors()[0]);
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    let bytes = encode_building_save(&world).unwrap();
    let source: Value = serde_json::from_slice(&bytes).unwrap();
    for mutation in 0..10 {
        let mut bad = source.clone();
        match mutation {
            0 => bad["version"] = json!(3),
            1 => bad["builders"][0]["walk"]["step"] = json!(usize::MAX),
            2 => bad["builders"][0]["id"]["town_id"] = json!("foreign"),
            3 => bad["towns"][0]["coins"] = json!(-1),
            4 => bad["builders"][0]["walk"]["path"][0]["x"] = json!(90),
            5 => bad["towns"][0]["sites"][0]["stored"]["wood"] = json!(9007199254740993u64),
            6 => bad["builders"][0]["walk"]["extra"] = json!("private-input"),
            7 => {
                let b = bad["builders"][0].clone();
                bad["builders"].as_array_mut().unwrap().push(b);
            }
            8 => {
                let s = bad["towns"][0]["sites"][0].clone();
                bad["towns"][0]["sites"].as_array_mut().unwrap().push(s);
            }
            9 => {
                bad["builders"] = json!([]);
            }
            _ => unreachable!(),
        }
        let raw = bad.to_string();
        let error = decode_building_save(raw.as_bytes())
            .err()
            .expect("invalid save accepted");
        assert!(!format!("{error:?}").contains("private-input"));
    }
    let raw = String::from_utf8(bytes).unwrap();
    let duplicate = raw.replacen("\"version\":2", "\"version\":2,\"version\":2", 1);
    assert!(decode_building_save(duplicate.as_bytes()).is_err());
    let bad = raw.replacen(
        "\"stored\":{",
        "\"stored\":{\"duplicate\":1,\"duplicate\":2,",
        1,
    );
    assert!(decode_building_save(bad.as_bytes()).is_err());
    let bad = raw.replacen("\"version\":3", "\"version\":3,\"version\":3", 1);
    assert!(decode_building_save(bad.as_bytes()).is_err());
}
#[test]
fn road_task_requires_v3_while_v2_model_saves_still_migrate() {
    let mut world = construction_world(&construction_vectors()[6]);
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    let raw = lantern_host::dto::encode_save(world.model()).unwrap();
    assert!(lantern_host::dto::decode_save(&raw).is_ok());
    assert!(
        lantern_host::dto::decode_save(&raw.replacen("\"version\":3", "\"version\":2", 1)).is_err()
    );
    let world = with_actors(vec![worker(10)]);
    let raw = lantern_host::dto::encode_save(world.model()).unwrap();
    assert!(
        lantern_host::dto::decode_save(&raw.replacen("\"version\":3", "\"version\":2", 1)).is_ok()
    );
}

#[test]
fn construction_preserves_fractional_pose_until_a_checked_waypoint_and_reloads_facing() {
    let mut case = construction_vectors()[1].clone();
    case["source"]["resources"] = json!([]);
    let mut actor = worker(10);
    actor.position = p(35.25, 30.);
    let mut world = source_world(&case["source"], vec![actor]);
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    world
        .set_builder_role(&owner(), &id, Role::Dedicated)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    assert_eq!(
        world.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .position,
        p(35.25, 30.)
    );
    world = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    let state = world.model().towns()[0].actor(&id).unwrap().state();
    assert_eq!(state.position, p(36., 30.));
    assert_eq!(state.facing.get(), 1);
    assert_eq!(state.cargo.goods()["wood"].get(), 2.125);
    assert_eq!(
        world.model().towns()[0].buildings()[1].construction,
        ConstructionState::InProgress {
            remaining: Work::new(30.).unwrap()
        }
    );
    let loaded = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    assert_eq!(loaded.model().towns()[0].actor(&id).unwrap().state(), state);
}
#[test]
fn completion_and_road_removal_preflight_generation_failure_without_partial_updates() {
    let case = construction_vectors()[1].clone();
    let mut actor = worker(10);
    actor.position = p(39., 30.);
    let mut world = source_world(&case["source"], vec![actor]);
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    world
        .set_builder_role(&owner(), &id, Role::Dedicated)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    let mut save: Value = serde_json::from_slice(&encode_building_save(&world).unwrap()).unwrap();
    save["model"]["towns"][0]["actors"][0]["task_generation"] = json!(u64::MAX.to_string());
    let mut world = decode_building_save(save.to_string().as_bytes()).unwrap();
    let before = encode_building_save(&world).unwrap();
    assert_eq!(
        world.advance_construction(&owner(), Seconds::new(30.).unwrap(), route),
        Err(Error::Model)
    );
    assert_eq!(encode_building_save(&world).unwrap(), before);
    let mut world = construction_world(&construction_vectors()[6]);
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    let id = world
        .builders()
        .iter()
        .find(|(_, b)| matches!(b.claim, Some(Claim::Road(_))))
        .unwrap()
        .0
        .clone();
    let mut save: Value = serde_json::from_slice(&encode_building_save(&world).unwrap()).unwrap();
    let serial = id.identity().serial().to_string();
    for a in save["model"]["towns"][0]["actors"].as_array_mut().unwrap() {
        if a["id"]["serial"].as_str() == Some(serial.as_str()) {
            a["task_generation"] = json!(u64::MAX.to_string());
        }
    }
    let mut world = decode_building_save(save.to_string().as_bytes()).unwrap();
    let before = encode_building_save(&world).unwrap();
    assert_eq!(
        world.execute(&owner(), Command::RemoveRoad(tile(36, 30))),
        Err(Error::Model)
    );
    assert_eq!(encode_building_save(&world).unwrap(), before);
}
#[test]
fn blocked_origin_recovers_only_after_route_readiness_and_unreachable_sites_release_claims() {
    let case = construction_vectors()[1].clone();
    let mut actor = worker(10);
    actor.position = p(40., 30.);
    let mut world = source_world(&case["source"], vec![actor]);
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    world
        .set_builder_role(&owner(), &id, Role::Dedicated)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(3.).unwrap(), |p| {
            assert_eq!(p.start, tile(39, 30));
            PlanResult::Pending
        })
        .unwrap();
    assert_eq!(
        world.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .position,
        p(40., 30.)
    );
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    assert_eq!(
        world.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .position,
        p(39., 30.)
    );
    world.set_builder_role(&owner(), &id, Role::Busy).unwrap();
    world
        .set_builder_role(&owner(), &id, Role::Dedicated)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(30.).unwrap(), |_| {
            PlanResult::Unreachable
        })
        .unwrap();
    assert!(world.builders()[&id].claim.is_none());
    assert_eq!(
        world.model().towns()[0].actor(&id).unwrap().state().task,
        Task::Idle
    );
}
#[test]
fn nullable_construction_fields_are_required_and_hall_upgrades_are_explicitly_unsupported() {
    let mut world = construction_world(&construction_vectors()[6]);
    world
        .advance_construction(&owner(), Seconds::new(0.).unwrap(), route)
        .unwrap();
    let raw: Value = serde_json::from_slice(&encode_building_save(&world).unwrap()).unwrap();
    for (field, road) in [("claim", false), ("walk", false), ("remaining", true)] {
        let mut bad = raw.clone();
        let entry = if road {
            &mut bad["towns"][0]["roads"][0]
        } else {
            &mut bad["builders"][0]
        };
        entry.as_object_mut().unwrap().remove(field);
        assert!(
            decode_building_save(bad.to_string().as_bytes()).is_err(),
            "missing {field} accepted"
        );
    }
    let mut bad = raw.clone();
    bad["model"]["towns"][0]["buildings"][0]["construction"] =
        json!({"in_progress":{"remaining":30}});
    assert!(decode_building_save(bad.to_string().as_bytes()).is_err());
}

fn upgraded_snapshot(w: &BuildingWorld) -> Value {
    let town = &w.model().towns()[0];
    let r = &w.rules()[&owner().town];
    json!({"level":r.level,"coins":r.coins.get(),"buildings":town.buildings().iter().map(|b|{
        let s=&r.sites[&b.id];
        let child=s.child.as_ref().map(|c|match c{
            ChildState::Field{stage,elapsed,progress,cycle}=>json!({"stage":stage,"elapsed":elapsed.get(),"progress":progress.get(),"cycle":cycle}),
            ChildState::Tree{stage,elapsed,fruit}=>json!({"stage":stage,"elapsed":elapsed.get(),"fruit":fruit.get()}),
        });
        json!({"kind":s.kind,"tile":[b.location.x,b.location.y],"level":s.level,"capacity":s.capacity_level,"equipment":s.equipment,
            "parent":s.parent.as_ref().map(|id|town.buildings().iter().position(|b|&b.id==id).unwrap()),"child":child,
            "remaining":match b.construction{ConstructionState::Complete=>None,ConstructionState::InProgress{remaining}=>Some(remaining.get())},"stored":values(&s.stored),"pending":values(&s.pending)})
    }).collect::<Vec<_>>()})
}
fn upgrade_vector_command(w: &BuildingWorld, input: &Value) -> Command {
    let id = || {
        w.model().towns()[0].buildings()[input["index"].as_u64().unwrap() as usize]
            .id
            .clone()
    };
    match input["action"].as_str().unwrap() {
        "upgrade_building" => Command::UpgradeStructure(id()),
        "upgrade_capacity" => Command::UpgradeCapacity(id()),
        "upgrade" => Command::UpgradeHall,
        "profession_upgrade" => Command::UpgradeEquipment {
            id: id(),
            key: input["upgrade"].as_str().unwrap().into(),
        },
        "build" if input["kind"] == "acre" || input["kind"] == "orchard_tree" => {
            let parent = input
                .get("farm_index")
                .or_else(|| input.get("orchard_index"))
                .unwrap()
                .as_u64()
                .unwrap() as usize;
            Command::BuildChild {
                kind: input["kind"].as_str().unwrap().into(),
                tile: tile(
                    input["x"].as_i64().unwrap() as i32,
                    input["y"].as_i64().unwrap() as i32,
                ),
                parent: w.model().towns()[0].buildings()[parent].id.clone(),
                orientation: input["orientation"].as_u64().unwrap_or(0) as u8,
            }
        }
        _ => command(w, input),
    }
}
#[test]
fn paired_python_upgrades_linked_groups_helpers_and_hall_rewards() {
    let vectors: Value =
        serde_json::from_str(include_str!("fixtures/building/r12_upgrade_cases.json")).unwrap();
    let catalog: Value =
        serde_json::from_str(include_str!("../src/building/upgrade_catalog.json")).unwrap();
    assert_eq!(catalog, vectors["catalog"]);
    for case in vectors["cases"].as_array().unwrap() {
        let mut actor = worker(10);
        actor.position = p(if case["hall"] == true { 29. } else { 39. }, 30.);
        if case["helper"] == true {
            let site = BuildingId::from_saved(EntityIdentity::from_saved(owner(), 2).unwrap());
            actor.task = Task::Work {
                workplace: site.clone(),
                target: Destination::Building(site),
                phase: WorkPhase::Working,
                elapsed: Seconds::new(2.125).unwrap(),
            };
        }
        let id = actor.id.clone();
        let mut world = source_world(&case["source"], vec![actor]);
        if case["helper"] == true {
            let site = world.model().towns()[0].buildings()[1].id.clone();
            world.set_upgrade_helper(&owner(), &id, Some(site)).unwrap();
        } else {
            world
                .set_builder_role(&owner(), &id, Role::Dedicated)
                .unwrap();
        }
        for (i, input) in case["actions"].as_array().unwrap().iter().enumerate() {
            let before = encode_building_save(&world).unwrap();
            let result = if input["action"] == "tick" {
                world.advance_construction(
                    &owner(),
                    Seconds::new(input["seconds"].as_f64().unwrap()).unwrap(),
                    route,
                )
            } else {
                world
                    .execute(&owner(), upgrade_vector_command(&world, input))
                    .map(|_| ())
            };
            assert_eq!(
                result.is_ok(),
                case["outcomes"][i]["ok"].as_bool().unwrap(),
                "{}[{i}] {input} {result:?}",
                case["name"]
            );
            if result.is_err() {
                assert_eq!(encode_building_save(&world).unwrap(), before);
            }
            equal_numbers(
                &upgraded_snapshot(&world),
                &case["outcomes"][i]["state"],
                &format!("{}[{i}]", case["name"]),
            );
            let saved = encode_building_save(&world).unwrap();
            let loaded = decode_building_save(&saved).unwrap();
            assert_eq!(world.builders(), loaded.builders());
            assert_eq!(encode_building_save(&loaded).unwrap(), saved);
            world = loaded;
        }
    }
}

fn farm_world(actors: Vec<ActorState>) -> BuildingWorld {
    let mut source = fixture()["cases"][0]["source"].clone();
    source["resources"] = json!([]);
    source["buildings"]
        .as_array_mut()
        .unwrap()
        .push(json!({"kind":"farm","x":40,"y":30,"stored_goods":{"wood":1.25}}));
    source_world(&source, actors)
}
fn add_acre(world: &mut BuildingWorld) -> BuildingId {
    let parent = world.model().towns()[0].buildings()[1].id.clone();
    world
        .execute(
            &owner(),
            Command::BuildChild {
                kind: "acre".into(),
                tile: tile(41, 31),
                parent,
                orientation: 0,
            },
        )
        .unwrap()
        .unwrap()
}
#[test]
fn linked_group_move_preserves_field_progress_and_stable_work_references_atomically() {
    let mut world = farm_world(vec![worker(10)]);
    let acre = add_acre(&mut world);
    let farm = world.model().towns()[0].buildings()[1].id.clone();
    let actor = world.model().towns()[0].actors()[0].state().id.clone();
    let mut town = world.model().towns()[0].clone();
    town.set_task(
        &actor,
        Task::Work {
            workplace: farm.clone(),
            target: Destination::Building(acre.clone()),
            phase: WorkPhase::Working,
            elapsed: Seconds::new(2.125).unwrap(),
        },
    )
    .unwrap();
    let request = town
        .request_route(
            &actor,
            Destination::Building(acre.clone()),
            NavigationRevision(1),
        )
        .unwrap();
    town.accept_route(
        &actor,
        &request,
        NavigationRevision(1),
        RouteOutcome::Ready(vec![p(40., 31.)]),
    )
    .unwrap();
    world = replace_town(world, town);
    let (model, mut rules, mut landscape, shared, builders) = world.into_parts();
    rules
        .get_mut(&owner().town)
        .unwrap()
        .sites
        .get_mut(&acre)
        .unwrap()
        .child = Some(ChildState::Field {
        stage: "ripe".into(),
        elapsed: Seconds::new(1.125).unwrap(),
        progress: Work::new(0.375).unwrap(),
        cycle: u64::MAX,
    });
    landscape.resources.push(Resource {
        kind: "rock".into(),
        footprint: Footprint::saved(tile(46, 31), None).unwrap(),
    });
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    let before = encode_building_save(&world).unwrap();
    assert_eq!(
        world.execute(
            &owner(),
            Command::Move {
                id: farm.clone(),
                tile: tile(45, 30)
            }
        ),
        Err(Error::Occupied)
    );
    assert_eq!(encode_building_save(&world).unwrap(), before);
    let state = world.model().towns()[0]
        .actor(&actor)
        .unwrap()
        .state()
        .clone();
    let child = world.rules()[&owner().town].sites[&acre].child.clone();
    world
        .execute(
            &owner(),
            Command::Move {
                id: farm.clone(),
                tile: tile(43, 30),
            },
        )
        .unwrap();
    assert_eq!(
        world.model().towns()[0].actor(&actor).unwrap().state(),
        &state
    );
    assert_eq!(
        world.model().towns()[0].actor(&actor).unwrap().route(),
        &RouteState::NotRequested
    );
    assert_eq!(
        world.model().towns()[0].building(&acre).unwrap().location,
        tile(44, 31)
    );
    assert_eq!(world.rules()[&owner().town].sites[&acre].child, child);
    let world = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    assert_eq!(world.rules()[&owner().town].sites[&acre].parent, Some(farm));
    assert_eq!(world.rules()[&owner().town].sites[&acre].child, child);
}
#[test]
fn transport_refunds_survive_group_moves_and_demolition_without_duplication() {
    let mut actor = worker(10);
    let farm = BuildingId::from_saved(EntityIdentity::from_saved(owner(), 2).unwrap());
    let hall = BuildingId::from_saved(EntityIdentity::from_saved(owner(), 1).unwrap());
    actor.task = Task::Transport {
        source: farm.clone(),
        destination: hall.clone(),
        phase: TransportPhase::ToDropoff,
    };
    let id = actor.id.clone();
    let mut world = farm_world(vec![actor]);
    let acre = add_acre(&mut world);
    world
        .execute(
            &owner(),
            Command::Move {
                id: farm.clone(),
                tile: tile(43, 30),
            },
        )
        .unwrap();
    let r = &world.rules()[&owner().town];
    assert_eq!(r.sites[&farm].stored["wood"].get(), 3.375);
    assert!(
        world.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .cargo
            .goods()
            .is_empty()
    );
    let (model, mut rules, landscape, shared, builders) = world.into_parts();
    rules
        .get_mut(&owner().town)
        .unwrap()
        .sites
        .get_mut(&acre)
        .unwrap()
        .pending
        .insert("planks".into(), q(0.375));
    let mut world =
        BuildingWorld::restore_with_builders(model, rules, landscape, shared, builders).unwrap();
    world
        .execute(&owner(), Command::Destroy { id: farm.clone() })
        .unwrap();
    // The acre cost is spent, while the parent's inventory and explicit carried
    // transport cargo are salvaged exactly once, together with pending output.
    assert_eq!(
        world.rules()[&owner().town].sites[&hall].stored["wood"].get(),
        98.375
    );
    assert_eq!(
        world.rules()[&owner().town].sites[&hall].stored["planks"].get(),
        30.375
    );
    let saved = encode_building_save(&world).unwrap();
    let mut world = decode_building_save(&saved).unwrap();
    assert_eq!(
        world.execute(&owner(), Command::Destroy { id: farm }),
        Err(Error::MissingBuilding)
    );
    assert_eq!(encode_building_save(&world).unwrap(), saved);
}
#[test]
fn destroying_a_loaded_transport_source_salvages_cargo_at_the_hall_and_failures_are_atomic() {
    let mut actor = worker(10);
    let id = actor.id.clone();
    let farm = BuildingId::from_saved(EntityIdentity::from_saved(owner(), 2).unwrap());
    let hall = BuildingId::from_saved(EntityIdentity::from_saved(owner(), 1).unwrap());
    actor.task = Task::Transport {
        source: farm.clone(),
        destination: hall.clone(),
        phase: TransportPhase::ToDropoff,
    };
    let mut world = farm_world(vec![actor]);
    add_acre(&mut world);
    let mut raw: Value = serde_json::from_slice(&encode_building_save(&world).unwrap()).unwrap();
    raw["model"]["towns"][0]["actors"][0]["task_generation"] = json!(u64::MAX.to_string());
    let mut blocked = decode_building_save(raw.to_string().as_bytes()).unwrap();
    let saved = encode_building_save(&blocked).unwrap();
    assert_eq!(
        blocked.execute(&owner(), Command::Destroy { id: farm.clone() }),
        Err(Error::Model)
    );
    assert_eq!(encode_building_save(&blocked).unwrap(), saved);
    world
        .execute(&owner(), Command::Destroy { id: farm })
        .unwrap();
    assert_eq!(
        world.rules()[&owner().town].sites[&hall].stored["wood"].get(),
        98.375
    );
    assert_eq!(
        world.model().towns()[0].actor(&id).unwrap().state().task,
        Task::Idle
    );
    assert!(
        world.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .cargo
            .goods()
            .is_empty()
    );
}
#[test]
fn acre_assigned_helper_resumes_exact_work_intent_after_saved_upgrade_and_relocation() {
    let mut world = farm_world(vec![worker(10)]);
    let acre = add_acre(&mut world);
    let farm = world.model().towns()[0].buildings()[1].id.clone();
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    let mut town = world.model().towns()[0].clone();
    let task = Task::Work {
        workplace: acre.clone(),
        target: Destination::Building(acre.clone()),
        phase: WorkPhase::ReturningWater,
        elapsed: Seconds::new(2.125).unwrap(),
    };
    town.set_task(&id, task.clone()).unwrap();
    town.request_route(
        &id,
        Destination::Building(acre.clone()),
        NavigationRevision(1),
    )
    .unwrap();
    world = replace_town(world, town);
    world
        .set_upgrade_helper(&owner(), &id, Some(acre.clone()))
        .unwrap();
    world
        .execute(&owner(), Command::UpgradeStructure(farm.clone()))
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    assert_eq!(
        world.builders()[&id].claim,
        Some(Claim::Building(farm.clone()))
    );
    assert!(world.builders()[&id].suspended.is_some());
    world = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    world
        .execute(
            &owner(),
            Command::Move {
                id: farm.clone(),
                tile: tile(43, 30),
            },
        )
        .unwrap();
    assert!(world.builders()[&id].walk.is_none());
    world
        .advance_construction(&owner(), Seconds::new(300.).unwrap(), route)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(30.).unwrap(), route)
        .unwrap();
    let actor = world.model().towns()[0].actor(&id).unwrap();
    assert_eq!(actor.state().task, task);
    assert_eq!(
        actor.state().travel,
        TravelIntent::Active(Destination::Building(acre))
    );
    assert_eq!(actor.state().cargo.goods()["wood"].get(), 2.125);
    assert_eq!(world.rules()[&owner().town].sites[&farm].level, 2);
    assert!(world.builders()[&id].claim.is_none());
    assert!(world.builders()[&id].suspended.is_none());
    let saved = encode_building_save(&world).unwrap();
    assert_eq!(
        encode_building_save(&decode_building_save(&saved).unwrap()).unwrap(),
        saved
    );
}
#[test]
fn helper_reassignment_and_demolition_never_restore_a_removed_profession_target() {
    let mut world = farm_world(vec![worker(10)]);
    let farm = world.model().towns()[0].buildings()[1].id.clone();
    let id = world.model().towns()[0].actors()[0].state().id.clone();
    let mut town = world.model().towns()[0].clone();
    town.set_task(
        &id,
        Task::Work {
            workplace: farm.clone(),
            target: Destination::Building(farm.clone()),
            phase: WorkPhase::Working,
            elapsed: Seconds::new(1.).unwrap(),
        },
    )
    .unwrap();
    world = replace_town(world, town);
    world
        .set_upgrade_helper(&owner(), &id, Some(farm.clone()))
        .unwrap();
    world
        .execute(&owner(), Command::UpgradeCapacity(farm.clone()))
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    let mut demolished = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    demolished
        .execute(&owner(), Command::Destroy { id: farm.clone() })
        .unwrap();
    assert!(demolished.builders()[&id].helper.is_none());
    assert!(demolished.builders()[&id].suspended.is_none());
    assert_eq!(
        demolished.model().towns()[0]
            .actor(&id)
            .unwrap()
            .state()
            .task,
        Task::Idle
    );
    decode_building_save(&encode_building_save(&demolished).unwrap()).unwrap();
    world
        .set_builder_role(&owner(), &id, Role::Dedicated)
        .unwrap();
    assert!(world.builders()[&id].helper.is_none());
    assert!(world.builders()[&id].suspended.is_none());
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    world
        .execute(&owner(), Command::Destroy { id: farm })
        .unwrap();
    assert_eq!(
        world.model().towns()[0].actor(&id).unwrap().state().task,
        Task::Idle
    );
    assert!(world.builders()[&id].claim.is_none());
    decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
}
#[test]
fn construction_v1_migration_and_v2_extension_validation_preserve_scope_and_rewards() {
    let world = with_actors(vec![worker(10)]);
    let raw: Value = serde_json::from_slice(&encode_building_save(&world).unwrap()).unwrap();
    let mut old = raw.clone();
    old["version"] = json!(1);
    old.as_object_mut().unwrap().remove("extensions");
    let restored = decode_building_save(old.to_string().as_bytes()).unwrap();
    assert_eq!(restored.rules(), world.rules());
    for mutation in 0..7 {
        let mut bad = raw.clone();
        match mutation {
            0 => {
                bad.as_object_mut().unwrap().remove("extensions");
            }
            1 => {
                bad["extensions"]["sites"][0]["parent"] =
                    bad["extensions"]["sites"][0]["id"].clone()
            }
            2 => {
                bad["extensions"]["sites"][0]["upgrade"] =
                    json!({"hall":{"level":2,"reward":100000}})
            }
            3 => bad["extensions"]["progression"][0]["skills"]["fishing"] = json!(-1),
            4 => {
                let x = bad["extensions"]["sites"][0].clone();
                bad["extensions"]["sites"].as_array_mut().unwrap().push(x);
            }
            5 => bad["version"] = json!(1),
            6 => bad["extensions"] = Value::Null,
            _ => unreachable!(),
        }
        assert!(
            decode_building_save(bad.to_string().as_bytes()).is_err(),
            "invalid extension {mutation} accepted"
        );
    }
}

#[test]
fn idle_upgrade_helpers_preserve_terminal_cancelled_intent_across_reload() {
    let mut actor = worker(10);
    actor.travel = TravelIntent::Cancelled;
    let id = actor.id.clone();
    let mut world = farm_world(vec![actor]);
    let farm = world.model().towns()[0].buildings()[1].id.clone();
    world
        .set_upgrade_helper(&owner(), &id, Some(farm.clone()))
        .unwrap();
    world
        .execute(&owner(), Command::UpgradeStructure(farm))
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(1.).unwrap(), route)
        .unwrap();
    world = decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
    world
        .advance_construction(&owner(), Seconds::new(300.).unwrap(), route)
        .unwrap();
    world
        .advance_construction(&owner(), Seconds::new(30.).unwrap(), route)
        .unwrap();
    let state = world.model().towns()[0].actor(&id).unwrap().state();
    assert_eq!(state.task, Task::Idle);
    assert_eq!(state.travel, TravelIntent::Cancelled);
    decode_building_save(&encode_building_save(&world).unwrap()).unwrap();
}
