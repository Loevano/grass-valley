//! R06 slice 1: read-only, lossless staging of ordinary Python saves.
//! This archive is not a playable world or a Rust model save. Unported payloads
//! remain private, in their original bytes/order; no migrations or clock updates.
//!
//! ```compile_fail
//! use lantern_host::legacy::ImportedRegistry;
//! fn unsafe_report(imported: &ImportedRegistry) { serde_json::to_string(imported).unwrap(); }
//! ```

mod json;
mod ordinary;

use crate::{BoundaryError, ErrorKind, state::PrivateCredentials};
use json::Node;
use lantern_core::ids::{ActorId, BuildingId, IdAllocator, Owner, TownId, WorldId};
use serde::Serialize;
use std::{collections::BTreeSet, fs, io::ErrorKind as IoErrorKind, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LandscapeLayout {
    Independent,
    Shared,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildingLink {
    Farm,
    Orchard,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoundReference {
    Assignment {
        actor: ActorId,
        building: BuildingId,
    },
    Building {
        source: BuildingId,
        field: BuildingLink,
        target: BuildingId,
    },
}

pub struct ImportedTown {
    pub(crate) owner: Owner,
    pub(crate) buildings: Vec<BuildingId>,
    pub(crate) villagers: Vec<ActorId>,
    pub(crate) founder: Option<ActorId>,
    pub(crate) references: Vec<BoundReference>,
}

impl ImportedTown {
    pub fn owner(&self) -> &Owner {
        &self.owner
    }
    pub fn buildings(&self) -> &[BuildingId] {
        &self.buildings
    }
    pub fn villagers(&self) -> &[ActorId] {
        &self.villagers
    }
    pub fn founder(&self) -> Option<&ActorId> {
        self.founder.as_ref()
    }
    pub fn references(&self) -> &[BoundReference] {
        &self.references
    }
}

pub struct ImportedWorld {
    id: WorldId,
    name: String,
    layout: LandscapeLayout,
    allocator: IdAllocator,
    towns: Vec<ImportedTown>,
    credentials: PrivateCredentials,
}

impl ImportedWorld {
    pub fn id(&self) -> &WorldId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn layout(&self) -> LandscapeLayout {
        self.layout
    }
    pub fn towns(&self) -> &[ImportedTown] {
        &self.towns
    }
    pub fn high_water(&self) -> u64 {
        self.allocator.high_water()
    }
    pub fn credentials(&self) -> &PrivateCredentials {
        &self.credentials
    }
}

/// No Debug/Serialize; private source and provisioning may contain credentials.
pub struct ImportedRegistry {
    source: Option<Vec<u8>>,
    provision: Option<Vec<u8>>,
    active_world: Option<WorldId>,
    worlds: Vec<ImportedWorld>,
    preserved: Vec<PreservedField>,
    ignored_provision_worlds: usize,
    ignored_provision_accounts: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PreservedField {
    pub path: String,
    pub reason: &'static str,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct WorldReport {
    pub world_id: String,
    pub layout: LandscapeLayout,
    pub towns: usize,
    pub buildings: usize,
    pub villagers: usize,
    pub founders: usize,
    pub accounts: usize,
}

/// A safe allowlist for future inspection output, never arbitrary source JSON.
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct InspectionReport {
    pub format: &'static str,
    pub readiness: &'static str,
    pub active_world: Option<String>,
    pub worlds: Vec<WorldReport>,
    pub preserved: Vec<PreservedField>,
    pub ignored_provision_worlds: usize,
    pub ignored_provision_accounts: usize,
}

impl ImportedRegistry {
    pub fn active_world(&self) -> Option<&WorldId> {
        self.active_world.as_ref()
    }
    pub fn worlds(&self) -> &[ImportedWorld] {
        &self.worlds
    }
    /// Explicit private access for later copy/export tooling. Never public output.
    pub fn private_source_bytes(&self) -> Option<&[u8]> {
        self.source.as_deref()
    }
    pub fn private_provision_bytes(&self) -> Option<&[u8]> {
        self.provision.as_deref()
    }
    pub fn report(&self) -> InspectionReport {
        InspectionReport {
            format: "lantern-legacy-inspection-v1",
            readiness: "preserved_only",
            active_world: self.active_world.as_ref().map(|id| id.as_str().into()),
            worlds: self
                .worlds
                .iter()
                .map(|world| WorldReport {
                    world_id: world.id.as_str().into(),
                    layout: world.layout,
                    towns: world.towns.len(),
                    buildings: world.towns.iter().map(|t| t.buildings.len()).sum(),
                    villagers: world.towns.iter().map(|t| t.villagers.len()).sum(),
                    founders: world.towns.iter().filter(|t| t.founder.is_some()).count(),
                    accounts: world.credentials.players.len(),
                })
                .collect(),
            preserved: self.preserved.clone(),
            ignored_provision_worlds: self.ignored_provision_worlds,
            ignored_provision_accounts: self.ignored_provision_accounts,
        }
    }
}

fn opaque_world(value: &str, path: &str) -> Result<WorldId, BoundaryError> {
    WorldId::new(value).map_err(|e| BoundaryError::at(path, ErrorKind::Identity(e)))
}

fn opaque_town(value: &str, path: &str) -> Result<TownId, BoundaryError> {
    TownId::new(value).map_err(|e| BoundaryError::at(path, ErrorKind::Identity(e)))
}

fn preserved_fields(
    node: &Node,
    known: &[&str],
    path: &str,
    preserved: &mut Vec<PreservedField>,
) -> Result<(), BoundaryError> {
    for (index, (key, _)) in node.object(path)?.iter().enumerate() {
        if !known.contains(&key.as_str()) {
            preserved.push(PreservedField {
                path: format!("{path}.fields[{index}]"),
                reason: "unrecognized field retained privately",
            });
        }
    }
    Ok(())
}

fn world(
    id: WorldId,
    name: String,
    payload: &Node,
    tokens: &Node,
    players: Option<&Node>,
    path: &str,
    preserved: &mut Vec<PreservedField>,
) -> Result<ImportedWorld, BoundaryError> {
    let (layout, towns, allocator) =
        ordinary::import_world(&id, payload, &format!("{path}.world"), preserved)?;
    let owners = towns
        .iter()
        .map(|t| t.owner.town.clone())
        .collect::<BTreeSet<_>>();
    let credentials = ordinary::credentials(tokens, players, &owners, path, preserved)?;
    Ok(ImportedWorld {
        id,
        name,
        layout,
        allocator,
        towns,
        credentials,
    })
}

/// Explicit bytes from synthetic copies. None means no source save, not empty JSON.
pub fn import_bytes(
    source: Option<&[u8]>,
    provision: Option<&[u8]>,
) -> Result<ImportedRegistry, BoundaryError> {
    let mut result = ImportedRegistry {
        source: source.map(Vec::from),
        provision: provision.map(Vec::from),
        active_world: None,
        worlds: Vec::new(),
        preserved: Vec::new(),
        ignored_provision_worlds: 0,
        ignored_provision_accounts: 0,
    };
    if let Some(bytes) = source {
        let root = Node::parse(bytes, "$")?;
        let registry = root.optional("worlds", "$")?;
        if let Some(entries) = registry {
            if root.optional("world", "$")?.is_some() {
                return Err(BoundaryError::invalid("$", "ambiguous legacy envelope"));
            }
            for (index, (id, entry)) in entries.object("$.worlds")?.iter().enumerate() {
                let path = format!("$.worlds[{index}]");
                preserved_fields(
                    entry,
                    &["name", "world", "tokens", "players"],
                    &path,
                    &mut result.preserved,
                )?;
                result.worlds.push(world(
                    opaque_world(id, &path)?,
                    entry
                        .required("name", &path)?
                        .string(&format!("{path}.name"))?,
                    entry.required("world", &path)?,
                    entry.required("tokens", &path)?,
                    entry.optional("players", &path)?,
                    &path,
                    &mut result.preserved,
                )?);
            }
            let active = root.required("active_world", "$")?;
            if !active.is_null() {
                let selected = active.string("$.active_world")?;
                result.active_world = result
                    .worlds
                    .iter()
                    .find(|w| w.id.as_str() == selected)
                    .map(|w| w.id.clone());
            }
            preserved_fields(
                &root,
                &["worlds", "active_world"],
                "$",
                &mut result.preserved,
            )?;
        } else {
            let id = opaque_world("legacy", "$")?;
            result.worlds.push(world(
                id.clone(),
                "My first world".into(),
                root.required("world", "$")?,
                root.required("tokens", "$")?,
                None,
                "$",
                &mut result.preserved,
            )?);
            result.active_world = Some(id);
            preserved_fields(&root, &["world", "tokens"], "$", &mut result.preserved)?;
        }
    }
    if let Some(bytes) = provision {
        let root = Node::parse(bytes, "$provision")?;
        for (index, (id, players)) in root.object("$provision")?.iter().enumerate() {
            let Some(world) = result
                .worlds
                .iter_mut()
                .find(|world| world.id.as_str() == id)
            else {
                result.ignored_provision_worlds += 1;
                continue;
            };
            ordinary::provision(
                world,
                players,
                &format!("$provision[{index}]"),
                &mut result.preserved,
                &mut result.ignored_provision_accounts,
            )?;
        }
    }
    Ok(result)
}

/// Reads only the explicit paths; no automatic discovery, directory creation,
/// migration, chunk loading, saving, generation, simulation or credential logging.
pub fn read_copies(
    source: &Path,
    provision: Option<&Path>,
) -> Result<ImportedRegistry, BoundaryError> {
    let source = match fs::read(source) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == IoErrorKind::NotFound => None,
        Err(_) => return Err(BoundaryError::invalid("$", "unable to read source")),
    };
    let provision = provision
        .map(|path| {
            fs::read(path)
                .map_err(|_| BoundaryError::invalid("$provision", "unable to read provisioning"))
        })
        .transpose()?;
    import_bytes(source.as_deref(), provision.as_deref())
}
