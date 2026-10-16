//! Experimental read-only decoded views of saved copies, not ChunkMap.read.
//! No generation, overlays, repairs, catch-up, activation, or save writes.
use super::{ChunkKey, ImportedWorld, chunks::CHUNK_SIZE, json::Node};
use crate::BoundaryError;
use lantern_core::units::Tile;
use std::collections::VecDeque;

pub const MAX_LOADED_CHUNKS: usize = 49;

/// Missing data stays distinct from world bounds and persisted void tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadedTile {
    OutsideWorld,
    Unloaded,
    Saved(char),
}

/// Read-only source values. Kind/species remain strings, not validated gameplay
/// rules. Unknown fields and exact numeric lexemes remain in private_bytes().
/// No Debug/Serialize: arbitrary kind/species/extension data can be private.
pub struct LoadedResource {
    tile: Tile,
    kind: String,
    species: Option<String>,
    supply: Option<f64>,
    supply_max: Option<f64>,
    regeneration: Option<f64>,
}
impl LoadedResource {
    pub fn tile(&self) -> Tile {
        self.tile
    }
    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn species(&self) -> Option<&str> {
        self.species.as_deref()
    }
    pub fn supply(&self) -> Option<f64> {
        self.supply
    }
    pub fn supply_max(&self) -> Option<f64> {
        self.supply_max
    }
    pub fn regeneration(&self) -> Option<f64> {
        self.regeneration
    }
}

pub struct LoadedStump {
    tile: Tile,
    remaining: f64,
}
impl LoadedStump {
    pub fn tile(&self) -> Tile {
        self.tile
    }
    pub fn remaining(&self) -> f64 {
        self.remaining
    }
}

/// Borrows the immutable archive, retaining every extension and numeric lexeme.
/// There is intentionally no mutable, Debug, or Serialize access to the record.
/// ```compile_fail
/// use lantern_host::legacy::LoadedChunk;
/// fn leak(chunk: &LoadedChunk<'_>) { serde_json::to_string(chunk).unwrap(); }
/// ```
pub struct LoadedChunk<'a> {
    key: ChunkKey,
    bytes: &'a [u8],
    rows: Vec<String>,
    resources: Vec<LoadedResource>,
    stumps: Vec<LoadedStump>,
    clock: Option<f64>,
}
impl LoadedChunk<'_> {
    pub fn key(&self) -> ChunkKey {
        self.key
    }
    pub fn terrain(&self) -> &[String] {
        &self.rows
    }
    pub fn resources(&self) -> &[LoadedResource] {
        &self.resources
    }
    pub fn stumps(&self) -> &[LoadedStump] {
        &self.stumps
    }
    /// None preserves an absent clock; callers must not infer catch-up here.
    pub fn clock(&self) -> Option<f64> {
        self.clock
    }
    pub fn private_bytes(&self) -> &[u8] {
        self.bytes
    }
}

