//! R09 evaluation only. Never linked into the authoritative host or no_std core.
use lantern_core::{
    spatial::{
        self, AccessProfile, CARDINAL, Construction, DIAGONAL, Footprint, LandAccess, Road,
        RoadMode, SpatialIndex, Structure,
    },
    units::Tile,
};
use num_traits::Zero;
use pathfinding::prelude::{astar, dijkstra_reach};
use std::{
    cell::Cell,
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    ops::Add,
};

pub fn tile(x: i32, y: i32) -> Tile {
    Tile { x, y }
}

/// All costs are finite, nonnegative sums of the core's road/diagonal costs.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cost(pub f64);
impl PartialEq for Cost {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Cost {}
impl PartialOrd for Cost {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Cost {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
impl Add for Cost {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self(self.0 + other.0)
    }
}
impl Zero for Cost {
    fn zero() -> Self {
        Self(0.0)
    }
    fn is_zero(&self) -> bool {
        self.0 == 0.0
    }
}

#[derive(Clone)]
pub enum Target {
    Exact(Tile),
    Adjacent(Footprint),
    Multiple(BTreeSet<Tile>),
}
#[derive(Clone)]
pub struct Fixture {
    pub name: String,
    pub size: u32,
    pub start: Tile,
    pub target: Target,
    pub structures: Vec<Structure>,
    pub roads: Vec<Road>,
    pub unavailable: BTreeSet<Tile>,
    pub void: BTreeSet<Tile>,
    pub bounds: Option<(i32, i32, i32, i32)>,
    pub diagonal: bool,
    pub mode: RoadMode,
    pub land: LandAccess,
    pub exemptions: BTreeSet<Tile>,
}
impl Fixture {
    pub fn open(name: &str, size: u32, start: Tile, goal: Tile) -> Self {
        Self {
            name: name.into(),
            size,
            start,
            target: Target::Exact(goal),
            structures: vec![],
            roads: vec![],
            unavailable: BTreeSet::new(),
            void: BTreeSet::new(),
            bounds: None,
            diagonal: false,
            mode: RoadMode::Smooth,
            land: LandAccess::Ordinary,
            exemptions: BTreeSet::new(),
        }
    }
    pub fn block(&mut self, t: Tile, kind: &str) {
        self.structures.push(Structure {
            footprint: Footprint::saved(t, None).unwrap(),
            kind: kind.into(),
            construction: Construction::Complete,
            fence_orientation: 0,
        });
    }
}

/// Owns a private immutable derived graph. Fixture creation is outside measurements;
/// cold preparation includes spatial/terrain/goal conversion, warm reuses this view.
pub struct Graph {
    pub fixture: Fixture,
    pub index: SpatialIndex,
    pub goals: BTreeSet<Tile>,
    goal_bounds: Option<(i32, i32, i32, i32)>,
}
impl Graph {
    pub fn prepare(f: &Fixture) -> Self {
        let index = SpatialIndex::derive(&f.structures, &[], &[], &f.roads, f.size).unwrap();
        let goals = match &f.target {
            Target::Exact(t) => BTreeSet::from([*t]),
            Target::Multiple(ts) => ts.clone(),
            Target::Adjacent(fp) => fp
                .perimeter()
                .into_iter()
                .filter(|p| {
                    !index.occupied_for(
                        *p,
                        &AccessProfile {
                            land: f.land,
                            exemptions: &f.exemptions,
                            only: None,
                        },
                    ) && fp.tiles().any(|q| {
                        p.x.abs_diff(q.x) + p.y.abs_diff(q.y) == 1
                            && !spatial::crosses(
                                lantern_core::units::Position::new(p.x as f64, p.y as f64).unwrap(),
                                lantern_core::units::Position::new(q.x as f64, q.y as f64).unwrap(),
                                index.fences(),
                            )
                    })
                })
                .collect(),
        };
        let goal_bounds = (!goals.is_empty()).then(|| {
            (
                goals.iter().map(|p| p.x).min().unwrap(),
                goals.iter().map(|p| p.x).max().unwrap(),
                goals.iter().map(|p| p.y).min().unwrap(),
                goals.iter().map(|p| p.y).max().unwrap(),
            )
        });
        Self {
            fixture: f.clone(),
            index,
            goals,
            goal_bounds,
        }
    }
    pub fn open(&self, t: Tile) -> bool {
        let f = &self.fixture;
        // Explicit 'v' adapter for missing chunk terrain, as in R08.
        let terrain = if f.void.contains(&t) || f.unavailable.contains(&t) {
            Some(Some('v'))
        } else {
            Some(Some('g'))
        };
        spatial::search_terrain_open(t, f.size, f.bounds, terrain)
            && !self.index.occupied_for(
                t,
                &AccessProfile {
                    land: f.land,
                    exemptions: &f.exemptions,
                    only: None,
                },
            )
    }
    pub fn successors(&self, from: Tile) -> Vec<(Tile, Cost)> {
        CARDINAL
            .iter()
            .chain(if self.fixture.diagonal {
                DIAGONAL.iter()
            } else {
                [].iter()
            })
            .filter_map(|&(dx, dy)| {
                let to = tile(from.x.checked_add(dx)?, from.y.checked_add(dy)?);
                self.index
                    .edge_open(from, to, self.fixture.diagonal, |t| self.open(t))
                    .then(|| {
                        (
                            to,
                            Cost(spatial::edge_cost(
                                from,
                                to,
                                self.index.roads(),
                                self.fixture.mode,
                            )),
                        )
                    })
            })
            .collect()
    }
    pub fn heuristic(&self, t: Tile) -> Cost {
        // Python uses BFS without roads/diagonals, and a goal bounding-box A*
        // lower bound for multiple/perimeter goals in weighted searches.
        if self.index.roads().is_empty() && !self.fixture.diagonal {
            return Cost(0.0);
        }
        let Some((x0, x1, y0, y1)) = self.goal_bounds else {
            return Cost(0.0);
        };
        let dx = (x0 - t.x).max(0).max(t.x - x1) as f64;
        let dy = (y0 - t.y).max(0).max(t.y - y1) as f64;
        let distance = if self.fixture.diagonal {
            dx.max(dy) + (std::f64::consts::SQRT_2 - 1.0) * dx.min(dy)
        } else {
            dx + dy
        };
        Cost(
            distance
                * if self.index.roads().is_empty() {
                    6.0
                } else {
                    4.0
                },
        )
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
#[derive(Clone, Copy, Debug)]
pub enum Backend {
    Custom,
    LibraryAstar,
    LibraryReach,
}
impl Backend {
    pub const ALL: [Self; 3] = [Self::Custom, Self::LibraryAstar, Self::LibraryReach];
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom => "custom",
            Self::LibraryAstar => "pathfinding_astar",
            Self::LibraryReach => "pathfinding_reach",
        }
    }
}
#[derive(Debug)]
pub struct Answer {
    pub outcome: Outcome,
    pub expansions: usize,
    pub work: usize,
}

/// Actor-independent comparison entry point, not a gameplay route API.
pub fn solve(graph: &Graph, backend: Backend) -> Answer {
    let start = graph.fixture.start;
    if graph.goals.contains(&start) {
        return Answer {
            outcome: Outcome::Arrived,
            expansions: 0,
            work: 0,
        };
    }
    if graph.goals.is_empty() {
        return Answer {
            outcome: Outcome::Unreachable,
            expansions: 0,
            work: 0,
        };
    }
    match backend {
        Backend::Custom => {
            let mut job = Incremental::new(graph);
            while job.outcome == Outcome::Pending {
                job.advance(128);
            }
            Answer {
                outcome: job.outcome,
                expansions: job.expansions,
                work: job.work,
            }
        }
        Backend::LibraryAstar => {
            let expansions = Cell::new(0);
            let result = astar(
                &start,
                |t| {
                    expansions.set(expansions.get() + 1);
                    graph.successors(*t)
                },
                |t| graph.heuristic(*t),
                |t| graph.goals.contains(t),
            );
            let outcome = result.map_or(Outcome::Unreachable, |(path, cost)| Outcome::Completed {
                // Library paths include start; conversion cost is measured.
                path: path.into_iter().skip(1).collect(),
                cost: cost.0,
            });
            Answer {
                outcome,
                expansions: expansions.get(),
                work: expansions.get(),
            }
        }
        Backend::LibraryReach => {
            let expansions = Cell::new(0);
            let mut parents = BTreeMap::new();
            for item in reach(graph, &expansions) {
                parents.insert(item.node, item.parent);
                if graph.goals.contains(&item.node) {
                    let mut path = vec![];
                    let mut at = item.node;
                    while at != start {
                        path.push(at);
                        at = parents[&at].unwrap();
                    }
                    path.reverse();
                    return Answer {
                        outcome: Outcome::Completed {
                            path,
                            cost: item.total_cost.0,
                        },
                        expansions: expansions.get(),
                        work: expansions.get(),
                    };
                }
            }
            Answer {
                outcome: Outcome::Unreachable,
                expansions: expansions.get(),
                work: expansions.get(),
            }
        }
    }
}

/// Real library iterator retains its frontier between next() calls. Each call
/// expands one node but can internally discard many stale heap entries.
pub fn reach<'a>(
    graph: &'a Graph,
    expansions: &'a Cell<usize>,
) -> impl Iterator<Item = pathfinding::directed::dijkstra::DijkstraReachableItem<Tile, Cost>> + 'a {
    dijkstra_reach(&graph.fixture.start, move |t| {
        expansions.set(expansions.get() + 1);
        graph.successors(*t)
    })
}

