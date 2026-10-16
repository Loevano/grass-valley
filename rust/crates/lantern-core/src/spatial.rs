//! Pure spatial geometry. Indexes are derived from host-owned records; actor
//! exceptions never edit the shared obstacle set. No route search or movement.
use crate::units::{Position, Tile};
use alloc::{collections::BTreeSet, vec::Vec};

pub const CARDINAL: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];
pub const DIAGONAL: [(i32, i32); 4] = [(1, 1), (-1, 1), (-1, -1), (1, -1)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryError {
    InvalidDimensions,
    CoordinateOverflow,
    InvalidMapSize,
}

/// Supported positive saved dimensions are already oriented. Nonpositive saved
/// dimensions explicitly reject, rather than reproducing Python empty ranges.
/// Missing dimensions mean 1×1,
/// including halls; catalog dimensions apply only to placement/relocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Footprint {
    origin: Tile,
    width: u32,
    depth: u32,
}
impl Footprint {
    pub fn saved(origin: Tile, dimensions: Option<(u32, u32)>) -> Result<Self, GeometryError> {
        let (width, depth) = dimensions.unwrap_or((1, 1));
        if width == 0 || depth == 0 {
            return Err(GeometryError::InvalidDimensions);
        }
        // Reserve one cardinal halo for perimeter tiles as well.
        if origin.x == i32::MIN
            || origin.y == i32::MIN
            || i64::from(origin.x) + i64::from(width) > i64::from(i32::MAX)
            || i64::from(origin.y) + i64::from(depth) > i64::from(i32::MAX)
        {
            return Err(GeometryError::CoordinateOverflow);
        }
        Ok(Self {
            origin,
            width,
            depth,
        })
    }
    pub fn origin(self) -> Tile {
        self.origin
    }
    pub fn dimensions(self) -> (u32, u32) {
        (self.width, self.depth)
    }
    pub fn tiles(self) -> impl Iterator<Item = Tile> {
        (0..self.width).flat_map(move |dx| {
            (0..self.depth).map(move |dy| Tile {
                x: (i64::from(self.origin.x) + i64::from(dx)) as i32,
                y: (i64::from(self.origin.y) + i64::from(dy)) as i32,
            })
        })
    }
    pub fn perimeter(self) -> BTreeSet<Tile> {
        let x = self.origin.x;
        let y = self.origin.y;
        let right = (i64::from(x) + i64::from(self.width)) as i32;
        let bottom = (i64::from(y) + i64::from(self.depth)) as i32;
        (x..right)
            .flat_map(|x| [Tile { x, y: y - 1 }, Tile { x, y: bottom }])
            .chain((y..bottom).flat_map(|y| [Tile { x: x - 1, y }, Tile { x: right, y }]))
            .collect()
    }
}

