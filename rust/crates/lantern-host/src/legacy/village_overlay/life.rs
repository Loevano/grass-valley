//! R18 ambient owner over an explicit prepared private world. R07 owns village
//! migrations and ordered active/camera overlays (`ChunkLifecycle::prepare`).
//! Restoring this owner never runs migrations, terrain reads, work or rewards.
use super::{
    BASE, EXTRA, LIFE, PLANTS,
    metadata::{array_mut, field_mut},
    scalar, set, swamp, version, write_node,
};
use crate::{
    BoundaryError,
    legacy::{ChunkKey, json::Node},
};
use lantern_core::{
    ids::WorldId,
    spatial::{self, Segment},
    units::{Position, Seconds},
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
mod routine;
use routine::Routine;
const PATH: &str = "$.village_life";
type Point = (i32, i32);
fn invalid(reason: &'static str) -> BoundaryError {
    BoundaryError::invalid(PATH, reason)
}
fn bytes(n: &Node) -> Vec<u8> {
    let mut out = Vec::new();
    write_node(n, &mut out);
    out
}
fn number(n: &Node) -> Result<f64, BoundaryError> {
    let Node::Scalar(raw) = n else {
        return Err(invalid("expected finite number"));
    };
    raw.get()
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| invalid("expected finite number"))
}
fn coord(n: &Node) -> Result<i32, BoundaryError> {
    let n = number(n)?;
    if n.fract() != 0. || n < i32::MIN as f64 || n > i32::MAX as f64 {
        return Err(invalid("expected integral coordinate"));
    }
    Ok(n as i32)
}
fn point(n: &Node) -> Result<Point, BoundaryError> {
    let a = n.array(PATH)?;
    if a.len() != 2 {
        return Err(invalid("expected coordinate pair"));
    }
    Ok((coord(&a[0])?, coord(&a[1])?))
}
fn xy(n: &Node) -> Result<(f64, f64), BoundaryError> {
    Ok((
        number(n.required("x", PATH)?)?,
        number(n.required("y", PATH)?)?,
    ))
}
fn anchor(n: &Node) -> Result<Point, BoundaryError> {
    Ok((
        coord(n.required("x", PATH)?)?,
        coord(n.required("y", PATH)?)?,
    ))
}
fn text(n: &Node, k: &str) -> Result<String, BoundaryError> {
    n.required(k, PATH)?.string(PATH)
}
fn opt_text(n: &Node, k: &str) -> Result<Option<String>, BoundaryError> {
    n.optional(k, PATH)?
        .filter(|v| !v.is_null())
        .map(|v| v.string(PATH))
        .transpose()
}
fn pos(p: (f64, f64)) -> Result<Position, BoundaryError> {
    Position::new(p.0, p.1).map_err(|_| invalid("invalid position"))
}
fn segment_open(a: (f64, f64), b: (f64, f64), fences: &[Segment]) -> bool {
    !spatial::crosses(pos(a).unwrap(), pos(b).unwrap(), fences)
}
fn grid(p: Point) -> (f64, f64) {
    (f64::from(p.0), f64::from(p.1))
}

