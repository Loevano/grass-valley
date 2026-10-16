# Backend-neutral replay and reports (R03/R04)

R03 is complete. `tools/replay.py` defines a versioned JSON execution boundary,
independent validation/comparison, and JSON/Markdown reporting. The working Python
adapter runs the five unchanged [recorded scenarios](PYTHON_REPLAYS.md). R04 adds
a Rust stdio adapter for a transport smoke fixture only. Gameplay recipes remain
unsupported; no Rust-host benchmark or gameplay parity evidence is established.

From the repository root:

```sh
# Two independent fresh Python worlds: semantic repeatability only.
python3 tools/replay.py --candidate python --output performance-results/replay-python

# Python runs; Rust gameplay is unsupported. Expected exit code 2 (inconclusive).
python3 tools/replay.py --output performance-results/replay-rust

# A single trace can be named explicitly.
python3 tools/replay.py --candidate python --output performance-results/replay-one tests/replay/python/blocked_delivery_v1.json
```

The CLI writes `report.json` and `report.md` under the requested output directory.
These are derived reports; it never modifies or refreshes trace fixtures. Default
inputs are the five recorded Python traces. Fabricated format examples cannot be
used as recorded baselines. An invalid fixture is reported without running either
adapter. Existing `tools/python_replay.py` commands remain supported; ordinary
replay now delegates to the neutral interface, while explicit recording retains
its separate path and clean-source/no-overwrite checks.

## Execution protocol v1

A `ReplayBackend` has a nonempty `name` and a `replay(request) -> response` method.
The interface is a Python structural protocol with JSON-only arguments/results;
the Rust adapter translates the same envelopes to the offline Rust executable. The neutral
module imports no game modules; the CLI loads Python lazily. Callers may supply
independent adapters without changing snapshot comparison or importing Python rules.
No dynamic imports or user-provided executable paths are accepted by the CLI.

Each request has exactly:

```json
{
  "protocol_version": 1,
  "trace_id": "scenario-name",
  "initial": {
    "recipe": "versioned-recipe-name",
    "terrain": "synthetic_flat",
    "parameters": {"seed": 42, "setup_draws": []}
  },
  "steps": [
    {"operation": {"kind": "advance", "clock": "economy", "seconds": 3}, "random_draws": []}
  ]
}
```

The parameters are exactly uint64 `seed` and ordered `setup_draws` for the existing
versioned recipe family. Operation and draw validation is shared with the trace
format; [time/randomness rules](REPLAY_CONTRACT.md) remain in force. New parameter
families require an explicit protocol change. Recipe/terrain support is the
adapter's responsibility; Python retains the five-recipe allowlist and exact
clock semantics. An unsupported adapter/recipe may raise `BackendUnavailable`.

The request contains **no expected initial state, step observations, invariants,
tolerances or baseline provenance**. `request_for` validates the trace and strips
those oracle fields before execution; adapters cannot pass by returning expected
data received from the harness. Every invocation gets an independent deep copy;
an adapter cannot mutate another invocation's request or the recorded fixture.
Baseline assertions stay outside adapters.

A successful response has exactly:

```json
{
  "protocol_version": 1,
  "trace_id": "scenario-name",
  "initial_state": {},
  "observations": [
    {"public": {"result": null}, "state": {}}
  ]
}
```

Those empty objects illustrate envelope shape only; actual state must match the
recipe's selected projection. The response contains an actual initial state plus
exactly one ordered `{public, state}` observation per requested operation. Both
initial state and observation state are objects; public results preserve their JSON
shape. Wrong protocol, wrong trace ID, missing/extra observations or envelope fields,
nonfinite data, and known private fields fail before comparison. Short responses
cannot silently pass through `zip`. The adapter must consume every supplied draw
and reject unexpected/mismatched draws; the existing Python tape enforces this.
Domain-command rejection remains a public observation, separate from adapter
execution errors. Unexpected adapter exceptions produce failures, not command
results or availability passes.

## Independence, comparison and reporting

The harness invokes reference then candidate sequentially, with separate fresh
worlds. Python's context restores patched entropy, clocks and global route-cache
bindings after each call, including failure. Both runs complete before comparing;
there is no simultaneous Python execution under process-global patches. Neither
backend calls the other, imports the other backend's state, or shares its world.
Python reload keeps the disposable temporary-data-root and migration restrictions
from slice 2. Normal host save/account functions remain forbidden. No private save,
credential, registry or provisioning data is read. The JSON private-field checks
are defense in depth, not a universal secret detector.

For each trace the report includes three separate checks:

1. Reference actual initial state and every observation versus the recorded Python
   baseline, including independent per-step invariants.
2. Candidate actual initial state and observations versus that same baseline and
   invariants, even when the reference check fails.
3. Reference actuals versus candidate actuals, including invariants on both sides,
   when two structurally valid complete responses exist.

Matching backend bugs do not turn an overall result into a pass: baseline failures
and independent invariant failures remain failures. No intentional differences,
new ignores, tolerances, or baseline updates were introduced. Initial state remains
exact; step comparisons reuse only their existing field-specific annotations. The
[projection/coverage limits](PYTHON_REPLAYS.md) remain unchanged. Full HTTP snapshots,
all terrain versions/chunks, stochastic fishing/departure outcomes, complete save
imports, latency/load budgets, and other R01 audit gaps are not established here.

