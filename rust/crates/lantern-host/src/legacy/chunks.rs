//! Raw external chunk copies, never ChunkMap views or generated/caught-up data.
use super::{ImportedRegistry, PreservedField, json::Node, ordinary, preserved_fields};
use crate::BoundaryError;
use lantern_core::ids::WorldId;
use serde::Serialize;
use std::{collections::BTreeSet, fs, path::Path};

pub(super) const CHUNK_SIZE: u32 = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChunkKey {
    pub x: u32,
    pub y: u32,
}

impl ChunkKey {
    pub fn file_name(self) -> String {
        format!("{},{}.json", self.x, self.y)
    }
    fn parse(name: &str, path: &str) -> Result<Self, BoundaryError> {
        let Some(stem) = name.strip_suffix(".json") else {
            return Err(BoundaryError::invalid(
                path,
                "unexpected chunk directory entry",
            ));
        };
        let Some((x, y)) = stem.split_once(',') else {
            return Err(BoundaryError::invalid(path, "invalid chunk filename"));
        };
        let key = match (x.parse::<u32>(), y.parse::<u32>()) {
            (Ok(x), Ok(y)) => Self { x, y },
            _ => return Err(BoundaryError::invalid(path, "invalid chunk filename")),
        };
        if key.file_name() != name {
            return Err(BoundaryError::invalid(path, "noncanonical chunk filename"));
        }
        Ok(key)
    }
    pub(super) fn validate(
        self,
        metadata: &ChunkMetadata,
        path: &str,
    ) -> Result<(), BoundaryError> {
        if self.x >= metadata.map_size / CHUNK_SIZE || self.y >= metadata.map_size / CHUNK_SIZE {
            return Err(BoundaryError::invalid(path, "chunk outside saved world"));
        }
        Ok(())
    }
}

/// Explicit per-world seed-directory copy. No discovery of neighboring roots.
/// Required keys are an optional caller-supplied copy manifest; other absent
/// sparse chunks cannot be distinguished from lost files without that manifest.
pub struct ChunkDirectory<'a> {
    world: &'a WorldId,
    directory: &'a Path,
    required: &'a [ChunkKey],
}

impl<'a> ChunkDirectory<'a> {
    pub fn new(world: &'a WorldId, directory: &'a Path) -> Self {
        Self {
            world,
            directory,
            required: &[],
        }
    }
    pub fn requiring(mut self, keys: &'a [ChunkKey]) -> Self {
        self.required = keys;
        self
    }
}

/// Private seed namespace is kept off public reports and Debug/Serialize output.
pub struct ChunkMetadata {
    seed_namespace: String,
    seed_kind: SeedKind,
    map_size: u32,
    starter_offset: u32,
    generator_version: u32,
}

