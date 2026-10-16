//! Immutable pre-catch-up ChunkMap.read integration; no authoritative lifecycle.
use super::{
    ChunkKey, ImportedRegistry, ImportedWorld, OrdinaryVillageMetadata, OrdinaryVillageOverlay,
    RawChunk, SwampCachePlan, SwampOverlay, SwampVillageMetadata,
    json::Node,
    village_overlay::{position, rows, validate_resources, version, write_node},
};
use crate::BoundaryError;
use lantern_core::ids::WorldId;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ROOT: &str = "$.post_overlay_terrain";
type Point = (i32, i32);

/// Explicit active OR camera record copy belonging to this world. Both resident
/// classes win over saved files and bypass fresh-read overlays/repair in Python.
/// No mutable cache, Debug, Serialize, role changes or clocks are implied.
pub struct ResidentChunkCopy<'a> {
    world: &'a WorldId,
    key: ChunkKey,
    bytes: &'a [u8],
}
impl<'a> ResidentChunkCopy<'a> {
    pub fn new(world: &'a WorldId, key: ChunkKey, bytes: &'a [u8]) -> Self {
        Self { world, key, bytes }
    }
}

/// Private owned result; retained extensions and scalar lexemes never enter a DTO.
/// ```compile_fail
/// use lantern_host::legacy::TerrainRecord;
/// fn leak(record: &TerrainRecord) { serde_json::to_string(record).unwrap(); }
/// ```
pub struct TerrainRecord {
    key: ChunkKey,
    bytes: Vec<u8>,
}
impl TerrainRecord {
    pub fn key(&self) -> ChunkKey {
        self.key
    }
    pub fn private_record_bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Explicit private metadata plus immutable resident copies. Reads never install
/// returned records. Runtime active/view writes, resource rebinding/invalidation,
/// cache bounds, supply defaults and catch-up remain the lifecycle stage.
/// ```compile_fail
/// use lantern_host::legacy::PostOverlayTerrain;
/// fn leak(view: &PostOverlayTerrain<'_>) { serde_json::to_string(view).unwrap(); }
/// ```
pub struct PostOverlayTerrain<'a> {
    world: &'a ImportedWorld,
    source: &'a [u8],
    prepared: Vec<u8>,
    villages: Vec<u8>,
    plan: SwampCachePlan,
    residents: BTreeMap<ChunkKey, Vec<u8>>,
    size: u32,
    generator: u32,
}
impl ImportedRegistry {
    /// Compose ordinary/swamp metadata on a private payload copy. Supplied resident
    /// copies receive each single-village swamp upgrade in order, exactly once.
    /// A missing resident argument means a cold materialization, not a live load.
    pub fn materialize_chunked(
        &self,
        world: &WorldId,
        residents: &[ResidentChunkCopy<'_>],
    ) -> Result<PostOverlayTerrain<'_>, BoundaryError> {
        let index = self
            .worlds
            .iter()
            .position(|w| w.id() == world)
            .ok_or_else(|| BoundaryError::invalid(ROOT, "unknown world owner"))?;
        let imported = &self.worlds[index];
        let metadata = imported
            .chunk_metadata()
            .ok_or_else(|| BoundaryError::invalid(ROOT, "world is not chunked"))?
            .report();
        let source = self
            .private_source_bytes()
            .ok_or_else(|| BoundaryError::invalid(ROOT, "source unavailable"))?;
        let root = Node::parse(source, ROOT)?;
        let payload = if let Some(entries) = root.optional("worlds", ROOT)? {
            entries.object(ROOT)?[index].1.required("world", ROOT)?
        } else {
            root.required("world", ROOT)?
        };
        let payload = bytes(payload);
        let ordinary = OrdinaryVillageMetadata::materialize(&payload)?;
        let swamp = SwampVillageMetadata::materialize(ordinary.private_world_bytes())?;
        let prepared = swamp.private_world_bytes().to_vec();
        let node = Node::parse(&prepared, ROOT)?;
        let villages = node
            .required("shared_map", ROOT)?
            .required("villages", ROOT)?;
        let village_bytes = bytes(villages);
        let mut copies = BTreeMap::new();
        for (i, resident) in residents.iter().enumerate() {
            let path = format!("{ROOT}.residents[{i}]");
            if resident.world != world {
                return Err(BoundaryError::invalid(path, "foreign resident world owner"));
            }
            resident
                .key
                .validate(imported.chunk_metadata().unwrap(), &path)?;
            if copies.contains_key(&resident.key) {
                return Err(BoundaryError::invalid(path, "duplicate resident chunk key"));
            }
            validate(
                resident.bytes,
                resident.key,
                metadata.generator_version,
                &path,
            )?;
            let mut record = resident.bytes.to_vec();
            for &ordinal in &swamp.cache_plan().village_ordinals {
                let single = bytes(&Node::Array(vec![villages.array(ROOT)?[ordinal].clone()]));
                record =
                    SwampOverlay::materialize(&record, &single, resident.key, metadata.map_size)?
                        .private_record_bytes()
                        .to_vec();
            }
            copies.insert(resident.key, record);
        }
        Ok(PostOverlayTerrain {
            world: imported,
            source,
            prepared,
            villages: village_bytes,
            plan: SwampCachePlan {
                village_ordinals: swamp.cache_plan().village_ordinals.clone(),
            },
            residents: copies,
            size: metadata.map_size,
            generator: metadata.generator_version,
        })
    }
}
impl PostOverlayTerrain<'_> {
    /// Whole registry source may contain credentials; never public output.
    pub fn private_source_bytes(&self) -> &[u8] {
        self.source
    }
    pub fn private_world_bytes(&self) -> &[u8] {
        &self.prepared
    }
    /// Still required for actual authoritative rebinding/invalidation in slice 3c.
    pub fn cache_plan(&self) -> &SwampCachePlan {
        &self.plan
    }
    pub fn read(&self, key: ChunkKey) -> Result<TerrainRecord, BoundaryError> {
        key.validate(self.world.chunk_metadata().unwrap(), ROOT)?;
        if let Some(record) = self.residents.get(&key) {
            return Ok(TerrainRecord {
                key,
                bytes: record.clone(),
            });
        }
        let record = self.post_overlay(key)?;
        if self.generator < 7 {
            return Ok(TerrainRecord { key, bytes: record });
        }
        let mut node = Node::parse(&record, ROOT)?;
        let water = waters(&node)?;
        let mut neighbors = BTreeMap::new();
        neighbors.insert(key, water.clone());
        let mut remove = BTreeSet::new();
        for &start in &water {
            if remove.contains(&start) {
                continue;
            }
            let mut seen = BTreeSet::from([start]);
            let mut pending = vec![start];
            // Establish three local tiles before loading any neighboring record.
            while let Some(point) = pending.pop() {
                if seen.len() >= 3 {
                    break;
                }
                for next in adjacent(point) {
                    if !seen.contains(&next) && water.contains(&next) {
                        seen.insert(next);
                        pending.push(next);
                        if seen.len() >= 3 {
                            break;
                        }
                    }
                }
            }
            if seen.len() >= 3 {
                continue;
            }
            pending = seen.iter().copied().collect();
            while let Some(point) = pending.pop() {
                if seen.len() >= 3 {
                    break;
                }
                for next in adjacent(point) {
                    if seen.contains(&next)
                        || next.0 < 0
                        || next.1 < 0
                        || next.0 >= self.size as i32
                        || next.1 >= self.size as i32
                    {
                        continue;
                    }
                    let other = ChunkKey {
                        x: next.0 as u32 / 40,
                        y: next.1 as u32 / 40,
                    };
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        neighbors.entry(other)
                    {
                        // No recursive repair: sample selected raw/starter/saved
                        // records AFTER ordinary/swamp edits, or resident copies.
                        let bytes = self.post_overlay(other)?;
                        entry.insert(waters(&Node::parse(&bytes, ROOT)?)?);
                    }
                    if neighbors[&other].contains(&next) {
                        seen.insert(next);
                        pending.push(next);
                        if seen.len() >= 3 {
                            break;
                        }
                    }
                }
            }
            if seen.len() < 3 {
                remove.extend(seen);
            }
        }
        let Node::Object(fields) = &mut node else {
            unreachable!()
        };
        let Node::Array(resources) = &mut fields
            .iter_mut()
            .find(|(name, _)| name == "map_resources")
            .unwrap()
            .1
        else {
            unreachable!()
        };
        resources.retain(|r| {
            r.required("kind", ROOT).unwrap().string(ROOT).unwrap() != "water"
                || !remove.contains(&position(r, ROOT).unwrap())
        });
        Ok(TerrainRecord {
            key,
            bytes: bytes(&node),
        })
    }
    fn post_overlay(&self, key: ChunkKey) -> Result<Vec<u8>, BoundaryError> {
        if let Some(record) = self.residents.get(&key) {
            return Ok(record.clone());
        }
        let raw = match self.world.raw_chunk(key)? {
            RawChunk::Saved(record) => record.private_bytes().to_vec(),
            RawChunk::Generated(record) => generated(record),
        };
        // These phases validate even completed/no-op records before editing.
        let ordinary = OrdinaryVillageOverlay::materialize(&raw, &self.villages, key, self.size)?;
        let swamp = SwampOverlay::materialize(
            ordinary.private_record_bytes(),
            &self.villages,
            key,
            self.size,
        )?;
        Ok(swamp.private_record_bytes().to_vec())
    }
}
fn adjacent((x, y): Point) -> [Point; 4] {
    [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
}
fn bytes(node: &Node) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_node(node, &mut bytes);
    bytes
}
fn waters(node: &Node) -> Result<BTreeSet<Point>, BoundaryError> {
    let mut points = BTreeSet::new();
    for item in node.required("map_resources", ROOT)?.array(ROOT)? {
        if item.required("kind", ROOT)?.string(ROOT)? == "water" {
            points.insert(position(item, ROOT)?);
        }
    }
    Ok(points)
}
fn validate(source: &[u8], key: ChunkKey, generator: u32, path: &str) -> Result<(), BoundaryError> {
    let record = Node::parse(source, path)?;
    rows(&record, path)?;
    validate_resources(&record, key.x as i32 * 40, key.y as i32 * 40, path)?;
    version(&record, "village_version", 3, path)?;
    version(&record, "swamp_version", 2, path)?;
    if let Some(version) = version(&record, "terrain_version", 7, path)?
        && version != u64::from(generator)
    {
        return Err(BoundaryError::invalid(
            path,
            "resident terrain version mismatch",
        ));
    }
    if let Some(clock) = record.optional("clock", path)? {
        clock.nonnegative_number(path)?;
    }
    Ok(())
}
fn generated(record: lantern_core::terrain::GeneratedChunk) -> Vec<u8> {
    #[derive(Serialize)]
    struct Resource {
        kind: &'static str,
        x: u32,
        y: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        species: Option<&'static str>,
    }
    #[derive(Serialize)]
    struct Record {
        terrain: Vec<String>,
        map_resources: Vec<Resource>,
        tree_stumps: [(); 0],
        clock: u8,
    }
    serde_json::to_vec(&Record {
        terrain: record.terrain,
        map_resources: record
            .resources
            .into_iter()
            .map(|r| Resource {
                kind: r.kind,
                x: r.x,
                y: r.y,
                species: r.species,
            })
            .collect(),
        tree_stumps: [],
        clock: 0,
    })
    .unwrap()
}
