//! Actual monotonic scheduled release evidence; never treats rounds as elapsed time.
use lantern_core::{
    ids::*,
    model::*,
    navigation::{Limits, Priority},
    search::{Access, Query, Target, Terrain},
    spatial::*,
    units::*,
};
use lantern_host::navigation::*;
use serde_json::{Value, json};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    time::{Duration, Instant},
};
mod support {
    pub mod navigation;
}
use support::navigation::*;
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

fn stats(samples: &[f64]) -> Value {
    let mut samples = samples.to_vec();
    samples.sort_by(f64::total_cmp);
    let p = |q: f64| samples[((samples.len() - 1) as f64 * q).ceil() as usize];
    json!({"p50":p(0.5),"p95":p(0.95),"p99":p(0.99),"max":p(1.)})
}
fn requests(towns: &mut [Town]) -> Vec<(usize, RouteRequest)> {
    let mut requests = vec![];
    for (town, t) in towns.iter_mut().enumerate() {
        let ids: Vec<_> = t
            .actors()
            .iter()
            .map(|a| {
                let Task::Move { destination } = a.state().task else {
                    panic!("move fixture")
                };
                (a.state().id.clone(), destination)
            })
            .collect();
        for (id, goal) in ids {
            requests.push((
                town,
                t.request_route(&id, Destination::Position(goal), NavigationRevision(1))
                    .unwrap(),
            ));
        }
    }
    requests
}
fn run(n: usize, scenario: &str, allowance: usize, interval_us: u64) -> Value {
    let mut first = vec![None; n];
    let mut ready = vec![0.; n];
    let mut diagnostic = Diagnostics::default();
    let mut restarts = 0;
    let mut retained = 0;
    let mut hits = 0;
    let mut units = (0, 0, 0);
    let mut setup_us = 0.;
    let mut preparation_us = 0.;
    let mut edit_us = 0.;
    let mut cancellation_us = 0.;
    let mut elapsed_us = 0.;
    let mut cancelled = 0;
    let mut overrun_probe = false;
    let memory = memory(|| {
        let setup = Instant::now();
        let (mut towns, _) = towns(n);
        let mut requests = requests(&mut towns);
        let clock = MonotonicClock::default();
        let mut pump = NavigationPump::new(
            Limits {
                cache_entries: if scenario == "cold" { 0 } else { 64 },
                ..Limits::default()
            },
            Cadence {
                allowance,
                interval: Duration::from_micros(interval_us),
                ..Cadence::default()
            },
            clock.now(),
        );
        setup_us = setup.elapsed().as_secs_f64() * 1e6;
        let enqueued = clock.now();
        let prep = Instant::now();
        let q_scenario = match scenario {
            "cold" | "warm" | "cancel" | "overrun" => "wall",
            "negative-warm" => "negative",
            _ => scenario,
        };
        let mut q = query(q_scenario, false);
        preparation_us = prep.elapsed().as_secs_f64() * 1e6;
        let warm =
            scenario == "warm" || scenario == "negative-warm" || scenario == "opened-negative";
        if warm {
            loop {
                let delay = pump.next_due().saturating_sub(clock.now());
                if !delay.is_zero() {
                    std::thread::sleep(delay);
                }
                let intents: Vec<_> = requests
                    .iter()
                    .map(|(town, request)| Intent {
                        town: *town,
                        request,
                        query: q.clone(),
                        priority: Priority::Ordinary,
                    })
                    .collect();
                pump.poll(&clock, &mut towns, &intents, |_| {}).unwrap();
                if towns
                    .iter()
                    .flat_map(Town::actors)
                    .all(|a| !matches!(a.route(), RouteState::Pending(_)))
                {
                    break;
                }
            }
            requests = self::requests(&mut towns);
            if scenario == "opened-negative" {
                let edit = Instant::now();
                q = query(scenario, true);
                edit_us += edit.elapsed().as_secs_f64() * 1e6;
            }
        }
        // Warm query priming is excluded from measured route latency, but included
        // in total memory/busy/CPU. Cold latency includes preparation after enqueue.
        let enqueued = if warm { clock.now() } else { enqueued };
        let before = pump.diagnostics();
        let mut rounds = 0;
        let mut done = vec![false; n];
        while done.iter().any(|d| !*d) {
            assert!(rounds < 10000, "finite 64x64 fixture did not terminate");
            let delay = pump.next_due().saturating_sub(clock.now());
            if !delay.is_zero() {
                std::thread::sleep(delay);
            }
            if rounds == 1
                && !matches!(
                    scenario,
                    "cold"
                        | "warm"
                        | "negative"
                        | "negative-warm"
                        | "opened-negative"
                        | "cancel"
                        | "overrun"
                )
            {
                let edit = Instant::now();
                q = query(scenario, true);
                edit_us += edit.elapsed().as_secs_f64() * 1e6;
            }
            if rounds == 1 && scenario == "cancel" {
                let before = Instant::now();
                for j in (0..n).step_by(4) {
                    if done[j] {
                        continue;
                    }
                    towns[requests[j].0]
                        .cancel_route(requests[j].1.actor())
                        .unwrap();
                    pump.cancel(&requests[j].1);
                    done[j] = true;
                    cancelled += 1;
                }
                cancellation_us = before.elapsed().as_secs_f64() * 1e6;
            }
            // Deliberately delay one release to verify actual coalescing evidence.
            if rounds == 1 && scenario == "overrun" {
                std::thread::sleep(Duration::from_millis(7));
                overrun_probe = true;
            }
            let intents: Vec<_> = requests
                .iter()
                .enumerate()
                .filter(|(j, _)| !done[*j])
                .map(|(j, (town, request))| Intent {
                    town: *town,
                    request,
                    query: q.clone(),
                    priority: [Priority::Blocked, Priority::Cargo, Priority::Ordinary][j % 3],
                })
                .collect();
            pump.poll(&clock, &mut towns, &intents, |e| {
                let j = requests.iter().position(|(_, r)| r == e.request).unwrap();
                let wait = (clock.now() - enqueued).as_secs_f64() * 1e6;
                if (e.work > 0 || e.adopted.is_some()) && first[j].is_none() {
                    first[j] = Some(wait);
                }
                if let Some((terminal, result)) = e.adopted {
                    result.unwrap();
                    assert_eq!(
                        terminal == Terminal::Unreachable,
                        matches!(scenario, "negative" | "negative-warm")
                    );
                    ready[j] = wait;
                    done[j] = true;
                }
            })
            .unwrap();
            assert!(pump.service().resident_jobs() <= 128);
            rounds += 1;
        }
        elapsed_us = (clock.now() - enqueued).as_secs_f64() * 1e6;
        diagnostic = pump.diagnostics();
        assert_eq!(diagnostic.rejected, 0);
        assert!(diagnostic.operations - before.operations <= rounds * allowance as u64);
        assert_eq!(pump.service().resident_jobs(), 0);
        assert!(!pump.service().has_reservation());
        let d = pump.service().diagnostics();
        restarts = d.restarts;
        retained = d.retained;
        hits = d.cache_hits;
        units = pump.service().cache_usage();
        for t in &towns {
            for a in t.actors() {
                assert_eq!(a.state().position, position(0., 0.));
            }
        }
        // Query ownership and every fixture/search/cache allocation are dropped
        // while tracking remains enabled; memory() verifies live bytes return 0.
    });
    let first: Vec<_> = first.into_iter().flatten().collect();
    let ready: Vec<_> = ready.into_iter().filter(|&x| x > 0.).collect();
    json!({"actors":n,"towns":4,"scenario":scenario,"allowance":allowance,"interval_us":interval_us,
        "first_service_samples_us":first,"route_ready_samples_us":ready,"observations":ready.len(),"first_service_wait_us":stats(&first),"route_ready_wait_us":stats(&ready),
        "setup_us":setup_us,"query_preparation_us":preparation_us,"edit_preparation_us":edit_us,"cancellation_us":cancellation_us,
        "elapsed_us":elapsed_us,"pumps":diagnostic.pumps,"work":diagnostic.operations,"busy_us":diagnostic.busy.as_secs_f64()*1e6,
        "max_pump_us":diagnostic.max_duration.as_secs_f64()*1e6,"max_release_lateness_us":diagnostic.max_lateness.as_secs_f64()*1e6,
        "missed_releases":diagnostic.missed_releases,"overruns":diagnostic.overruns,"delayed_release_probe":overrun_probe,
        "restarts":restarts,"retained_refreshes":retained,"cache_hits":hits,"cancelled":cancelled,"max_jobs":diagnostic.max_jobs,"max_observed_retained_units":diagnostic.max_observed_retained_units,
        "cache_entries_records_waypoints":units,"memory":memory,
        "first_movement":"unmeasured: R11 travel absent; positions verified unchanged",
        "sample_assertions":{"service_max_under_1s":first.iter().all(|&x| x<1e6),"finite_terminal_under_10s":ready.iter().all(|&x| x<1e7),"local_ready_p95_under_250ms":stats(&ready)["p95"].as_f64().unwrap()<250000.,"local_ready_p99_under_1s":stats(&ready)["p99"].as_f64().unwrap()<1e6,"local_ready_max_under_2s":ready.iter().all(|&x|x<2e6)},
        "acceptance":"sample only; aggregate repeated observations, no HTTP/browser/save contention"})
}
// Exact geometry/start/target/arrival/cost family from tools/server_perf.py:
// town wall x=30+town, y=0..54; actor i starts (2+i/64,2+i%16),
// anchor (50+i%8,5+i/16), diagonal=true, departure road costs.
fn w2(n: usize, state: &str, allowance: usize, interval_us: u64) -> Value {
    let mut first = vec![None; n];
    let mut ready = vec![0.; n];
    let mut d = Diagnostics::default();
    let mut core = lantern_core::navigation::Diagnostics::default();
    let mut cache = (0, 0, 0);
    let mut prep_us = 0.;
    let mut elapsed_us = 0.;
    let memory = memory(|| {
        let world = WorldId::new("w2").unwrap();
        let mut ids = IdAllocator::new(world.clone());
        let mut towns = Vec::new();
        let mut keys = vec![None; n];
        for town in 0..4 {
            let owner = Owner {
                world: world.clone(),
                town: TownId::new(format!("town-{town}")).unwrap(),
            };
            let actors = (town..n)
                .step_by(4)
                .map(|i| {
                    let id = ids.allocate(&owner).unwrap();
                    keys[i] = Some((town, id.clone()));
                    ActorState::new(
                        id,
                        position((2 + i / 64) as f64, (2 + i % 16) as f64),
                        Cargo::default(),
                    )
                })
                .collect();
            towns.push(Town::restore(owner, vec![], actors).unwrap());
        }
        let mut requests = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let (town, id) = key.as_ref().unwrap();
            let goal = position((50 + i % 8) as f64, (5 + i / 16) as f64);
            towns[*town]
                .set_task(id, Task::Move { destination: goal })
                .unwrap();
            requests.push((
                *town,
                towns[*town]
                    .request_route(id, Destination::Position(goal), NavigationRevision(1))
                    .unwrap(),
            ));
        }
        let clock = MonotonicClock::default();
        let enqueued = clock.now();
        let make_queries = |edited| {
            let indexes: Vec<_> = (0..4)
                .map(|town| {
                    let mut resources: Vec<_> = (0..55)
                        .map(|y| Resource {
                            footprint: Footprint::saved(Tile { x: 30 + town, y }, None).unwrap(),
                            kind: "stone".into(),
                        })
                        .collect();
                    if edited {
                        resources.push(Resource {
                            footprint: Footprint::saved(Tile { x: 29, y: 55 }, None).unwrap(),
                            kind: "stone".into(),
                        });
                    }
                    std::rc::Rc::new(SpatialIndex::derive(&[], &resources, &[], &[], 64).unwrap())
                })
                .collect();
            requests
                .iter()
                .map(|(town, r)| {
                    std::rc::Rc::new(
                        Query::new(
                            indexes[*town].clone(),
                            Terrain::default(),
                            Access::default(),
                            Tile {
                                x: r.origin().x.get() as i32,
                                y: r.origin().y.get() as i32,
                            },
                            Target::AnchorAdjacent(Tile {
                                x: r.goal().x.get() as i32,
                                y: r.goal().y.get() as i32,
                            }),
                            true,
                            RoadMode::Departure,
                        )
                        .unwrap(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let prepare = Instant::now();
        let mut queries = make_queries(false);
        prep_us += prepare.elapsed().as_secs_f64() * 1e6;
        let mut pump = NavigationPump::new(
            Limits::default(),
            Cadence {
                allowance,
                interval: Duration::from_micros(interval_us),
                ..Cadence::default()
            },
            enqueued,
        );
        if state != "w2-cold" {
            while towns
                .iter()
                .flat_map(Town::actors)
                .any(|a| matches!(a.route(), RouteState::Pending(_)))
            {
                std::thread::sleep(pump.next_due().saturating_sub(clock.now()));
                let intents: Vec<_> = requests
                    .iter()
                    .zip(&queries)
                    .enumerate()
                    .map(|(i, ((town, request), query))| Intent {
                        town: *town,
                        request,
                        query: query.clone(),
                        priority: [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i % 3],
                    })
                    .collect();
                pump.poll(&clock, &mut towns, &intents, |_| {}).unwrap();
            }
            if state == "w2-obstacle" {
                let prepare = Instant::now();
                queries = make_queries(true);
                prep_us += prepare.elapsed().as_secs_f64() * 1e6;
            }
            for (town, r) in &mut requests {
                *r = towns[*town]
                    .request_route(r.actor(), r.destination().clone(), NavigationRevision(1))
                    .unwrap();
            }
        }
        let enqueued = if state == "w2-cold" {
            enqueued
        } else {
            clock.now()
        };
        let mut done = 0;
        for _ in 0..10000 {
            std::thread::sleep(pump.next_due().saturating_sub(clock.now()));
            let intents: Vec<_> = requests
                .iter()
                .zip(&queries)
                .enumerate()
                .map(|(i, ((town, request), query))| Intent {
                    town: *town,
                    request,
                    query: query.clone(),
                    priority: [Priority::Blocked, Priority::Cargo, Priority::Ordinary][i % 3],
                })
                .collect();
            pump.poll(&clock, &mut towns, &intents, |e| {
                let i = requests.iter().position(|(_, r)| r == e.request).unwrap();
                let wait = (clock.now() - enqueued).as_secs_f64() * 1e6;
                if (e.work > 0 || e.adopted.is_some()) && first[i].is_none() {
                    first[i] = Some(wait);
                }
                if let Some((kind, result)) = e.adopted {
                    assert_eq!(kind, Terminal::Ready);
                    result.unwrap();
                    ready[i] = wait;
                    done += 1;
                }
            })
            .unwrap();
            if done == n {
                break;
            }
        }
        assert_eq!(done, n);
        elapsed_us = (clock.now() - enqueued).as_secs_f64() * 1e6;
        d = pump.diagnostics();
        core = pump.service().diagnostics();
        cache = pump.service().cache_usage();
        assert_eq!(pump.service().resident_jobs(), 0);
        assert!(!pump.service().has_reservation());
        for (i, (town, r)) in requests.iter().enumerate() {
            let actor = towns[*town].actor(r.actor()).unwrap();
            assert_eq!(actor.state().position, r.origin());
            let RouteState::Ready { waypoints, .. } = actor.route() else {
                panic!("ready")
            };
            let path: Vec<_> = waypoints
                .iter()
                .map(|p| Tile {
                    x: p.x.get() as i32,
                    y: p.y.get() as i32,
                })
                .collect();
            assert!(queries[i].route_safe(
                Tile {
                    x: r.origin().x.get() as i32,
                    y: r.origin().y.get() as i32
                },
                &path
            ));
        }
    });
    let first: Vec<_> = first.into_iter().map(Option::unwrap).collect();
    json!({"actors":n,"towns":4,"scenario":state,"family":"agreed tools/server_perf routing_scenarios geometry/start/target/diagonal/departure; Rust cache64 as contracted",
        "allowance":allowance,"interval_us":interval_us,"first_service_samples_us":first,"route_ready_samples_us":ready,"observations":n,"first_service_wait_us":stats(&first),"route_ready_wait_us":stats(&ready),
        "query_preparation_us":prep_us,"elapsed_us":elapsed_us,"pumps":d.pumps,"work":d.operations,"busy_us":d.busy.as_secs_f64()*1e6,"max_pump_us":d.max_duration.as_secs_f64()*1e6,
        "max_release_lateness_us":d.max_lateness.as_secs_f64()*1e6,"missed_releases":d.missed_releases,"overruns":d.overruns,"max_jobs":d.max_jobs,"max_observed_retained_units":d.max_observed_retained_units,
        "restarts":core.restarts,"retained_refreshes":core.retained,"cache_hits":core.cache_hits,"cache_entries_records_waypoints":cache,"memory":memory,
        "sample_assertions":{"service_max_under_1s":first.iter().all(|&x|x<1e6),"finite_terminal_under_10s":ready.iter().all(|&x|x<1e7),"local_ready_p95_under_250ms":stats(&ready)["p95"].as_f64().unwrap()<250000.,"local_ready_p99_under_1s":stats(&ready)["p99"].as_f64().unwrap()<1e6,"local_ready_max_under_2s":ready.iter().all(|&x|x<2e6)},
        "first_movement":"unmeasured; R11 absent; positions unchanged","acceptance":"isolated sample only; W1/W5/W6 contention absent"})
}
fn bounded_long_cases() -> Value {
    let mut cases = Vec::new();
    for seam in [false, true] {
        let mut elapsed_us = 0.;
        let mut max_jobs = 0;
        let mut pending = 0;
        let mut terminal = 0;
        let mut recovery = 0;
        let mut diagnostics = Diagnostics::default();
        let memory = memory(|| {
            let (start, goal) = if seam {
                (position(255., 1.), position(270., 1.))
            } else {
                (position(0., 0.), position(479., 479.))
            };
            let (mut towns, _) = towns_at(16, start, goal);
            let mut requests = self::requests(&mut towns);
            let clock = MonotonicClock::default();
            let mut pump = NavigationPump::new(
                Limits {
                    work_per_job: 4096,
                    cache_entries: 0,
                    ..Limits::default()
                },
                Cadence::default(),
                clock.now(),
            );
            let enqueued = clock.now();
            let make_query = |loaded: bool| {
                let terrain = if seam {
                    Terrain {
                        cells: Some(
                            (248..272)
                                .flat_map(|x| {
                                    (0..16).map(move |y| {
                                        (Tile { x, y }, if x == 256 && !loaded { 'v' } else { 'g' })
                                    })
                                })
                                .collect(),
                        ),
                        bounds: Some((248, 271, 0, 15)),
                    }
                } else {
                    Terrain::default()
                };
                std::rc::Rc::new(
                    Query::new(
                        std::rc::Rc::new(SpatialIndex::derive(&[], &[], &[], &[], 480).unwrap()),
                        terrain,
                        Access::default(),
                        Tile {
                            x: start.x.get() as i32,
                            y: start.y.get() as i32,
                        },
                        Target::Exact(Tile {
                            x: goal.x.get() as i32,
                            y: goal.y.get() as i32,
                        }),
                        false,
                        RoadMode::Smooth,
                    )
                    .unwrap(),
                )
            };
            let mut query = make_query(false);
            for _ in 0..100 {
                std::thread::sleep(pump.next_due().saturating_sub(clock.now()));
                let intents: Vec<_> = requests
                    .iter()
                    .map(|(town, request)| Intent {
                        town: *town,
                        request,
                        query: query.clone(),
                        priority: Priority::Ordinary,
                    })
                    .collect();
                pending = 0;
                pump.poll(&clock, &mut towns, &intents, |e| {
                    if matches!(
                        e.status,
                        lantern_core::navigation::Status::Waiting(
                            lantern_core::navigation::Waiting::ResourceLimit
                        )
                    ) {
                        pending += 1;
                    }
                    if let Some((kind, result)) = e.adopted {
                        assert!(seam);
                        assert_eq!(kind, Terminal::Unreachable);
                        result.unwrap();
                        terminal += 1;
                    }
                })
                .unwrap();
                max_jobs = max_jobs.max(pump.service().resident_jobs());
                if (!seam && pending == 16) || (seam && terminal == 16) {
                    break;
                }
            }
            if seam {
                assert_eq!(terminal, 16);
                requests = self::requests(&mut towns);
                query = make_query(true);
                for _ in 0..100 {
                    std::thread::sleep(pump.next_due().saturating_sub(clock.now()));
                    let intents: Vec<_> = requests
                        .iter()
                        .map(|(town, request)| Intent {
                            town: *town,
                            request,
                            query: query.clone(),
                            priority: Priority::Ordinary,
                        })
                        .collect();
                    pump.poll(&clock, &mut towns, &intents, |e| {
                        if let Some((kind, result)) = e.adopted {
                            assert_eq!(kind, Terminal::Ready);
                            result.unwrap();
                            recovery += 1;
                        }
                    })
                    .unwrap();
                    if recovery == 16 {
                        break;
                    }
                }
                assert_eq!(recovery, 16);
            } else {
                assert_eq!(pending, 16);
                assert_eq!(terminal, 0);
                // Resource waiting remains pending authority, not unreachable.
                for town in &towns {
                    for a in town.actors() {
                        assert!(matches!(a.route(), RouteState::Pending(_)));
                    }
                }
                for (_, r) in &requests {
                    pump.cancel(r);
                }
            }
            elapsed_us = (clock.now() - enqueued).as_secs_f64() * 1e6;
            assert_eq!(pump.service().resident_jobs(), 0);
            assert!(!pump.service().has_reservation());
            diagnostics = pump.diagnostics();
        });
        cases.push(json!({"scenario":if seam {"sparse480-loaded-unavailable-seam-recovery"} else {"sparse480-world-size-resource-wait"},"actors":16,"towns":4,"work_cap_per_attempt":4096,"pending_resource_waits":pending,"unreachable":terminal,"recovered_ready":recovery,"max_jobs":max_jobs,"elapsed_us":elapsed_us,"work":diagnostics.operations,"cancelled":diagnostics.cancellations,"max_pump_us":diagnostics.max_duration.as_secs_f64()*1e6,"max_observed_retained_units":diagnostics.max_observed_retained_units,"memory":memory,"acceptance":"bounded candidate-only diagnostic; low sample count inconclusive; no universal terminal deadline or first-movement claim"}));
    }
    json!(cases)
}
fn main() {
    if cfg!(debug_assertions) {
        eprintln!("use --release");
        std::process::exit(2);
    }
    let output = std::env::args().nth(1).expect("report JSON path");
    let allowance = std::env::args()
        .nth(2)
        .map(|s| s.parse().unwrap())
        .unwrap_or(32768);
    let interval_us = std::env::args()
        .nth(3)
        .map(|s| s.parse().unwrap())
        .unwrap_or(1000);
    let repeats = std::env::args()
        .nth(4)
        .map(|s| s.parse().unwrap())
        .unwrap_or(5);
    let mut scenarios = vec![];
    for repeat in 0..repeats {
        for n in [16, 64, 256] {
            for state in ["w2-cold", "w2-warm", "w2-obstacle"] {
                let mut s = w2(n, state, allowance, interval_us);
                s["repeat"] = json!(repeat);
                scenarios.push(s);
            }
            for scenario in [
                "cold",
                "warm",
                "negative",
                "negative-warm",
                "opened-negative",
                "distant-obstacle",
                "relevant-obstacle",
                "distant-road",
                "first-road",
                "relevant-road",
                "fence",
                "terrain",
                "cancel",
                "overrun",
            ] {
                let mut s = run(n, scenario, allowance, interval_us);
                s["repeat"] = json!(repeat);
                scenarios.push(s);
            }
        }
    }
    // R01 requires >=1000 observations and >=5 repeated same-workload runs.
    // Meet that floor per agreed W2 actor group without oversampling 256 actors.
    for n in [16usize, 64, 256] {
        let required = repeats.max(1000usize.div_ceil(n)).max(5);
        for repeat in repeats..required {
            for state in ["w2-cold", "w2-warm", "w2-obstacle"] {
                let mut s = w2(n, state, allowance, interval_us);
                s["repeat"] = json!(repeat);
                scenarios.push(s);
            }
        }
    }
    let mut aggregates = vec![];
    for n in [16, 64, 256] {
        for name in [
            "w2-cold",
            "w2-warm",
            "w2-obstacle",
            "cold",
            "warm",
            "negative",
            "negative-warm",
            "opened-negative",
            "distant-obstacle",
            "relevant-obstacle",
            "distant-road",
            "first-road",
            "relevant-road",
            "fence",
            "terrain",
            "cancel",
            "overrun",
        ] {
            let rows: Vec<_> = scenarios
                .iter()
                .filter(|s| s["actors"] == n && s["scenario"] == name)
                .collect();
            let observations: u64 = rows
                .iter()
                .map(|s| s["observations"].as_u64().unwrap())
                .sum();
            let first_samples: Vec<_> = rows
                .iter()
                .flat_map(|s| {
                    s["first_service_samples_us"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                })
                .collect();
            let ready_samples: Vec<_> = rows
                .iter()
                .flat_map(|s| {
                    s["route_ready_samples_us"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                })
                .collect();
            aggregates.push(json!({"actors":n,"scenario":name,"repeats":rows.len(),"observations":observations,
                "pooled_first_service_wait_us":stats(&first_samples),"pooled_route_ready_wait_us":stats(&ready_samples),
                "p99_sample_sufficiency":if observations>=1000 && rows.len()>=5 {"sufficient count; no contention acceptance"} else {"inconclusive: requires >=1000 observations and >=5 repeats"},
                "worst_repeat_service_max_us":rows.iter().map(|s|s["first_service_wait_us"]["max"].as_f64().unwrap()).fold(0.,f64::max),
                "worst_repeat_ready_p95_us":rows.iter().map(|s|s["route_ready_wait_us"]["p95"].as_f64().unwrap()).fold(0.,f64::max),
                "worst_repeat_ready_p99_us":rows.iter().map(|s|s["route_ready_wait_us"]["p99"].as_f64().unwrap()).fold(0.,f64::max),
                "worst_repeat_ready_max_us":rows.iter().map(|s|s["route_ready_wait_us"]["max"].as_f64().unwrap()).fold(0.,f64::max)}));
        }
    }
    let long_cases = bounded_long_cases();
    let report = json!({"bounded_long_cases":long_cases,"baseline":"67f3a58b26b1f389bfbd5726ee87715ec1238be2","starting_revision":"0104c6f295835cb9850c1ae33773a291e3fbf06d","candidate":"R10 slice 3 working tree; final revision recorded by integration owner","comparison":"baseline has no cadence API; absolute scheduled evidence only, no speedup claim","status":"inconclusive for complete W1-W6; inspect sample assertions, including failures","clock":"actual Instant monotonic + sleep(next_due-now); fixed delay after finish; simulation clock not advanced","cpu":"real process user/sys measured separately with /usr/bin/time -p; busy is wall time, not CPU","memory_scope":"fixtures, query/index preparation, all service, cache, conversion/adopted paths, cancellation/drop; sample vectors excluded; requested heap not RSS","scenarios":scenarios,"aggregates":aggregates});
    std::fs::write(&output, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    let mut md = String::from(
        "# R10 slice 3 scheduled service evidence\n\nActual monotonic releases, not simulated round durations. Baseline 67f3a58 has no cadence API: absolute candidate evidence only. Complete W1–W6 acceptance remains inconclusive. Positions remain unchanged; actual first movement awaits R11. CPU user/sys are in the adjacent time log; busy is wall time. Preparation/cache/certificate/conversion/drop costs are included in memory and timed outside operation allowance.\n\n| Actors | Scenario | Observations | Worst service max µs | Worst ready p95 µs | Worst ready p99 µs | Worst ready max µs |\n|---:|---|---:|---:|---:|---:|---:|\n",
    );
    for a in report["aggregates"].as_array().unwrap() {
        md.push_str(&format!(
            "| {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.0} |\n",
            a["actors"],
            a["scenario"].as_str().unwrap(),
            a["observations"],
            a["worst_repeat_service_max_us"].as_f64().unwrap(),
            a["worst_repeat_ready_p95_us"].as_f64().unwrap(),
            a["worst_repeat_ready_p99_us"].as_f64().unwrap(),
            a["worst_repeat_ready_max_us"].as_f64().unwrap()
        ));
    }
    md.push_str("\nTable shows worst per-repeat quantiles. JSON also contains pooled p50/p95/p99/max computed from every recorded observation. Only scenario/actor groups with >=1000 observations and >=5 repeats meet the R01 p99 count rule. JSON retains every sample assertion/failure, preparation, queue/ready quantiles, overrun/lateness, work, restart/cache/cancellation, retained units and requested heap metrics. W1/W5/W6 transport/browser/save contention, W3 real actor blocked-work/cargo and first movement, large/chunk/world-size cases are separate bounded low-sample diagnostics in JSON; ongoing arrivals remain covered deterministically rather than by wall-clock contention. No threshold or baseline is changed.\n");
    std::fs::write(std::path::Path::new(&output).with_extension("md"), md).unwrap();
}