// Retain the registry's JSON type: Random(42) and Random("42") differ.
// This metadata must never enter Debug/Serialize reports.
enum SeedKind {
    Integer,
    Text,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct ChunkMetadataReport {
    pub map_size: u32,
    pub chunk_size: u32,
    pub starter_offset: u32,
    pub generator_version: u32,
}

impl ChunkMetadata {
    pub fn private_seed_namespace(&self) -> &str {
        &self.seed_namespace
    }
    pub fn report(&self) -> ChunkMetadataReport {
        ChunkMetadataReport {
            map_size: self.map_size,
            chunk_size: CHUNK_SIZE,
            starter_offset: self.starter_offset,
            generator_version: self.generator_version,
        }
    }
}

/// Raw persisted data may contain arbitrary private extensions. No Debug/Serialize.
///
/// ```compile_fail
/// use lantern_host::legacy::ImportedChunk;
/// fn unsafe_report(chunk: &ImportedChunk) { serde_json::to_string(chunk).unwrap(); }
/// ```
pub struct ImportedChunk {
    key: ChunkKey,
    bytes: Vec<u8>,
}

impl ImportedChunk {
    pub fn key(&self) -> ChunkKey {
        self.key
    }
    pub fn private_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

fn flag(node: &Node, path: &str) -> Result<bool, BoundaryError> {
    let Some(value) = node.optional("chunked", path)? else {
        return Ok(false);
    };
    let path = format!("{path}.chunked");
    if let Ok(value) = value.boolean(&path) {
        return Ok(value);
    }
    match value.unsigned(&path)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(BoundaryError::invalid(path, "unsupported chunked flag")),
    }
}

pub(super) fn metadata(payload: &Node, path: &str) -> Result<Option<ChunkMetadata>, BoundaryError> {
    if flag(payload, path)? {
        return Err(BoundaryError::invalid(
            format!("{path}.chunked"),
            "chunked layout requires shared_map metadata",
        ));
    }
    for (index, (_, town)) in payload
        .required("towns", path)?
        .object(&format!("{path}.towns"))?
        .iter()
        .enumerate()
    {
        let tp = format!("{path}.towns[{index}]");
        if flag(town, &tp)? {
            return Err(BoundaryError::invalid(
                format!("{tp}.chunked"),
                "independent chunked towns are unsupported",
            ));
        }
    }
    let Some(shared) = payload.optional("shared_map", path)? else {
        return Ok(None);
    };
    let path = format!("{path}.shared_map");
    if !flag(shared, &path)? {
        return Ok(None);
    }
    let chunk_size = shared
        .required("chunk_size", &path)?
        .unsigned(&format!("{path}.chunk_size"))?;
    if chunk_size != u64::from(CHUNK_SIZE) {
        return Err(BoundaryError::invalid(
            format!("{path}.chunk_size"),
            "unsupported chunk size",
        ));
    }
    let map_size = shared
        .required("map_size", &path)?
        .unsigned(&format!("{path}.map_size"))?;
    if ![480, 1920].contains(&map_size) {
        return Err(BoundaryError::invalid(
            format!("{path}.map_size"),
            "unsupported chunked map dimensions",
        ));
    }
    let seed = shared.required("terrain_seed", &path)?;
    let seed_path = format!("{path}.terrain_seed");
    let seed_namespace = seed.seed_namespace(&seed_path)?;
    let seed_kind = if seed.string(&seed_path).is_ok() {
        SeedKind::Text
    } else {
        SeedKind::Integer
    };
    let offset = shared
        .optional("starter_offset", &path)?
        .map(|n| n.unsigned(&format!("{path}.starter_offset")))
        .transpose()?
        .unwrap_or(880);
    if offset > u64::from(u32::MAX) || offset % u64::from(CHUNK_SIZE) != 0 {
        return Err(BoundaryError::invalid(
            format!("{path}.starter_offset"),
            "unsupported starter offset",
        ));
    }
    let generator = shared
        .optional("terrain_version", &path)?
        .map(|n| n.unsigned(&format!("{path}.terrain_version")))
        .transpose()?
        .unwrap_or(1);
    if !(1..=7).contains(&generator) {
        return Err(BoundaryError::invalid(
            format!("{path}.terrain_version"),
            "unsupported terrain version",
        ));
    }
    if let Some(clock) = shared.optional("chunk_clock", &path)? {
        super::clock::Clock::parse(clock, &format!("{path}.chunk_clock"))?;
    }
    for field in ["terrain", "map_resources", "tree_stumps"] {
        if shared.optional(field, &path)?.is_some() {
            return Err(BoundaryError::invalid(
                format!("{path}.{field}"),
                "ambiguous inline chunk payload",
            ));
        }
    }
    Ok(Some(ChunkMetadata {
        seed_namespace,
        seed_kind,
        map_size: map_size as u32,
        starter_offset: offset as u32,
        generator_version: generator as u32,
    }))
}

pub(super) fn validate_record(
    bytes: &[u8],
    key: ChunkKey,
    metadata: &ChunkMetadata,
    path: &str,
    preserved: &mut Vec<PreservedField>,
    versions: &mut Vec<super::VersionReport>,
) -> Result<(), BoundaryError> {
    let node = Node::parse(bytes, path)?;
    versions.extend(ordinary::versions(&node, path)?);
    if flag(&node, path)? {
        return Err(BoundaryError::invalid(path, "nested chunked record"));
    }
    if let Some(version) = node.optional("terrain_version", path)?
        && version.unsigned(&format!("{path}.terrain_version"))?
            != u64::from(metadata.generator_version)
    {
        return Err(BoundaryError::invalid(
            format!("{path}.terrain_version"),
            "chunk generator version mismatch",
        ));
    }
    let rows = node
        .required("terrain", path)?
        .array(&format!("{path}.terrain"))?;
    if rows.len() != CHUNK_SIZE as usize {
        return Err(BoundaryError::invalid(
            format!("{path}.terrain"),
            "invalid chunk row count",
        ));
    }
    for (index, row) in rows.iter().enumerate() {
        let rp = format!("{path}.terrain[{index}]");
        if row.string(&rp)?.chars().count() != CHUNK_SIZE as usize {
            return Err(BoundaryError::invalid(rp, "invalid chunk row width"));
        }
    }
    for field in ["map_resources", "tree_stumps"] {
        for (index, item) in node
            .required(field, path)?
            .array(&format!("{path}.{field}"))?
            .iter()
            .enumerate()
        {
            let rp = format!("{path}.{field}[{index}]");
            let x = item.required("x", &rp)?.tile(&format!("{rp}.x"))?;
            let y = item.required("y", &rp)?.tile(&format!("{rp}.y"))?;
            if x < 0 || y < 0 || x as u32 / CHUNK_SIZE != key.x || y as u32 / CHUNK_SIZE != key.y {
                return Err(BoundaryError::invalid(
                    rp,
                    "record position outside owning chunk",
                ));
            }
            if field == "map_resources"
                && item
                    .required("kind", &rp)?
                    .string(&format!("{rp}.kind"))?
                    .is_empty()
            {
                return Err(BoundaryError::invalid(
                    format!("{rp}.kind"),
                    "empty resource kind",
                ));
            }
            if field == "tree_stumps" {
                item.required("remaining", &rp)?
                    .nonnegative_number(&format!("{rp}.remaining"))?;
            }
        }
    }
    if let Some(clock) = node.optional("clock", path)? {
        super::clock::Clock::parse(clock, &format!("{path}.clock"))?;
    }
    preserved_fields(
        &node,
        &["terrain", "map_resources", "tree_stumps", "clock"],
        path,
        preserved,
    )?;
    preserved.push(PreservedField {
        path: path.into(),
        reason: "raw chunk preserved privately; no generation, repair or catch-up",
    });
    Ok(())
}

pub(super) fn attach(
    registry: &mut ImportedRegistry,
    directories: &[ChunkDirectory<'_>],
) -> Result<(), BoundaryError> {
    let mut bindings = Vec::new();
    let mut seen_worlds = BTreeSet::new();
    let mut seen_directories = BTreeSet::new();
    for (index, source) in directories.iter().enumerate() {
        let path = format!("$.chunk_sources[{index}]");
        let Some(world_index) = registry.worlds.iter().position(|w| &w.id == source.world) else {
            return Err(BoundaryError::invalid(path, "unknown chunk world owner"));
        };
        let world = &registry.worlds[world_index];
        let Some(metadata) = &world.chunk_metadata else {
            return Err(BoundaryError::invalid(
                path,
                "ordinary world has no external chunk reference",
            ));
        };
        if !seen_worlds.insert(source.world.clone()) {
            return Err(BoundaryError::invalid(path, "duplicate chunk world source"));
        }
        if source.directory.file_name().and_then(|s| s.to_str())
            != Some(metadata.seed_namespace.as_str())
        {
            return Err(BoundaryError::invalid(
                path,
                "seed directory namespace mismatch",
            ));
        }
        let entry = fs::symlink_metadata(source.directory)
            .map_err(|_| BoundaryError::invalid(&path, "missing or unreadable chunk directory"))?;
        if !entry.file_type().is_dir() {
            return Err(BoundaryError::invalid(
                path,
                "chunk directory must be a real directory",
            ));
        }
        let canonical = fs::canonicalize(source.directory)
            .map_err(|_| BoundaryError::invalid(&path, "unreadable chunk directory"))?;
        if !seen_directories.insert(canonical) {
            return Err(BoundaryError::invalid(
                path,
                "chunk directory shared by different worlds",
            ));
        }
        bindings.push((world_index, source));
    }
    for (index, world) in registry.worlds.iter().enumerate() {
        if world.chunk_metadata.is_some() && !seen_worlds.contains(&world.id) {
            return Err(BoundaryError::invalid(
                format!("$.worlds[{index}].chunks"),
                "missing explicit chunk directory",
            ));
        }
    }
    bindings.sort_by_key(|(index, _)| *index);
    for (world_index, source) in bindings {
        let path = format!("$.worlds[{world_index}].chunks");
        let world = &mut registry.worlds[world_index];
        let metadata = world
            .chunk_metadata
            .as_ref()
            .expect("validated chunk reference");
        let mut entries = fs::read_dir(source.directory)
            .map_err(|_| BoundaryError::invalid(&path, "unable to list chunk directory"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| BoundaryError::invalid(&path, "unable to list chunk directory"))?;
        entries.sort_by_key(|entry| entry.file_name());
        let mut found = BTreeSet::new();
        for (index, entry) in entries.iter().enumerate() {
            let cp = format!("{path}[{index}]");
            let file_type = entry
                .file_type()
                .map_err(|_| BoundaryError::invalid(&cp, "unreadable chunk entry"))?;
            if !file_type.is_file() {
                return Err(BoundaryError::invalid(
                    cp,
                    "chunk entry must be a regular file",
                ));
            }
            let name = entry.file_name();
            let key = ChunkKey::parse(
                name.to_str()
                    .ok_or_else(|| BoundaryError::invalid(&cp, "unsupported chunk filename"))?,
                &cp,
            )?;
            key.validate(metadata, &cp)?;
            let bytes = fs::read(entry.path())
                .map_err(|_| BoundaryError::invalid(&cp, "missing or unreadable chunk file"))?;
            validate_record(
                &bytes,
                key,
                metadata,
                &cp,
                &mut registry.preserved,
                &mut world.versions,
            )?;
            found.insert(key);
            world.chunks.push(ImportedChunk { key, bytes });
        }
        let mut required = BTreeSet::new();
        for (index, key) in source.required.iter().enumerate() {
            let rp = format!("{path}.required[{index}]");
            key.validate(metadata, &rp)?;
            if !required.insert(*key) {
                return Err(BoundaryError::invalid(rp, "duplicate required chunk key"));
            }
            if !found.contains(key) {
                return Err(BoundaryError::invalid(rp, "required chunk copy missing"));
            }
        }
        world.required_chunks = required.len();
        world.chunks.sort_by_key(|chunk| chunk.key);
    }
    Ok(())
}

/// Explicit raw pre-overlay source selection. Generated data is not a playable
/// ChunkMap view: village overlays, tiny-water repair and catch-up are slice 3.
/// Saved bytes always win, including in the authored starter region.
pub enum RawChunk<'a> {
    Saved(&'a ImportedChunk),
    Generated(lantern_core::terrain::GeneratedChunk),
}

impl super::ImportedWorld {
    pub fn raw_chunk(&self, key: ChunkKey) -> Result<RawChunk<'_>, BoundaryError> {
        let path = "$.raw_chunks.key";
        let metadata = self
            .chunk_metadata()
            .ok_or_else(|| BoundaryError::invalid(path, "world is not chunked"))?;
        key.validate(metadata, path)?;
        if let Ok(index) = self
            .chunks()
            .binary_search_by_key(&key, |chunk| chunk.key())
        {
            return Ok(RawChunk::Saved(&self.chunks()[index]));
        }
        let (left, top) = (key.x * CHUNK_SIZE, key.y * CHUNK_SIZE);
        if left >= metadata.starter_offset
            && left - metadata.starter_offset < 120
            && top >= metadata.starter_offset
            && top - metadata.starter_offset < 120
        {
            use lantern_core::{
                legacy_map::MapSeed, starter::generate_starter, terrain::GeneratedChunk,
            };
            let seed = match metadata.seed_kind {
                SeedKind::Integer => MapSeed::Integer(&metadata.seed_namespace),
                SeedKind::Text => MapSeed::Text(&metadata.seed_namespace),
            };
            let base = generate_starter(seed, metadata.generator_version)
                .map_err(|_| BoundaryError::invalid(path, "unsupported starter reconstruction"))?
                .landscape;
            let x = left - metadata.starter_offset;
            let y = top - metadata.starter_offset;
            let terrain = base.terrain[y as usize..(y + CHUNK_SIZE) as usize]
                .iter()
                .map(|row| row[x as usize..(x + CHUNK_SIZE) as usize].replace('v', "g"))
                .collect();
            let resources = base
                .resources
                .into_iter()
                .filter(|r| r.x >= x && r.x < x + CHUNK_SIZE && r.y >= y && r.y < y + CHUNK_SIZE)
                .map(|mut r| {
                    r.x += metadata.starter_offset;
                    r.y += metadata.starter_offset;
                    r
                })
                .collect();
            return Ok(RawChunk::Generated(GeneratedChunk { terrain, resources }));
        }
        let generated = lantern_core::terrain::generate_chunk(
            metadata.private_seed_namespace(),
            key.x,
            key.y,
            metadata.generator_version,
            metadata.map_size,
        )
        .map_err(|_| BoundaryError::invalid(path, "unsupported raw chunk generation"))?;
        Ok(RawChunk::Generated(generated))
    }
}
