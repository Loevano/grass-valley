//! Slice-1 release microbenchmark; no production-cadence/end-to-end claim.
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
fn query(sealed: bool) -> Rc<Query> {
    let resources: Vec<_> = (0..if sealed { 64 } else { 63 })
        .map(|y| Resource {
            footprint: Footprint::saved(Tile { x: 32, y }, None).unwrap(),
            kind: "stone".into(),
        })
        .collect();
    Rc::new(
        Query::new(
            Rc::new(SpatialIndex::derive(&[], &resources, &[], &[], 64).unwrap()),
            Terrain::default(),
            Access::default(),
            Tile { x: 0, y: 0 },
            Target::Exact(Tile { x: 63, y: 0 }),
            false,
            RoadMode::Smooth,
        )
        .unwrap(),
    )
}
fn stats(samples: &[f64]) -> Value {
    let mut samples = samples.to_vec();
    samples.sort_by(f64::total_cmp);
    let p = |q: f64| samples[((samples.len() - 1) as f64 * q).ceil() as usize];
    json!({"p50":p(0.5),"p95":p(0.95),"p99":p(0.99),"max":p(1.)})
}
fn run(n: usize, sealed: bool) -> Value {
    // Measurement vectors deliberately preallocated outside the tracked service.
    let mut first = vec![None; n];
    let mut ready = vec![0.; n];
    let mut first_round = vec![0; n];
    let mut max_jobs = 0;
    let mut operations = 0;
    let mut rounds = 0;
    let mut setup_us = 0.;
    let mut cancel_us = 0.;
    let mut service_us = 0.;
    let mem = memory(|| {
        let setup = Instant::now();
        let (mut towns, keys) = towns(n);
        let requests: Vec<_> = keys
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
        let q = query(sealed);
        setup_us = setup.elapsed().as_secs_f64() * 1e6;
        let mut s = Service::new(Limits::default());
        let mut done = vec![false; n];
        let start = Instant::now();
        for round in 1..=10000 {
            rounds = round;
            for (i, r) in requests.iter().enumerate().filter(|(i, _)| !done[*i]) {
                let status = s.submit(
                    &towns[keys[i].0],
                    r,
                    NavigationRevision(1),
                    q.clone(),
                    [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i % 3],
                );
                assert!(!matches!(
                    status,
                    Status::Stale
                        | Status::Waiting(
                            Waiting::QueryLimit | Waiting::IdentityLimit | Waiting::ResourceLimit
                        )
                ));
            }
            max_jobs = max_jobs.max(s.resident_jobs());
            assert!(max_jobs <= 128);
            let used = s.advance(2048, |r| {
                towns
                    .iter()
                    .any(|t| t.route_is_current(r, NavigationRevision(1)))
            });
            assert!(used <= 2048);
            operations += used;
            for (i, r) in requests.iter().enumerate().filter(|(i, _)| !done[*i]) {
                if s.work(r).is_some_and(|w| w > 0) && first[i].is_none() {
                    first[i] = Some(start.elapsed().as_secs_f64() * 1e6);
                    first_round[i] = round;
                }
            }
            for (i, r) in requests.iter().enumerate() {
                if let Some(result) = s.take(r) {
                    assert_eq!(matches!(result.outcome(), Outcome::Unreachable), sealed);
                    assert!(matches!(
                        result.outcome(),
                        Outcome::Completed { .. } | Outcome::Unreachable
                    ));
                    done[i] = true;
                    ready[i] = start.elapsed().as_secs_f64() * 1e6;
                }
            }
            if done.iter().all(|d| *d) {
                break;
            }
        }
        assert!(done.iter().all(|d| *d));
        assert_eq!(s.resident_jobs(), 0);
        service_us = start.elapsed().as_secs_f64() * 1e6;
        // Separately charge cancellation after some retained work, not only an
        // empty initial search. Same prepared query/requests, four towns.
        for (i, r) in requests.iter().enumerate().take(128) {
            s.submit(
                &towns[keys[i].0],
                r,
                NavigationRevision(1),
                q.clone(),
                Priority::Ordinary,
            );
        }
        s.advance(16384, |_| true);
        let before = Instant::now();
        for r in &requests {
            s.cancel(r);
        }
        cancel_us = before.elapsed().as_secs_f64() * 1e6;
        assert_eq!(s.resident_jobs(), 0);
        assert!(!s.has_reservation());
        assert_eq!(Rc::strong_count(&q), 1);
    });
    let first: Vec<_> = first.into_iter().map(Option::unwrap).collect();
    json!({"actors":n,"towns":4,"scenario":if sealed {"sealed-wall"} else {"wall-detour"},
        "setup_us":setup_us,"service_cpu_wall_us":service_us,"cancel_us":cancel_us,
        "first_service_wait_us":stats(&first),"route_ready_wait_us":stats(&ready),
        "first_service_max_round":first_round.into_iter().max(),"rounds":rounds,
        "work":operations,"restarts":0,"max_resident_jobs":max_jobs,"memory":mem,
        "assertions":"passed: finite completion, correct terminal kind, 2048 allowance, <=128 jobs, cancellation/query release",
        "latency_acceptance":"inconclusive: tight-loop single-process microbenchmark, no scheduled cadence or contention"})
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("use --release");
        std::process::exit(2);
    }
    let output = std::env::args().nth(1).expect("report JSON path");
    let mut scenarios = Vec::new();
    for repeat in 0..3 {
        for n in [16, 64, 256] {
            for sealed in [false, true] {
                let mut value = run(n, sealed);
                value["repeat"] = json!(repeat);
                scenarios.push(value);
            }
        }
    }
    let report = json!({"baseline":"67f3a58b26b1f389bfbd5726ee87715ec1238be2",
        "candidate":"working tree R10 slice 1 (record final commit separately)",
        "comparison":"no scheduler exists at baseline; absolute candidate evidence only",
        "status":"inconclusive for W1-W6/cadence/end-to-end budgets",
        "units":"microseconds; work is deterministic core search operations; rounds have no assigned time",
        "memory_scope":"all four-town fixture/request/query/service allocations; preallocated measurement vectors excluded; live bytes must return to zero",
        "limitations":"setup includes graph preparation; cancellation and allocator costs unmetered by search allowance; single local machine, no HTTP/save/browser or production clock; no speedup claim",
        "scenarios":scenarios});
    std::fs::write(&output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    let mut md = String::from(
        "# R10 slice 1 release service measurements\n\nW1–W6 acceptance: **inconclusive**. Tight-loop candidate evidence only; no baseline service exists. No cadence, contention, browser, or speedup claim. Query preparation and cancellation are timed separately; operation allowance does not bound wall-clock.\n\nBaseline: `67f3a58b26b1f389bfbd5726ee87715ec1238be2`; candidate: working tree R10 slice 1.\n\n| Actors | Scenario | Repeat | Setup µs | Service µs | First max µs | Ready max µs | First max round | Rounds | Cancel µs | Peak bytes |\n|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    for s in report["scenarios"].as_array().unwrap() {
        md.push_str(&format!(
            "| {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} | {} | {} | {:.0} | {} |\n",
            s["actors"],
            s["scenario"].as_str().unwrap(),
            s["repeat"],
            s["setup_us"].as_f64().unwrap(),
            s["service_cpu_wall_us"].as_f64().unwrap(),
            s["first_service_wait_us"]["max"].as_f64().unwrap(),
            s["route_ready_wait_us"]["max"].as_f64().unwrap(),
            s["first_service_max_round"],
            s["rounds"],
            s["cancel_us"].as_f64().unwrap(),
            s["memory"]["peak_live_bytes"]
        ));
    }
    md.push_str("\nAll scenario assertions passed: finite route/unreachable completion, per-call work ≤2048, resident jobs ≤128, cancellation releases queries and all tracked live allocations return to zero. No timing threshold is applied. JSON contains p50/p95/p99/max, allocation calls/bytes and deterministic work. These are representative four-town 64×64 wall cases at 16/64/256 requests, not complete W2/W3/W4 workloads; warm caches, edit recovery, long/world-size routes and service cadence remain later slices.\n");
    std::fs::write(std::path::Path::new(&output).with_extension("md"), md).unwrap();
}
