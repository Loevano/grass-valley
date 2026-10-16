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
#[derive(Clone)]
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
    heap: BinaryHeap<Reverse<(Cost, u128, Cost, Tile)>>,
    previous: BTreeMap<Tile, Option<Tile>>,
    costs: BTreeMap<Tile, Cost>,
    order: u128,
    phase: Phase,
    path: Vec<Tile>,
    result_cost: f64,
    outcome: Outcome,
    pub expansions: usize,
    pub work: usize,
}
impl Search {
    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }
    /// Consuming the job transfers its path without an unmetered clone in advance.
    pub fn into_outcome(self) -> Outcome {
        self.outcome
    }
    pub fn new(query: Rc<Query>) -> Self {
        let start = query.start;
        let outcome = if query.reached(start) {
            Outcome::Arrived
        } else if query.anchor.is_none() && query.goals.is_empty() {
            Outcome::Unreachable
        } else {
            Outcome::Pending
        };
        Self {
            heap: BinaryHeap::from([Reverse((query.heuristic(start), 0, Cost(0.0), start))]),
            previous: BTreeMap::from([(start, None)]),
            costs: BTreeMap::from([(start, Cost(0.0))]),
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
            self.heap = BinaryHeap::new();
            self.previous = BTreeMap::new();
            self.costs = BTreeMap::new();
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
                    if self.costs[&at] != cost {
                        continue;
                    }
                    if self.query.reached(at) {
                        self.result_cost = cost.0;
                        self.phase = Phase::Trace(at);
                        continue;
                    }
                    self.expansions = self.expansions.saturating_add(1);
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
                        if !self
                            .query
                            .index
                            .edge_open(at, next, self.query.diagonal, |t| self.query.open(t))
                        {
                            continue;
                        }
                        let candidate = Cost(
                            cost.0
                                + spatial::edge_cost(
                                    at,
                                    next,
                                    self.query.index.roads(),
                                    self.query.mode,
                                ),
                        );
                        if self.costs.get(&next).is_some_and(|old| candidate >= *old) {
                            continue;
                        }
                        self.costs.insert(next, candidate);
                        self.previous.insert(next, Some(at));
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
                    if let Some(parent) = self.previous[&at] {
                        self.path.push(at);
                        self.phase = Phase::Trace(parent)
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
        assert!(!search.previous.is_empty());
        search.cancel();
        assert_eq!(search.heap.capacity(), 0);
        assert!(search.previous.is_empty());
        assert!(search.costs.is_empty());
        assert_eq!(search.path.capacity(), 0);
        assert_eq!(search.advance(100), 0);
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
