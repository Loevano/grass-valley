# Experimental Rust save snapshots

R19 slice 1 adds owned, immutable snapshot export and versioned reload at the
existing offline model boundary. This is not a durable playable-world save or
permission to use the Python data root. Python remains the playable default.

`dto::ModelSaveSnapshot::capture(&ModelWorld)` copies private facts under the
simulation owner. Release the owner before moving the snapshot to a worker and
calling `encode` or `write_to`. Capture does no JSON serialization or disk I/O.
It copies IDs/high-water marks, towns, buildings, actor positions/facing, cargo,
tasks, task/route generations, travel intents and private account/token records.
There is no implicit Debug/Serialize implementation or mutable field access.
Never send these snapshots or their encoded output to a browser or log.

`building::BuildingSaveSnapshot` additionally copies the construction owner's
landscape, town rules, inventories, linked groups, child progress, upgrades,
progression, helpers, builder claims and construction walks. Its nested model is
now an owned DTO rather than JSON serialized during capture. Existing synchronous
`encode_save` and `encode_building_save` wrappers remain available for offline
callers; a future save service must use capture and release the owner explicitly.
Snapshots are Send and can outlive the originating owner. Snapshot capture still
requires allocations proportional to the saved facts; no latency claim is made.

## Supported reloads

| Format | Reader versions | Export version | Boundary |
| --- | --- | --- | --- |
| `lantern-model` | 1, 2, 3 | 3 | Explicit model facts and private credentials |
| `lantern-building-model` | 1, 2 | 2, nested model 3 | Construction plus model facts |

`decode` validates through the existing strict readers and returns a normalized
current snapshot. `restore` returns a validated owner. Unknown fields/versions,
duplicate identities/keys, malformed references and invalid quantities remain
errors. Binary64 progress and quantities are not quantized. Model v1 has no saved
travel intent/facing: migration retains its position/cargo/task but defaults to
no travel intent and right facing, rather than inventing a missing leg. Versions
2/3 retain intents. Derived route searches, cached paths and playback rebuild;
construction-owned claim/walk facts retain their existing schema semantics.
Reload does not award goods or advance simulation time.

## Write failures and durability boundary

`write_to` emits private JSON to a caller-owned writer. It reports I/O error kinds
without the writer's potentially private error text. It neither flushes nor syncs,
renames, publishes, acknowledges, or closes that writer. Partial output must be
discarded. A failed writer cannot change either the snapshot or the live owner;
the same snapshot can be retried. Tests inject failure at every output byte and
block a writer while the originating owner continues to allocate IDs.

**No committed generation exists yet.** A complete JSON stream alone is not a
commit, and registry rename cannot make separately written chunks atomic. Slice 2
must assemble registry, all active/inactive worlds, private accounts/provisioning,
external sleeping/edited chunks, clocks/RNG and every supported gameplay owner
(including ledger-held cargo, trips and boat state) into one consistent generation.
The independent R14–R18 component codecs are not automatically included by these
two snapshot types. Do not claim that saving just ModelWorld persists all gameplay.
Slice 2 must implement one writer, flush/shutdown acknowledgement, file/directory
sync, publication, previous-generation retention and crash recovery, including
missing chunks and injected failures at each publication boundary. Those behaviors
are unimplemented and untested by this slice; truncated-JSON tests are not crash
recovery tests. Live host integration remains R20–21.

## Conversion and rollback

There is no Python-to-Rust playable conversion or Rust-to-Python reverse export.
The R06 `inspect-copy` CLI is read-only inspection, not conversion. Slice 3 must
use explicit source/destination paths, reject aliases/overlaps and existing
unsafe destinations, validate the complete supported save including external
chunks and private accounts, and publish only a complete new destination. It must
never overwrite or mutate the source. Experimental Rust must retain a separate
data root and refuse the normal Python root when a disk writer is introduced.

Rollback currently means running Python with the untouched original Python
registry, provisioning/accounts and entire chunk tree. That restores exactly the
Python backup's progress. All later Rust-side changes would be lost; no merge,
downgrade or transfer of post-conversion play is supported. Preserve the complete
source backup until reverse export has been separately implemented and verified.

## Slice 1 verification

Boundary tests cover current and legacy model readers, fractional positions,
work/travel/tasks/cargo, private credentials and monotonic IDs; construction tests
exercise snapshot restoration after paired Python construction steps, including
claims/walks/progress and legacy construction migration. Detached snapshot writes
are byte-identical to the synchronous encoders and succeed after owner destruction.
A blocked/failing background writer leaves authority usable and unchanged by I/O.
Every incomplete prefix is rejected; retry emits the complete original snapshot.
Compile-fail doctests prevent accidental serialization/logging of snapshot types.

Performance comparisons are not needed for this behavior slice: it introduces no
runtime save service or performance claim. Blocking-writer tests verify ownership
separation deterministically; integrated save contention remains R23.