Report schema version 1 contains mode, overall status, baseline revisions per trace,
runner HEAD and dirty-tree flag, separate reference/candidate/comparison summaries,
completed step counts for validated runs, and failures with trace, initial/step,
operation, field path and invariant context. Reports omit raw whole observations;
invalid private fixtures/responses do not copy sensitive payloads into the report.
Unknown adapter exceptions report their type without arbitrary exception payloads.
CLI reports use this repository's HEAD/dirty state for the executing harness/Python
source tree; the Rust adapter builds this same working tree with locked offline Cargo, rather
than accepting an external binary path.

| Outcome | Overall status / exit | Meaning |
| --- | --- | --- |
| Both runs and paired comparison pass | `pass` / 0 | CLI Python/Python mode establishes repeatability only |
| Invalid input/response, snapshot/invariant mismatch, or execution failure | `failed` / 1 | Any failure takes precedence over missing adapters |
| Adapter unavailable and no other failure | `inconclusive` / 2 | Missing work never counts as parity or performance passing |

The Rust adapter raises `BackendUnavailable` for unsupported workloads and never
falls back to Python. Missing commands, build/runtime failures, timeouts, and
invalid responses produce failures; they are distinct from unsupported gameplay.
The CLI and both report formats state that Rust supports transport smoke only.
JSON records `rust_adapter=transport_smoke_only`, `rust_host_benchmarks=unavailable`,
and `performance=not_run`. An unsupported workload leaves paired comparison
inconclusive even if Python passes its baseline. This capability field describes
the implementation, not a successful invocation on the current machine.

## Rust transport smoke (R04 slice 3)

`tests/replay/rust/transport_smoke_v1.json` is a **request fixture**, not a
recorded Python trace or gameplay baseline. It carries no expected state,
provenance, tolerances, or invariants. Do not feed it to the trace-comparison CLI
as a recorded baseline; that CLI correctly rejects it. Run from the repository:

```sh
cd rust
cargo run --quiet -p lantern-host --locked --offline -- replay < ../tests/replay/rust/transport_smoke_v1.json
```

The CLI uses [Serde strict deserialization](https://serde.rs/container-attrs.html)
and [serde_json](https://docs.rs/serde_json/1.0.151/serde_json/) in the host package.
It accepts one protocol v1 request on stdin. Only recipe `transport_smoke_v1`,
terrain `synthetic_flat`, seed 0, no setup/step draws, and zero-time movement
advances are supported. This exercises serialization and ordered response counts;
it constructs no world and advances no simulation. Initial and observation state
are exactly `{"recipe":"transport_smoke_v1","gameplay_ported":false}`; each public
observation is `{"result":null}`. The core remains an empty, dependency-free
`no_std` crate. No listener, save writer, or Python gameplay bridge is added.

Success emits one protocol v1 response with matching trace ID and one observation
per step, exiting 0. Invalid JSON/schema/protocol/private fields exit 1; valid but
unsupported recipes, nonzero time, commands, reloads, or random draws exit 2.
Failures emit no JSON response. Error text never echoes request data. The CLI
requires the explicit `replay` subcommand and accepts no file/save path arguments.

`RustBackend` validates requests, invokes a fixed locked/offline `cargo run` in
`rust/` with a 300-second timeout, and validates responses before returning them.
It accepts no executable path and sends no oracle fields. The existing comparison
CLI now uses it; all five Python gameplay fixtures still report unsupported Rust
and inconclusive comparisons. Python/Python checks remain repeatability checks.
The Rust transport is independently smoke-tested through the executable and
through the real adapter; unsupported and invalid paths also have tests.

R04 validation: `python3 tools/check.py --output performance-results/r04-slice3`
passed 43 Python cases and all six Cargo format/test/Clippy commands, including
six host unit/integration cases. The standalone smoke request succeeds. The
`performance-results/r04-slice3-gameplay/` reports show all five Python baselines
passing and Rust gameplay unsupported/inconclusive (CLI exit 2). JSON/Markdown
were inspected. No fixture baselines were refreshed, no gameplay parity is
claimed, and no performance comparison was needed for transport scaffolding.

## R03 evidence (historical)

The new tests use clearly labeled fabricated backend doubles to check the protocol
and oracle without a Rust implementation. They also pair the **real Python adapter**
across every committed fixture, check all input/output isolation and report paths,
and exercise CLI exit codes 0/1/2. Deliberately corrupted cargo, ownership, position
and rewards produce trace/step/field failures. Shared cargo-accounting bugs fail
independent conservation even when both supplied backend outputs agree. Schema,
privacy, missing observations, wrong trace IDs, random tape and private-save guards
are covered alongside the existing slice 1–2 tests.

The existing runtime-corruption test was adjusted to inject only the first step;
batch execution collects subsequent observations before comparing and a completed
trip no longer has cargo to corrupt. It still requires a useful step-0 failure
for every corruption. No application behavior or baseline changed.

Final affected correctness: `python3 tools/check.py` passed all 38 cases across
three modules; reports: `performance-results/20261016-014504/report.{md,json}`.
CLI reports under `performance-results/r03-slice3-python/` pass all five paired
Python traces; `performance-results/r03-slice3-rust/` records all five comparisons
inconclusive with Rust unavailable (exit 2). These are ignored local artifacts.
Runner revision was `049fe101d3a11ffec7d999682dc6e477ce3f83ec` with this slice's
uncommitted tooling diff; baseline producing revision remains
`9637eb5bf982fa2342556feaa4059fcb3f0d4b17`.
Performance checks were not run: this is semantic replay tooling, with no performance
investigation or optimization. These R03 results predate the R04 transport smoke
implementation above. Gameplay parity and broader release gates remain unfinished.
