# Experimental Rust saves

R19 slices 1–3 implement private snapshot export/versioned reload, durable native
generations and explicit offline copy/recovery/archive tools. Python remains the
playable default. The approved Python tooling scope is **lossless preserved-only
archives**, not playable native conversion; complete actor adapters remain missing.
There is no live Rust host save loop or native-to-Python gameplay exporter.

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
| `lantern-generation` manifest | 1, 2 | 1 normally; 2 for copies with source provenance |
| `lantern-python-archive` | 1 | 1, `preserved-only` |
| Ledger DTO | 1 | 1 |
| Worker observation evidence | 1, optional | 1, omitted when empty |
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

## Frozen worker observation evidence

R20-1h adds optional private `TownState.work_observations` records, with their own
strict version-1 codec inside registry v1. Empty observations omit the new field,
so existing generations still load without inventing history. Older binaries reject
nonempty observation-bearing generations as an unknown schema extension; rollback
and native-to-Python gameplay export remain separate acceptance gates.

The checkpoint is captured from native R11 actors, then copied and encoded with the
same detached whole-generation snapshot. It retains the already-confirmed public
movement observation, bound to stable actor/task/workplace/travel facts and native
generations. It is neither a navigation cache nor an authoritative actor restore
format: queries, frontiers, ready routes, cursors and playback are never installed
into a restored model. No ticks, goods transfers or receipt drains reconstruct it.
Read-only admission compares it to current owners (or R12's suspended intent) and
to the complete archived public animation before exposing any endpoint.

Capture cannot accept arbitrary animation JSON as evidence. Checkpoint labels and
public DTOs do not expose credentials or private extensions. The aggregate live
host must refresh/discard this observation alongside task/assignment changes in
R20 2b–3c; this frozen adapter does not provide ordinary-save actor hydration or
live scheduling. Missing/stale evidence refuses snapshot admission.

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
and cleanup tools are described below.

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

## Copy, recovery and archive commands

From `rust/`, invoke `cargo run --locked --offline -p lantern-host -- COMMAND ...`.
`save-convert --help` lists all options. Every source and destination is explicit;
parent directories must exist, and the destination directory must be new.

```text
save-convert --source-format native --source RUST_ROOT --destination NEW_RUST_ROOT
save-inspect --source RUST_ROOT
save-recover --source RUST_ROOT --destination NEW_RUST_ROOT
save-recover --source RUST_ROOT --destination NEW_RUST_ROOT --generation 1
save-cleanup --source RUST_ROOT --pending-only
```

Native conversion copies the newest generation byte-for-byte after complete
native schema/reference/integrity validation. It refuses a damaged newest save;
use `save-recover` explicitly to select the newest usable fallback or a specific
valid generation. Recovery copies into a new root; it never edits/switches the
source in place. Reports include chosen/skipped generation numbers and source
manifest SHA-256, without paths, world/account names or credential material.
Manifest v2 stores the source generation/hash and operation; v1 remains readable.
Native copies retain model reader versions 1–3 without rewriting old facts.

Native inspection/copy/recovery uses an existing read-only lock handle: it neither
creates lock files nor opens source files writable, and it refuses a live writer.
Canonical destination-parent checks reject overlaps/aliases; exclusive creation
rejects existing directories, files and dangling links. A new native destination
retains `.conversion-in-progress` until its complete generation is verified and
synced. Readers refuse it until that gate is removed and the root is synced.
Interrupted copies may leave an unusable new directory; choose another destination
for retry. A failure after final publication may leave a complete valid destination.

`save-inspect` reports each native generation's validity and selected fallback.
`save-cleanup --pending-only` is the explicitly destructive operation: under the
writer lock it prevalidates and removes only uncommitted `pending-N` directories,
rejects symlinks/nonregular entries, and syncs the root. It never removes committed
generations. There is no automatic deletion or in-place repair of corrupt commits.

### Preserved-only Python archives

```text
save-convert --source-format python-archive --source COPY.json --destination NEW_ARCHIVE --provision COPY.players.json --chunks WORLD SEED_DIRECTORY --require-chunk WORLD 0 0
save-inspect --source-format python-archive --source ARCHIVE
save-restore-python --source ARCHIVE --destination NEW_PYTHON_ROOT
```

Provisioning is optional and never discovered automatically. Repeat `--chunks`
for **every** chunked world, including inactive worlds; each directory's leaf must
match its saved seed namespace. All files in each sparse directory are captured.
`--require-chunk` optionally supplies known required coordinates. Original Python
saves have no complete historical chunk manifest, so a file already missing before
capture cannot otherwise be distinguished from an unvisited chunk. Every record
actually archived becomes a required, checksummed archive member thereafter.

Use quiescent/offline copies, or stop Python first. Python does not participate in
the native writer lock. The tool rereads the entire selected input set and compares
manifests before writing, detecting ordinary concurrent changes; that is not a
replacement for an atomic filesystem snapshot of an actively changing Python host.
Source files/directories are only read, never normalized, migrated, renamed or
written. No Python process, simulation, terrain generation or clock catch-up runs.

The distinct archive marker, versioned `preserved-only` manifest, per-file hashes,
world/chunk membership, exact original registry/provision bytes and persistent
identity overlay preserve all source data, private accounts and unknown extensions.
Validation covers the R06 supported envelopes, private account ownership/merge
policy, known version markers and references, complete selected directory sets and
chunk structure. Unsupported gameplay payloads remain opaque, not semantically
validated or playable. The persisted overlay must be checked by future adapters;
entities are never reordered to invent new bindings.

A Python archive cannot open as a native Store. `--source-format python` (playable
import) is explicitly refused before destination creation. Archives use private
0700 directories/0600 files and an incomplete-write gate; readers accept them only
after all files/manifest are synced and the gate is removed. Missing, corrupt,
extra, duplicate, symlinked or unsupported archive data rejects the whole archive.
Different contents sharing one Python seed namespace are refused, because they
could not be restored faithfully to Python's single namespace tree.

`save-restore-python` exports **only the original Python archive** to a new root:
`savegame.json`, optional `savegame.players.json`, and
`savegame_chunks/SEED/X,Y.json`. Registry and provisioning bytes/chunks are exact;
accounts are not rehashed, renormalized or reissued. Normal Python provisioning
merging will therefore retain its original existing-account-wins behavior. Chunk
and provisioning files are synced before the first main registry publication.
This is safe because the destination is fresh; it is not a JSON-only replacement
over mutable old chunks. Interrupted restores leave only a new incomplete target;
when a registry has been published, all of its chunks are already complete. Wait
for success before launching a host on that explicit root. Neither source archive
nor original Python data is changed.

### Progress-loss and remaining compatibility gates

Native restart restores the selected complete generation. Mutations after its
captured revision are lost. Fallback/explicit older-generation recovery loses all
progress after that older generation; the chosen/skipped numbers make the boundary
visible. A published but unacknowledged new generation may survive a crash, so
inspect recovery instead of assuming a failed request could not have published.

Archive restore is a byte-exact reversal of Python **archiving**, with no Rust
play added or lost because archives are not playable native worlds. It is not
native gameplay reverse export. `save-export` refuses native-to-Python export.
Returning to Python after Rust play still means the untouched original Python
backup, losing **all** subsequent Rust-side progress. No merge/downgrade adapter
exists. This approved archive/tooling scope does not accept one-way production
cutover or that rollback loss; R24's reverse-export/rollback decision remains open.
Complete playable Python actor hydration and live host save invocation remain
explicit compatibility gates for R20–21, not achievements of archival success.

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
Tool tests additionally cover exact native/legacy copy and source-file immutability,
selected/fallback recovery, provenance manifests, pending-only cleanup, incomplete
conversion gates, malformed/unsupported inputs, preserved-only Python archive and
restore, and CLI privacy/refusals. Native copy and Python archive writes are both
failure-injected and interrupted by child-process exit at each publication stage.
Python restore failures verify that registry publication never precedes chunks.

Performance comparisons are not run for this durability/ownership behavior slice.
There is no runtime optimization or host-performance claim. Blocking-writer tests
use synchronization; integrated save/snapshot contention remains R23.
