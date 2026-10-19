# Experimental Rust saves

R19 slices 1–2 implement private snapshot export/versioned reload and a durable
native generation store at the offline Rust model boundary. Python remains the
playable default. There is no live Rust host save loop or Python conversion CLI.

## Capture and ownership

`ModelSaveSnapshot::capture` and `BuildingSaveSnapshot::capture` copy private facts
without serialization or I/O. `WorldSnapshot::capture` / `capture_building` adds a
copy of every admitted `TownState`; `RegistrySnapshot::capture` binds all active
and inactive worlds and the active selection. Capture them in one owner turn,
then release authority before submitting to the background writer.

A TownState includes its physical ledger (stores/input/output/pending/held cargo,
coins, caps, produced totals, sales and activity), logistics/market trips,
woodcutters/claims, processors/workshop requests, fields, orchards, fishing/piers,
boat voyages, exploration boat, livestock/butchery, population and social state.
Standalone worker practice/talents/tools are included; workers already owned by
Population must not also appear in the standalone worker map. Credentials and
monotonic IDs are part of the model snapshot. The construction variant returns a
single BuildingWorld authority on restoration, rather than a second mutable model.

Callers must supply all of their admitted owners. Optional owners mean that the
subsystem has not been admitted, not permission to omit live state. Explicitly
attach the world name, shared tree/water/iron supplies, simulation/economic clocks,
named random streams, terrain archive and VillageLife where present. These APIs
still require a future aggregate gameplay host adapter in R20–21; they do not
make previously unported horses/adventures playable.

`ChunkLifecycle::capture_save` copies active state and the entire imported sleeping
archive without draining pending intents or changing the live owner. The writer
flushes that detached copy, with current active records overriding archived ones.
Derived inline active resource/stump aggregates are omitted from the saved terrain
payload; external chunk records own those facts. Camera/cache copies do not replace
saved authority. Exact clock/number lexemes and private extensions are retained.

Snapshots have no implicit Debug/Serialize and no mutable field access. Their
encoded output contains credentials: never send it to a browser or log. Capturing
still allocates/copies in proportion to saved facts; no latency claim is made.

## Supported native formats

| Format | Reader versions | Export version |
| --- | --- | --- |
| `lantern-model` | 1, 2, 3 | 3 |
| `lantern-building-model` | 1, 2 | 2, nested model 3 |
| `lantern-registry` | 1 | 1 |
| `lantern-generation` manifest | 1 | 1 |
| Ledger DTO | 1 | 1 |
| Named RNG stream | `python-mt19937-v1` | Same; 624 words and index |

Aggregate load uses every subsystem's existing strict codec and validates owners,
references, complete town coverage, chunk membership, inventories, clocks and RNG
state. Unknown/malformed schemas and missing or duplicate references reject the
entire generation. Binary64 progress/goods are not quantized. Private account
records and token mappings survive; optional provisioning source bytes can also be
archived exactly with `with_provision_archive`. That archive validates as a private
JSON object but is not applied as native authentication; semantic source-account
validation and merging belong to explicit conversion in slice 3.

Model v1 has no saved travel intent/facing: migration keeps position/cargo/task but
defaults to no intent and right facing rather than inventing a missing leg.
Versions 2/3 retain intents. Route searches, cached paths and playback rebuild;
construction-owned claims/walks retain their existing schema semantics. Reload
neither awards goods nor advances any simulation clock. Supplied RNG streams resume
exactly; automatic gameplay-stream adoption remains host integration work.

## Dedicated root and commit protocol

`Store::create(path)` requires a **new** directory. `Store::open(path)` requires the
Rust marker; existing Python/unmarked directories and root symlinks are refused.
No source path is inferred or discovered. On Unix, roots/generation directories are
0700 and files 0600. An OS file lock admits only one writer, releases on process
exit, and does not depend on deleting a possibly stale lock file.

Each save is self-contained:

1. Encode and validate the detached registry and complete chunk set on the writer.
2. Create a fresh `pending-N` directory, with a never-reused monotonic N.
3. Write numbered registry/chunk files, then `sync_all` each file.
4. Write/sync `manifest.json`, recording roles, owners/keys, lengths and SHA-256
   digests. Filenames derive from indices; private IDs never become disk paths.
