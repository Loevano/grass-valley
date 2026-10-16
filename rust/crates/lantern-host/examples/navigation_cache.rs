//! Slice-2 cache and local-invalidation diagnostics; no production cadence claim.
use lantern_core::{ids::*, model::*, navigation::*, search::*, spatial::*, units::*};
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    rc::Rc,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::Instant,
};
struct Meter;
static TRACK: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
fn allocated(n: usize) {
    if TRACK.load(Ordering::Relaxed) {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(n, Ordering::Relaxed);
        let live = LIVE.fetch_add(n, Ordering::Relaxed) + n;
        PEAK.fetch_max(live, Ordering::Relaxed);
    }
}
// Safety: allocation and deallocation are forwarded unchanged to System. Atomic
// accounting never allocates; tracking is enabled only around single-threaded
// requests whose allocations are also dropped before disabling it.
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        if TRACK.load(Ordering::Relaxed) {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, layout, new_size) };
        if !q.is_null() && TRACK.load(Ordering::Relaxed) {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            allocated(new_size);
        }
        q
    }
}
#[global_allocator]
static ALLOCATOR: Meter = Meter;
fn memory(f: impl FnOnce()) -> Value {
    for counter in [&CALLS, &BYTES, &LIVE, &PEAK] {
        counter.store(0, Ordering::Relaxed);
    }
    TRACK.store(true, Ordering::Relaxed);
    f();
    TRACK.store(false, Ordering::Relaxed);
    assert_eq!(
        LIVE.load(Ordering::Relaxed),
        0,
        "request allocations must be released"
    );
    json!({"allocation_calls":CALLS.load(Ordering::Relaxed),"allocated_bytes":BYTES.load(Ordering::Relaxed),"peak_live_bytes":PEAK.load(Ordering::Relaxed)})
}

