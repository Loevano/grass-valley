//! Disposable protocol fixture builder, never a playable-save conversion tool.
//! Reads only synthetic test input on stdin and creates a NEW explicit native root.
use lantern_core::{
    ids::{Owner, TownId, WorldId},
    model::Town,
    units::{Quantity, TerrainSeed},
};
use lantern_host::{
    legacy::{ChunkKey, TerrainArchive},
    persistence::{RegistrySnapshot, Store, TownState, WorldSnapshot},
    resources::Ledger,
    state::{ModelWorld, PrivateCredentials},
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    format: String,
    active: String,
    worlds: Vec<World>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct World {
    id: String,
    name: String,
    world: Box<RawValue>,
    records: BTreeMap<String, Box<RawValue>>,
}
#[derive(Deserialize)]
struct Ids {
    towns: BTreeMap<String, serde::de::IgnoredAny>,
}
fn main() {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .expect("explicit new disposable root"),
    );
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let fixture: Fixture = serde_json::from_str(&input).unwrap();
    assert_eq!(fixture.format, "lantern-http-synthetic-v1");
    let active = WorldId::new(fixture.active).unwrap();
    let mut snapshots = Vec::new();
    for w in fixture.worlds {
        let id = WorldId::new(w.id).unwrap();
        let ids: Ids = serde_json::from_str(w.world.get()).unwrap();
        let mut towns = Vec::new();
        let mut states = Vec::new();
        for key in ids.towns.keys() {
            let owner = Owner {
                world: id.clone(),
                town: TownId::new(key).unwrap(),
            };
            towns.push(Town::restore(owner.clone(), vec![], vec![]).unwrap());
            states.push(TownState::empty(
                Ledger::new(owner, Quantity::new(0.).unwrap(), vec![], vec![]).unwrap(),
            ));
        }
        let model =
            ModelWorld::new(id, TerrainSeed(1), 0, towns, PrivateCredentials::default()).unwrap();
        let records = w
            .records
            .into_iter()
            .map(|(key, value)| {
                let (x, y) = key.split_once(',').unwrap();
                (
                    ChunkKey {
                        x: x.parse().unwrap(),
                        y: y.parse().unwrap(),
                    },
                    value.get().as_bytes().to_vec(),
                )
            })
            .collect();
        let terrain = TerrainArchive::decode(w.world.get().as_bytes(), records).unwrap();
        snapshots.push(
            WorldSnapshot::capture(&model, &states)
                .with_name(&w.name)
                .with_archive(&terrain),
        );
    }
    Store::create(&root)
        .unwrap()
        .commit(RegistrySnapshot::capture(Some(&active), snapshots))
        .unwrap();
}
