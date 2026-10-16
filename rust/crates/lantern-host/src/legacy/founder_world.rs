//! Immutable walking_map/ChunkTerrain assembly from an explicitly supplied copy.
//! No activation, generation, source rebinding, or writes happen during commands.
use super::{ChunkKey, PrivateFounderCopy, json::Node, village_overlay::write_node};
use crate::BoundaryError;
use lantern_core::{
    founder::{Direction, FounderScene, FounderState, MovementResult},
    units::{Position, Tile},
};
use std::collections::BTreeMap;
const PATH: &str = "$.founder_world";
const TILE_LIMIT: usize = 1_000_000;

/// Arbitrary private world fields and original bytes are deliberately not Debug
/// or Serialize. Chunk records must be supplied in ChunkTerrain insertion order.
pub struct PrivateFounderWorldCopy {
    source: Vec<u8>,
    scene: Node,
    chunks: Option<Vec<(ChunkKey, Vec<Vec<char>>)>>,
    local_resources: bool,
}
impl PrivateFounderWorldCopy {
    pub fn decode(source: &[u8], town: &str) -> Result<Self, BoundaryError> {
        Self::with_active_chunks(source, town, &[])
    }
    /// Records are already hydrated/overlaid R07 active copies, not raw saves.
    /// shared_map.map_resources is the rebound authoritative aggregate (it may
    /// include in-place changes not yet synced into records); camera resources
    /// must not be included. Chunk records supply only terrain and recovery order.
    /// Use ChunkLifecycle::founder_scene to obtain that boundary automatically.
    /// Empty supplied active terrain stays unavailable; this never loads chunks.
    pub fn with_active_chunks(
        source: &[u8],
        town: &str,
        records: &[(ChunkKey, &[u8])],
    ) -> Result<Self, BoundaryError> {
        let world = Node::parse(source, PATH)?;
        let towns = world.required("towns", PATH)?.object(PATH)?;
        let selected = towns
            .iter()
            .find(|(key, _)| key == town)
            .ok_or_else(|| invalid("unknown town"))?
            .1
            .clone();
        let shared = world.optional("shared_map", PATH)?;
        let mut scene = selected;
        if let Some(landscape) = shared {
            // bind_map updates all landscape fields, including NPC/resource ownership.
            for (key, value) in landscape.object(PATH)? {
                set(&mut scene, key, value.clone())?;
            }
            for field in ["buildings", "roads"] {
                let mut values = Vec::new();
                for (_, town) in towns {
                    if let Some(items) = town.optional(field, PATH)? {
                        values.extend(items.array(PATH)?.iter().cloned());
                    }
                }
                set(&mut scene, field, Node::Array(values))?;
            }
        }
        if scene.optional("player", PATH)?.is_none() {
            set(
                &mut scene,
                "player",
                Node::parse(br#"{"x":5.0,"y":7.0,"facing":1}"#, PATH)?,
            )?;
        }
        let chunked = match scene.optional("chunked", PATH)? {
            None => false,
            Some(Node::Scalar(raw)) if raw.get() == "1" => true,
            Some(Node::Scalar(raw)) if raw.get() == "0" => false,
            Some(value) => value.boolean(PATH)?,
        };
        let chunks = if chunked {
            if shared.is_none() {
                return Err(invalid("chunk terrain requires shared world"));
            }
            let size = scene.required("map_size", PATH)?.unsigned(PATH)?;
            if size == 0 || size % 40 != 0 || size > i32::MAX as u64 {
                return Err(invalid("unsupported chunk map size"));
            }
            if records.len() > TILE_LIMIT / 1600 {
                return Err(invalid("active terrain exceeds recovery limit"));
            }
            let mut seen = BTreeMap::new();
            let mut chunks = Vec::new();
            for &(key, record) in records {
                if u64::from(key.x) >= size / 40
                    || u64::from(key.y) >= size / 40
                    || seen.insert(key, ()).is_some()
                {
                    return Err(invalid("invalid active chunk key"));
                }
                let root = Node::parse(record, PATH)?;
                let rows = root.required("terrain", PATH)?.array(PATH)?;
                let rows: Vec<Vec<char>> = rows
                    .iter()
                    .map(|r| r.string(PATH).map(|r| r.chars().collect()))
                    .collect::<Result<_, _>>()?;
                if rows.len() != 40 || rows.iter().any(|r| r.len() != 40) {
                    return Err(invalid("invalid active terrain dimensions"));
                }
                chunks.push((key, rows));
            }
            set(&mut scene, "chunked", Node::parse(b"false", PATH)?)?;
            set(&mut scene, "terrain", Node::Array(Vec::new()))?;
            Some(chunks)
        } else {
            if !records.is_empty() {
                return Err(invalid("active chunks on ordinary map"));
            }
            None
        };
        // Validate all geometry/access now; exact original remains separate.
        let decoded = PrivateFounderCopy::decode(&bytes(&scene))?;
        if chunks.is_none()
            && u64::from(decoded.geometry.derive_index()?.size()).pow(2) > TILE_LIMIT as u64
        {
            return Err(invalid("ordinary terrain exceeds recovery limit"));
        }
        Ok(Self {
            source: source.to_vec(),
            scene,
            chunks,
            local_resources: chunked,
        })
    }
    pub fn private_bytes(&self) -> &[u8] {
        &self.source
    }
    pub fn state(&self) -> Result<FounderState, BoundaryError> {
        Ok(PrivateFounderCopy::decode(&bytes(&self.scene))?.state())
    }
    fn scene(&self, state: FounderState, local: bool) -> Result<PrivateFounderCopy, BoundaryError> {
        let mut scene = self.scene.clone();
        if local && self.local_resources {
            let x = (state.position.x.get() + 0.5).floor();
            let y = (state.position.y.get() + 0.5).floor();
            let mut resources = Vec::new();
            // Python's index window is y-major, then x, then source-list order.
            for ty in -2..=2 {
                for tx in -2..=2 {
                    for r in scene.required("map_resources", PATH)?.array(PATH)? {
                        if f64::from(r.required("x", PATH)?.tile(PATH)?) == x + f64::from(tx)
                            && f64::from(r.required("y", PATH)?.tile(PATH)?) == y + f64::from(ty)
                        {
                            resources.push(r.clone());
                        }
                    }
                }
            }
            set(&mut scene, "map_resources", Node::Array(resources))?;
        }
        let mut result = PrivateFounderCopy::decode(&bytes(&scene))?;
        result.state = state;
        Ok(result)
    }
    fn terrain_at(&self, copy: &PrivateFounderCopy, tile: Tile) -> Option<char> {
        if let Some(chunks) = &self.chunks {
            if tile.x < 0 || tile.y < 0 {
                return Some('v');
            }
            return Some(
                chunks
                    .iter()
                    .find(|(k, _)| k.x == tile.x as u32 / 40 && k.y == tile.y as u32 / 40)
                    .map(|(_, rows)| rows[tile.y as usize % 40][tile.x as usize % 40])
                    .unwrap_or('v'),
            );
        }
        copy.terrain_at(tile)
    }
    pub fn walkable(&self, position: Position) -> Result<bool, BoundaryError> {
        let copy = self.scene(self.state()?, false)?;
        let index = copy.geometry.derive_index()?;
        Ok(FounderScene {
            spatial: &index,
            terrain: |t| self.terrain_at(&copy, t),
        }
        .walkable(position, copy.access))
    }
    pub fn walk_batch(&self, inputs: &[(i32, i32)]) -> Result<MovementResult, BoundaryError> {
        if inputs.is_empty() || inputs.len() > 5 {
            return Err(invalid("invalid movement batch"));
        }
        let directions: Vec<_> = inputs
            .iter()
            .map(|&(x, y)| Direction::new(x, y).map_err(|_| invalid("invalid movement direction")))
            .collect::<Result<_, _>>()?;
        let mut result = MovementResult {
            state: self.state()?,
            clear_activity: false,
        };
        for direction in directions {
            // Rebuild nearby resources from the *pre-recovery* start each command.
            let copy = self.scene(result.state, true)?;
            let index = copy.geometry.derive_index()?;
            let scene = FounderScene {
                spatial: &index,
                terrain: |t| self.terrain_at(&copy, t),
            };
            if !scene.walkable(result.state.position, copy.access) {
                let mut candidates = Vec::new();
                if let Some(chunks) = &self.chunks {
                    for (key, _) in chunks {
                        for y in key.y * 40..(key.y + 1) * 40 {
                            for x in key.x * 40..(key.x + 1) * 40 {
                                candidates.push((x, y));
                            }
                        }
                    }
                } else {
                    for x in 0..index.size() {
                        for y in 0..index.size() {
                            candidates.push((x, y));
                        }
                    }
                }
                let start = result.state.position;
                let distance = |&(x, y): &(u32, u32)| {
                    (f64::from(x) - start.x.get()).abs() + (f64::from(y) - start.y.get()).abs()
                };
                candidates.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
                for (x, y) in candidates {
                    let position = Position::new(f64::from(x), f64::from(y)).unwrap();
                    if scene.walkable(position, copy.access) {
                        result.state.position = position;
                        break;
                    }
                }
            }
            let next = scene
                .walk_after_recovery(result.state, direction, copy.access)
                .map_err(|_| invalid("invalid movement access"))?;
            result.state = next.state;
            result.clear_activity |= next.clear_activity;
        }
        Ok(result)
    }
}
fn invalid(reason: &'static str) -> BoundaryError {
    BoundaryError::invalid(PATH, reason)
}
fn bytes(node: &Node) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_node(node, &mut bytes);
    bytes
}
fn set(node: &mut Node, key: &str, value: Node) -> Result<(), BoundaryError> {
    let Node::Object(fields) = node else {
        return Err(invalid("expected object"));
    };
    if let Some((_, existing)) = fields.iter_mut().find(|(name, _)| name == key) {
        *existing = value;
    } else {
        fields.push((key.into(), value));
    }
    Ok(())
}