#[derive(Clone, Copy)]
enum Phase {
    Search,
    Trace(Tile),
    Reverse(usize),
}
/// Evaluation implementation: one counted work unit is one heap pop (including
/// stale entries), one predecessor, or one reverse swap. ≤8 edges per expansion.
/// Setup, allocation, final destruction are not real-time bounded by this quantum.
pub struct Incremental<'a> {
    graph: &'a Graph,
    heap: BinaryHeap<Reverse<(Cost, u64, Cost, Tile)>>,
    previous: BTreeMap<Tile, Option<Tile>>,
    costs: BTreeMap<Tile, Cost>,
    order: u64,
    phase: Phase,
    path: Vec<Tile>,
    result_cost: f64,
    pub outcome: Outcome,
    pub expansions: usize,
    pub work: usize,
}
impl<'a> Incremental<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        let start = graph.fixture.start;
        Self {
            graph,
            heap: BinaryHeap::from([Reverse((graph.heuristic(start), 0, Cost(0.0), start))]),
            previous: BTreeMap::from([(start, None)]),
            costs: BTreeMap::from([(start, Cost(0.0))]),
            order: 0,
            phase: Phase::Search,
            path: vec![],
            result_cost: 0.0,
            outcome: if graph.goals.contains(&start) {
                Outcome::Arrived
            } else if graph.goals.is_empty() {
                Outcome::Unreachable
            } else {
                Outcome::Pending
            },
            expansions: 0,
            work: 0,
        }
    }
    pub fn cancel(&mut self) {
        if self.outcome == Outcome::Pending {
            self.outcome = Outcome::Cancelled;
        }
    }
    pub fn advance(&mut self, quantum: usize) -> usize {
        let before = self.work;
        for _ in 0..quantum {
            if self.outcome != Outcome::Pending {
                break;
            }
            self.work += 1;
            match self.phase {
                Phase::Search => {
                    let Some(Reverse((_, _, cost, at))) = self.heap.pop() else {
                        self.outcome = Outcome::Unreachable;
                        break;
                    };
                    if self.costs[&at] != cost {
                        continue;
                    }
                    if self.graph.goals.contains(&at) {
                        self.result_cost = cost.0;
                        self.phase = Phase::Trace(at);
                        continue;
                    }
                    self.expansions += 1;
                    for (next, weight) in self.graph.successors(at) {
                        let candidate = cost + weight;
                        if self.costs.get(&next).is_some_and(|old| candidate >= *old) {
                            continue;
                        }
                        self.costs.insert(next, candidate);
                        self.previous.insert(next, Some(at));
                        self.order += 1;
                        self.heap.push(Reverse((
                            candidate + self.graph.heuristic(next),
                            self.order,
                            candidate,
                            next,
                        )));
                    }
                }
                Phase::Trace(at) => {
                    if let Some(parent) = self.previous[&at] {
                        self.path.push(at);
                        self.phase = Phase::Trace(parent);
                    } else {
                        self.phase = Phase::Reverse(0);
                    }
                }
                Phase::Reverse(i) => {
                    if i < self.path.len() / 2 {
                        let j = self.path.len() - i - 1;
                        self.path.swap(i, j);
                        self.phase = Phase::Reverse(i + 1);
                    } else {
                        self.outcome = Outcome::Completed {
                            path: std::mem::take(&mut self.path),
                            cost: self.result_cost,
                        };
                    }
                }
            }
        }
        self.work - before
    }
}