pub fn placement_dimensions(kind: &str, orientation: i32) -> (u32, u32) {
    let (width, depth) = match kind {
        "hall" => (3, 3),
        "sawpit" => (3, 2),
        "mill" | "workshop" | "butcher" | "bakery" | "smokehouse" | "tannery" => (2, 2),
        _ => (1, 1),
    };
    if orientation.rem_euclid(2) == 1 {
        (depth, width)
    } else {
        (width, depth)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Construction {
    Complete,
    Site,
    EmptySite,
    Upgrade,
}
/// Kind names can come from private imported extensions; diagnostic formatting
/// must not reveal them implicitly.
/// ```compile_fail
/// use lantern_core::spatial::Structure;
/// fn leak(value: &Structure) { std::format!("{value:?}"); }
/// ```
#[derive(Clone)]
pub struct Structure {
    pub footprint: Footprint,
    pub kind: alloc::string::String,
    pub construction: Construction,
    pub fence_orientation: i32,
}
/// Arbitrary imported kind strings stay outside diagnostic formatting.
/// ```compile_fail
/// use lantern_core::spatial::Resource;
/// fn leak(value: &Resource) { std::format!("{value:?}"); }
/// ```
#[derive(Clone)]
pub struct Resource {
    pub footprint: Footprint,
    pub kind: alloc::string::String,
}
#[derive(Clone, Copy, Debug)]
pub struct Road {
    pub tile: Tile,
    pub construction: Construction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LandAccess {
    Ordinary,
    Farmer,
    Construction,
}
/// Explicit task-derived exceptions: valid piers/decks, boarding hut/berth,
/// and permitted sailing water are supplied by the future fishing planner.
/// They remove occupied tiles, just as Python's per-worker set subtraction does.
pub struct AccessProfile<'a> {
    pub land: LandAccess,
    pub exemptions: &'a BTreeSet<Tile>,
    /// Some(set) restricts travel to that set (e.g. permitted sailing water).
    pub only: Option<&'a BTreeSet<Tile>>,
}

/// Rebuildable, read-only geometry; moving villagers are deliberately absent.
/// This is an owned derived view, not a second mutable authoritative world.
pub struct SpatialIndex {
    blocked: BTreeSet<Tile>,
    completed_acres: BTreeSet<Tile>,
    roads: BTreeSet<Tile>,
    fences: Vec<Segment>,
}
impl SpatialIndex {
    pub fn derive(
        structures: &[Structure],
        resources: &[Resource],
        npcs: &[Footprint],
        roads: &[Road],
        size: u32,
    ) -> Result<Self, GeometryError> {
        if size == 0 || size > i32::MAX as u32 {
            return Err(GeometryError::InvalidMapSize);
        }
        let mut index = Self {
            blocked: BTreeSet::new(),
            completed_acres: BTreeSet::new(),
            roads: BTreeSet::new(),
            fences: Vec::new(),
        };
        for b in structures {
            index.blocked.extend(b.footprint.tiles());
            if b.kind == "acre" && b.construction == Construction::Complete {
                index.completed_acres.insert(b.footprint.origin);
            }
            if b.kind == "barn" && b.construction != Construction::Site {
                index
                    .fences
                    .extend(pen(b.footprint.origin, size, b.fence_orientation)?.segments);
            }
        }
        for r in resources {
            index.blocked.extend(r.footprint.tiles());
        }
        for n in npcs {
            index.blocked.extend(n.tiles());
        }
        for r in roads {
            if r.construction == Construction::Complete {
                index.roads.insert(r.tile);
            }
        }
        Ok(index)
    }
    pub fn blocked(&self) -> &BTreeSet<Tile> {
        &self.blocked
    }
    pub fn fences(&self) -> &[Segment] {
        &self.fences
    }
    pub fn roads(&self) -> &BTreeSet<Tile> {
        &self.roads
    }
    pub fn occupied_for(&self, tile: Tile, profile: &AccessProfile<'_>) -> bool {
        if profile.only.is_some_and(|allowed| !allowed.contains(&tile)) {
            return true;
        }
        let acres = match profile.land {
            LandAccess::Ordinary => None,
            LandAccess::Farmer | LandAccess::Construction => Some(&self.completed_acres),
        };
        self.blocked.contains(&tile)
            && !profile.exemptions.contains(&tile)
            && !acres.is_some_and(|a| a.contains(&tile))
    }
    /// Preserve eight-direction metric and no diagonal corner cutting. The host
    /// supplies terrain/bounds access independently from occupied-tile access.
    pub fn edge_open(
        &self,
        from: Tile,
        to: Tile,
        diagonal: bool,
        mut open: impl FnMut(Tile) -> bool,
    ) -> bool {
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        if dx.abs() > 1 || dy.abs() > 1 || (dx == 0 && dy == 0) {
            return false;
        }
        let slant = dx != 0 && dy != 0;
        (!slant || diagonal)
            && !crosses(tile_position(from), tile_position(to), &self.fences)
            && (!slant || (open(Tile { x: to.x, y: from.y }) && open(Tile { x: from.x, y: to.y })))
            && open(to)
    }
}
fn tile_position(t: Tile) -> Position {
    Position::new(f64::from(t.x), f64::from(t.y)).unwrap()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadMode {
    Smooth,
    Departure,
    Arrival,
}
pub fn edge_cost(from: Tile, to: Tile, roads: &BTreeSet<Tile>, mode: RoadMode) -> f64 {
    let departure = if roads.contains(&from) { 2.0 } else { 3.0 };
    let arrival = if roads.contains(&to) { 2.0 } else { 3.0 };
    let cost = match mode {
        RoadMode::Smooth => departure + arrival,
        RoadMode::Departure => 2.0 * departure,
        RoadMode::Arrival => 2.0 * arrival,
    };
    if from.x != to.x && from.y != to.y {
        cost * core::f64::consts::SQRT_2
    } else {
        cost
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub a: Position,
    pub b: Position,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Pen {
    pub orientation: usize,
    pub gate: (Position, Position),
    pub segments: [Segment; 5],
}
fn pos(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn segment(x1: f64, y1: f64, x2: f64, y2: f64) -> Segment {
    Segment {
        a: pos(x1, y1),
        b: pos(x2, y2),
    }
}
pub fn pen(origin: Tile, size: u32, orientation: i32) -> Result<Pen, GeometryError> {
    if size == 0 || size > i32::MAX as u32 {
        return Err(GeometryError::InvalidMapSize);
    }
    let (x, y, size) = (f64::from(origin.x), f64::from(origin.y), f64::from(size));
    let (left, right, back, front) = (
        (x - 3.5).max(-0.5),
        (x + 3.5).min(size - 0.5),
        (y - 3.5).max(-0.5),
        (y + 3.5).min(size - 0.5),
    );
    let sides = [
        segment(right, back, right, front),
        segment(right, front, left, front),
        segment(left, front, left, back),
        segment(left, back, right, back),
    ];
    let available = [
        right < size - 0.5,
        front < size - 0.5,
        left > -0.5,
        back > -0.5,
    ];
    let orientation = if (0..4).contains(&orientation) && available[orientation as usize] {
        orientation as usize
    } else {
        available.iter().position(|v| *v).unwrap_or(0)
    };
    let (nx, ny) = CARDINAL[orientation];
    let (nx, ny) = (f64::from(nx), f64::from(ny));
    let (tx, ty) = (-ny, nx);
    let edge = sides[orientation];
    let project = |p: Position| (p.x.get() - x) * tx + (p.y.get() - y) * ty;
    let (u, v) = (project(edge.a), project(edge.b));
    let (lo, hi) = (u.min(v), u.max(v));
    let start = (0.5_f64.min(hi - 2.0)).max(lo + 2.0);
    let distance = (edge.a.x.get() - x) * nx + (edge.a.y.get() - y) * ny;
    let a = pos(
        x + distance * nx + start * tx,
        y + distance * ny + start * ty,
    );
    let b = pos(a.x.get() + 2.0 * tx, a.y.get() + 2.0 * ty);
    let mut segments = [sides[0]; 5];
    let mut next = 0;
    for (i, side) in sides.into_iter().enumerate() {
        if i != orientation {
            segments[next] = side;
            next += 1;
        }
    }
    segments[3] = Segment { a: edge.a, b: a };
    segments[4] = Segment { a: b, b: edge.b };
    Ok(Pen {
        orientation,
        gate: (a, b),
        segments,
    })
}
pub fn fence_blocked(point: Position, segments: &[Segment], radius: f64) -> bool {
    let (x, y) = (point.x.get(), point.y.get());
    segments.iter().any(|s| {
        let (x1, y1, x2, y2) = (s.a.x.get(), s.a.y.get(), s.b.x.get(), s.b.y.get());
        if x1 == x2 {
            (x - x1).abs() < radius && y1.min(y2) - radius < y && y < y1.max(y2) + radius
        } else {
            (y - y1).abs() < radius && x1.min(x2) - radius < x && x < x1.max(x2) + radius
        }
    })
}
pub fn crosses(start: Position, end: Position, segments: &[Segment]) -> bool {
    let (ax, ay, bx, by) = (start.x.get(), start.y.get(), end.x.get(), end.y.get());
    segments.iter().any(|s| {
        let (x1, y1, x2, y2) = (s.a.x.get(), s.a.y.get(), s.b.x.get(), s.b.y.get());
        if x1 == x2 && (bx - ax).abs() > 1e-9 {
            let t = (x1 - ax) / (bx - ax);
            (0.0..=1.0).contains(&t)
                && y1.min(y2) - 1e-9 <= ay + t * (by - ay)
                && ay + t * (by - ay) <= y1.max(y2) + 1e-9
        } else if y1 == y2 && (by - ay).abs() > 1e-9 {
            let t = (y1 - ay) / (by - ay);
            (0.0..=1.0).contains(&t)
                && x1.min(x2) - 1e-9 <= ax + t * (bx - ax)
                && ax + t * (bx - ax) <= x1.max(x2) + 1e-9
        } else {
            false
        }
    })
}
/// Construction protects every barn's seven-tile range, including unfinished
/// barns. Roads/resources are not inputs: Python protects against buildings.
pub fn violates_pen(candidate: Footprint, candidate_is_barn: bool, others: &[Structure]) -> bool {
    let near = |a: Tile, b: Tile| {
        (i64::from(a.x) - i64::from(b.x))
            .abs()
            .max((i64::from(a.y) - i64::from(b.y)).abs())
            <= 3
    };
    others.iter().any(|b| {
        (b.kind == "barn" && candidate.tiles().any(|t| near(t, b.footprint.origin)))
            || (candidate_is_barn && b.footprint.tiles().any(|t| near(t, candidate.origin)))
    })
}
