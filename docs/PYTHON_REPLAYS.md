# Python replay fixtures (R03 slice 2)

Five recorded synthetic scenarios replay on the unchanged Python game. This
completed slice 2; slice 3 adds the [backend-neutral execution/comparison
interface and paired reporting](REPLAY_BACKENDS.md). The pure comparator remains in `tools/replay_contract.py`.
The R04 Rust adapter supports transport smoke only; gameplay and Rust-host benchmarks remain unavailable. `tools/server_perf.py` imports Python
and cannot establish Rust performance. These are semantic fixtures, not load tests,
a complete public snapshot contract, HTTP tests, or legacy-save acceptance.

Run from the repository root:

```sh
python3 tools/python_replay.py
python3 tools/python_replay.py tests/replay/python/blocked_delivery_v1.json
python3 tools/check.py --tests tests/test_python_replay.py
```

Default replay runs every fixture twice in fresh worlds and never changes a baseline.
Every failure identifies the trace, step/operation and differing field or invariant.
The CLI explicitly reports missing Rust and Rust-host benchmarks. Its replay path
now delegates to protocol v1 through `PythonBackend`; recording retains its explicit
separate path. Format validation
alone remains available through `tools/replay_contract.py`.

## Provenance, initial state and recording

All five baselines record full Python source revision
`9637eb5bf982fa2342556feaa4059fcb3f0d4b17`. Recording checked that `server/` was clean
(including staged and untracked changes). Python game files have not changed in
this task. Recording tooling was developed in this slice; its implementation and
tests are committed with these fixtures, separately from the producing game revision.
The original `tests/replay/contract_v1.json` remains a fabricated format example.

Each allowlisted recipe accepts exactly `seed` (uint64, currently 42),
`setup_draws` (ordered constructor entropy) and `initial_state` (selected initial
state) inside `initial.parameters`. Initial state is re-created from the versioned
recipe and compared before step 0; it is never imported as a save. All constructors
currently need no global entropy, so `setup_draws` is empty. Recipe setup explicitly
uses real `server.new_town` defaults. Flat rule fixtures pass a synthetic shared
landscape to initialization, then use independent 16×16 towns without NPCs. Terrain
fixtures use real world generation, sites and NPC overlays. Initial recipe constants
(buildings, stocks, worker assignments/skills and positions) are versioned in
`setup` and independently visible in the recorded initial projection.

To deliberately record new baselines into an **empty** destination:

```sh
python3 tools/python_replay.py --record /private/tmp/new-python-replays
```

Recording captures results/draws, checks independent invariants, then replays every
new trace before writing any files. It refuses dirty Python game sources and any
nonempty output directory. Review changes and potential Python bugs before copying
new files into the fixture directory. Replay never automatically refreshes goldens.
No intentional gameplay differences or known Python bugs have been approved here.

## Scenarios and independent checks

| Recipe | Steps | Setup and exercised behavior | Independent evidence |
| --- | ---: | --- | --- |
| `relocation_v1` | 13 | One cutter carries 2 wood, returning from x=11.25/y=2 to camp 9/3. Move camp to 8/4, reload, advance return leg through drop-off and repeat service. | Position/cargo survive command and reload; physical ledger stays 65 stored + 2 carried = 67, then 67 stored; output/stock credited once. |
| `blocked_delivery_v1` | 7 | Hauler collects 3 wood from camp 10/3 for store 2/2. A preplanned saved trip encounters an impassable vertical wall at x=7 with a removable camp gate at y=10. Service twice while unreachable, reload, destroy gate, replan and deliver. | Hauler remains at 8/2 with picked-up cargo before reopening; no delivery while blocked; physical ledger stays 68; final store has exactly 3 and cargo clears. |
| `workshop_v1` | 17 | Workshop with 4 planks/2 wood, qualified worker at 9/8, first tools request 2 then request 1, reject amount 0, tick 3s, reload partial work, tick twelve times. | Invalid request leaves selected state unchanged; FIFO IDs/order and 3s work survive reload; exactly 2 tools produced/deposited, unfunded request of 1 remains. |
| `meals_v1` | 4 | Five idle residents, 40 untyped food and 65 wood; tick twice, reload, tick again. | Independent formula 40 − ticks × 5 × .6 × 3/60 agrees with logical food and physical hall food within 1e-9; reload consumes nothing and wood remains 65. |
| `terrain_quarry_v1` | 20 | Real 120×120 v1 world, seed 42, three real town sites, NPC overlays. Two founders enter quarry, operate plate/catch, reload party, solve sun/leaf/moon, collect treasure, retry. | Both members gain exactly 20 coins/one keepsake; spectator gains nothing; valley positions unchanged; party reload preserves state; duplicate collection changes no state and returns an error. |

Independent constants are authored in `plans` and tests, not copied from Python
output. Physical wood counts building stored/input/output inventories plus pending
worker output and picked-up, undelivered trip cargo. Building `pending_output` is
a mirror/reservation and is not counted twice; a delivered trip's retained cargo
manifest is not carried inventory. Town logical stock is checked separately.
Food uses the actual untyped `food` inventory key, not vegetables. Fractional rate
checks live in tests, separately from the recorded snapshots. The format's `equals`
invariant preserves literal JSON types, so a produced fractional quantity of 2.0
is written explicitly as 2.0; it is not a numeric tolerance or a gameplay exception.

