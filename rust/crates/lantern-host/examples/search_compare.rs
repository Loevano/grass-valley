//! Run with cargo run --release -p lantern-host --example search_compare -- REPORT.json
mod search_support;
use search_support::*;
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
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
fn stats(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let p = |q: f64| samples[((samples.len() - 1) as f64 * q).ceil() as usize];
    json!({"p50_us":p(0.5),"p95_us":p(0.95),"p99_us":p(0.99),"max_us":samples[samples.len()-1]})
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("comparison requires --release");
        std::process::exit(2);
    }
    let output = std::env::args().nth(1).expect("report JSON path");
    let fixtures = fixtures();
    let cancellation_graph = Graph::prepare(&fixtures[0]);
    let mut cancelled = Incremental::new(cancellation_graph.query.clone());
    cancelled.advance(1);
    cancelled.cancel();
    assert_eq!(cancelled.outcome(), &Outcome::Cancelled);
    // Correctness is required before any timed measurements. The reference is
    // intentionally not timed against optimized search implementations.
    for f in &fixtures {
        let expected = reference(f);
        for b in Backend::ALL {
            let got = solve(&Graph::prepare(f), b).outcome.cost();
            assert!(
                match (expected, got) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                    (None, None) => true,
                    _ => false,
                },
                "{} {}",
                f.name,
                b.name()
            );
        }
    }
    let mut rows = vec![];
    for f in &fixtures {
        let warm = Graph::prepare(f);
        let setup_memory = memory(|| drop(black_box(Graph::prepare(f))));
        for cold in [true, false] {
            for _ in 0..8 {
                for b in Backend::ALL {
                    if cold {
                        let g = Graph::prepare(f);
                        black_box(solve(&g, b));
                    } else {
                        black_box(solve(&warm, b));
                    }
                }
            }
            let mut total: [Vec<f64>; 3] = std::array::from_fn(|_| vec![]);
            let mut setup: [Vec<f64>; 3] = std::array::from_fn(|_| vec![]);
            let mut search: [Vec<f64>; 3] = std::array::from_fn(|_| vec![]);
            // Paired rounds rotate candidate order to reduce ordering bias.
            for round in 0..41 {
                for offset in 0..3 {
                    let i = (round + offset) % 3;
                    let b = Backend::ALL[i];
                    let start = Instant::now();
                    let owned = cold.then(|| Graph::prepare(black_box(f)));
                    let setup_done = Instant::now();
                    let answer = solve(owned.as_ref().unwrap_or(&warm), b);
                    black_box(&answer);
                    let search_done = Instant::now();
                    drop(answer);
                    drop(owned);
                    total[i].push(start.elapsed().as_secs_f64() * 1e6);
                    setup[i].push(setup_done.duration_since(start).as_secs_f64() * 1e6);
                    search[i].push(search_done.duration_since(setup_done).as_secs_f64() * 1e6);
                }
            }
            for (i, b) in Backend::ALL.into_iter().enumerate() {
                let allocations = memory(|| {
                    let g = cold.then(|| Graph::prepare(f));
                    drop(black_box(solve(g.as_ref().unwrap_or(&warm), b)));
                    drop(g);
                });
                let answer = solve(&warm, b);
                rows.push(json!({"fixture":f.name,"graph":if cold{"cold"}else{"warm"},"backend":b.name(),"total":stats(total[i].clone()),"setup":stats(setup[i].clone()),"search_and_conversion":stats(search[i].clone()),"paired_total_us":total[i],"memory":allocations,"graph_setup_memory":setup_memory,"expansions":answer.expansions,"work_units":answer.work,"cost":answer.outcome.cost()}));
            }
        }
    }
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let report = json!({"schema":"r09-search-comparison-v1","starting_revision":String::from_utf8(revision.stdout).unwrap().trim(),"candidate":"working tree; see commit carrying this report","os":std::env::consts::OS,"arch":std::env::consts::ARCH,"release":true,"rounds":41,"warmup_rounds":8,"library":"pathfinding 4.16.0","memory":"requested heap bytes including result conversion; separate untimed instrumented request, not RSS or allocator metadata","warm":"immutable derived graph reuse only, no route-result cache","total":"graph setup (cold), search, path conversion and destruction; fixture authoring, oracle, process startup and compilation excluded","limitations":["single process/machine; scheduler and host integration absent","no W1-W6 latency or production host speed claim","library work counts successor calls only; hidden stale pops not observable","allocation counters cover Rust global allocator requests, not process RSS","cold is derived graph rebuild, not OS page cache flush"],"rows":rows});
    std::fs::write(&output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    println!(
        "Wrote {} request summaries to {output}",
        report["rows"].as_array().unwrap().len()
    );
}