/// Bounded decoded views only. Eviction never edits or drops an archive record.
/// The whole R06 archive still retains every supplied file; this is not a disk
/// streaming loader or a bound on the archive's total memory use.
pub struct ChunkReader<'a> {
    world: &'a ImportedWorld,
    cache: VecDeque<LoadedChunk<'a>>,
}
impl<'a> ChunkReader<'a> {
    pub fn new(world: &'a ImportedWorld) -> Result<Self, BoundaryError> {
        if world.chunk_metadata().is_none() {
            return Err(BoundaryError::invalid(
                "$.loaded_chunks",
                "world is not chunked",
            ));
        }
        Ok(Self {
            world,
            cache: VecDeque::new(),
        })
    }
    pub fn loaded_keys(&self) -> impl Iterator<Item = ChunkKey> + '_ {
        self.cache.iter().map(|record| record.key)
    }
    /// Lookup never loads or generates. Unloaded is impassable, not grassland.
    pub fn tile(&self, tile: Tile) -> LoadedTile {
        let size = self.world.chunk_metadata().unwrap().report().map_size as i32;
        if tile.x < 0 || tile.y < 0 || tile.x >= size || tile.y >= size {
            return LoadedTile::OutsideWorld;
        }
        let key = ChunkKey {
            x: tile.x as u32 / CHUNK_SIZE,
            y: tile.y as u32 / CHUNK_SIZE,
        };
        match self.cache.iter().find(|record| record.key == key) {
            Some(record) => LoadedTile::Saved(
                record.rows[tile.y as usize % CHUNK_SIZE as usize].as_bytes()
                    [tile.x as usize % CHUNK_SIZE as usize] as char,
            ),
            None => LoadedTile::Unloaded,
        }
    }
    pub fn read(&mut self, key: ChunkKey) -> Result<&LoadedChunk<'a>, BoundaryError> {
        let metadata = self.world.chunk_metadata().unwrap();
        key.validate(metadata, "$.loaded_chunks.key")?;
        if let Some(index) = self.cache.iter().position(|record| record.key == key) {
            let record = self.cache.remove(index).unwrap();
            self.cache.push_back(record);
        } else {
            let index = self
                .world
                .chunks()
                .binary_search_by_key(&key, |record| record.key())
                .map_err(|_| {
                    BoundaryError::invalid(
                        "$.loaded_chunks.key",
                        "saved chunk unavailable; generation and starter reconstruction unported",
                    )
                })?;
            let source = &self.world.chunks()[index];
            let record = decode(source.private_bytes(), key)?;
            self.cache.push_back(record);
            if self.cache.len() > MAX_LOADED_CHUNKS {
                self.cache.pop_front();
            }
        }
        Ok(self.cache.back().unwrap())
    }
    /// Explicit view eviction. Saved data remains available for a later read.
    pub fn unload(&mut self, key: ChunkKey) {
        if let Some(index) = self.cache.iter().position(|record| record.key == key) {
            self.cache.remove(index);
        }
    }
}

fn number(node: &Node, field: &str, path: &str) -> Result<Option<f64>, BoundaryError> {
    node.optional(field, path)?
        .map(|value| value.nonnegative_number(&format!("{path}.{field}")))
        .transpose()
}
fn position(node: &Node, path: &str) -> Result<Tile, BoundaryError> {
    Ok(Tile {
        x: node.required("x", path)?.tile(&format!("{path}.x"))?,
        y: node.required("y", path)?.tile(&format!("{path}.y"))?,
    })
}
fn decode(bytes: &[u8], key: ChunkKey) -> Result<LoadedChunk<'_>, BoundaryError> {
    // Dimensions, positions, record versions and stump timers already validated
    // by the archive. Loading additionally validates supported terrain codes and
    // typed optional fields, without narrowing otherwise preserved R06 input.
    let path = "$.loaded_chunks.record";
    let node = Node::parse(bytes, path)?;
    let mut rows = Vec::new();
    for (index, row) in node
        .required("terrain", path)?
        .array(path)?
        .iter()
        .enumerate()
    {
        let rp = format!("{path}.terrain[{index}]");
        let row = row.string(&rp)?;
        if !row.bytes().all(|byte| b"gfmsrbdiv".contains(&byte)) {
            return Err(BoundaryError::invalid(rp, "unsupported terrain code"));
        }
        rows.push(row);
    }
    let mut resources = Vec::new();
    for (index, item) in node
        .required("map_resources", path)?
        .array(path)?
        .iter()
        .enumerate()
    {
        let rp = format!("{path}.map_resources[{index}]");
        resources.push(LoadedResource {
            tile: position(item, &rp)?,
            kind: item.required("kind", &rp)?.string(&format!("{rp}.kind"))?,
            species: item
                .optional("species", &rp)?
                .map(|v| v.string(&format!("{rp}.species")))
                .transpose()?,
            supply: number(item, "supply", &rp)?,
            supply_max: number(item, "supply_max", &rp)?,
            regeneration: number(item, "regeneration", &rp)?,
        });
    }
    let mut stumps = Vec::new();
    for (index, item) in node
        .required("tree_stumps", path)?
        .array(path)?
        .iter()
        .enumerate()
    {
        let rp = format!("{path}.tree_stumps[{index}]");
        stumps.push(LoadedStump {
            tile: position(item, &rp)?,
            remaining: item
                .required("remaining", &rp)?
                .nonnegative_number(&format!("{rp}.remaining"))?,
        });
    }
    Ok(LoadedChunk {
        key,
        bytes,
        rows,
        resources,
        stumps,
        clock: number(&node, "clock", path)?,
    })
}
