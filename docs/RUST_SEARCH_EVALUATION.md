# R09 slice 1: search backend evaluation

Starting revision: `8cc78b4e5420063e9c3349c4d7245bb687dd6aec`.
Candidate: the working tree carried by the commit containing this document and
[evidence.json](../rust/crates/lantern-host/examples/search_support/evidence.json).
Python remains the playable default. This slice adds an offline dev-only comparison;
no search implementation is yet linked into the authoritative Rust host or core.

## Decision

Promote the small incremental implementation to `lantern-core` in R09 slice 2.
Do **not** adopt `pathfinding` as a production dependency. Retain the pinned
`pathfinding = 4.16.0` and `num-traits = 0.2.19` as host dev dependencies so the
comparison stays executable. The library has lower observed medians in this noisy experiment; the decision
rests on the bounded-work and stable-discovery contracts, not a speed claim.
No new production dependency, host integration, scheduler, or cache is introduced.

The library version, API and source were checked on docs.rs and in the downloaded
crate source, rather than inferring resumability from a method name:

- [Package manifest](https://docs.rs/crate/pathfinding/4.16.0/source/Cargo.toml):
  version 4.16.0, Rust 1.88 minimum, Apache-2.0/MIT license. It builds on this
  workspace's Rust 1.99.0 toolchain. The crate uses `std`; no supported no_std
  feature was found. This evaluation covers macOS/aarch64 only; other release
  targets remain R23 gates. The comparison's local `Cost` implements total ordering
  for finite nonnegative f64 costs; no integer rounding changes road/diagonal costs.
- [A* source](https://docs.rs/pathfinding/4.16.0/src/pathfinding/directed/astar.rs.html):
  `astar` runs through completion and returns a path including its start. Its heap
  prefers larger accumulated cost for equal estimated cost, with no discovery
  serial. `astar_bag` searches before returning its solution-enumeration iterator;
  that iterator is not a suspended expansion frontier.
- [Dijkstra source](https://docs.rs/pathfinding/4.16.0/src/pathfinding/directed/dijkstra.rs.html):
  `dijkstra_reach` **does** retain a resumable frontier. The executable continuation
  test pauses after three items and reaches all 81 nodes without restarting. One
  `next()` expands successors before returning and can discard arbitrarily many
  stale heap entries internally. Its heap compares only cost, without stable
  discovery ordering. `dijkstra_partial` returns a parent map, not a frontier that
  can resume by feeding the result back. Dropping an iterator cancels it, but
  releases its state in one unmetered destruction operation.

The comparison tests demonstrate an actual equal-cost route difference between
this pinned library A* and the custom solver on the fixture family. Different
optimal routes in the **evaluation** are compared for legal edges, arrival and cost;
this is not permission to change the existing stable host tie contract. Adding
sequence and budget support around private library heaps would require replacing
or forking the very solver state being evaluated. The custom prototype already
retains that state explicitly, including stale heap pops and reconstruction.

## Executable boundary and coverage

`examples/search_support/mod.rs` contains actor-independent `Fixture`, `Graph`,
`Backend`, `Outcome`, and `Incremental` evaluation types. They are compiled only
by the example and integration test. All candidates consume the same immutable
private graph and ordered neighbor adapter, using R08 `SpatialIndex`,
`AccessProfile`, `edge_open`, `edge_cost`, and terrain/bounds rules. Preparing a
cold graph includes deriving occupancy/roads/fences, cloning the private terrain
and access data, and converting exact/footprint/multiple goals. There is no
mutation of snapshots, imported archives or live game state.

The custom prototype uses E,S,W,N then SE,SW,NW,NE neighbors, a serial after the
estimated-cost heap key, and strictly better-cost relaxation. Equal-cost earlier
discovery wins. No-road cardinal searches use zero heuristic to preserve Python's
BFS discovery behavior; weighted searches use the current Python source's
bounding-box lower bound for multi-goal/perimeter requests. Paths exclude start.
Arrived, unreachable, pending, cancelled and completed remain distinct. Each
custom advance charges a heap pop (including stale entries), predecessor visit,
or reversal swap; zero budget does no work and cancellation prevents resumption.
An expansion checks at most eight neighbors. This is an operation bound, **not** a
wall-clock deadline: allocation, tree/heap operations, setup and destruction still
need R10 admission/memory/service measurement.

Ten integration tests cover twelve representative 64×64 fixtures plus 48 varied
9×9 graphs: local cardinal, detour, unreachable, multiple goals, all three road
modes with diagonals, barn fences/perimeter access, unavailable chunk seams,
footprints, relevant/distant obstacle edits, void/bounds, diagonal side clearance,
farmer/construction acre-anchor subtraction including larger saved acre footprints
and overlapping occupancy, explicit goal sets, stable ties, quantum accounting,
cancellation, and actual library continuation. These are synthetic deterministic
fixtures, not production save data.

The independent reference uses dense distances/visited arrays and an O(V²)
minimum scan, no candidate heap/heuristic/successor/open/cost function. Its own
access and road/diagonal arithmetic checks reachability and costs within `1e-9`.
Returned candidate paths are checked edge by edge and summed independently from
the returned cost. Goal conversion and the already paired R08 low-level fence
intersection geometry are shared with the evaluation graph; direct expected
footprint goal-set and corner tests supplement this boundary. This is a search
oracle, not a second full geometry implementation or a Python gameplay replay.
Full backend promotion and Python route/access compatibility remain slices 2/3.
In particular, evaluation `Adjacent` always means an explicit saved footprint.
Python's raw single-anchor adjacent request is a distinct later adapter contract:
it checks Manhattan distance == 1 plus the fence before validating a blocked
start tile, and uses `max(0, Manhattan/octile distance to anchor - 1)` rather than
the explicit-perimeter bounding-box heuristic. Promotion must retain that separate
form, including blocked-start arrival and heuristic/tie behavior. Permitted-water
`AccessProfile.only` routes and absent ordinary terrain also need slice 3 parity.

## Release experiment

Run from the repository root:

```sh
cargo run --manifest-path rust/Cargo.toml --release -p lantern-host --example search_compare --locked --offline -- /tmp/r09-search.json
```

Recorded environment: macOS, aarch64-apple-darwin, Rust 1.99.0
`b940084d7` / LLVM 23.1.2. CPU model lookup was sandbox-restricted. All candidates
ran in the same process on this machine. Each fixture/mode received eight warmup
rounds followed by 41 paired rounds with rotating candidate order. Correctness
was checked before measurements. The committed JSON includes every sample,
p50/p95/p99/max, graph setup and search+conversion timings, expansions and work.
The high percentiles are exploratory observations from 41 samples, not W1–W6
acceptance measurements or confidence bounds.

Cold means rebuilding the derived graph per request. Warm reuses only that
immutable graph, not route results or a search frontier. Timing includes result
conversion (including removing the library start tile), search-state destruction,
and cold graph destruction. Fixture authoring, reference checks, process startup
and compilation are excluded. Separate untimed instrumented requests record
allocation/reallocation calls, cumulative requested bytes, and peak live requested
heap bytes, including conversion. JSON also records separate graph-setup allocation
measurements. These are not RSS, allocator metadata, or a hard memory bound.

Cold whole-request p50 in microseconds (lower is faster):

| Fixture | Incremental custom | Library A* | Library reach |
| --- | ---: | ---: | ---: |
| edited_on_route | 169.33 | 70.58 | 91.17 |
| edited_distant | 171.00 | 68.67 | 88.46 |
| local_cardinal | 161.21 | 64.67 | 82.67 |
| wall_detour | 1858.67 | 685.96 | 895.04 |
| unreachable | 1061.21 | 382.12 | 502.79 |
| multi_goal | 12.50 | 7.71 | 175.54 |
| roads_smooth | 108.62 | 66.29 | 151.92 |
| roads_departure | 104.58 | 66.92 | 154.00 |
| roads_arrival | 107.88 | 65.08 | 154.17 |
| barn_fence | 151.71 | 80.96 | 378.25 |
| chunk_boundary | 39.62 | 21.46 | 273.38 |
| footprint | 225.33 | 89.29 | 117.38 |

For the wall detour, cold custom p50/p95/p99/max were
1858.67/5580.50/5863.21/5863.21 µs; A* was
685.96/1426.46/1716.88/1716.88 µs. Warm p50 was 1839.96/681.21 µs.
Cold graph setup p50 was 4.08/4.25 µs. Custom/A* performed
3452/3460 successor expansions, 4689/3564 allocation calls, and peaked at
266987/159819 requested heap bytes. Custom counted 3626 work units, including
reconstruction; library work is only observable successor calls and cannot
include its hidden stale pops. Reach also expands the goal before yielding it,
whereas A* and custom stop before expanding goal successors. Allocation counting
was disabled in timed runs.

The final Rust experiment ran after all task tests and legacy measurements had
finished. Nevertheless, its raw samples show substantial host jitter (custom
detour p95 is about 3.0× p50). Timing is **inconclusive for stable latency or gain
estimates**. These are descriptive paired measurements; the lower observed A*
medians do not establish a general performance advantage or any acceptance budget.
Allocation and expansion counts are deterministic observations on these fixtures.
No extra run was selected to replace the noisy final samples.

The observed medians and allocation counts expose the current custom
prototype's cost relative to synchronous library A* on this fixture family. It is
not optimized and does not justify claiming a Rust host gain. The multi-goal/chunk scenarios also show
why a resumable zero-heuristic reach iterator is not equivalent to a resumable A*
backend: reach expands 522/594 nodes versus custom 18/49 there. No threshold was
introduced, loosened, or promoted to an acceptance baseline.

The experiment does not cover simultaneous admitted orders, town/worker queue
fairness, route-cache reuse/invalidation, queue-to-first-movement latency,
HTTP/save contention, process peak memory, arbitrary world sizes, or browser work.
W1–W6 require the later scheduler/host integration. Small fixture search speed
and a bounded number of explicit operations cannot establish those budgets.

## Validation and remaining work

Validation results and the required legacy Python/browser performance comparison
are recorded beside the R09 stage in [RUST_MIGRATION.md](RUST_MIGRATION.md#r09--search-and-library-decision).
The legacy harness measures Python/browser behavior only and provides no Rust
performance evidence. Its five-pair baseline/candidate run at starting revision
`8cc78b4` returned **inconclusive**: 126 comparisons passed, 25 were inconclusive,
and none regressed; all 790 scenario assertions and all 22 absolute frame budgets
passed (maximum reported scored frame p95 2.8 ms / 8 ms). The final affected Rust
checks overlapped part of that legacy run, an additional contention limitation.
Both its Markdown and JSON reports and per-scenario files were inspected at
`performance-results/r09-legacy-performance/`; this run cannot support a host
performance conclusion. No thresholds or baselines were changed. R09 slice 2 must
promote/validate the chosen solver in the no_std core; slice 3 must pair the route/access/cost contracts with Python. R10 owns
request identity, admission, scheduling, invalidation and service instrumentation;
R11 owns persistent worker travel and playback.