fn position(x: f64, y: f64) -> Position {
    Position::new(x, y).unwrap()
}
fn towns(n: usize) -> (Vec<Town>, Vec<(usize, ActorId)>) {
    let world = WorldId::new("synthetic-service").unwrap();
    let mut ids = IdAllocator::new(world.clone());
    let mut keys = Vec::new();
    let towns = (0..4)
        .map(|i| {
            let owner = Owner {
                world: world.clone(),
                town: TownId::new(format!("town-{i}")).unwrap(),
            };
            let actors = (0..n / 4)
                .map(|_| {
                    let id = ids.allocate(&owner).unwrap();
                    keys.push((i, id.clone()));
                    ActorState::new(id, position(0., 0.), Cargo::default())
                })
                .collect();
            let mut town = Town::restore(owner, vec![], actors).unwrap();
            for (_, id) in keys.iter().filter(|(town, _)| *town == i) {
                town.set_task(
                    id,
                    Task::Move {
                        destination: position(63., 0.),
                    },
                )
                .unwrap();
            }
            town
        })
        .collect();
    (towns, keys)
}
fn query(scenario: &str, edited: bool) -> Rc<Query> {
    let mut resources: Vec<_> = if scenario == "negative" || scenario == "opened-negative" {
        (0..64)
            .filter(|&y| !(edited && scenario == "opened-negative" && y == 0))
            .map(|y| Resource {
                footprint: Footprint::saved(Tile { x: 32, y }, None).unwrap(),
                kind: "stone".into(),
            })
            .collect()
    } else {
        vec![]
    };
    if edited && scenario == "distant-obstacle" {
        resources.push(Resource {
            footprint: Footprint::saved(Tile { x: 63, y: 63 }, None).unwrap(),
            kind: "stone".into(),
        });
    }
    if edited && scenario == "relevant-obstacle" {
        resources.push(Resource {
            footprint: Footprint::saved(Tile { x: 1, y: 0 }, None).unwrap(),
            kind: "stone".into(),
        });
    }
    let mut roads = vec![];
    if scenario == "distant-road" {
        roads.push(Road {
            tile: Tile { x: 63, y: 62 },
            construction: Construction::Complete,
        });
    }
    if edited
        && (scenario == "first-road" || scenario == "distant-road" || scenario == "relevant-road")
    {
        roads.push(Road {
            tile: if scenario == "relevant-road" {
                Tile { x: 1, y: 0 }
            } else {
                Tile { x: 63, y: 63 }
            },
            construction: Construction::Complete,
        });
    }
    let structures = if edited && scenario == "fence" {
        vec![Structure {
            footprint: Footprint::saved(Tile { x: 4, y: 0 }, None).unwrap(),
            kind: "barn".into(),
            construction: Construction::Complete,
            fence_orientation: 1,
        }]
    } else {
        vec![]
    };
    let terrain = if edited && scenario == "terrain" {
        Terrain {
            cells: Some([(Tile { x: 1, y: 0 }, 'v')].into()),
            bounds: None,
        }
    } else {
        Terrain::default()
    };
    Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&structures, &resources, &[], &roads, 64).unwrap()),
            terrain,
            Access::default(),
            Tile { x: 0, y: 0 },
            Target::Exact(Tile { x: 63, y: 0 }),
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
fn run(scenario: &str) -> Value {
    let mut setup = 0.;
    let mut maintenance = 0.;
    let mut service = 0.;
    let mut work = 0;
    let mut rounds = 0;
    let mut diagnostics = Diagnostics::default();
    let mut cache = (0, 0, 0);
    let mut wait = Vec::with_capacity(16);
    let mem = memory(|| {
        let timer = Instant::now();
        let (mut towns, keys) = towns(16);
        let q = query(scenario, false);
        let mut s = Service::new(Limits::default());
        setup = timer.elapsed().as_secs_f64() * 1e6;
        let mut requests: Vec<_> = keys
            .iter()
            .map(|(i, id)| {
                towns[*i]
                    .request_route(
                        id,
                        Destination::Position(position(63., 0.)),
                        NavigationRevision(1),
                    )
                    .unwrap()
            })
            .collect();
        let start = Instant::now();
        for (i, r) in requests.iter().enumerate() {
            s.submit_current(&towns[keys[i].0], r, q.clone(), Priority::Ordinary);
        }
        s.advance(64, |r| {
            towns.iter().any(|t| t.route_generation_is_current(r))
        });
        if scenario == "opened-negative" {
            let timer = Instant::now();
            for _ in 0..10000 {
                work += s.advance(2048, |r| {
                    towns.iter().any(|t| t.route_generation_is_current(r))
                });
                if requests.iter().all(|r| s.status(r) == Some(Status::Ready)) {
                    break;
                }
            }
            for r in &requests {
                assert_eq!(s.take(r).unwrap().outcome(), &Outcome::Unreachable);
            }
            requests = keys
                .iter()
                .map(|(i, id)| {
                    towns[*i]
                        .request_route(
                            id,
                            Destination::Position(position(63., 0.)),
                            NavigationRevision(1),
                        )
                        .unwrap()
                })
                .collect();
            let edited = query(scenario, true);
            for (i, r) in requests.iter().enumerate() {
                assert_eq!(
                    s.submit_current(&towns[keys[i].0], r, edited.clone(), Priority::Ordinary),
                    Status::Searching
                );
            }
            maintenance += timer.elapsed().as_secs_f64() * 1e6;
        }
        if !matches!(scenario, "cold" | "warm" | "negative") {
            let timer = Instant::now();
            let edited = if scenario == "unchanged" {
                q.clone()
            } else {
                query(scenario, true)
            };
            for r in &requests {
                s.refresh(r, edited.clone());
            }
            maintenance += timer.elapsed().as_secs_f64() * 1e6;
        }
        let timer = Instant::now();
        let mut done = [false; 16];
        for _ in 0..10000 {
            rounds += 1;
            work += s.advance(2048, |r| {
                towns.iter().any(|t| t.route_generation_is_current(r))
            });
            for (i, r) in requests.iter().enumerate() {
                if !done[i] && s.take(r).is_some() {
                    done[i] = true;
                    wait.push(start.elapsed().as_secs_f64() * 1e6);
                }
            }
            if done.iter().all(|v| *v) {
                break;
            }
        }
        assert!(done.iter().all(|v| *v));
        service += timer.elapsed().as_secs_f64() * 1e6;
        if matches!(scenario, "warm" | "negative") {
            let timer = Instant::now();
            for _ in 0..4 {
                requests = keys
                    .iter()
                    .map(|(i, id)| {
                        towns[*i]
                            .request_route(
                                id,
                                Destination::Position(position(63., 0.)),
                                NavigationRevision(1),
                            )
                            .unwrap()
                    })
                    .collect();
                for (i, r) in requests.iter().enumerate() {
                    assert_eq!(
                        s.submit_current(&towns[keys[i].0], r, q.clone(), Priority::Ordinary),
                        Status::Ready
                    );
                    s.take(r).unwrap();
                }
            }
            maintenance += timer.elapsed().as_secs_f64() * 1e6;
            assert_eq!(s.diagnostics().cache_hits, 64);
        }
        diagnostics = s.diagnostics();
        cache = s.cache_usage();
    });
    wait.sort_by(f64::total_cmp);
    let pct = |f: f64| wait[((wait.len() - 1) as f64 * f).ceil() as usize];
    json!({"scenario":scenario,"actors":16,"towns":4,"setup_us":setup,"maintenance_us":maintenance,"service_us":service,"work_after_initial64":work,"rounds":rounds,"wait_us":{"p50":pct(0.5),"p95":pct(0.95),"p99":pct(0.99),"max":pct(1.)},"restarts":diagnostics.restarts,"retained":diagnostics.retained,"validation_record_upper_bound":diagnostics.validation_records,"cache_hits":diagnostics.cache_hits,"cache_usage":{"entries":cache.0,"records":cache.1,"waypoints":cache.2},"memory":mem})
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("use --release");
        std::process::exit(2);
    }
    let output = std::env::args().nth(1).expect("report JSON path");
    let mut scenarios = vec![];
    for repeat in 0..3 {
        for name in [
            "cold",
            "warm",
            "negative",
            "opened-negative",
            "unchanged",
            "distant-obstacle",
            "relevant-obstacle",
            "first-road",
            "distant-road",
            "relevant-road",
            "fence",
            "terrain",
        ] {
            let mut value = run(name);
            value["repeat"] = json!(repeat);
            scenarios.push(value);
        }
    }
    let report = json!({"baseline":"24fcaa722e9e7c4e6bd2cf849dd939b039fd1519","r10_baseline":"67f3a58b26b1f389bfbd5726ee87715ec1238be2","candidate":"R10 slice2 working tree","status":"inconclusive for W1-W6 and production cadence","scope":"release single-thread tight-loop, sixteen requests/four towns, three repetitions; allocator includes graph preparation, dependency maintenance, cache clones and destruction; requested heap, not RSS; wall process CPU proxy; warm repeated64 hits","limitations":"separate no-cache navigation_service baseline/candidate comparison; this new API has candidate-only diagnostics; no HTTP/save/browser/real cadence, no timing threshold; initial64 work excluded from work_after_initial64","scenarios":scenarios});
    std::fs::write(output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
}
