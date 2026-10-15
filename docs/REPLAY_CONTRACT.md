# Replay contract v1 (R03 slice 1)

This is a disposable-fixture format and comparison contract, not a working game
replay runner. `tools/replay_contract.py` validates the format and compares supplied
observations without importing the game. `tests/replay/contract_v1.json` is a
fabricated **contract example**, not a recorded Python baseline. Its revision
identifies the Python source inspected when designing the format; no gameplay
result in that example is claimed as measured evidence. R03 slices 2 and 3 remain.

Run `python3 tools/replay_contract.py tests/replay/contract_v1.json` for format
validation. The command explicitly reports that Python replay/recording is pending,
Rust adapters and Rust-host benchmarks do not exist, and `tools/server_perf.py`
measures Python only. Validation is not replay, parity, or performance evidence.

## Trace envelope

Only JSON values are allowed; nonfinite numbers are errors. Version 1 requires:

| Field | Meaning |
| --- | --- |
| `format_version` | Integer `1`; incompatible changes require a new version |
| `id` | Nonempty, stable synthetic scenario name |
| `provenance.kind` | `contract_example` or `recorded_python`; only the latter is replay evidence |
| `provenance.python_revision` | Full 40-character commit that produced a recorded baseline; recording must require a clean relevant Python source tree |
| `initial.recipe` | Versioned, allowlisted synthetic setup name, never an executable expression, import name, or filename |
| `initial.parameters` | Explicit synthetic initial state/recipe inputs, town/entity IDs, terrain seed/version and all clock origins needed by that recipe |
| `initial.terrain` | `synthetic_flat` for isolated rules or `real_generator` for terrain/world coverage |
| `steps` | Ordered, nonempty list of operations, each with draws and observations |

Recipe names/parameters are currently descriptive. Slice 2 must implement their
allowlist and validate each recipe's parameter schema; arbitrary names do not imply
executable support. Recipes must construct independent fresh worlds from public
code, never read local saves, registries, provisioning, credentials, or environment
secrets. Use temporary data roots for any reload and discard them after each run.
Never invoke the normal `server.load`/`save` entry points against their default root.
The schema accepts no initial save path. Recursive validation rejects common private
field names without printing their values. This is defense in depth, not a general
secret detector: fixture review and synthetic-only construction remain required.

Terrain scenarios must exercise the actual requested generator and overlays with
explicit version/seed; do not replace terrain with a flat fixture or record only a
seed and assume Python/Rust RNG equivalence. Flat fixtures remain appropriate for
isolated cargo/production rules, following `tests/fixtures.py`.

## Ordered operations, time and randomness

Each step contains exactly `operation`, `random_draws`, `expected`, `numeric_fields`,
and `invariants`. Operations have one of these shapes:

```json
{"kind":"command","town":"town-a","data":{"action":"move","index":1,"x":8,"y":4}}
{"kind":"advance","clock":"movement","seconds":0.05}
{"kind":"advance","clock":"economy","seconds":3}
{"kind":"advance","clock":"adventure","seconds":0.05}
{"kind":"reload"}
```

These are ordered clock/service invocations, not implicit host sleeps. Movement
means host movement/courier service; economy means one three-second economic tick;
adventure means adventure service. Slice 2 must freeze the exact Python calls and
clock origins per recipe, validate supported quanta (the format only requires
nonnegative finite seconds), and avoid double-advancing subsystems. Inspect
`server.tick`, the fast loop and economic loop, and adventure updates rather than
assuming that ticking once advances all clocks equivalently. Command steps do not
implicitly advance time. Reload serializes and reconstructs the independent world
using the supported migration path, retains stable cargo/position/task state, and
rebuilds derived routes. It must not accrue wall-clock offline production.

