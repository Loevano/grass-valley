//! Lossless Python copy archives. Deliberately NOT a playable Rust import.
use super::{destination, regular_tree};
use crate::{
    legacy::{self, ChunkDirectory, ChunkKey, ImportedRegistry},
    persistence::{
        Result, invalid,
        store::{digest, private_dir, private_file, read_regular, sync_dir},
    },
};
use lantern_core::ids::WorldId;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
};
const MARKER: &str = ".lantern-python-archive";
const GATE: &str = ".archive-in-progress";
const MAGIC: &[u8] = b"lantern-python-archive-v1 preserved-only\n";
pub struct ChunkSource {
    pub world: WorldId,
    pub directory: PathBuf,
    pub required: Vec<ChunkKey>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveReport {
    pub worlds: usize,
    pub chunks: usize,
    pub accounts: usize,
    pub source_sha256: String,
    pub provisioning_present: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: String,
    version: u32,
    readiness: String,
    worlds: Vec<World>,
    files: Vec<Entry>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct World {
    id: String,
    high_water: u64,
    towns: Vec<Town>,
    #[serde(deserialize_with = "required")]
    seed: Option<String>,
    keys: Vec<[u32; 2]>,
}
#[derive(Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Town {
    id: String,
    buildings: Vec<u64>,
    actors: Vec<u64>,
    #[serde(deserialize_with = "required")]
    founder: Option<u64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    role: Role,
    length: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
enum Role {
    Source,
    Provision,
    Chunk { world: usize, x: u32, y: u32 },
}
struct Archive {
    manifest: Manifest,
    files: Vec<Vec<u8>>,
}
fn required<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
) -> std::result::Result<Option<T>, D::Error> {
    Option::<T>::deserialize(d)
}
fn bindings(registry: &ImportedRegistry) -> Vec<World> {
    registry
        .worlds()
        .iter()
        .map(|w| World {
            id: w.id().as_str().into(),
            high_water: w.high_water(),
            towns: w
                .towns()
                .iter()
                .map(|t| Town {
                    id: t.owner().town.as_str().into(),
                    buildings: t
                        .buildings()
                        .iter()
                        .map(|id| id.identity().serial())
                        .collect(),
                    actors: t
                        .villagers()
                        .iter()
                        .map(|id| id.identity().serial())
                        .collect(),
                    founder: t.founder().map(|id| id.identity().serial()),
                })
                .collect(),
            seed: w
                .chunk_metadata()
                .map(|m| m.private_seed_namespace().into()),
            keys: w.chunks().iter().map(|c| [c.key().x, c.key().y]).collect(),
        })
        .collect()
}
impl Archive {
    fn capture(registry: &ImportedRegistry) -> Result<Self> {
        let mut entries = vec![];
        let mut files = vec![];
        let mut push = |role, bytes: &[u8]| {
            entries.push(Entry {
                role,
                length: bytes.len() as u64,
                sha256: digest(bytes),
            });
            files.push(bytes.to_vec());
        };
        push(
            Role::Source,
            registry
                .private_source_bytes()
                .ok_or_else(|| invalid("missing Python source"))?,
        );
        if let Some(bytes) = registry.private_provision_bytes() {
            push(Role::Provision, bytes);
        }
        for (index, world) in registry.worlds().iter().enumerate() {
            for chunk in world.chunks() {
                push(
                    Role::Chunk {
                        world: index,
                        x: chunk.key().x,
                        y: chunk.key().y,
                    },
                    chunk.private_bytes(),
                );
            }
        }
        let archive = Self {
            manifest: Manifest {
                format: "lantern-python-archive".into(),
                version: 1,
                readiness: "preserved-only".into(),
                worlds: bindings(registry),
                files: entries,
            },
            files,
        };
        archive.validate()?;
        Ok(archive)
    }
    fn validate(&self) -> Result<ImportedRegistry> {
        if self.manifest.format != "lantern-python-archive"
            || self.manifest.version != 1
            || self.manifest.readiness != "preserved-only"
            || self.files.len() != self.manifest.files.len()
        {
            return Err(invalid("unsupported Python archive"));
        }
        let mut source = None;
        let mut provision = None;
        let mut records: Vec<Vec<(ChunkKey, Vec<u8>)>> =
            (0..self.manifest.worlds.len()).map(|_| vec![]).collect();
        for (index, (entry, bytes)) in self.manifest.files.iter().zip(&self.files).enumerate() {
            if entry.length != bytes.len() as u64 || entry.sha256 != digest(bytes) {
                return Err(invalid("archive integrity"));
            }
            match entry.role {
                Role::Source if index == 0 && source.is_none() => source = Some(bytes.as_slice()),
                Role::Provision if source.is_some() && provision.is_none() => {
                    provision = Some(bytes.as_slice())
                }
                Role::Chunk { world, x, y } => records
                    .get_mut(world)
                    .ok_or_else(|| invalid("archive chunk world"))?
                    .push((ChunkKey { x, y }, bytes.clone())),
                _ => return Err(invalid("duplicate or unordered source archive")),
            }
        }
        let mut chunks = vec![];
        for (w, records) in self.manifest.worlds.iter().zip(records) {
            if w.seed.is_some() {
                chunks.push((
                    WorldId::new(w.id.clone()).map_err(|_| invalid("archive world"))?,
                    records,
                ));
            } else if !records.is_empty() {
                return Err(invalid("ordinary archive world chunks"));
            }
        }
        let registry = legacy::import_record_copies(
            source.ok_or_else(|| invalid("missing source archive"))?,
            provision,
            &chunks,
        )
        .map_err(|_| invalid("Python source validation"))?;
        if bindings(&registry) != self.manifest.worlds {
            return Err(invalid("archive references or identity bindings"));
        }
        // Python uses one seed namespace per data root. Two different archives
        // under the same namespace cannot be restored faithfully into that layout.
        let mut namespaces = BTreeMap::new();
        for world in registry.worlds() {
            if let Some(meta) = world.chunk_metadata() {
                let records: Vec<_> = world
                    .chunks()
                    .iter()
                    .map(|c| (c.key(), c.private_bytes().to_vec()))
                    .collect();
                if let Some(previous) =
                    namespaces.insert(meta.private_seed_namespace(), records.clone())
                    && previous != records
                {
                    return Err(invalid("conflicting Python seed namespaces"));
                }
            }
        }
        Ok(registry)
    }
    fn report(&self) -> Result<ArchiveReport> {
        let registry = self.validate()?;
        Ok(ArchiveReport {
            worlds: registry.worlds().len(),
            chunks: registry.worlds().iter().map(|w| w.chunks().len()).sum(),
            accounts: registry
                .worlds()
                .iter()
                .map(|w| w.credentials().players.len())
                .sum(),
            source_sha256: self.manifest.files[0].sha256.clone(),
            provisioning_present: registry.private_provision_bytes().is_some(),
        })
    }
}
fn capture_paths(
    source: &Path,
    provision: Option<&Path>,
    chunks: &[ChunkSource],
) -> Result<Archive> {
    read_regular(source)?;
    if let Some(path) = provision {
        read_regular(path)?;
    }
    let directories: Vec<_> = chunks
        .iter()
        .map(|c| ChunkDirectory::new(&c.world, &c.directory).requiring(&c.required))
        .collect();
    let registry = legacy::read_copies_with_chunks(source, provision, &directories)
        .map_err(|_| invalid("Python copy validation"))?;
    Archive::capture(&registry)
}
/// Archive explicit quiescent/offline copies; no source discovery or source writes.
pub fn archive(
    source: &Path,
    provision: Option<&Path>,
    chunks: &[ChunkSource],
    target: &Path,
) -> Result<ArchiveReport> {
    archive_inner(source, provision, chunks, target, |_| Ok(()))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Checkpoint {
    Created,
    FileWritten(usize),
    FilesSynced,
    ManifestSynced,
    Ready,
}
fn archive_inner(
    source: &Path,
    provision: Option<&Path>,
    chunks: &[ChunkSource],
    target: &Path,
    mut checkpoint: impl FnMut(Checkpoint) -> Result<()>,
) -> Result<ArchiveReport> {
    let mut protected = vec![source];
    if let Some(p) = provision {
        protected.push(p);
    }
    for c in chunks {
        protected.push(&c.directory);
    }
    let target = destination(target, &protected)?;
    let archive = capture_paths(source, provision, chunks)?;
    // Re-read the entire selected source set, including directory membership,
    // before creating the destination. This detects ordinary concurrent changes.
    let repeated = capture_paths(source, provision, chunks)?;
    if serde_json::to_vec(&archive.manifest).map_err(|_| invalid("archive manifest"))?
        != serde_json::to_vec(&repeated.manifest).map_err(|_| invalid("archive manifest"))?
    {
        return Err(invalid("source changed during capture"));
    }
    private_dir(&target)?;
    let mut gate = private_file(&target.join(GATE))?;
    gate.write_all(b"incomplete\n")?;
    gate.sync_all()?;
    sync_dir(&target)?;
    checkpoint(Checkpoint::Created)?;
    let mut marker = private_file(&target.join(MARKER))?;
    marker.write_all(MAGIC)?;
    marker.sync_all()?;
    for (i, bytes) in archive.files.iter().enumerate() {
        let mut file = private_file(&target.join(format!("{i:08}.bin")))?;
        file.write_all(bytes)?;
        checkpoint(Checkpoint::FileWritten(i))?;
        file.sync_all()?;
    }
    checkpoint(Checkpoint::FilesSynced)?;
    let mut file = private_file(&target.join("manifest.json"))?;
    file.write_all(
        &serde_json::to_vec(&archive.manifest).map_err(|_| invalid("archive manifest"))?,
    )?;
    file.sync_all()?;
    sync_dir(&target)?;
    checkpoint(Checkpoint::ManifestSynced)?;
    fs::remove_file(target.join(GATE))?;
    sync_dir(&target)?;
    sync_dir(target.parent().ok_or_else(|| invalid("archive parent"))?)?;
    checkpoint(Checkpoint::Ready)?;
    archive.report()
}
fn read_archive(root: &Path) -> Result<Archive> {
    if !fs::symlink_metadata(root)?.is_dir()
        || root.join(GATE).try_exists()?
        || read_regular(&root.join(MARKER))? != MAGIC
    {
        return Err(invalid("incomplete or non-archive root"));
    }
    let manifest: Manifest = serde_json::from_slice(&read_regular(&root.join("manifest.json"))?)
        .map_err(|_| invalid("archive manifest schema"))?;
    let mut expected = BTreeSet::from([MARKER.to_owned(), "manifest.json".into()]);
    let mut files = vec![];
    for i in 0..manifest.files.len() {
        let name = format!("{i:08}.bin");
        files.push(read_regular(&root.join(&name))?);
        expected.insert(name);
    }
    for entry in fs::read_dir(root)? {
        let name = entry?.file_name();
        if !name.to_str().is_some_and(|n| expected.remove(n)) {
            return Err(invalid("unexpected archive entry"));
        }
    }
    if !expected.is_empty() {
        return Err(invalid("missing archive entry"));
    }
    let archive = Archive { manifest, files };
    archive.validate()?;
    Ok(archive)
}
pub fn inspect(root: &Path) -> Result<ArchiveReport> {
    read_archive(root)?.report()
}
/// Reverse export applies ONLY to a preserved-only Python archive. Native Rust
/// gameplay has no reverse adapter and is never guessed into Python fields.
pub fn restore(source: &Path, target: &Path) -> Result<ArchiveReport> {
    restore_inner(source, target, |_| Ok(()))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RestoreCheckpoint {
    Created,
    DataSynced,
    RegistryPublished,
    Ready,
}
fn restore_inner(
    source: &Path,
    target: &Path,
    mut checkpoint: impl FnMut(RestoreCheckpoint) -> Result<()>,
) -> Result<ArchiveReport> {
    let target = destination(target, &[source])?;
    let archive = read_archive(source)?;
    let registry = archive.validate()?;
    private_dir(&target)?;
    let gate_name = ".python-restore-in-progress";
    let mut gate = private_file(&target.join(gate_name))?;
    gate.write_all(b"incomplete\n")?;
    gate.sync_all()?;
    sync_dir(&target)?;
    checkpoint(RestoreCheckpoint::Created)?;
    let mut namespaces = BTreeSet::new();
    if registry
        .worlds()
        .iter()
        .any(|w| w.chunk_metadata().is_some())
    {
        private_dir(&target.join("savegame_chunks"))?;
    }
    for world in registry.worlds() {
        if let Some(meta) = world.chunk_metadata() {
            if !namespaces.insert(meta.private_seed_namespace()) {
                continue;
            }
            let dir = target
                .join("savegame_chunks")
                .join(meta.private_seed_namespace());
            private_dir(&dir)?;
            for c in world.chunks() {
                let mut file = private_file(&dir.join(c.key().file_name()))?;
                file.write_all(c.private_bytes())?;
                file.sync_all()?;
            }
            sync_dir(&dir)?;
        }
    }
    if target.join("savegame_chunks").exists() {
        sync_dir(&target.join("savegame_chunks"))?;
    }
    if let Some(bytes) = registry.private_provision_bytes() {
        let mut file = private_file(&target.join("savegame.players.json"))?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    checkpoint(RestoreCheckpoint::DataSynced)?;
    // Main registry is published only after ALL chunk/provision files are durable.
    let mut file = private_file(&target.join("registry.pending"))?;
    file.write_all(
        registry
            .private_source_bytes()
            .ok_or_else(|| invalid("source registry"))?,
    )?;
    file.sync_all()?;
    sync_dir(&target)?;
    fs::rename(
        target.join("registry.pending"),
        target.join("savegame.json"),
    )?;
    sync_dir(&target)?;
    checkpoint(RestoreCheckpoint::RegistryPublished)?;
    fs::remove_file(target.join(gate_name))?;
    sync_dir(&target)?;
    sync_dir(target.parent().ok_or_else(|| invalid("restore parent"))?)?;
    regular_tree(&target)?;
    checkpoint(RestoreCheckpoint::Ready)?;
    archive.report()
}

#[cfg(test)]
mod tests;