5. Sync the pending directory, rename it to `generation-N`, then sync the root.
6. Only then acknowledge success.

The directory rename is the publication boundary. All references resolve inside
that immutable directory, so registry and chunks cannot come from different
commits. A failure before publication leaves an ignored pending directory. A crash
or error after rename but before acknowledgement may leave the complete new
version visible; acknowledgement is intentionally conservative. The previous
committed generation is never modified. OS-reported sync/rename semantics on local
Unix filesystems underpin durability; tests simulate abrupt process exit, not
physical power loss or storage-controller faults. Release/soak qualification
remains R23.

`Store::recover` chooses the newest fully valid committed generation. It checks
every file, digest, reference and schema, rejects missing/extra/symlinked files,
and reports skipped generations in `Recovery.skipped`. It falls back as a complete
unit; it never borrows missing chunks from another generation or regenerates them.
If committed directories exist but none is usable, recovery errors rather than
inventing an empty world. Valid pending directories are still uncommitted and are
ignored. All old/pending directories are retained; inspection, repair, selection
and cleanup tools remain slice 3.

## Background save, flush and shutdown

`SaveService::start(store)` owns the store on one background thread. `submit` uses
one pending queue slot and returns Busy on overload; a rejected snapshot is not
saved and must be recaptured/retried. A SaveTicket supports nonblocking `try_result`
for owner turns, or `wait` outside authority. A successful ticket identifies the
synced committed generation. Only acknowledge the captured revision, not later
mutations. Do not wait on tickets or call synchronous Store::commit in the
simulation owner.

`flush` is a FIFO barrier for all earlier accepted requests and reports any save
failure since the previous barrier, including dropped tickets. `shutdown` closes
admission, drains accepted work, joins the worker, and reports failures not already
reported by a barrier. Invoke both outside authority. Shutdown does not capture a
final snapshot itself: submit the final owner revision first. Dropping the service
closes the queue but does not join/block the owner; explicit shutdown is required
before process exit if accepted work must finish.

## Conversion and rollback boundaries

Slice 3 remains unfinished: no Python-to-Rust playable conversion, Rust-to-Python
reverse export, or recovery/rollback CLI exists. `inspect-copy` remains read-only.
Conversion must take explicit source/destination paths, validate all supported
source data and private account/provisioning records, include every world's external
chunks, reject aliases/overlaps, and publish a complete new destination without
mutating the source. Experimental Rust always uses its separate marked root.

Restarting native Rust restores the selected complete generation. Any mutations
after its captured revision are lost. Fallback loses all progress after the chosen
older generation; `Recovery.skipped` makes that boundary visible. An unacknowledged
new generation can survive a crash, so a caller must inspect recovery rather than
assuming a failed request could not have published.

Returning to Python currently means using the untouched original Python registry,
provisioning/accounts and entire chunk tree. It restores exactly that source
backup's progress and loses **all** subsequent Rust-side play. No merge, downgrade
or reverse export is supported. Keep the full source backup through rollback.

## Verification

Tests cover current and older model/construction readers, private credentials and
provision archives, ledger ownership/quantities/activity, named RNG continuation,
multiple worlds/active selection, missing/corrupt chunks, malformed/future schemas,
symlinks, exclusive locks, permissions and ignored complete pending directories.
Every commit write/sync/publication checkpoint is failure-injected and separately
interrupted in a child process; reopen yields the prior or complete new generation
with matching registry/chunk data. A writer blocked during actual file writing
leaves authority usable, rejects queue overload and drains accepted work on shutdown.
Flush/shutdown report failures even when tickets are dropped.

Real boat/hauler scenarios save, close, reopen and resume at fractional positions
with cargo, supplies and worker progress intact, then unload exactly once.
Construction work/claims also survive disk reload. Terrain capture keeps sleeping
archives and exact scalars without mutating source files, authority or write intents.
Compile-fail doctests protect private snapshots from implicit logging/serialization.

Performance comparisons are not run for this durability/ownership behavior slice.
There is no runtime optimization or host-performance claim. Blocking-writer tests
use synchronization; integrated save/snapshot contention remains R23.