`random_draws` is an ordered list of `{site, method, args, value}`. `site` is a
stable subsystem/call-site label, `method` identifies the operation (e.g. `choice`),
`args` records its inputs, and `value` records the return. An empty list explicitly
means no gameplay draws. Slice 2 must intercept every covered gameplay draw and
fail on missing, extra, reordered, wrong-site, wrong-method or wrong-argument draws;
a shared seed alone is insufficient. Constructor randomness and random origins
must be controlled too. Reload retains the trace cursor. Real terrain generation
uses the legacy Python-compatible generator and explicit seed/version rather than
substituting gameplay draws. Secure credential/ID entropy is excluded: use synthetic
stable identities without generating real accounts. Nothing in slice 1 intercepts
randomness or wall clocks yet.

## Observation and comparison

`expected` has exactly `public` and `state`. Public observations hold selected
command results/errors or public snapshots; state holds selected authoritative
fields. Slice 2 defines each recipe's explicit projections; compare every field
inside them, and document omitted fields. Projection must expose ownership, cargo,
position, phase, rewards, and relevant accounting; do not omit inconvenient values.
Errors should retain their public type/status/message rather than exception stacks.
Paths are arrays of string object keys and integer array indexes rooted at this
observation, e.g. `["state","position","x"]`. No wildcards or implicit ID remapping.

`compare_observation(expected, actual, numeric_fields, invariants)` returns useful
field paths and invariant names on failure. It has no Python dependencies or
snapshot-building logic, so its checks are independent of gameplay implementation.
The later backend interface supplies observations to it; no backend protocol,
Python adapter, or Rust implementation is introduced in this slice.

Normalization only canonicalizes object-key order and JSON whitespace. Arrays,
IDs, integer quantities, ownership, rewards, null versus missing, phases, timers,
and unannotated numbers compare exactly. Integral JSON numbers compare by exact
numeric value, without converting them through binary64 (including IDs above 2^53).
Booleans are distinct from numbers. Selected floating fields may declare precisely
one `numeric_fields` entry `{path, kind}`: `position` permits absolute error 1e-6
tiles; `fraction` permits 1e-9 in goods/work/practice/elapsed units (R01 D03). There
is no relative tolerance. Integer reference values stay exact even if annotated.
Use floating reference values for continuous positions. Unknown/duplicate tolerance
entries and nonexistent paths fail. An annotation never ignores a field or permits
phase/ownership/boundary changes. No intentional gameplay differences are approved.
A necessary future exception needs a named exact path, rationale and regression
case; do not add blanket ignores or automatically refresh failing goldens.

Invariants use `{name, kind, paths, value}` (`nonnegative` omits `value`):

- `equals`: every selected value equals the explicit literal (use ownership,
  reward ceilings or a known unchanged position/phase).
- `sum`: selected numeric quantities sum exactly to the explicit expected total.
  Use integer cargo conservation; fractional rounding needs a separately justified
  contract, not an implicit tolerance.
- `nonnegative`: all selected quantities are at least zero.

Invariants check **both reference and candidate**. A known cargo duplication bug
cannot pass just because both backends reproduce it. Per-step constants can express
conservation after specified production, consumption, or rewards and preservation
across reloads. Slice 2 must derive those constants from independently stated rules,
not copy them from Python output. Declare the physical inventory ledger carefully:
logical town stock, reserved buffers, and carried output can overlap and must not
be blindly added together. Snapshot parity alone does not prove correctness.

## Evidence and next slices

Slice 1 tests intentionally corrupt cargo, ownership, position, rewards, array order,
phase, timers and schema metadata; they exercise numeric limits, private fields,
nonfinite values and shared reference/candidate bugs. These are contract tests only.
No private save data was accessed. No game/runtime files changed.

Slice 2 must record and repeatedly replay Python traces for relocation,
blocked/unreachable travel and recovery, reload mid-trip, exact-once cargo transfer,
workshop queues, food/stock accounting, multiple towns and an adventure, preserving
real terrain in terrain scenarios. Each baseline records its producing revision;
inspect discrepancies as potential Python bugs before blessing them. Capture any
required environment/hardware evidence under the migration contracts without
claiming Rust measurements. Slice 3 implements the backend-neutral replay adapter
interface and paired reporting, with Rust explicitly unavailable until implemented.
