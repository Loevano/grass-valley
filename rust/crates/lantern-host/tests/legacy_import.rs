use lantern_host::{
    BoundaryError,
    legacy::{import_bytes, *},
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const REGISTRY: &[u8] = include_bytes!("fixtures/ordinary_registry.json");
const PROVISION: &[u8] = include_bytes!("fixtures/ordinary_registry.players.json");
const LEGACY: &[u8] = include_bytes!("fixtures/legacy_world.json");

fn reject(bytes: &[u8]) -> BoundaryError {
    match import_bytes(Some(bytes), None) {
        Err(error) => error,
        Ok(_) => panic!("invalid legacy save accepted"),
    }
}

fn changed(pointer: &str, value: Value) -> Vec<u8> {
    let mut root: Value = serde_json::from_slice(REGISTRY).unwrap();
    *root.pointer_mut(pointer).unwrap() = value;
    serde_json::to_vec(&root).unwrap()
}

#[test]
fn legacy_envelope_keeps_ownership_and_does_not_invent_missing_fields() {
    let imported = import_bytes(Some(LEGACY), None).unwrap();
    assert_eq!(imported.private_source_bytes(), Some(LEGACY));
    assert_eq!(imported.active_world().unwrap().as_str(), "legacy");
    let world = &imported.worlds()[0];
    assert_eq!(world.name(), "My first world");
    assert_eq!(world.layout(), LandscapeLayout::Independent);
    assert_eq!(
        world.credentials().tokens["SYNTHETIC_LEGACY_TOKEN"].as_str(),
        "old-town"
    );
    assert!(world.credentials().players.is_empty());
    assert!(world.towns()[0].founder().is_none());
    assert_eq!(world.towns()[0].buildings()[0].identity().serial(), 1);
    assert_eq!(world.towns()[0].villagers()[0].identity().serial(), 3);
    assert!(
        matches!(&world.towns()[0].references()[0], BoundReference::Assignment { building, .. } if building == &world.towns()[0].buildings()[1])
    );
    let original: Value = serde_json::from_slice(LEGACY).unwrap();
    let preserved: Value =
        serde_json::from_slice(imported.private_source_bytes().unwrap()).unwrap();
    let total = |root: &Value| {
        root["world"]["towns"]["old-town"]["wood"].as_f64().unwrap()
            + root["world"]["towns"]["old-town"]["villagers"][0]["pending_output"]["wood"]
                .as_f64()
                .unwrap()
    };
    assert_eq!(total(&original), 12.25);
    assert_eq!(total(&preserved), total(&original));
    assert!(
        preserved["world"]["towns"]["old-town"]
            .get("player")
            .is_none()
    );
}

#[test]
fn registry_import_preserves_record_order_scopes_and_deterministic_bindings() {
    let first = import_bytes(Some(REGISTRY), Some(PROVISION)).unwrap();
    let second = import_bytes(Some(REGISTRY), Some(PROVISION)).unwrap();
    assert_eq!(first.active_world().unwrap().as_str(), "a-world");
    assert_eq!(
        first
            .worlds()
            .iter()
            .map(|w| w.id().as_str())
            .collect::<Vec<_>>(),
        ["z-world", "a-world"]
    );
    let world = &first.worlds()[0];
    assert_eq!(
        world
            .towns()
            .iter()
            .map(|t| t.owner().town.as_str())
            .collect::<Vec<_>>(),
        ["z-town", "a-town"]
    );
    assert_eq!(world.high_water(), 10);
    let town = &world.towns()[0];
    assert_eq!(
        town.buildings()
            .iter()
            .map(|id| id.identity().serial())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(
        town.villagers()
            .iter()
            .map(|id| id.identity().serial())
            .collect::<Vec<_>>(),
        [6, 7]
    );
    assert_eq!(town.founder().unwrap().identity().serial(), 8);
    assert_eq!(world.towns()[1].buildings()[0].identity().serial(), 9);
    assert_eq!(world.towns()[1].villagers()[0].identity().serial(), 10);
    assert_eq!(town.references().len(), 3);
    assert!(
        matches!(&town.references()[0], BoundReference::Building { field: BuildingLink::Farm, source, target } if source == &town.buildings()[2] && target == &town.buildings()[1])
    );
    assert!(
        matches!(&town.references()[1], BoundReference::Building { field: BuildingLink::Orchard, source, target } if source == &town.buildings()[4] && target == &town.buildings()[3])
    );
    assert_ne!(
        town.buildings()[0],
        first.worlds()[1].towns()[0].buildings()[0]
    );
    assert_eq!(first.worlds()[1].layout(), LandscapeLayout::Shared);
    for (left, right) in first.worlds().iter().zip(second.worlds()) {
        assert_eq!(left.high_water(), right.high_water());
        for (left, right) in left.towns().iter().zip(right.towns()) {
            assert_eq!(left.buildings(), right.buildings());
            assert_eq!(left.villagers(), right.villagers());
            assert_eq!(left.founder(), right.founder());
            assert_eq!(left.references(), right.references());
        }
    }
    assert_eq!(first.report(), second.report());
}

#[test]
fn private_payloads_keep_exact_quantities_and_every_unported_field() {
    let imported = import_bytes(Some(REGISTRY), Some(PROVISION)).unwrap();
    assert_eq!(imported.private_source_bytes(), Some(REGISTRY));
    assert_eq!(imported.private_provision_bytes(), Some(PROVISION));
    let source = std::str::from_utf8(imported.private_source_bytes().unwrap()).unwrap();
    for exact in [
        "9007199254740993",
        "18446744073709551615",
        "1.2345600e-3",
        "123456789012345678901234567890",
        "\"phase\": \"returning\"",
        "opaque-offer-kept",
        "SYNTHETIC_PRIVATE_VALUE",
    ] {
        assert!(source.contains(exact));
    }
    let public = serde_json::to_string(&imported.report()).unwrap();
    for private in [
        "SYNTHETIC_TOKEN",
        "SYNTHETIC_HASH",
        "SYNTHETIC_SALT",
        "synthetic-é",
        "Synthetic É",
        "SYNTHETIC_PRIVATE",
        "SYNTHETIC_IGNORED",
        "opaque-offer-kept",
        "12.345678901234567",
        "returning",
        "future_payload",
    ] {
        assert!(!public.contains(private), "private payload leaked");
    }
    assert_eq!(imported.report().readiness, "preserved_only");
    assert!(!imported.report().preserved.is_empty());
    // Unknown keys can themselves be credentials. Reports use safe ordinal paths.
    let hostile = br#"{"world":{"towns":{}},"tokens":{},"SECRET_UNRECOGNIZED_KEY":{"SECRET_NESTED_KEY":"SECRET_VALUE"}}"#;
    let imported = import_bytes(Some(hostile), None).unwrap();
    assert!(!format!("{:?}", imported.report()).contains("SECRET_"));
    assert_eq!(imported.private_source_bytes(), Some(hostile.as_slice()));
}

#[test]
fn provisioning_merges_known_owners_and_saved_accounts_win_verbatim() {
    let imported = import_bytes(Some(REGISTRY), Some(PROVISION)).unwrap();
    let players = &imported.worlds()[0].credentials().players;
    assert_eq!(players.len(), 2);
    let main = &players["synthetic-é"];
    assert_eq!(main.name, "  Synthetic É  ");
    assert_eq!(main.pin_hash, "SYNTHETIC_HASH_MAIN");
    assert_eq!(main.salt, "SYNTHETIC_SALT_MAIN");
    assert_eq!(players["second-player"].town_id.as_str(), "a-town");
    assert_eq!(players["second-player"].pin_hash, "SYNTHETIC_HASH_SECOND");
    assert_eq!(imported.report().ignored_provision_accounts, 2);
    assert_eq!(imported.report().ignored_provision_worlds, 1);
    let unknown_main = changed(
        "/worlds/z-world/tokens/SYNTHETIC_TOKEN_Z",
        json!("foreign-town"),
    );
    assert!(reject(&unknown_main).path.contains(".tokens["));
    let bad_account = changed(
        "/worlds/z-world/players/synthetic-é/town_id",
        json!("foreign-town"),
    );
    assert!(reject(&bad_account).path.ends_with(".town_id"));
    let bad_type = br#"{"z-world":{"new-login":{"town_id":"z-town","name":"Test","salt":"SYNTHETIC_SECRET","pin_hash":{}}}}"#;
    let error = match import_bytes(Some(REGISTRY), Some(bad_type)) {
        Err(e) => e,
        Ok(_) => panic!("malformed account accepted"),
    };
    assert!(!format!("{error:?}").contains("SYNTHETIC_SECRET"));
    assert!(error.path.ends_with(".pin_hash"));
}

#[test]
fn active_selection_absent_saves_and_empty_registries_do_not_create_worlds() {
    let missing = import_bytes(None, None).unwrap();
    assert!(missing.worlds().is_empty());
    assert!(missing.active_world().is_none());
    assert!(missing.private_source_bytes().is_none());
    for selection in [json!(null), json!("deleted-world")] {
        let source = changed("/active_world", selection);
        let imported = import_bytes(Some(&source), None).unwrap();
        assert!(imported.active_world().is_none());
        assert_eq!(imported.worlds().len(), 2);
        assert_eq!(imported.private_source_bytes(), Some(source.as_slice()));
    }
    let empty = import_bytes(Some(br#"{"worlds":{},"active_world":null}"#), None).unwrap();
    assert!(empty.worlds().is_empty());
    assert!(import_bytes(Some(b""), None).is_err());
    let missing_active = br#"{"worlds":{}}"#;
    assert_eq!(reject(missing_active).path, "$.active_world");
}

#[test]
fn invalid_indices_coordinates_shapes_and_foreign_owners_fail_explicitly() {
    for (path, value, suffix) in [
        (
            "/worlds/z-world/world/towns/z-town/villagers/0/building_index",
            json!(999),
            ".building_index",
        ),
        (
            "/worlds/z-world/world/towns/z-town/villagers/0/building_index",
            json!(-1),
            ".building_index",
        ),
        (
            "/worlds/z-world/world/towns/z-town/villagers/0/building_index",
            json!(true),
            ".building_index",
        ),
        (
            "/worlds/z-world/world/towns/z-town/buildings/2/farm_index",
            json!(0),
            ".farm_index",
        ),
        (
            "/worlds/z-world/world/towns/z-town/buildings/4/orchard_index",
            json!(99),
            ".orchard_index",
        ),
        (
            "/worlds/z-world/world/towns/z-town/buildings/0/x",
            json!(0.5),
            ".x",
        ),
        (
            "/worlds/z-world/world/towns/z-town/buildings/0/kind",
            json!(""),
            ".kind",
        ),
        (
            "/worlds/z-world/world/towns/z-town/villagers",
            json!({}),
            ".villagers",
        ),
        (
            "/worlds/z-world/world/towns/z-town/player",
            json!(null),
            ".player",
        ),
        (
            "/worlds/z-world/world/towns/z-town/buildings",
            json!([]),
            ".building_index",
        ),
    ] {
        let source = changed(path, value);
        assert!(
            reject(&source).path.ends_with(suffix),
            "unexpected error path"
        );
    }
    assert!(
        import_bytes(
            Some(br#"{"worlds":{},"active_world":null,"world":{}}"#),
            None
        )
        .is_err()
    );
    assert!(import_bytes(Some(br#"{"format":"lantern-model","version":1}"#), None).is_err());
}

#[test]
fn versions_are_validated_without_migration_or_chunk_attachment() {
    for version in 1..=7 {
        let source = changed(
            "/worlds/a-world/world/shared_map/terrain_version",
            json!(version),
        );
        let imported = import_bytes(Some(&source), None).unwrap();
        assert_eq!(imported.private_source_bytes(), Some(source.as_slice()));
    }
    for bad in [json!(0), json!(8), json!(1.5), json!("7"), json!(null)] {
        let error = reject(&changed(
            "/worlds/a-world/world/shared_map/terrain_version",
            bad,
        ));
        assert!(error.path.ends_with(".terrain_version"));
    }
    for marker in [
        "inventory_version",
        "wood_chain_version",
        "industry_version",
        "food_balance_version",
        "npc_roster_version",
    ] {
        let mut root: Value = serde_json::from_slice(REGISTRY).unwrap();
        root["worlds"]["z-world"]["world"]["towns"]["z-town"][marker] = json!(99);
        assert!(
            reject(&serde_json::to_vec(&root).unwrap())
                .path
                .ends_with(marker)
        );
    }
    for location in [
        "/worlds/a-world/world/shared_map",
        "/worlds/z-world/world/towns/z-town",
    ] {
        let mut root: Value = serde_json::from_slice(REGISTRY).unwrap();
        root.pointer_mut(location)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("chunked".into(), json!(true));
        let error = reject(&serde_json::to_vec(&root).unwrap());
        assert!(error.path.ends_with(".chunked"));
        assert!(format!("{error:?}").contains("slice 2"));
    }
}

#[test]
fn malformed_and_duplicate_private_json_never_echoes_values_or_loses_keys() {
    for source in [
        br#"{"world":{"towns":{}},"tokens":{"SECRET_KEY":"SECRET_VALUE","SECRET_KEY":"other"}}"#
            .as_slice(),
        br#"{"world":{"towns":{},"opaque":{"x":1,"x":2}},"tokens":{}}"#.as_slice(),
        br#"{"world":{"towns":{}},"tokens":{},"opaque":{"private":1e999}}"#.as_slice(),
        br#"{"world":{"towns":{}},"tokens":{},"opaque":NaN}"#.as_slice(),
        br#"{"world":{"towns":{}},"tokens":{},"opaque":Infinity}"#.as_slice(),
        br#"{"world":{"towns":{}},"tokens":{},"opaque":"SECRET_UNTERMINATED}"#.as_slice(),
    ] {
        let error = reject(source);
        assert!(error.path.starts_with('$'));
        assert!(!format!("{error:?}").contains("SECRET_"));
    }
    let deep = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    assert!(import_bytes(Some(deep.as_bytes()), None).is_err());
}

struct Copies(PathBuf);
impl Copies {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "lantern-legacy-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        Self(directory)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Copies {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn read_only(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn filesystem_import_leaves_read_only_copies_untouched_on_success_and_failure() {
    let copies = Copies::new();
    let save = copies.path("synthetic-save.json");
    let provision = copies.path("synthetic-save.players.json");
    let bad = copies.path("bad.json");
    fs::write(&save, REGISTRY).unwrap();
    fs::write(&provision, PROVISION).unwrap();
    fs::write(&bad, b"invalid private source").unwrap();
    for path in [&save, &provision, &bad] {
        read_only(path);
    }
    let before = [&save, &provision, &bad].map(|p| {
        (
            fs::read(p).unwrap(),
            fs::metadata(p).unwrap().modified().unwrap(),
        )
    });
    let imported = read_copies(&save, Some(&provision)).unwrap();
    assert_eq!(imported.private_source_bytes(), Some(REGISTRY));
    assert!(read_copies(&bad, Some(&provision)).is_err());
    assert!(read_copies(&save, Some(&bad)).is_err());
    assert!(read_copies(&save, Some(&copies.path("absent-players.json"))).is_err());
    assert!(read_copies(&copies.0, None).is_err());
    assert!(
        read_copies(&copies.path("absent-save.json"), None)
            .unwrap()
            .worlds()
            .is_empty()
    );
    for (path, original) in [&save, &provision, &bad].into_iter().zip(before) {
        assert_eq!(fs::read(path).unwrap(), original.0);
        assert_eq!(fs::metadata(path).unwrap().modified().unwrap(), original.1);
    }
    assert_eq!(fs::read_dir(&copies.0).unwrap().count(), 3);
}
