//! Single private in-memory chunk owner. No filesystem writes or playable host.
use super::{
    CaughtUpChunk, ChunkKey, ImportedRegistry, PostOverlayTerrain, ResidentChunkCopy,
    json::Node,
    village_overlay::{position, write_node},
};
use crate::BoundaryError;
use lantern_core::ids::WorldId;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
const PATH: &str = "$.chunk_lifecycle";
const MAX_VIEWS: usize = 49;

/// Explicit private persistence request. R19 owns durable acknowledgement/recovery.
pub struct ChunkWriteIntent {
    world: WorldId,
    key: ChunkKey,
    bytes: Vec<u8>,
}
impl ChunkWriteIntent {
    pub fn world(&self) -> &WorldId {
        &self.world
    }
    pub fn key(&self) -> ChunkKey {
        self.key
    }
    pub fn private_record_bytes(&self) -> &[u8] {
        &self.bytes
    }
}
/// Independent derived private copy; public wire filtering/versioning is not implied.
pub struct PrivateChunkSnapshot {
    records: BTreeMap<ChunkKey, Vec<u8>>,
}
impl PrivateChunkSnapshot {
    pub fn keys(&self) -> impl Iterator<Item = ChunkKey> + '_ {
        self.records.keys().copied()
    }
    pub fn private_record_bytes(&self, key: ChunkKey) -> Option<&[u8]> {
        self.records.get(&key).map(Vec::as_slice)
    }
}
#[derive(Clone)]
struct Owned {
    world: Node,
    active: BTreeMap<ChunkKey, Vec<u8>>,
    active_order: Vec<ChunkKey>,
    camera: BTreeMap<ChunkKey, Vec<u8>>,
    order: VecDeque<ChunkKey>,
    archive: BTreeMap<ChunkKey, Vec<u8>>,
    saved: BTreeMap<ChunkKey, Vec<u8>>,
    pending: BTreeMap<ChunkKey, Vec<u8>>,
    snapshots: BTreeMap<ChunkKey, Vec<u8>>,
}
/// Owns all mutable records separately from immutable import and returned copies.
/// Active records are authoritative; camera/snapshot records are disposable.
/// Supplied clocks retain CaughtUpChunk's explicit <=2^53 compatibility domain.
/// Active renew/regrowth, public wire snapshots and real persistence remain gated.
/// ```compile_fail
/// use lantern_host::legacy::ChunkLifecycle;
/// fn leak(owner: &ChunkLifecycle<'_>) { serde_json::to_string(owner).unwrap(); }
/// ```
pub struct ChunkLifecycle<'a> {
    owner: WorldId,
    terrain: PostOverlayTerrain<'a>,
    state: Owned,
    size: u32,
    generator: u32,
}
impl<'a> ChunkLifecycle<'a> {
    pub fn prepare(
        registry: &'a ImportedRegistry,
        world: &WorldId,
        active: &[ResidentChunkCopy<'_>],
        camera: &[ResidentChunkCopy<'_>],
    ) -> Result<Self, BoundaryError> {
        if camera.len() > MAX_VIEWS {
            return Err(invalid("too many camera chunks"));
        }
        let copies: Vec<_> = active.iter().chain(camera).map(|c| c.copy()).collect();
        let terrain = registry.materialize_chunked(world, &copies)?;
        let metadata = registry
            .worlds()
            .iter()
            .find(|w| w.id() == world)
            .unwrap()
            .chunk_metadata()
            .unwrap()
            .report();
        let mut state = Owned {
            world: Node::parse(terrain.private_world_bytes(), PATH)?,
            active: BTreeMap::new(),
            active_order: Vec::new(),
            camera: BTreeMap::new(),
            order: VecDeque::new(),
            archive: BTreeMap::new(),
            saved: BTreeMap::new(),
            pending: BTreeMap::new(),
            snapshots: BTreeMap::new(),
        };
        for copy in active {
            state.active_order.push(copy.key());
            state.active.insert(
                copy.key(),
                terrain.read(copy.key())?.private_record_bytes().to_vec(),
            );
        }
        for copy in camera {
            state.camera.insert(
                copy.key(),
                terrain.read(copy.key())?.private_record_bytes().to_vec(),
            );
            state.order.push_back(copy.key());
        }
        // materialize_chunked already applies EACH plan ordinal, in order, to
        // every resident. Install those results once; never overlay on promotion.
        // Bootstrap aggregate ownership even without upgrades: imported registry
        // payloads may omit aggregates or contain stale inspection copies.
        let order = if terrain.cache_plan().village_ordinals.is_empty() {
            sorted(state.active.keys().copied())
        } else {
            state.active_order.clone()
        };
        rebind_in_order(&mut state, order)?;
        let result = Self {
            owner: world.clone(),
            terrain,
            state,
            size: metadata.map_size,
            generator: metadata.generator_version,
        };
        result.desired()?;
        let occupied = result.occupied_tiles()?;
        let clock = clock(&result.state.world)?;
        super::catch_up::time(&Node::parse(&clock, PATH)?)?;
        for (&key, record) in result.state.active.iter().chain(&result.state.camera) {
            CaughtUpChunk::prepare(
                record,
                key,
                result.size,
                result.generator,
                &clock,
                &occupied,
            )?;
        }
        Ok(result)
    }
    pub fn private_world_bytes(&self) -> Vec<u8> {
        bytes(&self.state.world)
    }
    pub fn active_keys(&self) -> Vec<ChunkKey> {
        sorted(self.state.active.keys().copied())
    }
    pub fn camera_keys_lru(&self) -> Vec<ChunkKey> {
        self.state.order.iter().copied().collect()
    }
    pub fn snapshot_cache_len(&self) -> usize {
        self.state.snapshots.len()
    }
    pub fn private_active_bytes(&self, key: ChunkKey) -> Option<&[u8]> {
        self.state.active.get(&key).map(Vec::as_slice)
    }
    pub fn desired(&self) -> Result<BTreeSet<ChunkKey>, BoundaryError> {
        desired(&self.state.world, self.size)
    }
    pub fn occupied_tiles(&self) -> Result<Vec<(i32, i32)>, BoundaryError> {
        occupied(&self.state.world, self.size)
    }
    /// Only changes supplied simulation time. Active records do not catch up.
    pub fn set_clock(&mut self, clock: &[u8]) -> Result<(), BoundaryError> {
        let value = Node::parse(clock, PATH)?;
        super::catch_up::time(&value)?;
        set(landscape(&mut self.state.world)?, "chunk_clock", value);
        Ok(())
    }
    pub fn move_founder(&mut self, town: &str, x: f64, y: f64) -> Result<(), BoundaryError> {
        let px = coordinate(x)?;
        let py = coordinate(y)?;
        let towns = field_mut(&mut self.state.world, "towns")?;
        let town = field_mut(towns, town)?;
        let player = field_mut(town, "player")?;
        player.object(PATH)?;
        set(player, "x", scalar(px));
        set(player, "y", scalar(py));
        Ok(())
    }
    /// Returns a copy, so callers cannot change active or cached authoritative data.
    pub fn read(&mut self, key: ChunkKey) -> Result<Vec<u8>, BoundaryError> {
        let mut state = self.state.clone();
        let record = self.read_into(&mut state, key)?;
        self.state = state;
        Ok(record)
    }
    fn read_into(&self, state: &mut Owned, key: ChunkKey) -> Result<Vec<u8>, BoundaryError> {
        if let Some(record) = state.active.get(&key) {
            return Ok(record.clone());
        }
        let clock = clock(&state.world)?;
        let source = if let Some(record) = state.camera.get(&key) {
            let node = Node::parse(record, PATH)?;
            if node
                .optional("clock", PATH)?
                .map(super::catch_up::time)
                .transpose()?
                == Some(super::catch_up::time(&Node::parse(&clock, PATH)?)?)
            {
                state.order.retain(|k| *k != key);
                state.order.push_back(key);
                return Ok(record.clone());
            }
            record.clone()
        } else {
            let residents = state
                .active
                .iter()
                .chain(&state.camera)
                .map(|(&k, v)| (k, v.clone()))
                .collect();
            self.terrain
                .read_owned(key, &residents, &state.archive)?
                .private_record_bytes()
                .to_vec()
        };
        let record = CaughtUpChunk::prepare(
            &source,
            key,
            self.size,
            self.generator,
            &clock,
            &occupied(&state.world, self.size)?,
        )?
        .private_record_bytes()
        .to_vec();
        state.camera.insert(key, record.clone());
        state.order.retain(|k| *k != key);
        state.order.push_back(key);
        while state.order.len() > MAX_VIEWS {
            state.camera.remove(&state.order.pop_front().unwrap());
        }
        Ok(record)
    }
    /// Validates a complete host-owned active edit, with no catch-up installed.
    /// Membership is represented directly by records: rebind aggregates immediately.
    pub fn replace_active(&mut self, key: ChunkKey, record: &[u8]) -> Result<(), BoundaryError> {
        if !self.state.active.contains_key(&key) {
            return Err(invalid("chunk is not active"));
        }
        CaughtUpChunk::prepare(
            record,
            key,
            self.size,
            self.generator,
            &clock(&self.state.world)?,
            &self.occupied_tiles()?,
        )?;
        let mut state = self.state.clone();
        state.active.insert(key, record.to_vec());
        state.snapshots.remove(&key);
        rebind(&mut state)?;
        self.state = state;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), BoundaryError> {
        let mut state = self.state.clone();
        flush(&mut state)?;
        self.state = state;
        Ok(())
    }
    /// Matches Python's unchanged-set early return, lexical activation order and
    /// flush-before-eviction. Failure never publishes partial records/write intents.
    pub fn activate(&mut self) -> Result<(), BoundaryError> {
        let desired = self.desired()?;
        if desired == self.state.active.keys().copied().collect() {
            return Ok(());
        }
        let mut state = self.state.clone();
        flush(&mut state)?;
        for key in state.active.keys().copied().collect::<Vec<_>>() {
            if !desired.contains(&key) {
                state.active.remove(&key);
                state.active_order.retain(|k| *k != key);
                state.saved.remove(&key);
                state.camera.remove(&key);
                state.order.retain(|k| *k != key);
            }
        }
        for key in sorted(desired.iter().copied()) {
            if !state.active.contains_key(&key) {
                let record = self.read_into(&mut state, key)?;
                state.camera.remove(&key);
                state.order.retain(|k| *k != key);
                state.saved.insert(key, record.clone());
                state.active.insert(key, record);
                state.active_order.push(key);
            }
        }
        rebind(&mut state)?;
        self.state = state;
        Ok(())
    }
    /// Bounded private snapshot preparation, not a public/client snapshot DTO.
    pub fn snapshot(&mut self, camera: &[ChunkKey]) -> Result<PrivateChunkSnapshot, BoundaryError> {
        if camera.len() > MAX_VIEWS {
            return Err(invalid("too many camera chunks"));
        }
        let mut state = self.state.clone();
        let keys: BTreeSet<_> = state
            .active
            .keys()
            .copied()
            .chain(camera.iter().copied())
            .collect();
        let mut records = BTreeMap::new();
        for key in sorted(keys.iter().copied()) {
            records.insert(key, self.read_into(&mut state, key)?);
        }
        state.snapshots = records.clone();
        self.state = state;
        Ok(PrivateChunkSnapshot { records })
    }
    pub fn take_write_intents(&mut self) -> Vec<ChunkWriteIntent> {
        std::mem::take(&mut self.state.pending)
            .into_iter()
            .map(|(key, bytes)| ChunkWriteIntent {
                world: self.owner.clone(),
                key,
                bytes,
            })
            .collect()
    }
}
fn flush(state: &mut Owned) -> Result<(), BoundaryError> {
    let clock = Node::parse(&clock(&state.world)?, PATH)?;
    for (&key, record) in &mut state.active {
        let mut node = Node::parse(record, PATH)?;
        let stumps = !node.required("tree_stumps", PATH)?.array(PATH)?.is_empty();
        let mut depleted = false;
        for r in node.required("map_resources", PATH)?.array(PATH)? {
            let max = r
                .optional("supply_max", PATH)?
                .map(|n| n.nonnegative_number(PATH))
                .transpose()?
                .unwrap_or(0.0);
            let supply = r
                .optional("supply", PATH)?
                .map(|n| n.nonnegative_number(PATH))
                .transpose()?
                .unwrap_or(max);
            let regen = r
                .optional("regeneration", PATH)?
                .map(|n| n.nonnegative_number(PATH))
                .transpose()?
                .unwrap_or(0.0);
            depleted |= supply < max && regen != 0.0;
        }
        if depleted || stumps {
            set(&mut node, "clock", clock.clone());
        }
        *record = bytes(&node);
        if state.saved.get(&key) != Some(record) {
            state.archive.insert(key, record.clone());
            state.pending.insert(key, record.clone());
            state.saved.insert(key, record.clone());
        }
    }
    Ok(())
}
fn rebind(state: &mut Owned) -> Result<(), BoundaryError> {
    let order = sorted(state.active.keys().copied());
    rebind_in_order(state, order)
}
fn rebind_in_order(state: &mut Owned, order: Vec<ChunkKey>) -> Result<(), BoundaryError> {
    for field in ["map_resources", "tree_stumps"] {
        let mut aggregate = Vec::new();
        for key in &order {
            aggregate.extend(
                Node::parse(&state.active[key], PATH)?
                    .required(field, PATH)?
                    .array(PATH)?
                    .iter()
                    .cloned(),
            );
        }
        set(landscape(&mut state.world)?, field, Node::Array(aggregate));
    }
    // No alias-based membership/resource index can survive a replacement. This
    // owner uses records directly and never retains such an index.
    state.snapshots.clear();
    Ok(())
}
fn desired(world: &Node, size: u32) -> Result<BTreeSet<ChunkKey>, BoundaryError> {
    let towns = world.required("towns", PATH)?.object(PATH)?;
    let mut points = Vec::new();
    for field in ["town_origin", "player"] {
        for (_, town) in towns {
            points.push(town.required(field, PATH)?);
        }
    }
    if points.is_empty() {
        points.extend(world.required("town_sites", PATH)?.array(PATH)?);
    }
    let mut result = BTreeSet::new();
    for point in points {
        let (x, y) = float_position(point)?;
        let cx = (x / 40.0).floor() as i64;
        let cy = (y / 40.0).floor() as i64;
        for x in 0.max(cx - 1)..(size as i64 / 40).min(cx + 2) {
            for y in 0.max(cy - 1)..(size as i64 / 40).min(cy + 2) {
                result.insert(ChunkKey {
                    x: x as u32,
                    y: y as u32,
                });
            }
        }
    }
    Ok(result)
}
fn occupied(world: &Node, size: u32) -> Result<Vec<(i32, i32)>, BoundaryError> {
    let mut result = BTreeSet::new();
    let mut obstacles = Vec::new();
    for (_, town) in world.required("towns", PATH)?.object(PATH)? {
        obstacles.extend(town.required("buildings", PATH)?.array(PATH)?);
        if let Some(roads) = town.optional("roads", PATH)? {
            obstacles.extend(roads.array(PATH)?);
        }
        if let Some(player) = town.optional("player", PATH)? {
            let (x, y) = float_position(player)?;
            result.insert((x.round_ties_even() as i32, y.round_ties_even() as i32));
        }
    }
    if let Some(npcs) = world.required("shared_map", PATH)?.optional("npcs", PATH)? {
        obstacles.extend(npcs.array(PATH)?);
    }
    // Only in-world tiles can block this world's generated horses; clipping
    // also avoids expanding arbitrarily large imported rectangle dimensions.
    for item in obstacles {
        let (x, y) = position(item, PATH)?;
        let (w, h) = if let Some(fp) = item.optional("footprint", PATH)? {
            let fp = fp.array(PATH)?;
            if fp.len() != 2 {
                return Err(invalid("invalid footprint"));
            }
            (fp[0].tile(PATH)?, fp[1].tile(PATH)?)
        } else {
            (1, 1)
        };
        for tx in i64::from(x).max(0)..(i64::from(x) + i64::from(w)).min(i64::from(size)) {
            for ty in i64::from(y).max(0)..(i64::from(y) + i64::from(h)).min(i64::from(size)) {
                result.insert((tx as i32, ty as i32));
            }
        }
    }
    Ok(result.into_iter().collect())
}
fn float_position(node: &Node) -> Result<(f64, f64), BoundaryError> {
    fn number(n: &Node) -> Result<f64, BoundaryError> {
        let Node::Scalar(raw) = n else {
            return Err(invalid("invalid coordinate"));
        };
        coordinate(
            raw.get()
                .parse()
                .map_err(|_| invalid("invalid coordinate"))?,
        )
    }
    Ok((
        number(node.required("x", PATH)?)?,
        number(node.required("y", PATH)?)?,
    ))
}
fn coordinate(v: f64) -> Result<f64, BoundaryError> {
    if v.is_finite() && v.floor() >= i32::MIN as f64 && v.ceil() <= i32::MAX as f64 {
        Ok(v)
    } else {
        Err(invalid("unsupported coordinate"))
    }
}
fn clock(world: &Node) -> Result<Vec<u8>, BoundaryError> {
    Ok(world
        .required("shared_map", PATH)?
        .optional("chunk_clock", PATH)?
        .map(bytes)
        .unwrap_or_else(|| b"0".to_vec()))
}
fn sorted(keys: impl IntoIterator<Item = ChunkKey>) -> Vec<ChunkKey> {
    let mut keys: Vec<_> = keys.into_iter().collect();
    keys.sort_by_key(|k| k.file_name());
    keys
}
fn bytes(node: &Node) -> Vec<u8> {
    let mut out = Vec::new();
    write_node(node, &mut out);
    out
}
fn scalar(v: impl serde::Serialize) -> Node {
    Node::Scalar(serde_json::value::to_raw_value(&v).unwrap())
}
fn set(node: &mut Node, name: &str, value: Node) {
    let Node::Object(fields) = node else {
        unreachable!()
    };
    if let Some((_, old)) = fields.iter_mut().find(|(k, _)| k == name) {
        *old = value
    } else {
        fields.push((name.into(), value))
    }
}
fn field_mut<'a>(node: &'a mut Node, name: &str) -> Result<&'a mut Node, BoundaryError> {
    let Node::Object(fields) = node else {
        return Err(invalid("expected object"));
    };
    fields
        .iter_mut()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v)
        .ok_or_else(|| invalid("missing required field"))
}
fn landscape(world: &mut Node) -> Result<&mut Node, BoundaryError> {
    field_mut(world, "shared_map")
}
fn invalid(reason: &'static str) -> BoundaryError {
    BoundaryError::invalid(PATH, reason)
}
