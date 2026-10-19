//! Single private in-memory chunk owner. No filesystem writes or playable host.
use super::{
    CaughtUpChunk, ChunkKey, ImportedRegistry, PostOverlayTerrain, ResidentChunkCopy,
    clock::Clock,
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
/// Complete detached persistence copy, including sleeping imported archives.
/// Active records override archive records; camera/cache copies are never saved.
pub struct TerrainSaveSnapshot {
    owner: WorldId,
    state: Owned,
    originals: BTreeMap<ChunkKey, Vec<u8>>,
}
impl TerrainSaveSnapshot {
    pub fn world(&self) -> &WorldId {
        &self.owner
    }
    pub fn encode(&self) -> Result<super::TerrainArchive, BoundaryError> {
        let mut state = self.state.clone();
        flush(&mut state)?;
        let mut records = self.originals.clone();
        records.extend(state.archive);
        records.extend(state.active);
        // These are derived active-window aggregates. The committed chunk files
        // own their contents; retaining inline mirrors would be ambiguous on load.
        if let Node::Object(shared) = landscape(&mut state.world)? {
            shared.retain(|(name, _)| {
                !["terrain", "map_resources", "tree_stumps"].contains(&name.as_str())
            });
        }
        super::TerrainArchive::decode(&bytes(&state.world), records.into_iter().collect())
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
/// Clock scalars retain Python integer/float arithmetic and exact source lexemes.
/// Public wire snapshots and real persistence remain gated.
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
        Clock::parse(&Node::parse(&clock, PATH)?, PATH)?;
        for (&key, record) in result.state.active.iter().chain(&result.state.camera) {
            // Validate resident rules without performing a sleeping catch-up:
            // active reads bypass it, and camera reads apply it only on demand.
            let resident = Node::parse(record, PATH)?;
            let resident_clock = resident
                .optional("clock", PATH)?
                .map(bytes)
                .unwrap_or_else(|| b"0".to_vec());
            CaughtUpChunk::prepare(
                record,
                key,
                result.size,
                result.generator,
                &resident_clock,
                &occupied,
            )?;
        }
        Ok(result)
    }
    pub fn capture_save(&self) -> TerrainSaveSnapshot {
        TerrainSaveSnapshot {
            owner: self.owner.clone(),
            state: self.state.clone(),
            originals: self.terrain.private_saved_records(),
        }
    }
    pub fn private_world_bytes(&self) -> Vec<u8> {
        bytes(&self.state.world)
    }
    /// Assemble a read-only movement scene from current active records only.
    /// Camera records remain unavailable. This captures the already-active
    /// snapshot; preview_founder_batch also activates between commands.
    pub fn founder_scene(
        &self,
        town: &str,
    ) -> Result<super::PrivateFounderWorldCopy, BoundaryError> {
        let records: Vec<_> = self
            .state
            .active_order
            .iter()
            .map(|key| (*key, self.state.active[key].as_slice()))
            .collect();
        super::PrivateFounderWorldCopy::with_active_chunks(
            &self.private_world_bytes(),
            town,
            &records,
        )
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
        Clock::parse(&value, PATH)?;
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
            let now = Clock::parse(&Node::parse(&clock, PATH)?, PATH)?;
            if node
                .optional("clock", PATH)?
                .map(|node| Clock::parse(node, PATH))
                .transpose()?
                .is_some_and(|previous| previous.equal(&now))
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
        let node = Node::parse(record, PATH)?;
        let resident_clock = node
            .optional("clock", PATH)?
            .map(bytes)
            .unwrap_or_else(|| b"0".to_vec());
        CaughtUpChunk::prepare(
            record,
            key,
            self.size,
            self.generator,
            &resident_clock,
            &self.occupied_tiles()?,
        )?;
        let mut state = self.state.clone();
        state.active.insert(key, record.to_vec());
        state.snapshots.remove(&key);
        rebind(&mut state)?;
        self.state = state;
        Ok(())
    }
    /// One explicit Python three-second renewable-resource batch. Does not advance
    /// the clock or run production; the host increments time after this phase.
    /// Passive reads, snapshots and activation never invoke this method.
    pub fn renew_active(&mut self) -> Result<(), BoundaryError> {
        let mut state = self.state.clone();
        renew(&mut state, self.generator)?;
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
        let mut state = self.state.clone();
        self.activate_into(&mut state)?;
        self.state = state;
        Ok(())
    }
    fn activate_into(&self, state: &mut Owned) -> Result<(), BoundaryError> {
        let desired = desired(&state.world, self.size)?;
        if desired == state.active.keys().copied().collect() {
            return Ok(());
        }
        flush(state)?;
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
                let record = self.read_into(state, key)?;
                state.camera.remove(&key);
                state.order.retain(|k| *k != key);
                state.saved.insert(key, record.clone());
                state.active.insert(key, record);
                state.active_order.push(key);
            }
        }
        rebind(state)?;
        Ok(())
    }
    /// Evaluate authoritative founder movement on a disposable owner clone.
    /// Whole-batch validation precedes activation/recovery. Python attaches its
    /// manager on every recursive command, so refresh active terrain between
    /// inputs, including saved-first reads/catch-up and source-resource rebinding.
    /// No owner records, caches, world, clocks or pending write intents change.
    pub fn preview_founder_batch(
        &self,
        town: &str,
        inputs: &[(i32, i32)],
    ) -> Result<lantern_core::founder::MovementResult, BoundaryError> {
        use lantern_core::founder::{Direction, MovementResult};
        if inputs.is_empty() || inputs.len() > 5 {
            return Err(invalid("invalid movement batch"));
        }
        for &(x, y) in inputs {
            Direction::new(x, y).map_err(|_| invalid("invalid movement direction"))?;
        }
        let mut state = self.state.clone();
        let mut result: Option<MovementResult> = None;
        for &input in inputs {
            self.activate_into(&mut state)?;
            let records: Vec<_> = state
                .active_order
                .iter()
                .map(|key| (*key, state.active[key].as_slice()))
                .collect();
            let scene = super::PrivateFounderWorldCopy::with_active_chunks(
                &bytes(&state.world),
                town,
                &records,
            )?;
            let next = scene.walk_batch(&[input])?;
            let clear_activity = next.clear_activity || result.is_some_and(|r| r.clear_activity);
            result = Some(MovementResult {
                clear_activity,
                ..next
            });
            let player = field_mut(
                field_mut(field_mut(&mut state.world, "towns")?, town)?,
                "player",
            )?;
            set(
                player,
                "x",
                scalar(coordinate(next.state.position.x.get())?),
            );
            set(
                player,
                "y",
                scalar(coordinate(next.state.position.y.get())?),
            );
            set(player, "facing", scalar(next.state.facing));
        }
        Ok(result.unwrap())
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
fn renew(state: &mut Owned, generator: u32) -> Result<(), BoundaryError> {
    let mut terrain = BTreeMap::new();
    for (&key, record) in &state.active {
        terrain.insert(
            key,
            super::village_overlay::rows(&Node::parse(record, PATH)?, PATH)?,
        );
    }
    let biome = |r: &Node| -> Result<u8, BoundaryError> {
        let (x, y) = position(r, PATH)?;
        let key = ChunkKey {
            x: (x / 40) as u32,
            y: (y / 40) as u32,
        };
        terrain
            .get(&key)
            .and_then(|rows| rows.get((y % 40) as usize))
            .and_then(|row| row.get((x % 40) as usize))
            .copied()
            .ok_or_else(|| invalid("resource outside active terrain"))
    };
    let mut obstacles = BTreeSet::new();
    for (_, town) in state.world.required("towns", PATH)?.object(PATH)? {
        for field in ["buildings", "roads"] {
            if let Some(items) = town.optional(field, PATH)? {
                for r in items.array(PATH)? {
                    obstacles.insert(position(r, PATH)?);
                }
            }
        }
        if let Some(player) = town.optional("player", PATH)? {
            let (x, y) = float_position(player)?;
            obstacles.insert(((x + 0.5).floor() as i32, (y + 0.5).floor() as i32));
        }
    }
    let map = landscape(&mut state.world)?;
    if let Some(npcs) = map.optional("npcs", PATH)? {
        for r in npcs.array(PATH)? {
            obstacles.insert(position(r, PATH)?);
        }
    }
    let mut resources = map.required("map_resources", PATH)?.array(PATH)?.to_vec();
    let mut stumps = map.required("tree_stumps", PATH)?.array(PATH)?.to_vec();
    // Reject malformed rule fields even when a later terrain filter would drop
    // the resource, or an update would overwrite them.
    for r in resources.iter().chain(&stumps) {
        if !matches!(
            resource_kind(r)?.as_str(),
            "tree" | "water" | "rock" | "berry_bush" | "herb_patch"
        ) {
            continue;
        }
        for field in ["supply", "supply_max", "regeneration"] {
            number_or(r, field, 0.0)?;
        }
    }
    for stump in &stumps {
        stump
            .required("remaining", PATH)?
            .nonnegative_number(PATH)?;
    }
    // Match prepare_landscapes' remove_invalid_trees before renew.
    for (items, allowed) in [
        (
            &mut resources,
            if generator < 6 {
                &b"gfmb"[..]
            } else {
                &b"gfb"[..]
            },
        ),
        (
            &mut stumps,
            if generator < 6 {
                &b"gfmb"[..]
            } else {
                &b"fb"[..]
            },
        ),
    ] {
        let mut kept = Vec::new();
        for r in items.drain(..) {
            if resource_kind(&r)? != "tree" || allowed.contains(&biome(&r)?) {
                kept.push(r);
            }
        }
        *items = kept;
    }
    let mut occupied = obstacles;
    for r in &resources {
        occupied.insert(position(r, PATH)?);
    }
    let mut remaining = Vec::new();
    for mut stump in stumps {
        let p = position(&stump, PATH)?;
        if generator >= 6 && !b"fb".contains(&biome(&stump)?) {
            continue;
        }
        if occupied.contains(&p) {
            remaining.push(stump);
            continue;
        }
        let timer = stump
            .required("remaining", PATH)?
            .nonnegative_number(PATH)?;
        let timer: f64 = format!("{:.9}", timer - 3.0 * 0.9).parse().unwrap();
        if timer <= 0.0 {
            let Node::Object(fields) = &mut stump else {
                unreachable!()
            };
            fields.retain(|(k, _)| k != "remaining");
            resources.push(stump);
            occupied.insert(p);
        } else {
            set(&mut stump, "remaining", scalar(timer));
            remaining.push(stump);
        }
    }
    // Python dict assignment keeps the first coordinate's insertion position but
    // replaces its value with the LAST water resource at a duplicate coordinate.
    let mut water_order = Vec::new();
    let mut waters = BTreeMap::new();
    for (i, r) in resources.iter().enumerate() {
        if resource_kind(r)? == "water" {
            let p = position(r, PATH)?;
            if !waters.contains_key(&p) {
                water_order.push(p);
            }
            waters.insert(p, i);
        }
    }
    let mut groups = Vec::new();
    for start in water_order {
        if !waters.contains_key(&start) {
            continue;
        }
        let mut queue = VecDeque::from([start]);
        let mut members = Vec::new();
        while let Some(p) = queue.pop_front() {
            if let Some(i) = waters.remove(&p) {
                members.push(i);
                let (x, y) = p;
                for q in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
                    if waters.contains_key(&q) {
                        queue.push_back(q);
                    }
                }
            }
        }
        groups.push(members);
    }
    for (i, r) in resources.iter().enumerate() {
        if matches!(
            resource_kind(r)?.as_str(),
            "tree" | "rock" | "berry_bush" | "herb_patch"
        ) {
            groups.push(vec![i]);
        }
    }
    for members in groups {
        let first = &resources[members[0]];
        let kind = resource_kind(first)?;
        let (maximum, regeneration) = match kind.as_str() {
            "tree" => {
                let ancient = first
                    .optional("species", PATH)?
                    .map(|n| n.string(PATH))
                    .transpose()?
                    .is_some_and(|s| s == "ancient");
                (
                    if biome(first)? == b'b' || ancient {
                        40.0
                    } else {
                        8.0
                    },
                    0.0,
                )
            }
            "water" => (12.0, 3.2),
            "rock" => (60.0, 0.15),
            "berry_bush" => (6.0, 0.3),
            "herb_patch" => (4.0, 0.2),
            _ => unreachable!(),
        };
        let root = *members
            .iter()
            .min_by_key(|&&i| position(&resources[i], PATH).unwrap())
            .unwrap();
        let supply = number_or(&resources[root], "supply", maximum)?;
        let mut available = (supply + regeneration).min(maximum);
        let old_max = number_or(&resources[root], "supply_max", maximum)?;
        if kind == "tree" && old_max != maximum {
            if old_max == 0.0 {
                return Err(invalid("invalid tree supply maximum"));
            }
            available = (supply * maximum / old_max).min(maximum);
        }
        for i in members {
            let r = &mut resources[i];
            // Python updates all three only when some value differs. Preserve
            // untouched numeric lexemes and arbitrary private extensions.
            if number_or(r, "supply", f64::NAN)? != available
                || number_or(r, "supply_max", f64::NAN)? != maximum
                || number_or(r, "regeneration", f64::NAN)? != regeneration
            {
                set(r, "supply", scalar(available));
                set(r, "supply_max", scalar(maximum));
                set(r, "regeneration", scalar(regeneration));
            }
        }
    }
    set(map, "map_resources", Node::Array(resources.clone()));
    set(map, "tree_stumps", Node::Array(remaining.clone()));
    // Sync membership immediately while retaining Python's aggregate append order.
    for (&key, record) in &mut state.active {
        let mut node = Node::parse(record, PATH)?;
        for (field, items) in [("map_resources", &resources), ("tree_stumps", &remaining)] {
            let mut local = Vec::new();
            for r in items {
                let (x, y) = position(r, PATH)?;
                if x / 40 == key.x as i32 && y / 40 == key.y as i32 {
                    local.push(r.clone());
                }
            }
            set(&mut node, field, Node::Array(local));
        }
        *record = bytes(&node);
    }
    state.snapshots.clear();
    Ok(())
}
fn resource_kind(node: &Node) -> Result<String, BoundaryError> {
    node.required("kind", PATH)?.string(PATH)
}
fn number_or(node: &Node, field: &str, default: f64) -> Result<f64, BoundaryError> {
    node.optional(field, PATH)?
        .map(|n| n.nonnegative_number(PATH))
        .transpose()
        .map(|n| n.unwrap_or(default))
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