/// Already-active records only. Supplying a record never loads/generates or
/// catches up a sleeping chunk. Camera records are deliberately absent here.
pub struct ActiveVillageChunk<'a> {
    pub world: &'a WorldId,
    pub key: ChunkKey,
    pub private_record: &'a [u8],
}
#[derive(Clone)]
struct Village {
    name: String,
    center: Point,
    layout: u8,
    life: bool,
    swamp: u8,
}
impl Village {
    fn read(n: &Node) -> Result<Self, BoundaryError> {
        Ok(Self {
            name: text(n, "name")?,
            center: anchor(n)?,
            layout: version(n, "layout_version", 2, PATH)?.unwrap_or(0) as u8,
            life: version(n, "life_version", 1, PATH)? == Some(1),
            swamp: version(n, "swamp_version", 2, PATH)?.unwrap_or(0) as u8,
        })
    }
    fn expanded(&self) -> bool {
        self.layout == 2 && matches!(self.swamp, 1 | 2)
    }
    fn organic(&self) -> bool {
        self.expanded() && self.swamp == 2
    }
    fn radius(&self) -> i32 {
        if self.organic() {
            31
        } else if self.expanded() {
            19
        } else if self.layout == 2 {
            9
        } else {
            5
        }
    }
}
/// Ordered, private local routines and saved visit cooldowns. Shared NPC anchor
/// identity is name/x/y, independent of the continuously moving routine position.
/// The payload is never a public wire snapshot or an authoritative Town DTO.
/// ```compile_fail
/// use lantern_host::legacy::VillageLife;
/// fn leak(v: &VillageLife) { serde_json::to_string(v).unwrap(); }
/// ```
#[derive(Clone)]
pub struct VillageLife {
    owner: WorldId,
    world: Node,
}
impl VillageLife {
    /// Accept prepared current/legacy village facts without advancing them. Use
    /// R07 metadata/lifecycle preparation first when migrations are requested.
    pub fn restore(owner: WorldId, source: &[u8]) -> Result<Self, BoundaryError> {
        let world = Node::parse(source, PATH)?;
        validate(&world)?;
        Ok(Self { owner, world })
    }
    fn authorize(&self, owner: &WorldId) -> Result<(), BoundaryError> {
        if owner != &self.owner {
            Err(invalid("foreign world owner"))
        } else {
            Ok(())
        }
    }
    pub fn private_world_bytes(&self) -> Vec<u8> {
        bytes(&self.world)
    }
    pub fn encode(&self) -> Vec<u8> {
        bytes(&Node::Object(vec![
            ("version".into(), scalar(1)),
            ("world_id".into(), scalar(self.owner.as_str())),
            ("payload".into(), self.world.clone()),
        ]))
    }
    pub fn decode(owner: &WorldId, source: &[u8]) -> Result<Self, BoundaryError> {
        let saved = Node::parse(source, PATH)?;
        if saved.object(PATH)?.len() != 3
            || saved.required("version", PATH)?.unsigned(PATH)? != 1
            || text(&saved, "world_id")? != owner.as_str()
        {
            return Err(invalid("unsupported version or foreign world owner"));
        }
        let world = saved.required("payload", PATH)?.clone();
        validate(&world)?;
        Ok(Self {
            owner: owner.clone(),
            world,
        })
    }
    pub fn position(&self, owner: &WorldId, index: usize) -> Result<Position, BoundaryError> {
        self.authorize(owner)?;
        let n = npc(&self.world, index)?;
        let current = n.optional("routine", PATH)?.unwrap_or(n);
        pos(xy(current)?)
    }
    pub fn local_key(&self, owner: &WorldId, index: usize) -> Result<String, BoundaryError> {
        self.authorize(owner)?;
        let n = npc(&self.world, index)?;
        anchor(n)?;
        let key_number = |field| -> Result<String, BoundaryError> {
            let node = n.required(field, PATH)?;
            let Node::Scalar(raw) = node else {
                return Err(invalid("invalid anchor"));
            };
            Ok(if raw.get().contains(['.', 'e', 'E']) {
                format!("{:?}", number(node)?)
            } else {
                coord(node)?.to_string()
            })
        };
        Ok(format!(
            "{}:{}:{}",
            text(n, "name")?,
            key_number("x")?,
            key_number("y")?
        ))
    }
    /// Host fast-loop time, separate from economic restocks and adventures.
    /// All players in all towns pause locals at <=1.8 tiles. No online-presence
    /// filter or town ownership shortcut is added to Python's visitor semantics.
    pub fn advance(
        &mut self,
        owner: &WorldId,
        seconds: Seconds,
        active: &[ActiveVillageChunk<'_>],
    ) -> Result<(), BoundaryError> {
        self.authorize(owner)?;
        if active.iter().any(|c| c.world != owner) {
            return Err(invalid("foreign active world owner"));
        }
        if seconds.get() == 0. {
            return Ok(());
        }
        let mut next = self.world.clone();
        let villages = villages(&next)?;
        if villages.is_empty() {
            return Ok(());
        }
        let scenes = Scene::read(&next, active)?;
        let lp = field_mut(&mut next, "shared_map", PATH)?;
        let npcs = field_mut(lp, "npcs", PATH)?;
        for v in villages {
            let blocked = scenes.blocked(&v);
            let mut index = 0;
            for n in array_mut(npcs) {
                if opt_text(n, "village")?.as_deref() != Some(&v.name) {
                    continue;
                }
                routine::advance(
                    n,
                    &v,
                    index,
                    seconds.get(),
                    &blocked,
                    &scenes.fences,
                    &scenes.players,
                )?;
                index += 1;
            }
        }
        validate(&next)?;
        self.world = next;
        Ok(())
    }
    /// Call only on the three-second economic boundary (or its explicit legacy
    /// test interval). No migration, reload or ambient movement advances it.
    pub fn advance_restocks(
        &mut self,
        owner: &WorldId,
        seconds: Seconds,
    ) -> Result<(), BoundaryError> {
        self.authorize(owner)?;
        let mut next = self.world.clone();
        let towns = field_mut(&mut next, "towns", PATH)?;
        let Node::Object(towns) = towns else {
            unreachable!()
        };
        for (_, t) in towns {
            if let Some(visits) = t.optional("local_visits", PATH)? {
                visits.object(PATH)?;
                let Node::Object(visits) = field_mut(t, "local_visits", PATH)? else {
                    unreachable!()
                };
                for (_, p) in visits {
                    if let Some(n) = p.optional("trade_remaining", PATH)? {
                        let n = n.nonnegative_number(PATH)?;
                        if n != 0. {
                            set(p, "trade_remaining", scalar((n - seconds.get()).max(0.)));
                        }
                    }
                }
            }
        }
        self.world = next;
        Ok(())
    }
    /// Same deterministic four-neighbor BFS used by ambient advance. The start
    /// is omitted and the goal included; empty also means no safe route.
    pub fn route(
        &self,
        owner: &WorldId,
        village: usize,
        start: [i32; 2],
        goal: [i32; 2],
        active: &[ActiveVillageChunk<'_>],
    ) -> Result<Vec<[i32; 2]>, BoundaryError> {
        self.authorize(owner)?;
        if active.iter().any(|c| c.world != owner) {
            return Err(invalid("foreign active world owner"));
        }
        let vs = villages(&self.world)?;
        let v = vs.get(village).ok_or_else(|| invalid("missing village"))?;
        let scene = Scene::read(&self.world, active)?;
        Ok(route(
            (start[0], start[1]),
            (goal[0], goal[1]),
            &scene.blocked(v),
            v,
            &scene.fences,
            true,
        )
        .into_iter()
        .map(|(x, y)| [x, y])
        .collect())
    }
}
fn npc(world: &Node, index: usize) -> Result<&Node, BoundaryError> {
    world
        .required("shared_map", PATH)?
        .required("npcs", PATH)?
        .array(PATH)?
        .get(index)
        .ok_or_else(|| invalid("missing local"))
}
fn villages(world: &Node) -> Result<Vec<Village>, BoundaryError> {
    world
        .optional("shared_map", PATH)?
        .and_then(|n| n.optional("villages", PATH).transpose())
        .transpose()?
        .map(|n| n.array(PATH)?.iter().map(Village::read).collect())
        .unwrap_or(Ok(vec![]))
}
fn validate(world: &Node) -> Result<(), BoundaryError> {
    if world.optional("_chunks", PATH)?.is_some() {
        return Err(invalid("runtime manager unsupported"));
    }
    for (_, t) in world.required("towns", PATH)?.object(PATH)? {
        if let Some(p) = t.optional("player", PATH)? {
            pos(xy(p)?)?;
        }
        if let Some(visits) = t.optional("local_visits", PATH)? {
            for (_, p) in visits.object(PATH)? {
                p.object(PATH)?;
                if let Some(n) = p.optional("trade_remaining", PATH)? {
                    n.nonnegative_number(PATH)?;
                }
            }
        }
    }
    let vs = villages(world)?;
    if let Some(lp) = world.optional("shared_map", PATH)? {
        let size = lp
            .optional("map_size", PATH)?
            .map(|n| n.unsigned(PATH))
            .transpose()?
            .unwrap_or(120);
        if size == 0 || size > 1920 {
            return Err(invalid("unsupported map bounds"));
        }
        for v in &vs {
            if v.center.0 < 0
                || v.center.1 < 0
                || v.center.0 as u64 >= size
                || v.center.1 as u64 >= size
            {
                return Err(invalid("village outside map"));
            }
        }
        if let Some(npcs) = lp.optional("npcs", PATH)? {
            for n in npcs.array(PATH)? {
                text(n, "name")?;
                anchor(n)?;
                if let Some(r) = n.optional("routine", PATH)? {
                    Routine::read(r)?;
                }
                if let Some(name) = opt_text(n, "village")?
                    && let Some(v) = vs.iter().find(|p| p.name == name)
                {
                    text(n, "role")?;
                    if let Some(stop) = n.optional("ambient_stop", PATH)? {
                        let (x, y) = point(stop)?;
                        if v.center.0.checked_add(x).is_none()
                            || v.center.1.checked_add(y).is_none()
                        {
                            return Err(invalid("ambient stop coordinate overflow"));
                        }
                    }
                }
            }
        } else if !vs.is_empty() {
            return Err(invalid("missing NPC roster"));
        }
    }
    if !vs.is_empty() {
        Scene::read(world, &[])?;
    }
    Ok(())
}
struct Scene {
    players: Vec<(f64, f64)>,
    buildings: BTreeSet<Point>,
    standalone: BTreeSet<Point>,
    fences: Vec<Segment>,
    active: BTreeMap<(i32, i32), BTreeSet<Point>>,
}
impl Scene {
    fn read(world: &Node, active: &[ActiveVillageChunk<'_>]) -> Result<Self, BoundaryError> {
        let mut budget = 1_000_000i64;
        let mut s = Self {
            players: vec![],
            buildings: BTreeSet::new(),
            standalone: BTreeSet::new(),
            fences: vec![],
            active: BTreeMap::new(),
        };
        let lp = world.required("shared_map", PATH)?;
        let size = lp
            .optional("map_size", PATH)?
            .map(|n| n.unsigned(PATH))
            .transpose()?
            .unwrap_or(120) as u32;
        for (_, t) in world.required("towns", PATH)?.object(PATH)? {
            if let Some(p) = t.optional("player", PATH)? {
                s.players.push(xy(p)?);
            }
            if let Some(bs) = t.optional("buildings", PATH)? {
                for b in bs.array(PATH)? {
                    let p = anchor(b)?;
                    let (w, h) = if let Some(fp) = b.optional("footprint", PATH)? {
                        point(fp)?
                    } else {
                        (1, 1)
                    };
                    budget -= i64::from(w.max(0)) * i64::from(h.max(0));
                    if budget < 0 {
                        return Err(invalid("footprint budget"));
                    }
                    for dx in 0..w {
                        for dy in 0..h {
                            s.buildings.insert((
                                p.0.checked_add(dx)
                                    .ok_or_else(|| invalid("footprint overflow"))?,
                                p.1.checked_add(dy)
                                    .ok_or_else(|| invalid("footprint overflow"))?,
                            ));
                        }
                    }
                    let construction = b.optional("construction", PATH)?;
                    let active = match construction {
                        None => true,
                        Some(c) => {
                            c.object(PATH)?.is_empty() || c.optional("upgrade", PATH)?.is_some()
                        }
                    };
                    if text(b, "kind")? == "barn" && active {
                        let orientation = b
                            .optional("fence_orientation", PATH)?
                            .map(|n| n.tile(PATH))
                            .transpose()?
                            .unwrap_or(0);
                        s.fences.extend(
                            spatial::pen(
                                lantern_core::units::Tile { x: p.0, y: p.1 },
                                size,
                                orientation,
                            )
                            .map_err(|_| invalid("invalid pen"))?
                            .segments,
                        );
                    }
                }
            }
        }
        if let Some(ns) = lp.optional("npcs", PATH)? {
            for n in ns.array(PATH)? {
                if opt_text(n, "village")?.is_none_or(|v| v.is_empty()) {
                    s.standalone.insert(anchor(n)?);
                }
            }
        }
        for r in active {
            let key = (r.key.x as i32, r.key.y as i32);
            if r.key.x >= size.div_ceil(40)
                || r.key.y >= size.div_ceil(40)
                || s.active.contains_key(&key)
            {
                return Err(invalid("duplicate or foreign active chunk"));
            }
            let record = Node::parse(r.private_record, PATH)?;
            let mut points = BTreeSet::new();
            for n in record.required("map_resources", PATH)?.array(PATH)? {
                let p = anchor(n)?;
                if p.0.div_euclid(40) != key.0 || p.1.div_euclid(40) != key.1 {
                    return Err(invalid("foreign chunk resource"));
                }
                points.insert(p);
            }
            s.active.insert(key, points);
        }
        Ok(s)
    }
    fn blocked(&self, v: &Village) -> BTreeSet<Point> {
        let (vx, vy) = v.center;
        let mut result: BTreeSet<_> = BASE
            .iter()
            .chain(if v.layout == 2 {
                EXTRA.iter()
            } else {
                [].iter()
            })
            .map(|r| (vx + r.1, vy + r.2))
            .collect();
        if v.life {
            result.extend(LIFE.iter().map(|r| (vx + r.1, vy + r.2)));
            result.extend(PLANTS.iter().map(|r| (vx + r.1, vy + r.2)));
        }
        let radius = if v.organic() {
            31
        } else if v.expanded() {
            19
        } else {
            9
        };
        if v.expanded() {
            result.extend(
                swamp::ambient_obstacles(v.swamp)
                    .into_iter()
                    .map(|(x, y)| (vx + x, vy + y)),
            );
        }
        if v.organic() {
            result.extend(swamp::layout().water.iter().map(|&(x, y)| (vx + x, vy + y)));
        }
        for (&(cx, cy), resources) in &self.active {
            if cx < (vx - radius).div_euclid(40)
                || cx > (vx + radius).div_euclid(40)
                || cy < (vy - radius).div_euclid(40)
                || cy > (vy + radius).div_euclid(40)
            {
                continue;
            }
            result.retain(|&(x, y)| x.div_euclid(40) != cx || y.div_euclid(40) != cy);
            result.extend(resources);
        }
        result.extend(&self.buildings);
        result.extend(&self.standalone);
        result
    }
}
fn route(
    start: Point,
    goal: Point,
    blocked: &BTreeSet<Point>,
    v: &Village,
    fences: &[Segment],
    lanes: bool,
) -> Vec<Point> {
    let data = if v.organic() {
        Some(swamp::layout())
    } else {
        None
    };
    let mut queue = VecDeque::from([start]);
    let mut parents = BTreeMap::from([(start, None)]);
    while let Some((x, y)) = queue.pop_front() {
        if (x, y) == goal {
            let mut p = goal;
            let mut path = vec![];
            while let Some(previous) = parents[&p] {
                path.push(p);
                p = previous;
            }
            path.reverse();
            return path;
        }
        for (dx, dy) in [(1, 0), (0, 1), (-1, 0), (0, -1)] {
            let (nx, ny) = (i64::from(x) + dx, i64::from(y) + dy);
            let (rx, ry) = (nx - i64::from(v.center.0), ny - i64::from(v.center.1));
            if rx.abs().max(ry.abs()) > i64::from(v.radius()) {
                continue;
            }
            let next = (nx as i32, ny as i32);
            let relative = (rx as i32, ry as i32);
            if parents.contains_key(&next)
                || blocked.contains(&next)
                || !segment_open(grid((x, y)), grid(next), fences)
                || data.is_some_and(|d| {
                    if lanes {
                        !d.roads.iter().any(|r| r.0 == relative)
                    } else {
                        !d.footprint.contains(&relative) || d.water.contains(&relative)
                    }
                })
            {
                continue;
            }
            parents.insert(next, Some((x, y)));
            queue.push_back(next);
        }
    }
    if data.is_some() && lanes {
        route(start, goal, blocked, v, fences, false)
    } else {
        vec![]
    }
}