/// Independent O(V²) Dijkstra oracle: no shared heap, heuristic, successors,
/// edge_open or edge_cost. It deliberately duplicates contract mathematics and
/// uses low-level fence intersections only. Suitable for small finite fixtures.
pub fn reference(f: &Fixture) -> Option<f64> {
    let graph = Graph::prepare(f);
    let n = f.size as usize;
    let id = |t: Tile| t.y as usize * n + t.x as usize;
    let mut distance = vec![f64::INFINITY; n * n];
    let mut visited = vec![false; n * n];
    distance[id(f.start)] = 0.0;
    loop {
        let next = (0..n * n)
            .filter(|&i| !visited[i] && distance[i].is_finite())
            .min_by(|&a, &b| distance[a].total_cmp(&distance[b]));
        let i = next?;
        let at = tile((i % n) as i32, (i / n) as i32);
        if graph.goals.contains(&at) {
            return Some(distance[i]);
        }
        visited[i] = true;
        let open = |t: Tile| {
            if t.x < 0
                || t.y < 0
                || t.x >= n as i32
                || t.y >= n as i32
                || f.void.contains(&t)
                || f.unavailable.contains(&t)
            {
                return false;
            }
            if let Some((x0, x1, y0, y1)) = f.bounds
                && (t.x < x0 || t.x > x1 || t.y < y0 || t.y > y1)
            {
                return false;
            }
            let occupied = f
                .structures
                .iter()
                .any(|s| s.footprint.tiles().any(|p| p == t));
            // Python subtracts a set of acre anchors from the whole occupancy
            // union, including overlaps. A saved larger acre exempts one anchor.
            let acre_anchor = f.land != LandAccess::Ordinary
                && f.structures.iter().any(|s| {
                    s.kind == "acre"
                        && s.construction == Construction::Complete
                        && s.footprint.origin() == t
                });
            !occupied || f.exemptions.contains(&t) || acre_anchor
        };
        for dy in -1..=1 {
            for dx in -1..=1 {
                if (dx == 0 && dy == 0) || (!f.diagonal && dx != 0 && dy != 0) {
                    continue;
                }
                let to = tile(at.x + dx, at.y + dy);
                if !open(to)
                    || (dx != 0 && dy != 0 && (!open(tile(to.x, at.y)) || !open(tile(at.x, to.y))))
                {
                    continue;
                }
                if spatial::crosses(
                    lantern_core::units::Position::new(at.x as f64, at.y as f64).unwrap(),
                    lantern_core::units::Position::new(to.x as f64, to.y as f64).unwrap(),
                    graph.index.fences(),
                ) {
                    continue;
                }
                let road = |t| {
                    f.roads
                        .iter()
                        .any(|r| r.tile == t && r.construction == Construction::Complete)
                };
                let a = if road(at) { 2.0 } else { 3.0 };
                let b = if road(to) { 2.0 } else { 3.0 };
                let mut w = match f.mode {
                    RoadMode::Smooth => a + b,
                    RoadMode::Departure => 2.0 * a,
                    RoadMode::Arrival => 2.0 * b,
                };
                if dx != 0 && dy != 0 {
                    w *= 2.0_f64.sqrt();
                }
                let j = id(to);
                distance[j] = distance[j].min(distance[i] + w);
            }
        }
    }
}