## Time, randomness and reload

Clock origin is exactly zero in every recipe. No sleeping or host threads run.
`time.time` and `time.monotonic` return the explicit accumulated advances; commands
and reloads do not change that clock. Economy operations require exactly 3 seconds
and invoke `server.tick` once, including production, logistics and meals. They do
not also invoke movement service. Movement operations are **isolated rule service**:
`work_cycles.advance` plus drop-off for relocation, or `logistics.advance_trip` for
delivery. They do not run meals/production or promise equivalence to arbitrary
subdivision of the whole host loop. The quarry is command-driven; these recipes
reject adventure-clock operations rather than pretending a courier timer exists.
Other host services, pause/overrun behavior and courier timing are not covered.

Process-global gameplay entropy is intercepted with ordered `{site, method, args,
value}` records. Site includes Python module, function and source line: moving a
draw site requires explicit fixture review. Missing/extra/wrong-site/wrong-method/
wrong-argument draws and invalid result ranges fail; changed valid queue IDs also
fail snapshots. Currently the nonempty tapes are the two workshop queue ID draws;
they come from a local deterministic MT19937 stream, never OS credential entropy.
Local `random.Random` is permitted only for explicitly seeded terrain/NPC modules;
real terrain draws are **not mocked**. Uncovered global random methods, unseeded
local RNGs, credential entropy, wall-clock sleeps and live save entry points fail.
This is coverage for these recipes, not a universal random interception API or a
frozen cross-language gameplay RNG algorithm. Fishing/departure stochastic behavior
and other generator versions need future coverage before their Rust migration.

Reload writes only `export_world` JSON into a fresh temporary directory, reads it
back, binds the shared map, migrates footprints and ensures resource/name defaults.
That exercises the existing serialization and relevant migration helpers while
excluding global registry/account activation, chunk persistence and provisioning.
Each run has its own shared navigation cache, reset again on reload and restored
on exit. No `server.load`, `save` or `activate` call is allowed. Nothing touches the normal
save root, private saves or credentials; temporary files are deleted even on failure.
Reload retains the random stream/tape position and explicit clock. No offline
production accrues. Full persistence/import compatibility remains R06/R19 work.

## Selected observations and omissions

Public results contain the actual domain-command result (including JSON null) or
`ValueError` type/message. These are direct domain calls, not HTTP status evidence.
State projects each town under its literal owner ID, with wood/food/coins, goods,
produced totals, the complete founder/player value, and every building's identity,
position/footprint, construction, inventory buffers, recipe and craft queue when
present. Worker fields cover name/job/assignment, carried output/workplace, logistics
position/full trip, partial production, production status and relevant practice.
Animation projections retain x/y, phase, task, elapsed, path and resume phase.
Other worker attributes, animation playback history, UI catalogs, diagnostic caches,
spatial indexes and unrelated progression settings are omitted; these fixtures do
not establish parity for omitted fields. Absent aggregate goods/produced dictionaries
project to empty logical inventories; within selected native structures, missing
fields remain absent and compare distinctly from null or an empty value.

Terrain initial state records all 120 rows; subsequent observations compare their
exact canonical SHA-256 plus seed/version, resource count and canonical resource
checksum, NPC count and town sites. Hashing is an exact, bounded whole-landscape
check, not a tolerance; a mismatch names the affected terrain/resource section but
needs a separate generation diff to locate the tile/resource. NPC content, unvisited
chunks and other terrain versions are outside this fixture's projection. This does
not substitute for R07 terrain goldens. Arrays/routes stay ordered. Only logical
food in the meals trace uses its named 1e-9 fractional tolerance; other observed
numbers currently compare exactly, including positions, phases, cargo and rewards.

## Validation and environment

The tooling tests replay each baseline twice, reproduce recordings, independently
check accounting/continuity/queues/rewards, inject corrupt runtime observations,
reject corrupted draw tapes and invalid parameters/time/terrain, and verify private
save/global isolation and refusal to overwrite baselines. The initial new food test
incorrectly assumed vegetable stock; inspection showed untyped `food` inventory,
and the assertion was corrected. No application test or threshold was weakened.

Recorded on Apple M1 Max, 64 GiB RAM, macOS 26.7 arm64, Python 3.9.6, AC power.
Python source is interpreted (no release/debug binary); no compiler or Chromium was
used for these offline semantic traces. Power mode and browser/compiler metadata
for release performance acceptance remain unmeasured. Final affected validation: `python3 tools/check.py` selected the two tooling test
modules and passed all 25 cases. Markdown/JSON reports are
`performance-results/20261016-013951/report.{md,json}` (ignored local artifacts).
The standalone CLI also replayed all five traces twice (61 operations per pass). Performance checks
were not run: this is new replay tooling/fixture coverage, with no performance
investigation or optimization. No Rust performance, load budgets or LAN latency
claim is made. R03 slice 3 is complete; the other R01 audit gaps remain open.
