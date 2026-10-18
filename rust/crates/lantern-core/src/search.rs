//! Actor-independent incremental routing over immutable, derived query data.
//!
//! A query freezes terrain, access and geometry. Its caller must check navigation
//! and request generations before accepting a result; this module owns no actor,
//! scheduler, cache, saved state, or authoritative snapshot.
use crate::{
    spatial::{
        self, AccessProfile, CARDINAL, DIAGONAL, Footprint, LandAccess, RoadMode, SpatialIndex,
    },
    units::{Position, Tile},
};
use alloc::{
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    rc::Rc,
    vec::Vec,
};
use core::cmp::{Ordering, Reverse};

#[derive(Clone)]
pub enum Target {
    Exact(Tile),
    /// Raw anchor adjacency is Manhattan distance one, with a final fence check.
    AnchorAdjacent(Tile),
    /// Explicit saved footprint: actor-filtered cardinal perimeter, including fences.
    Perimeter(Footprint),
    Multiple(BTreeSet<Tile>),
}
#[derive(Clone, PartialEq, Eq)]
pub struct Access {
    pub land: LandAccess,
    pub exemptions: BTreeSet<Tile>,
    pub only: Option<BTreeSet<Tile>>,
}
impl Default for Access {
    fn default() -> Self {
        Self {
            land: LandAccess::Ordinary,
            exemptions: BTreeSet::new(),
            only: None,
        }
    }
}
#[derive(Clone, Default)]
pub struct Terrain {
    /// None means absent ordinary terrain. Missing cells in Some(map) are
    /// unavailable cells; chunk adapters must use explicit 'v' for missing chunks.
    pub cells: Option<BTreeMap<Tile, char>>,
    pub bounds: Option<(i32, i32, i32, i32)>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    InvalidBounds,
}
/// Prepared once, shareable without a self-referential owner. Construction and
/// eventual destruction are outside advance's operation budget.
pub struct Query {
    index: Rc<SpatialIndex>,
    terrain: Terrain,
    access: Access,
    start: Tile,
    goals: BTreeSet<Tile>,
    anchor: Option<Tile>,
    goal_bounds: Option<(i32, i32, i32, i32)>,
    diagonal: bool,
    mode: RoadMode,
}
impl Query {
    pub fn new(
        index: Rc<SpatialIndex>,
        terrain: Terrain,
        access: Access,
        start: Tile,
        target: Target,
        diagonal: bool,
        mode: RoadMode,
    ) -> Result<Self, QueryError> {
        if terrain
            .bounds
            .is_some_and(|(x0, x1, y0, y1)| x0 > x1 || y0 > y1)
        {
            return Err(QueryError::InvalidBounds);
        }
        let mut query = Self {
            index,
            terrain,
            access,
            start,
            goals: BTreeSet::new(),
            anchor: None,
            goal_bounds: None,
            diagonal,
            mode,
        };
        query.goals = match target {
            Target::Exact(t) => BTreeSet::from([t]),
            Target::AnchorAdjacent(t) => {
                query.anchor = Some(t);
                BTreeSet::new()
            }
            Target::Multiple(goals) => goals,
            Target::Perimeter(fp) => fp
                .perimeter()
                .into_iter()
                .filter(|p| {
                    !query.occupied(*p)
                        && fp
                            .tiles()
                            .any(|q| manhattan(*p, q) == 1 && !query.fenced(*p, q))
                })
                .collect(),
        };
        if !query.goals.is_empty() {
            query.goal_bounds = Some((
                query.goals.iter().map(|p| p.x).min().unwrap(),
                query.goals.iter().map(|p| p.x).max().unwrap(),
                query.goals.iter().map(|p| p.y).min().unwrap(),
                query.goals.iter().map(|p| p.y).max().unwrap(),
            ));
        }
        Ok(query)
    }
    /// Conservative record count: shared indexes are charged in full per job.
    /// Query construction (including perimeter generation) precedes admission;
    /// callers must bound source preparation separately.
    pub fn retained_records(&self) -> usize {
        [
            self.index.retained_records(),
            self.terrain.cells.as_ref().map_or(0, BTreeMap::len),
            self.access.exemptions.len(),
            self.access.only.as_ref().map_or(0, BTreeSet::len),
            self.goals.len(),
        ]
        .into_iter()
        .fold(0usize, usize::saturating_add)
    }
    // Global algorithm inputs: changing any can alter every frontier estimate.
    fn compatible(&self, other: &Self) -> bool {
        self.start == other.start
            && self.goals == other.goals
            && self.anchor == other.anchor
            && self.goal_bounds == other.goal_bounds
            && self.diagonal == other.diagonal
            && self.mode == other.mode
            && self.access == other.access
            && self.terrain.bounds == other.terrain.bounds
            && self.index.size() == other.index.size()
            && self.index.roads().is_empty() == other.index.roads().is_empty()
    }
    fn edge(&self, a: Tile, b: Tile) -> Option<f64> {
        self.index
            .edge_open(a, b, self.diagonal, |t| self.open(t))
            .then(|| spatial::edge_cost(a, b, self.index.roads(), self.mode))
    }
    /// Safety of an already accepted remaining route, deliberately independent
    /// of road costs and global heuristic policy. The host owns progress/origin.
    pub fn route_safe(&self, origin: Tile, remaining: &[Tile]) -> bool {
        if !self.open(origin) {
            return false;
        }
        let mut at = origin;
        for &next in remaining {
            if self.edge(at, next).is_none() {
                return false;
            }
            at = next;
        }
        self.reached(at)
    }
    fn occupied(&self, t: Tile) -> bool {
        self.index.occupied_for(
            t,
            &AccessProfile {
                land: self.access.land,
                exemptions: &self.access.exemptions,
                only: self.access.only.as_ref(),
            },
        )
    }
    fn open(&self, t: Tile) -> bool {
        spatial::search_terrain_open(
            t,
            self.index.size(),
            self.terrain.bounds,
            self.terrain.cells.as_ref().map(|m| m.get(&t).copied()),
        ) && !self.occupied(t)
    }
    fn fenced(&self, a: Tile, b: Tile) -> bool {
        spatial::crosses(
            Position::new(f64::from(a.x), f64::from(a.y)).unwrap(),
            Position::new(f64::from(b.x), f64::from(b.y)).unwrap(),
            self.index.fences(),
        )
    }
    fn reached(&self, t: Tile) -> bool {
        self.anchor.map_or_else(
            || self.goals.contains(&t),
            |a| manhattan(t, a) == 1 && !self.fenced(t, a),
        )
    }
    fn heuristic(&self, t: Tile) -> Cost {
        if self.index.roads().is_empty() && !self.diagonal {
            return Cost(0.0);
        }
        let (dx, dy) = if let Some(a) = self.anchor {
            (
                (i64::from(t.x) - i64::from(a.x)).abs() as f64,
                (i64::from(t.y) - i64::from(a.y)).abs() as f64,
            )
        } else if let Some((x0, x1, y0, y1)) = self.goal_bounds {
            (
                (i64::from(x0) - i64::from(t.x))
                    .max(0)
                    .max(i64::from(t.x) - i64::from(x1)) as f64,
                (i64::from(y0) - i64::from(t.y))
                    .max(0)
                    .max(i64::from(t.y) - i64::from(y1)) as f64,
            )
        } else {
            return Cost(0.0);
        };
        let distance = if self.diagonal {
            dx.max(dy) + (core::f64::consts::SQRT_2 - 1.0) * dx.min(dy)
        } else {
            dx + dy
        };
        Cost(
            (distance - if self.anchor.is_some() { 1.0 } else { 0.0 }).max(0.0)
                * if self.index.roads().is_empty() {
                    6.0
                } else {
                    4.0
                },
        )
    }
}
fn manhattan(a: Tile, b: Tile) -> i64 {
    (i64::from(a.x) - i64::from(b.x)).abs() + (i64::from(a.y) - i64::from(b.y)).abs()
}
#[derive(Clone, Copy, Debug)]
struct Cost(f64);
impl PartialEq for Cost {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o).is_eq()
    }
}
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Cost {
    fn cmp(&self, o: &Self) -> Ordering {
        self.0.total_cmp(&o.0)
    }
}
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Arrived,
    Unreachable,
    Pending,
    Cancelled,
    Completed { path: Vec<Tile>, cost: f64 },
}
impl Outcome {
    pub fn cost(&self) -> Option<f64> {
        match self {
            Self::Arrived => Some(0.0),
            Self::Completed { cost, .. } => Some(*cost),
            _ => None,
        }
    }
}
// Small finite maps use bounded direct lookup; large/unusual domains retain sparse
// storage. This changes neither heap ordering nor the operation meter. Allocation
// and zero initialization happen at search construction, outside advance.
const DENSE_CELLS: usize = 4096;
fn dense_size(query: &Query) -> i32 {
    let size = query.index.size();
    if u64::from(size) * u64::from(size) <= DENSE_CELLS as u64 {
        size as i32
    } else {
        0
    }
}
fn offset(size: i32, t: Tile) -> Option<usize> {
    (t.x >= 0 && t.y >= 0 && t.x < size && t.y < size)
        .then(|| t.y as usize * size as usize + t.x as usize)
}
#[derive(Clone, Copy)]
struct Node {
    cost: Cost,
    previous: Tile,
}
struct Nodes {
    size: i32,
    dense: Vec<Node>,
    sparse: BTreeMap<Tile, Node>,
}
impl Nodes {
    fn new(query: &Query, dense: bool) -> Self {
        let size = if dense { dense_size(query) } else { 0 };
        Self {
            size,
            dense: alloc::vec![Node { cost: Cost(f64::NAN), previous: query.start }; (size as usize).pow(2)],
            sparse: BTreeMap::new(),
        }
    }
    fn get(&self, t: Tile) -> Option<&Node> {
        match offset(self.size, t) {
            Some(i) => self.dense.get(i).filter(|n| n.cost.0.is_finite()),
            None => self.sparse.get(&t),
        }
    }
    fn insert(&mut self, t: Tile, node: Node) {
        if let Some(i) = offset(self.size, t) {
            self.dense[i] = node;
        } else {
            self.sparse.insert(t, node);
        }
    }
}
/// Executed-prefix certificate. Flags record arrival checks and expansions,
/// including failed edges, with at most two new observations per operation.
#[derive(Clone)]
pub struct Dependencies {
    query: Rc<Query>,
    size: i32,
    dense: Vec<u8>,
    sparse: BTreeMap<Tile, u8>,
    records: usize,
}
impl Dependencies {
    fn new(query: Rc<Query>, dense: bool) -> Self {
        let size = if dense { dense_size(&query) } else { 0 };
        Self {
            query,
            size,
            dense: alloc::vec![0; (size as usize).pow(2)],
            sparse: BTreeMap::new(),
            records: 0,
        }
    }
    fn observe(&mut self, t: Tile, flag: u8) {
        let flags = if let Some(i) = offset(self.size, t) {
            &mut self.dense[i]
        } else {
            self.sparse.entry(t).or_default()
        };
        if *flags & flag == 0 {
            *flags |= flag;
            self.records += 1;
        }
    }
    fn observations(&self) -> impl Iterator<Item = (Tile, u8)> + '_ {
        self.dense
            .iter()
            .enumerate()
            .filter(|(_, flags)| **flags != 0)
            .map(|(i, &flags)| {
                (
                    Tile {
                        x: (i % self.size as usize) as i32,
                        y: (i / self.size as usize) as i32,
                    },
                    flags,
                )
            })
            .chain(self.sparse.iter().map(|(&t, &flags)| (t, flags)))
    }
    pub(crate) fn refreshed(&self, query: Rc<Query>) -> Self {
        let mut result = self.clone();
        result.query = query;
        result
    }
    pub(crate) fn replace_query(&mut self, query: Rc<Query>) {
        self.query = query;
    }
    pub fn validation_records(&self, query: &Query) -> usize {
        if core::ptr::eq(self.query.as_ref(), query) {
            0
        } else {
            self.records()
        }
    }
    pub fn records(&self) -> usize {
        self.records
    }
    /// Reserved observation units, including unused dense backing (two per tile).
    pub fn retained_records(&self) -> usize {
        self.dense.len() * 2
            + self
                .sparse
                .values()
                .map(|f| f.count_ones() as usize)
                .sum::<usize>()
    }
    pub fn query(&self) -> &Rc<Query> {
        &self.query
    }
    /// Full bounded semantic comparison, outside Search::advance's meter.
    /// Equal observations and global policy prove the executed prefix unchanged.
    pub fn valid_for(&self, next: &Query) -> bool {
        core::ptr::eq(self.query.as_ref(), next)
            || (self.query.compatible(next)
                && self.observations().all(|(at, flags)| {
                    (flags & 1 == 0 || self.query.reached(at) == next.reached(at))
                        && (flags & 2 == 0
                            || CARDINAL
                                .iter()
                                .chain(if self.query.diagonal {
                                    DIAGONAL.iter()
                                } else {
                                    [].iter()
                                })
                                .all(|&(dx, dy)| {
                                    let (Some(x), Some(y)) =
                                        (at.x.checked_add(dx), at.y.checked_add(dy))
                                    else {
                                        return true;
                                    };
                                    let neighbor = Tile { x, y };
                                    self.query.edge(at, neighbor) == next.edge(at, neighbor)
                                }))
                }))
    }
}
#[derive(Clone, Copy)]
enum Phase {
    Search,
    Trace(Tile),
    Reverse(usize),
}
/// One unit is a heap pop (stale included), predecessor visit, reverse swap or
/// terminal phase check. Each expansion examines at most eight edges. Tree/heap
/// operations and allocation are not wall-clock bounded. Zero quantum does nothing.
/// Costs stay finite: positive edges and a finite i32-sized map bound any simple
/// discovered path well below f64 overflow. Serial uses u128 (at most eight
/// insertions per simple-path relaxation over the finite map).
pub struct Search {
    query: Rc<Query>,
    dependencies: Dependencies,
    heap: BinaryHeap<Reverse<(Cost, u128, Cost, Tile)>>,
    nodes: Nodes,
    order: u128,
    phase: Phase,
    path: Vec<Tile>,
    result_cost: f64,
    outcome: Outcome,
    pub expansions: usize,
    pub work: usize,
}
impl Search {
    /// Backing units include eagerly allocated dense nodes (cost + predecessor),
    /// reserved observation flags, frontier/path capacities and query records.
    /// Allocator headers/capacity rounding are measured by release diagnostics.
    pub fn retained_records(&self) -> usize {
        self.query.retained_records()
            + self.dependencies.retained_records()
            + self.nodes.dense.len() * 2
            + self.nodes.sparse.len() * 2
            + self.heap.capacity()
            + self.path.capacity()
    }
    pub fn dependencies(&self) -> &Dependencies {
        &self.dependencies
    }
    /// Returns false without altering the search if any observed dependency changed.
    pub fn refresh(&mut self, query: Rc<Query>) -> bool {
        if !self.dependencies.valid_for(&query) {
            return false;
        }
        self.dependencies.query = query.clone();
        self.query = query;
        true
    }
    pub fn into_parts(self) -> (Outcome, Dependencies) {
        (self.outcome, self.dependencies)
    }
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
    /// Consuming the job transfers its path without an unmetered clone in advance.
    pub fn into_outcome(self) -> Outcome {
        self.outcome
    }
    pub fn new(query: Rc<Query>) -> Self {
        Self::with_storage(query, true)
    }
    /// Preserve configured small-budget envelopes: eagerly reserve dense backing
    /// only when both observation storage and the existing node work bound fit.
    pub(crate) fn bounded(query: Rc<Query>, work: usize, dependencies: usize) -> Self {
        let size = dense_size(&query);
        let cells = (size as usize).pow(2);
        let dense = cells > 0
            && offset(size, query.start).is_some()
            && cells.saturating_mul(2) <= dependencies
            && cells <= work.saturating_mul(8).saturating_add(1);
        Self::with_storage(query, dense)
    }
    fn with_storage(query: Rc<Query>, dense: bool) -> Self {
        let start = query.start;
        let outcome = if query.reached(start) {
            Outcome::Arrived
        } else if query.anchor.is_none() && query.goals.is_empty() {
            Outcome::Unreachable
        } else {
            Outcome::Pending
        };
        let mut dependencies = Dependencies::new(query.clone(), dense);
        dependencies.observe(start, 1);
        let mut nodes = Nodes::new(&query, dense);
        nodes.insert(
            start,
            Node {
                cost: Cost(0.0),
                previous: start,
            },
        );
        Self {
            dependencies,
            heap: BinaryHeap::from([Reverse((query.heuristic(start), 0, Cost(0.0), start))]),
            nodes,
            query,
            order: 0,
            phase: Phase::Search,
            path: Vec::new(),
            result_cost: 0.0,
            outcome,
            expansions: 0,
            work: 0,
        }
    }
    /// Immediately releases frontier/predecessors; destruction is unmetered and
    /// must be accounted for separately by a future service owner.
    pub fn cancel(&mut self) {
        if self.outcome == Outcome::Pending {
            self.outcome = Outcome::Cancelled;
            self.dependencies.dense = Vec::new();
            self.dependencies.sparse = BTreeMap::new();
            self.dependencies.records = 0;
            self.heap = BinaryHeap::new();
            self.nodes.dense = Vec::new();
            self.nodes.sparse = BTreeMap::new();
            self.path = Vec::new();
        }
    }
    pub fn advance(&mut self, quantum: usize) -> usize {
        let mut used = 0;
        for _ in 0..quantum {
            if self.outcome != Outcome::Pending {
                break;
            }
            used += 1;
            self.work = self.work.saturating_add(1);
            match self.phase {
                Phase::Search => {
                    let Some(Reverse((_, _, cost, at))) = self.heap.pop() else {
                        self.outcome = Outcome::Unreachable;
                        break;
                    };
                    if self.nodes.get(at).expect("queued node").cost != cost {
                        continue;
                    }
                    self.dependencies.observe(at, 1);
                    if self.query.reached(at) {
                        self.result_cost = cost.0;
                        self.phase = Phase::Trace(at);
                        continue;
                    }
                    self.expansions = self.expansions.saturating_add(1);
                    self.dependencies.observe(at, 2);
                    for &(dx, dy) in CARDINAL.iter().chain(if self.query.diagonal {
                        DIAGONAL.iter()
                    } else {
                        [].iter()
                    }) {
                        let (Some(x), Some(y)) = (at.x.checked_add(dx), at.y.checked_add(dy))
                        else {
                            continue;
                        };
                        let next = Tile { x, y };
                        let Some(edge_cost) = self.query.edge(at, next) else {
                            continue;
                        };
                        let candidate = Cost(cost.0 + edge_cost);
                        if self
                            .nodes
                            .get(next)
                            .is_some_and(|old| candidate >= old.cost)
                        {
                            continue;
                        }
                        self.nodes.insert(
                            next,
                            Node {
                                cost: candidate,
                                previous: at,
                            },
                        );
                        self.order += 1;
                        self.heap.push(Reverse((
                            Cost(candidate.0 + self.query.heuristic(next).0),
                            self.order,
                            candidate,
                            next,
                        )));
                    }
                }
                Phase::Trace(at) => {
                    if at != self.query.start {
                        self.path.push(at);
                        self.phase = Phase::Trace(self.nodes.get(at).expect("traced node").previous)
                    } else {
                        self.phase = Phase::Reverse(0)
                    }
                }
                Phase::Reverse(i) => {
                    if i < self.path.len() / 2 {
                        let j = self.path.len() - i - 1;
                        self.path.swap(i, j);
                        self.phase = Phase::Reverse(i + 1)
                    } else {
                        self.outcome = Outcome::Completed {
                            path: core::mem::take(&mut self.path),
                            cost: self.result_cost,
                        }
                    }
                }
            }
        }
        used
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn job() -> Search {
        let index = Rc::new(SpatialIndex::derive(&[], &[], &[], &[], 8).unwrap());
        Search::new(Rc::new(
            Query::new(
                index,
                Terrain::default(),
                Access::default(),
                Tile { x: 0, y: 0 },
                Target::Exact(Tile { x: 7, y: 7 }),
                true,
                RoadMode::Smooth,
            )
            .unwrap(),
        ))
    }
    #[test]
    fn stale_heap_pops_are_individually_metered_and_cancellation_releases_frontier() {
        let mut search = job();
        let start = search.query.start;
        // Force consecutive stale entries ahead of the real initial entry.
        for serial in 1..=4 {
            search
                .heap
                .push(Reverse((Cost(-1.0), serial, Cost(9.0), start)));
        }
        for _ in 0..4 {
            let before = search.heap.len();
            assert_eq!(search.advance(1), 1);
            assert_eq!(search.heap.len(), before - 1);
            assert_eq!(search.expansions, 0);
        }
        search.advance(3);
        assert!(!search.heap.is_empty());
        assert!(!search.nodes.dense.is_empty());
        search.cancel();
        assert_eq!(search.dependencies.records(), 0);
        assert_eq!(search.heap.capacity(), 0);
        assert!(search.nodes.dense.is_empty());
        assert!(search.nodes.sparse.is_empty());
        assert_eq!(search.path.capacity(), 0);
        assert_eq!(search.advance(100), 0);
    }
    #[test]
    fn dense_and_sparse_have_identical_paths_operations_and_dependency_proofs() {
        for diagonal in [false, true] {
            for road in [false, true] {
                let resources: Vec<_> = (0..15)
                    .map(|y| crate::spatial::Resource {
                        footprint: Footprint::saved(Tile { x: 8, y }, None).unwrap(),
                        kind: "stone".into(),
                    })
                    .collect();
                let roads = if road {
                    alloc::vec![crate::spatial::Road {
                        tile: Tile { x: 2, y: 2 },
                        construction: crate::spatial::Construction::Complete
                    }]
                } else {
                    Vec::new()
                };
                let make = |size| {
                    Rc::new(
                        Query::new(
                            Rc::new(
                                SpatialIndex::derive(&[], &resources, &[], &roads, size).unwrap(),
                            ),
                            Terrain {
                                bounds: Some((0, 15, 0, 15)),
                                ..Terrain::default()
                            },
                            Access::default(),
                            Tile { x: 0, y: 0 },
                            Target::Exact(Tile { x: 15, y: 0 }),
                            diagonal,
                            RoadMode::Smooth,
                        )
                        .unwrap(),
                    )
                };
                let query = make(16);
                let mut dense = Search::with_storage(query.clone(), true);
                let mut sparse = Search::with_storage(query.clone(), false);
                assert_eq!(dense.dependencies.retained_records(), 512);
                assert_eq!(sparse.dependencies.retained_records(), 1);
                while dense.outcome() == &Outcome::Pending {
                    assert_eq!(dense.advance(7), sparse.advance(7));
                    assert_eq!(dense.work, sparse.work);
                    assert_eq!(dense.expansions, sparse.expansions);
                    assert_eq!(dense.outcome(), sparse.outcome());
                    assert_eq!(dense.dependencies.records(), sparse.dependencies.records());
                    assert_eq!(
                        dense.dependencies.valid_for(&make(16)),
                        sparse.dependencies.valid_for(&make(16))
                    );
                }
                // Large maps allocate no dense backing; finite explicit bounds
                // keep this route identical and exercise production fallback.
                let mut large = Search::new(make(100000));
                assert!(large.nodes.dense.is_empty());
                assert!(large.dependencies.dense.is_empty());
                large.advance(usize::MAX);
                assert_eq!(dense.outcome(), large.outcome());
                assert_eq!(dense.work, large.work);
                dense.cancel();
                sparse.cancel();
            }
        }
    }
    #[test]
    fn raw_anchor_and_goal_box_heuristics_are_distinct_over_extreme_coordinates() {
        let search = job();
        let index = search.query.index.clone();
        let anchor = Tile { x: 3, y: 3 };
        let raw = Query::new(
            index.clone(),
            Terrain::default(),
            Access::default(),
            anchor,
            Target::AnchorAdjacent(anchor),
            true,
            RoadMode::Smooth,
        )
        .unwrap();
        let perimeter = Query::new(
            index,
            Terrain::default(),
            Access::default(),
            anchor,
            Target::Perimeter(Footprint::saved(anchor, None).unwrap()),
            true,
            RoadMode::Smooth,
        )
        .unwrap();
        assert!(
            (raw.heuristic(Tile { x: 0, y: 0 }).0 - (3.0 * core::f64::consts::SQRT_2 - 1.0) * 6.0)
                .abs()
                < 1e-12
        );
        assert!(
            (perimeter.heuristic(Tile { x: 0, y: 0 }).0 - 2.0 * core::f64::consts::SQRT_2 * 6.0)
                .abs()
                < 1e-12
        );
        assert!(
            raw.heuristic(Tile {
                x: i32::MIN,
                y: i32::MAX
            })
            .0
            .is_finite()
        );
    }
}
