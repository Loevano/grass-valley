# R09 search backend evaluation

**Current status:** R09 is complete at the experimental core boundary: the custom
no_std backend matches actual Python route/access/cost contracts; see
[the slice 3 audit](#r09-slice-3-python-compatibility-and-current-remaining-work). The slice 1 text and
`evidence.json` below describe the historical prototype at `9426ef4` (the
completed slice 1 revision). Use that revision for exact slice 1 reproduction.
The current example invokes core Search and prepares an owned query, so rerunning
it at a later revision does not reproduce the historical setup costs.

## Historical slice 1: backend evaluation

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
At slice 1, backend promotion and Python route/access compatibility remained slices 2/3.
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

## Slice 1 validation and historical remaining work

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


## R09 slice 2: promoted core backend

Starting revision: `9426ef493942802293412948f146570b1b5e6c20`.
Candidate: the working tree in the commit carrying this section. The slice 1
production dependency decision is unchanged. No production search dependency,
HTTP/save behavior, host scheduler, or playable-default change is introduced.

`lantern-core::search` now owns the chosen implementation. `Query::new` validates
bounds before creating retained search state; typed geometry/cost modes exclude
invalid dimensions and nonfinite caller costs. Query owns immutable terrain,
access and targets and shares immutable derived `SpatialIndex` through `Rc`.
`Search::new(Rc<Query>)`, `advance(quantum)`, `outcome()`, `cancel()` and consuming
`into_outcome()` form the actor-independent interface. There is no borrowed
self-reference to a host owner. `Rc` deliberately restricts this version to a
single owner thread; parallel workers require an explicit later boundary decision.

Exact, raw-anchor-adjacent, explicit saved-footprint perimeter and multiple goals
are separate inputs. Raw adjacency checks Manhattan distance one and the final
fence before start passability; its weighted heuristic subtracts one from the
anchor metric. Explicit perimeters subtract actor occupancy and reject fenced
footprint edges, then use a goal bounding box. The implementation uses R08's
access, terrain, edge and cost rules, including acre-anchor subtraction from the
whole occupancy union, exemptions, permitted-water `only` sets, diagonal side
clearance, void/missing terrain and inclusive bounds. Checked neighbor arithmetic
and i64 distance differences support the entire i32 coordinate representation.
The heap orders estimated cost then discovery serial; strictly better relaxation
and E,S,W,N then SE,SW,NW,NE order preserve stable ties, including cardinal BFS
ordering when no roads exist. Paths exclude start and all five outcomes remain
distinct. These are implemented semantics; exhaustive actual Python compatibility
is still slice 3, not established by the synthetic core tests.

Each advance charges stale or current heap pops, predecessor visits, reverse swaps
and terminal phase checks; each expansion examines at most eight edges. A zero
quantum performs no work. Cancellation immediately releases heap/predecessor/cost/
path storage and cannot resume. Consuming the job moves the result path and drops
all remaining frontier storage. Allocation and tree/heap/fence operations, query
preparation, cancellation and destruction have no wall-clock deadline. R10 must
account for these costs and add memory/admission caps, dependency tracking
(including examined terrain, diagonal side tiles and fences), validity checks,
and refresh after proven-irrelevant edits. This slice intentionally freezes query
data and does not change an in-flight query.

The evaluation's `Incremental` is now an alias for the core Search; its disconnected
prototype was removed. The ten existing integration tests run actual core search
against the independent dense/scan Dijkstra and retained library comparators on
12 representative and 48 varied small graphs. Eight new core cases verify quantum
partition equivalence and zero work, forced consecutive stale pops, cancellation
storage release, raw/perimeter heuristic differences, arrival before passability,
permitted-water/exemption ownership, fence-sensitive arrival/perimeter admission,
terrain validation and extreme coordinates. Goal conversion and fence primitives
retain the oracle-sharing limitations documented in slice 1.

Remaining at slice 2 (completed below): slice 3 must pair actual Python `RouteCache.route/search_steps` and actor access
contracts over exhaustive and adversarial fixtures, including exact returned tie
paths and all cost modes. R10 and R11 retain scheduling/cache and worker travel.


### Slice 2 validation and release evidence

`python3 tools/check.py --list` selected the two Rust crates. The final affected
run (`--output performance-results/r09-slice2-correctness`) passed all six fmt,
test and clippy commands: 42 core and 138 host tests/doc tests. Both Markdown and
JSON were inspected, with no failing commands. `git diff --check` passes.

After correctness completed, run from `rust/`:

```sh
cargo run --release -p lantern-host --example search_compare -- /private/tmp/r09-slice2-search-raw.json
```

[Core-only JSON evidence](../rust/crates/lantern-host/examples/search_support/evidence-core.json)
retains the 24 `backend == "custom"` rows (12 identical fixture families × cold/warm)
from the raw 72-row example output, annotated with this scope. To inspect a fresh
run's equivalent rows, filter its JSON `rows` on `backend == "custom"`. The
starting revision and candidate description are in the artifact. Historical
`evidence.json` is unchanged. The harness still rotates all three backends across
41 rounds after eight warmups, but this slice makes **no new library cost
comparison**: shared cold preparation now additionally builds the owned core
query/terrain for every backend, so current library cold costs cannot describe
library-only setup. The retained core cold setup also includes duplicate evaluation
fixture/goal data; it measures this complete adapter request, not minimal pure-core
preparation. Warm requests reuse the immutable Query. Totals include search,
result transfer and destruction; allocations are measured separately from timings.

On macOS/aarch64, cold core wall-detour total p50/p95/p99/max was
2456.12/3630.08/4812.92/4812.92 µs; warm p50 was 2330.75 µs. Cold setup p50 was
136.08 µs and search/result-transfer p50 was 2308.25 µs. It expanded 3452 nodes
in 3626 work units; cold/warm allocation calls were 1623/1167 and peak requested
live heap bytes were 326715/264872. These values include the adapter scope above
and are not process RSS. Every fixture passed independent reachability/cost
checks before timing. The experiment has no absolute acceptance budgets or
baseline/candidate speed comparison. Raw samples show substantial jitter (for
example local-cardinal cold p95 1008.92 µs versus p50 356.96 µs), so timing is
**inconclusive for stable latency or improvement**, rather than an acceptance pass.
No thresholds were introduced or relaxed.

The legacy Python/browser performance suite was not repeated: this is backend
promotion, with no optimization or production-host performance claim; that suite
cannot measure the promoted Rust solver. Slice 1's required legacy run and its
inconclusive result remain historical evidence. The release experiment above
measures this slice's actual Rust implementation, outside simultaneous checks.


## R09 slice 3: Python compatibility and current remaining work

Starting revision: `98e6eceea2870a996e4b576592d067a77225ee63`. The selected custom
core backend matches 418 frozen actual Python `RouteCache.route` cases. See the
[completion audit and validation](RUST_MIGRATION.md#r09-slice-3--actual-python-parity-and-completion-audit).
The fixture generator's `--check` mode recomputes production Python routes and an
independent dense minimum-scan Dijkstra without updating the frozen JSON. The oracle
has separate search, access and cost arithmetic; it shares Python perimeter/fence
geometry, covered independently in R08. Exact deterministic paths and terminal
states are mandatory; costs use absolute `1e-9` tolerance, with no coordinate/path
normalization or permissive alternative-path comparison.

The 288-way matrix crosses targets, actor access, directed road modes, diagonal
travel and terrain/bounds. Another 96 varied topologies and focused cases exercise
arrival/blocked starts, final fences, actor-specific perimeter admission, absent and
missing ordinary terrain, actual sparse loaded/unloaded chunks, real pier/water
helpers and multi-town walking-map assembly. All cases run four quantum partitions,
zero-work advances and pending cancellation. Empty ordinary terrain (`[]`) maps to
`Terrain.cells=None`; a truthy grid with empty cells maps to `Some` with missing cells,
which is permissive unbounded and unavailable bounded. Unloaded chunks map to `v`.
Actual caller-owned water/exemption inputs do not port fishing gameplay or silently
change the builder fallback NPC-anchor behavior. Private bytes/indexes remain intact.
No concrete core discrepancy was found, so runtime code is unchanged.

R09 is complete at the experimental core boundary. R10 must still supply request
identity/generations, examined dependencies, cache invalidation, admission/fairness,
memory caps and service cadence. R11 owns persistent travel/playback; later stages
own gameplay planners, authoritative HTTP/save integration and platform validation.
The prior decision/release evidence files are untouched and remain inconclusive for
stable latency/gain estimates. No performance rerun is needed for this parity-only
test/documentation slice; no W1–W6 or production Rust speed claim follows.

Reproduce frozen contracts independently of Cargo:

```sh
python3 rust/crates/lantern-host/tests/fixtures/search/generate.py --check
cargo test --manifest-path rust/Cargo.toml -p lantern-host --test search_python --locked --offline
```
