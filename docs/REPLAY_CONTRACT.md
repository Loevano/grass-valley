# Replay contract v1 (R03 complete)

This defines the disposable-fixture format and comparison contract. Slice 2 adds
a [Python recorder/replayer](PYTHON_REPLAYS.md); slice 3 adds the
[backend-neutral execution interface and reports](REPLAY_BACKENDS.md). `tools/replay_contract.py` validates the format and
compares supplied observations without importing the game. `tests/replay/contract_v1.json` is a
fabricated **contract example**, not a recorded Python baseline. Its revision
identifies the Python source inspected when designing the format; no gameplay
result in that example is claimed as measured evidence. All three R03 slices are
complete; R04 adds a [transport-only Rust smoke adapter](REPLAY_BACKENDS.md#rust-transport-smoke-r04-slice-3).

Run `python3 tools/replay_contract.py tests/replay/contract_v1.json` for format
validation. The command points to `tools/python_replay.py` for Python execution
and reports that Rust supports transport smoke only, with gameplay and Rust-host
benchmarks unavailable; `tools/server_perf.py`
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

The format validator treats recipe parameters as JSON. The Python runner accepts
only its five versioned recipes and validates their exact parameter schema; see
[Python replay recipes](PYTHON_REPLAYS.md). Arbitrary names have no executable
support. Recipes must construct independent fresh worlds from public code, never read local saves, registries, provisioning, credentials, or environment
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
adventure means adventure service. The Python recipes freeze exact calls and a
zero clock origin, validate supported quanta (the format only requires nonnegative finite seconds), and avoid
double-advancing subsystems; see [time semantics](PYTHON_REPLAYS.md). Inspect
`server.tick`, the fast loop and economic loop, and adventure updates rather than
assuming that ticking once advances all clocks equivalently. Command steps do not
implicitly advance time. Reload serializes and reconstructs the independent world
using the supported migration path, retains stable cargo/position/task state, and
rebuilds derived routes. It must not accrue wall-clock offline production.

`random_draws` is an ordered list of `{site, method, args, value}`. `site` is a
stable subsystem/call-site label, `method` identifies the operation (e.g. `choice`),
`args` records its inputs, and `value` records the return. An empty list explicitly
means no gameplay draws. The Python runner intercepts covered gameplay draws and fails on missing, extra, reordered, wrong-site, wrong-method or wrong-argument draws;
a shared seed alone is insufficient. Constructor randomness and random origins
must be controlled too. Reload retains the trace cursor. Real terrain generation
uses the legacy Python-compatible generator and explicit seed/version rather than
substituting gameplay draws. Secure credential/ID entropy is excluded: use synthetic
stable identities without generating real accounts. The format validator itself does not execute random draws or clocks.

## Observation and comparison

`expected` has exactly `public` and `state`. Public observations hold selected
command results/errors or public snapshots; state holds selected authoritative
fields. The Python recipe projection is explicit; every field inside it compares, with
omissions documented in [Python replays](PYTHON_REPLAYS.md). Projection must expose ownership, cargo,
position, phase, rewards, and relevant accounting; do not omit inconvenient values.
Errors should retain their public type/status/message rather than exception stacks.
Paths are arrays of string object keys and integer array indexes rooted at this
observation, e.g. `["state","position","x"]`. No wildcards or implicit ID remapping.

`compare_observation(expected, actual, numeric_fields, invariants)` returns useful
field paths and invariant names on failure. It has no Python dependencies or
snapshot-building logic, so its checks are independent of gameplay implementation.
The Python adapter and backend-neutral harness supply observations to it.
[Protocol v1](REPLAY_BACKENDS.md) keeps expected data out of execution requests.
Rust supports a separate transport smoke request; these gameplay observations
remain unported.

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
across reloads. The Python recipe constants derive from independently stated rules, with
additional rate/continuity checks in `tests/test_python_replay.py`. Declare the physical inventory ledger carefully:
logical town stock, reserved buffers, and carried output can overlap and must not
be blindly added together. Snapshot parity alone does not prove correctness.

## Evidence and next slices

Slice 1 tests intentionally corrupt cargo, ownership, position, rewards, array order,
phase, timers and schema metadata; they exercise numeric limits, private fields,
nonfinite values and shared reference/candidate bugs. These are contract tests only.
No private save data was accessed. No game/runtime files changed.

Slice 2 records and repeatedly replays five Python traces covering relocation,
blocked/unreachable travel and recovery, reload mid-trip, exact-once cargo transfer,
workshop queues, food/stock accounting, multiple towns and a quarry adventure on
real generated terrain. [Python replay evidence](PYTHON_REPLAYS.md) records the
producing revision, limitations and environment. Slice 3 implements the
[backend-neutral replay interface and paired reporting](REPLAY_BACKENDS.md),
validates complete actual responses, and checks each run against the recorded
baseline and the other independent run. Rust remains explicitly unavailable.
R03 is complete; these fixtures do not fill every R01 audit gap.
