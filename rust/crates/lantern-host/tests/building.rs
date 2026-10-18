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
            sites.insert(id.clone(), site);
            Building {
                id,
                location: tile(
                    b["x"].as_i64().unwrap() as i32,
                    b["y"].as_i64().unwrap() as i32,
                ),
                construction: ConstructionState::Complete,
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
                remaining: None,
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
            let (model, rules, landscape, shared) = world.into_parts();
            world = BuildingWorld::restore(model, rules, landscape, shared).unwrap();
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
    let (model, rules, landscape, shared) = world.into_parts();
    let model = ModelWorld::new(
        model.id().clone(),
        model.seed(),
        model.high_water(),
        vec![town],
        PrivateCredentials::default(),
    )
    .unwrap();
    BuildingWorld::restore(model, rules, landscape, shared).unwrap()
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
    let (model, mut rules, landscape, shared) = world.into_parts();
    for site in rules.get_mut(&owner().town).unwrap().sites.values_mut() {
        site.stored.clear();
        site.pending.insert("wood".into(), q(100.));
    }
    let mut world = BuildingWorld::restore(model, rules, landscape, shared).unwrap();
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
    let (model, mut rules, mut landscape, _) = world.into_parts();
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
    let (model, mut rules, landscape, shared) = world.into_parts();
    rules.get_mut(&owner().town).unwrap().level = 1;
    let mut world = BuildingWorld::restore(model, rules, landscape, shared).unwrap();
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
    let (model, rules, mut landscape, shared) = world.into_parts();
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
    let mut world = BuildingWorld::restore(model, rules, landscape, shared).unwrap();
    assert_eq!(
        world.execute(&owner(), Command::RotateFence { id, orientation: 1 }),
        Err(Error::Player)
    );
}
#[test]
fn restore_rejects_missing_or_unsupported_records() {
    let world = with_actors(vec![]);
    let (model, mut rules, landscape, shared) = world.into_parts();
    let id = model.towns()[0].buildings()[1].id.clone();
    rules.get_mut(&owner().town).unwrap().sites.remove(&id);
    assert!(matches!(
        BuildingWorld::restore(model, rules, landscape, shared),
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
    let (model, mut rules, landscape, shared) = world.into_parts();
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
    let mut world = BuildingWorld::restore(model, rules, landscape, shared).unwrap();
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