pub fn fixtures() -> Vec<Fixture> {
    let mut local = Fixture::open("local_cardinal", 64, tile(2, 2), tile(15, 13));
    let mut detour = Fixture::open("wall_detour", 64, tile(2, 30), tile(60, 30));
    for y in 0..58 {
        detour.block(tile(32, y), "tree");
    }
    let mut unreachable = detour.clone();
    unreachable.name = "unreachable".into();
    for y in 58..64 {
        unreachable.block(tile(32, y), "tree");
    }
    let mut multi = local.clone();
    multi.name = "multi_goal".into();
    multi.diagonal = true;
    multi.target = Target::Multiple(BTreeSet::from([tile(40, 40), tile(20, 16), tile(35, 18)]));
    let mut roads = local.clone();
    roads.name = "roads_smooth".into();
    roads.diagonal = true;
    for x in 2..45 {
        roads.roads.push(Road {
            tile: tile(x, 7),
            construction: Construction::Complete,
        });
    }
    let mut departure = roads.clone();
    departure.name = "roads_departure".into();
    departure.mode = RoadMode::Departure;
    let mut arrival = roads.clone();
    arrival.name = "roads_arrival".into();
    arrival.mode = RoadMode::Arrival;
    let mut fence = Fixture::open("barn_fence", 64, tile(25, 30), tile(30, 30));
    fence.block(tile(30, 30), "barn");
    fence.target = Target::Adjacent(fence.structures[0].footprint);
    fence.diagonal = true;
    let mut chunk = Fixture::open("chunk_boundary", 64, tile(28, 30), tile(40, 35));
    chunk.diagonal = true;
    chunk.bounds = Some((0, 63, 0, 63));
    for y in 0..64 {
        if y != 36 {
            chunk.unavailable.insert(tile(32, y));
        }
    }
    let mut footprint = local.clone();
    footprint.name = "footprint".into();
    let fp = Footprint::saved(tile(18, 15), Some((3, 2))).unwrap();
    footprint.structures.push(Structure {
        footprint: fp,
        kind: "workshop".into(),
        construction: Construction::Complete,
        fence_orientation: 0,
    });
    footprint.target = Target::Adjacent(fp);
    local.void.insert(tile(5, 5));
    let mut edited = local.clone();
    edited.name = "edited_on_route".into();
    edited.block(tile(8, 2), "tree");
    let mut distant = local.clone();
    distant.name = "edited_distant".into();
    distant.block(tile(60, 60), "tree");
    vec![
        edited,
        distant,
        local,
        detour,
        unreachable,
        multi,
        roads,
        departure,
        arrival,
        fence,
        chunk,
        footprint,
    ]
}
