use lantern_core::{
    ids::{ActorId, BuildingId, EntityIdentity, IdAllocator, Owner, TownId, WorldId},
    model::{
        ActorState, Building, Cargo, ConstructionPhase, ConstructionState, Destination,
        NavigationRevision, RouteOutcome, RouteState, Task, Town, TransportPhase, WorkPhase,
    },
    units::{Position, Quantity, Seconds, TerrainSeed, Tile, Work},
};
use lantern_host::{
    BoundaryError,
    dto::{decode_save, encode_save, public_snapshot},
    references::*,
    state::{ModelWorld, PrivateAccount, PrivateCredentials},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn position(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}

fn fixture() -> ModelWorld {
    let world = WorldId::new("opaque-世界").unwrap();
    let mut ids = IdAllocator::restore(world.clone(), 1 << 53, &[]).unwrap();
    let mut towns = Vec::new();
    for name in ["a", "b"] {
        let owner = Owner {
            world: world.clone(),
            town: TownId::new(name).unwrap(),
        };
        let workplace: BuildingId = ids.allocate(&owner).unwrap();
        let store = ids.allocate(&owner).unwrap();
        let worker = ids.allocate(&owner).unwrap();
        let idle = ids.allocate(&owner).unwrap();
        let goods = Cargo::from_entries([
            ("wood".into(), Quantity::new(0.1).unwrap()),
            ("fish".into(), Quantity::new(65.12345678901234).unwrap()),
            ("tiny".into(), Quantity::new(f64::from_bits(1)).unwrap()),
            ("zero".into(), Quantity::new(-0.0).unwrap()),
        ])
        .unwrap();
        let mut town = Town::restore(
            owner,
            vec![
                Building {
                    id: workplace.clone(),
                    location: Tile { x: 9, y: 3 },
                    construction: ConstructionState::Complete,
                },
                Building {
                    id: store,
                    location: Tile {
                        x: i32::MIN,
                        y: i32::MAX,
                    },
                    construction: ConstructionState::InProgress {
                        remaining: Work::new(0.12345678901234567).unwrap(),
                    },
                },
            ],
            vec![
                ActorState::new(
                    worker.clone(),
                    position(11.25000000012345, -1.23456789012345),
                    goods,
                ),
                ActorState::new(idle, position(-0.0, f64::from_bits(1)), Cargo::default()),
            ],
        )
        .unwrap();
        town.set_task(
            &worker,
            Task::Work {
                workplace: workplace.clone(),
                target: Destination::Position(position(12.0, 3.0)),
                phase: WorkPhase::Returning,
                elapsed: Seconds::new(0.12345678901234567).unwrap(),
            },
        )
        .unwrap();
        let request = town
            .request_route(
                &worker,
                Destination::Building(workplace),
                NavigationRevision(8),
            )
            .unwrap();
        town.accept_route(
            &worker,
            &request,
            NavigationRevision(8),
            RouteOutcome::Ready(vec![position(987654.321, 123456.789)]),
        )
        .unwrap();
        towns.push(town);
    }
    let town_id = TownId::new("a").unwrap();
    let credentials = PrivateCredentials {
        tokens: BTreeMap::from([("SYNTHETIC_BEARER_ONLY".into(), town_id.clone())]),
        players: BTreeMap::from([(
            "synthetic-login-key".into(),
            PrivateAccount {
                name: "Synthetic player".into(),
                town_id,
                salt: "SYNTHETIC_SALT_ONLY".into(),
                pin_hash: "SYNTHETIC_HASH_ONLY".into(),
            },
        )]),
    };
    ModelWorld::new(
        world,
        TerrainSeed(u64::MAX),
        ids.high_water() + 10,
        towns,
        credentials,
    )
    .unwrap()
}

fn value(world: &ModelWorld) -> Value {
    serde_json::from_str(&encode_save(world).unwrap()).unwrap()
}

#[test]
fn v1_migrates_explicitly_without_guessing_an_unsaved_leg_and_v2_requires_travel_facts() {
    use lantern_core::{model::TravelIntent, travel::Facing};
    let mut old = value(&fixture());
    assert_eq!(old["version"], 2);
    old["version"] = json!(1);
    for town in old["towns"].as_array_mut().unwrap() {
        for actor in town["actors"].as_array_mut().unwrap() {
            actor.as_object_mut().unwrap().remove("facing");
            actor.as_object_mut().unwrap().remove("travel");
        }
    }
    let migrated = decode_save(&old.to_string()).unwrap();
    for town in migrated.towns() {
        for actor in town.actors() {
            assert_eq!(actor.state().facing, Facing::Right);
            assert_eq!(actor.state().travel, TravelIntent::None);
        }
    }
    assert_eq!(value(&migrated)["version"], 2);
    for field in ["facing", "travel"] {
        let mut bad = value(&fixture());
        bad["towns"][0]["actors"][0]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(decode_save(&bad.to_string()).is_err());
        let mut bad = old.clone();
        bad["towns"][0]["actors"][0][field] = Value::Null;
        assert!(decode_save(&bad.to_string()).is_err());
    }
    let original = value(&fixture());
    for (path, invalid) in [
        ("/towns/0/actors/0/facing", json!(0)),
        ("/towns/0/actors/0/facing", json!(2)),
        ("/towns/0/actors/0/facing", json!(1.0)),
        ("/towns/0/actors/0/facing", Value::Null),
        ("/towns/0/actors/0/travel", Value::Null),
        (
            "/towns/0/actors/0/travel/active/destination/building/id/town_id",
            json!("foreign"),
        ),
        (
            "/towns/0/actors/0/travel/active/destination/building/id/serial",
            json!("999"),
        ),
        ("/towns/0/actors/0/task", json!({"idle":{}})),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(path).unwrap() = invalid;
        assert!(decode_save(&bad.to_string()).is_err(), "accepted {path}");
    }
    for path in [
        "/towns/0/actors/0/travel",
        "/towns/0/actors/0/travel/active",
        "/towns/0/actors/0/travel/active/destination/building",
    ] {
        let mut bad = original.clone();
        bad.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("waypoints".into(), json!([]));
        assert!(decode_save(&bad.to_string()).is_err());
    }
}

#[test]
fn every_route_status_roundtrips_pose_cargo_and_intent_but_not_paths_or_playback() {
    use lantern_core::travel::{Facing, Speed, Status};
    for kind in [
        "pending",
        "ready",
        "blocked",
        "unreachable",
        "arrived",
        "cancelled",
    ] {
        let mut world = fixture();
        let town_id = world.towns()[0].owner().town.clone();
        let id = world.towns()[0].actors()[0].state().id.clone();
        let destination = world.towns()[0].buildings()[0].id.clone();
        let town = world.town_mut(&town_id).unwrap();
        let r = town
            .request_route(
                &id,
                Destination::Building(destination),
                NavigationRevision(9),
            )
            .unwrap();
        match kind {
            "pending" => {}
            "unreachable" => town
                .accept_route(&id, &r, r.navigation(), RouteOutcome::Unreachable)
                .unwrap(),
            _ => {
                let from = town.actor(&id).unwrap().state().position;
                town.accept_route(
                    &id,
                    &r,
                    r.navigation(),
                    RouteOutcome::Ready(vec![position(from.x.get() - 1.0, from.y.get())]),
                )
                .unwrap();
                town.advance_travel(
                    &id,
                    Seconds::new(if kind == "arrived" { 2.0 } else { 0.125 }).unwrap(),
                    10,
                    |_, _| Speed::new(1.0),
                )
                .unwrap();
                assert_eq!(town.actor(&id).unwrap().state().facing, Facing::Left);
                if kind == "blocked" {
                    town.block_route(&id).unwrap();
                }
                if kind == "cancelled" {
                    town.cancel_route(&id).unwrap();
                }
            }
        }
        let before = town.actor(&id).unwrap().state().clone();
        let saved = encode_save(&world).unwrap();
        assert!(!saved.contains("samples"));
        assert!(!saved.contains("waypoints"));
        let mut restored = decode_save(&saved).unwrap();
        let town = restored.town_mut(&town_id).unwrap();
        assert_eq!(town.actor(&id).unwrap().state(), &before);
        assert_eq!(town.actor(&id).unwrap().route(), &RouteState::NotRequested);
        assert_eq!(
            town.actor(&id)
                .unwrap()
                .confirmed_playback()
                .samples()
                .len(),
            1
        );
        if ["arrived", "cancelled"].contains(&kind) {
            assert_eq!(
                town.resume_travel(&id, NavigationRevision(10)).unwrap(),
                None
            );
            assert!(
                !town
                    .advance_travel(&id, Seconds::new(10.0).unwrap(), 10, |_, _| panic!(
                        "terminal leg"
                    ))
                    .unwrap()
                    .arrived_now
            );
            assert_eq!(
                town.actor(&id).unwrap().travel_status(),
                if kind == "arrived" {
                    Status::Arrived
                } else {
                    Status::Cancelled
                }
            );
        } else {
            let new = town
                .resume_travel(&id, NavigationRevision(10))
                .unwrap()
                .unwrap();
            assert_eq!(new.origin(), before.position);
            assert_ne!(new.route_generation(), r.route_generation());
            assert!(
                town.accept_route(&id, &r, r.navigation(), RouteOutcome::Arrived)
                    .is_err()
            );
        }
    }
}

#[test]
fn public_playback_contains_only_confirmed_samples_and_is_detached_from_actor_state() {
    use lantern_core::travel::Speed;
    let mut world = fixture();
    let town_id = world.towns()[0].owner().town.clone();
    let id = world.towns()[0].actors()[0].state().id.clone();
    let town = world.town_mut(&town_id).unwrap();
    town.advance_travel(&id, Seconds::new(0.125).unwrap(), 1, |_, _| Speed::new(1.0))
        .unwrap();
    let facts = town.actor(&id).unwrap().state().clone();
    let mut public = serde_json::to_value(public_snapshot(&world)).unwrap();
    assert_eq!(public["format"], "lantern-model-observation-v2");
    let samples = &public["towns"]["a"]["villagers"][0]["playback"]["samples"];
    assert_eq!(samples.as_array().unwrap().len(), 2);
    assert_eq!(samples[1]["x"], json!(facts.position.x.get()));
    assert!(!samples.to_string().contains("cargo"));
    assert!(!samples.to_string().contains("task"));
    assert!(!samples.to_string().contains("arrived_now"));
    public["towns"]["a"]["villagers"][0]["playback"]["samples"][1]["x"] = json!(999);
    assert_eq!(world.towns()[0].actor(&id).unwrap().state(), &facts);
    assert!(decode_save(&public.to_string()).is_err());
}

fn rejected(value: Value) -> BoundaryError {
    match decode_save(&value.to_string()) {
        Err(e) => e,
        Ok(_) => panic!("invalid model save accepted"),
    }
}

fn reordered(town: &Town) -> Town {
    let mut buildings = town.buildings().to_vec();
    buildings.reverse();
    let mut actors: Vec<_> = town.actors().iter().map(|a| a.state().clone()).collect();
    actors.reverse();
    Town::restore(town.owner().clone(), buildings, actors).unwrap()
}

fn index(index: usize, id: &Value) -> IndexedReferenceDto {
    serde_json::from_value(json!({"index": index, "expected_id": id})).unwrap()
}

#[test]
fn save_roundtrip_retains_owned_facts_private_accounts_and_allocator() {
    let mut world = fixture();
    let a = TownId::new("a").unwrap();
    let building = world.towns()[0].buildings()[0].id.clone();
    let before = world.towns()[0].actors()[0].state().clone();
    world
        .town_mut(&a)
        .unwrap()
        .relocate_building(&building, Tile { x: 8, y: 4 })
        .unwrap();
    let saved = encode_save(&world).unwrap();
    let mut dto: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(dto["seed"], u64::MAX.to_string());
    assert_eq!(
        dto["towns"][0]["buildings"][0]["id"]["serial"],
        ((1_u64 << 53) + 1).to_string()
    );
    // Reordering serialized records cannot change stable references.
    dto["towns"][0]["buildings"]
        .as_array_mut()
        .unwrap()
        .reverse();
    dto["towns"][0]["actors"].as_array_mut().unwrap().reverse();
    let mut restored = decode_save(&dto.to_string()).unwrap();
    assert_eq!(restored.seed(), TerrainSeed(u64::MAX));
    assert_eq!(
        restored.towns()[0].actor(&before.id).unwrap().state(),
        &before
    );
    assert_eq!(
        restored.towns()[0].building(&building).unwrap().location,
        Tile { x: 8, y: 4 }
    );
    for town in restored.towns() {
        for actor in town.actors() {
            assert_eq!(actor.route(), &RouteState::NotRequested);
        }
    }
    assert_eq!(restored.credentials().tokens["SYNTHETIC_BEARER_ONLY"], a);
    assert_eq!(
        restored.credentials().players["synthetic-login-key"].pin_hash,
        "SYNTHETIC_HASH_ONLY"
    );
    assert_eq!(
        restored.credentials().players["synthetic-login-key"].salt,
        "SYNTHETIC_SALT_ONLY"
    );
    assert_eq!(
        restored.credentials().players["synthetic-login-key"].name,
        "Synthetic player"
    );
    let next: ActorId = restored.allocate(&a).unwrap();
    assert_eq!(next.identity().serial(), world.high_water() + 1);
    assert_eq!(encode_save(&decode_save(&saved).unwrap()).unwrap(), saved);
}

#[test]
fn binary64_fields_roundtrip_without_quantization() {
    let mut dto = value(&fixture());
    let amounts = dto["towns"][0]["actors"][0]["cargo"]
        .as_object_mut()
        .unwrap();
    let mut bits = 0x123456789abcdef0_u64;
    let mut expected = Vec::new();
    for n in 0..256 {
        bits ^= bits << 13;
        bits ^= bits >> 7;
        bits ^= bits << 17;
        let value = f64::from_bits(bits & 0x7fefffffffffffff);
        amounts.insert(format!("sample_{n}"), json!(value));
        expected.push(value);
    }
    amounts.insert("largest".into(), json!(f64::MAX));
    let restored = decode_save(&dto.to_string()).unwrap();
    let again = decode_save(&encode_save(&restored).unwrap()).unwrap();
    let goods = again.towns()[0].actors()[0].state().cargo.goods();
    for (n, value) in expected.iter().enumerate() {
        assert_eq!(
            goods[&format!("sample_{n}")].get().to_bits(),
            value.to_bits(),
            "sample {n}"
        );
    }
    assert_eq!(goods["largest"].get().to_bits(), f64::MAX.to_bits());
    assert_eq!(goods["tiny"].get().to_bits(), 1);
    assert_eq!(goods["zero"].get().to_bits(), (-0.0_f64).to_bits());
    let original = fixture();
    let restored = decode_save(&encode_save(&original).unwrap()).unwrap();
    for (old, new) in original.towns()[0]
        .actors()
        .iter()
        .zip(restored.towns()[0].actors())
    {
        assert_eq!(
            old.state().position.x.get().to_bits(),
            new.state().position.x.get().to_bits()
        );
        assert_eq!(
            old.state().position.y.get().to_bits(),
            new.state().position.y.get().to_bits()
        );
    }
}

#[test]
fn all_task_and_phase_variants_have_explicit_roundtrips() {
    let mut world = fixture();
    let town_id = world.towns()[0].owner().town.clone();
    let actor = world.towns()[0].actors()[0].state().id.clone();
    let building = world.towns()[0].buildings()[0].id.clone();
    let destination = world.towns()[0].buildings()[1].id.clone();
    let mut tasks = vec![
        Task::Idle,
        Task::Move {
            destination: position(0.1, -0.2),
        },
    ];
    for phase in [
        WorkPhase::Outbound,
        WorkPhase::Working,
        WorkPhase::Returning,
        WorkPhase::Depositing,
        WorkPhase::FetchingWater,
        WorkPhase::CollectingWater,
        WorkPhase::ReturningWater,
        WorkPhase::WaitingStorage,
        WorkPhase::WaitingField,
        WorkPhase::HarvestReady,
        WorkPhase::FindingAnimal,
        WorkPhase::Dragging,
    ] {
        tasks.push(Task::Work {
            workplace: building.clone(),
            target: Destination::Building(destination.clone()),
            phase,
            elapsed: Seconds::new(0.05).unwrap(),
        });
    }
    for phase in [
        TransportPhase::ToPickup,
        TransportPhase::PickingUp,
        TransportPhase::ToDropoff,
        TransportPhase::DroppingOff,
        TransportPhase::Returning,
    ] {
        tasks.push(Task::Transport {
            source: building.clone(),
            destination: destination.clone(),
            phase,
        });
    }
    for phase in [ConstructionPhase::Outbound, ConstructionPhase::Building] {
        tasks.push(Task::Construction {
            site: building.clone(),
            phase,
            elapsed: Seconds::new(0.1).unwrap(),
        });
    }
    for task in tasks {
        world
            .town_mut(&town_id)
            .unwrap()
            .set_task(&actor, task.clone())
            .unwrap();
        let restored = decode_save(&encode_save(&world).unwrap()).unwrap();
        assert_eq!(
            restored.towns()[0].actor(&actor).unwrap().state().task,
            task
        );
    }
}

#[test]
fn public_allowlist_excludes_credentials_counters_requests_and_paths() {
    let world = fixture();
    let public = serde_json::to_value(public_snapshot(&world)).unwrap();
    let text = public.to_string();
    for secret in [
        "SYNTHETIC_BEARER_ONLY",
        "SYNTHETIC_SALT_ONLY",
        "SYNTHETIC_HASH_ONLY",
        "synthetic-login-key",
        "987654.321",
        "123456.789",
    ] {
        assert!(!text.contains(secret));
    }
    fn keys(value: &Value) {
        match value {
            Value::Object(object) => {
                for (key, value) in object {
                    assert!(
                        ![
                            "tokens",
                            "players",
                            "credentials",
                            "salt",
                            "pin_hash",
                            "high_water",
                            "task_generation",
                            "route_generation",
                            "request",
                            "waypoints",
                            "frontier",
                            "cache"
                        ]
                        .contains(&key.as_str()),
                        "unexpected private field {key}"
                    );
                    keys(value);
                }
            }
            Value::Array(array) => {
                for value in array {
                    keys(value);
                }
            }
            _ => {}
        }
    }
    keys(&public);
    assert_eq!(
        public
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["format", "seed", "towns", "world_id"]
    );
    let villager = &public["towns"]["a"]["villagers"][0];
    assert_eq!(villager["route_status"], "ready");
    assert_eq!(villager["building_index"], 0);
    assert_eq!(villager["pending_output"]["wood"], json!(0.1));
    assert!(decode_save(&text).is_err());
    let save = encode_save(&world).unwrap();
    assert!(save.contains("SYNTHETIC_HASH_ONLY"));
    assert!(!save.contains("987654.321"));
    assert!(!save.contains("route_status"));
    // DTOs are detached copies, never aliases into authoritative state.
    let mut changed = public;
    changed["towns"]["a"]["villagers"][0]["pending_output"]["wood"] = json!(900);
    assert_eq!(
        world.towns()[0].actors()[0].state().cargo.goods()["wood"].get(),
        0.1
    );
}

#[test]
fn unknown_fields_and_versions_are_rejected_at_every_boundary_depth() {
    let original = value(&fixture());
    for path in [
        "",
        "/towns/0",
        "/towns/0/buildings/0",
        "/towns/0/buildings/0/id",
        "/towns/0/buildings/0/construction",
        "/towns/0/buildings/0/construction/complete",
        "/towns/0/actors/0",
        "/towns/0/actors/0/position",
        "/towns/0/actors/0/task",
        "/towns/0/actors/0/task/work",
        "/towns/0/actors/0/task/work/target",
        "/towns/0/actors/0/task/work/target/position",
        "/towns/0/actors/0/task/work/target/position/position",
        "/credentials",
        "/credentials/players/synthetic-login-key",
    ] {
        let mut bad = original.clone();
        bad.pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(
                "unsupported_private_payload".into(),
                json!({"secret": "SYNTHETIC_UNSUPPORTED"}),
            );
        assert!(
            decode_save(&bad.to_string()).is_err(),
            "unknown field accepted at {path}"
        );
    }
    let mut bad = original.clone();
    bad["version"] = json!(3);
    assert_eq!(rejected(bad).path, "$.version");
    let mut bad = original;
    bad["towns"][0]["actors"][1]["task"] = json!({"idle":{"private_cache":123}});
    assert!(decode_save(&bad.to_string()).is_err());
}

#[test]
fn duplicate_objects_do_not_overwrite_goods_tokens_or_accounts() {
    let world = fixture();
    let raw = encode_save(&world).unwrap();
    for (needle, replacement) in [
        ("\"cargo\":{", "\"cargo\":{\"duplicate\":1,\"duplicate\":2,"),
        (
            "\"tokens\":{",
            "\"tokens\":{\"duplicate\":\"a\",\"duplicate\":\"b\",",
        ),
        ("\"version\":2", "\"version\":2,\"version\":2"),
    ] {
        assert!(decode_save(&raw.replacen(needle, replacement, 1)).is_err());
    }
    let account = value(&world)["credentials"]["players"]["synthetic-login-key"].to_string();
    let replacement = format!("\"players\":{{\"duplicate\":{account},\"duplicate\":{account},");
    assert!(decode_save(&raw.replacen("\"players\":{", &replacement, 1)).is_err());
}

#[test]
fn malformed_units_ids_and_scopes_are_rejected_with_safe_diagnostics() {
    let original = value(&fixture());
    for (path, replacement) in [
        ("/seed", json!(u64::MAX)),
        ("/seed", json!("01")),
        ("/seed", json!("18446744073709551616")),
        ("/towns/0/actors/0/id/serial", json!("0")),
        ("/towns/0/actors/0/id/world_id", json!("foreign")),
        ("/towns/0/actors/0/task/work/workplace/town_id", json!("b")),
        ("/towns/0/actors/0/task/work/workplace/serial", json!("999")),
        ("/towns/0/actors/0/task/work/elapsed", json!(-0.1)),
        ("/towns/0/actors/0/cargo/wood", json!(-0.1)),
        ("/towns/0/actors/0/position/x", Value::Null),
        (
            "/towns/0/actors/0/position/x",
            json!({"$serde_json::private::Number":"0.1"}),
        ),
        (
            "/credentials/tokens/SYNTHETIC_BEARER_ONLY",
            json!("missing"),
        ),
        (
            "/credentials/players/synthetic-login-key/town_id",
            json!("missing"),
        ),
        (
            "/credentials/players/synthetic-login-key/salt",
            json!({"secret":"SYNTHETIC_DO_NOT_LOG"}),
        ),
    ] {
        let mut bad = original.clone();
        *bad.pointer_mut(path).unwrap() = replacement;
        let error = rejected(bad);
        assert!(error.path.starts_with('$'));
        assert!(!format!("{error:?}").contains("SYNTHETIC_DO_NOT_LOG"));
    }
    let raw = encode_save(&fixture()).unwrap();
    for number in ["NaN", "Infinity", "-Infinity", "1e999"] {
        assert!(
            decode_save(&raw.replacen("\"wood\":0.1", &format!("\"wood\":{number}"), 1)).is_err()
        );
    }
    let mut bad = original;
    bad["towns"][0]["actors"][0]["cargo"]["wood"] = json!(-0.5);
    assert!(rejected(bad).path.contains(".cargo["));
}

#[test]
fn reload_rejects_duplicate_world_serials_towns_and_stale_allocators() {
    let original = value(&fixture());
    let mut bad = original.clone();
    bad["high_water"] = json!("1");
    assert!(decode_save(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["towns"]
        .as_array_mut()
        .unwrap()
        .push(original["towns"][0].clone());
    assert!(decode_save(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["towns"][1]["actors"][1]["id"]["serial"] =
        original["towns"][0]["actors"][1]["id"]["serial"].clone();
    assert!(decode_save(&bad.to_string()).is_err());
    let mut bad = original.clone();
    bad["towns"][0]["actors"][0]["id"]["serial"] =
        original["towns"][0]["buildings"][0]["id"]["serial"].clone();
    assert!(decode_save(&bad.to_string()).is_err());
    let mut bad = original;
    bad["towns"][0]["buildings"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert!(decode_save(&bad.to_string()).is_err());
}

#[test]
fn index_validation_before_queue_and_id_resolution_after_queue_prevent_retargeting() {
    let world = fixture();
    let town = &world.towns()[0];
    let public = serde_json::to_value(public_snapshot(&world)).unwrap();
    let building = &public["towns"]["a"]["buildings"][1]["id"];
    let actor = &public["towns"]["a"]["villagers"][1]["id"];
    let queued_building = admit_building(town, town.owner(), index(1, building)).unwrap();
    let queued_actor = admit_actor(town, town.owner(), index(1, actor)).unwrap();
    let changed = reordered(town);
    assert!(admit_building(&changed, town.owner(), index(1, building)).is_err());
    assert!(admit_actor(&changed, town.owner(), index(1, actor)).is_err());
    assert_eq!(
        resolve_building(&changed, town.owner(), &queued_building)
            .unwrap()
            .id,
        *queued_building.id()
    );
    assert_eq!(
        resolve_actor(&changed, town.owner(), &queued_actor)
            .unwrap()
            .state()
            .id,
        *queued_actor.id()
    );
    for reference in [
        json!({"index":1}),
        json!({"index":-1,"expected_id":building}),
        json!({"index":1,"expected_id":building,"unknown":1}),
    ] {
        assert!(serde_json::from_value::<IndexedReferenceDto>(reference).is_err());
    }
    assert!(admit_building(town, town.owner(), index(999, building)).is_err());
    let wrong = world.towns()[1].owner();
    assert!(admit_building(town, wrong, index(1, building)).is_err());
    assert!(resolve_building(&world.towns()[1], wrong, &queued_building).is_err());
    assert!(resolve_actor(&world.towns()[1], wrong, &queued_actor).is_err());
    let foreign = Owner {
        world: WorldId::new("foreign").unwrap(),
        town: town.owner().town.clone(),
    };
    assert!(resolve_building(town, &foreign, &queued_building).is_err());
    let retained_buildings = vec![town.buildings()[0].clone()];
    let retained_actors = vec![town.actors()[0].state().clone()];
    let removed = Town::restore(town.owner().clone(), retained_buildings, retained_actors).unwrap();
    assert!(resolve_building(&removed, town.owner(), &queued_building).is_err());
    assert!(resolve_actor(&removed, town.owner(), &queued_actor).is_err());
}

#[test]
fn public_indices_are_derived_and_never_mutate_saved_references() {
    let world = fixture();
    let mut saved = value(&world);
    saved["towns"][0]["buildings"]
        .as_array_mut()
        .unwrap()
        .reverse();
    let mut restored = decode_save(&saved.to_string()).unwrap();
    let town = &restored.towns()[0];
    let id = town.actors()[0].state().id.clone();
    let public = serde_json::to_value(public_snapshot(&restored)).unwrap();
    assert_eq!(public["towns"]["a"]["villagers"][0]["building_index"], 1);
    assert!(
        public["towns"]["a"]["villagers"][1]
            .get("building_index")
            .is_none()
    );
    let owner = town.owner().clone();
    let building_id = public["towns"]["a"]["buildings"][1]["id"].clone();
    restored
        .town_mut(&owner.town)
        .unwrap()
        .set_task(&id, Task::Idle)
        .unwrap();
    // Task/output changes do not invalidate an unchanged index/ID slot.
    assert!(admit_building(&restored.towns()[0], &owner, index(1, &building_id)).is_ok());
    let missing = BuildingId::from_saved(EntityIdentity::from_saved(owner.clone(), 42).unwrap());
    let wrong = serde_json::to_value(lantern_host::dto::EntityIdDto::from_id(&missing)).unwrap();
    assert!(admit_building(&restored.towns()[0], &owner, index(1, &wrong)).is_err());
}

#[test]
fn json_reload_rebuilds_pending_routes_without_reusing_request_generations() {
    let mut world = fixture();
    let owner = world.towns()[0].owner().clone();
    let actor = world.towns()[0].actors()[0].state().id.clone();
    let building = world.towns()[0].buildings()[0].id.clone();
    let old = world
        .town_mut(&owner.town)
        .unwrap()
        .request_route(
            &actor,
            Destination::Building(building.clone()),
            NavigationRevision(8),
        )
        .unwrap();
    let mut restored = decode_save(&encode_save(&world).unwrap()).unwrap();
    let town = restored.town_mut(&owner.town).unwrap();
    assert_eq!(
        town.actor(&actor).unwrap().route(),
        &RouteState::NotRequested
    );
    assert!(
        town.accept_route(&actor, &old, NavigationRevision(8), RouteOutcome::Arrived)
            .is_err()
    );
    let new = town
        .request_route(
            &actor,
            Destination::Building(building),
            NavigationRevision(8),
        )
        .unwrap();
    assert_ne!(new, old);
    assert!(
        town.accept_route(&actor, &old, NavigationRevision(8), RouteOutcome::Arrived)
            .is_err()
    );
    town.accept_route(&actor, &new, NavigationRevision(8), RouteOutcome::Arrived)
        .unwrap();
}

#[test]
fn oversized_integer_amounts_are_rejected_instead_of_silently_rounded() {
    let original = value(&fixture());
    for integer in [(1_u64 << 53) + 1, u64::MAX] {
        let mut bad = original.clone();
        bad["towns"][0]["actors"][0]["cargo"]["wood"] = json!(integer);
        assert!(decode_save(&bad.to_string()).is_err());
    }
    let raw = encode_save(&fixture()).unwrap();
    assert!(
        decode_save(&raw.replacen("\"wood\":0.1", "\"wood\":184467440737095516170001", 1)).is_err()
    );
    let mut exact = original;
    exact["towns"][0]["actors"][0]["cargo"]["wood"] = json!(1_u64 << 53);
    let world = decode_save(&exact.to_string()).unwrap();
    assert_eq!(
        world.towns()[0].actors()[0].state().cargo.goods()["wood"].get(),
        (1_u64 << 53) as f64
    );
}
