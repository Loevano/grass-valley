# R06 legacy staging import

R06 implements read-only registry/account/ordinary-world and external-chunk
staging in `lantern_host::legacy`, with an explicit inspection CLI. It does not
create a playable core world, invoke Python, run migrations, generate terrain/names,
advance clocks, authenticate players or write a destination save. Python remains the normal playable host.

## Entry points and privacy

`import_bytes(source: Option<&[u8]>, provision: Option<&[u8]>)` imports explicit
synthetic input. `None` means no source save; empty or malformed bytes are errors.
`read_copies(source: &Path, provision: Option<&Path>)` only calls filesystem reads
on the explicit paths. A missing source produces an empty registry; a missing
explicit provisioning path is an error. Chunked worlds require the separate
`import_with_chunk_copies(source, provision, directories)` or
`read_copies_with_chunks(source, provision, directories)` entry points and explicit
`ChunkDirectory::new(&world_id, &seed_directory_copy)` bindings. Every chunked world,
including inactive worlds, must have exactly one binding. Ordinary worlds reject
chunk bindings. There is no sidecar/root discovery, output directory creation or
implicit production-root inspection. The CLI also requires explicit copy paths.

The result is an immutable `ImportedRegistry` with typed world/town ownership,
effective private credentials and an identity/reference overlay. Original source
and optional provisioning bytes are retained exactly, including whitespace,
object/list order, unknown fields and numeric lexemes. Explicitly named private
accessors allow later copy/export tools to obtain those bytes; they must not be
used for public output. No Debug/Serialize implementation exists for the archive,
imported worlds/towns, chunk metadata, raw chunks or credential records. Compile-fail
tests protect archive and chunk serialization boundaries.

`report()` returns a separate Serialize/Debug allowlist: effective active world,
public world IDs, landscape layout, counts, identity high-water marks, reference
counts, audited version markers, numeric chunk metadata, sparse coverage,
preserved-field paths and ignored provisioning counts. Its readiness is always
`preserved_only`, never playable. Raw JSON, filesystem paths, seeds, tokens,
account/login names, salts, hashes, inventory values and unknown field names are
excluded. Unknown names can themselves be secrets, so preservation
paths use source field ordinals (`fields[n]`) instead of echoing those names.
Semantic errors contain schema paths/categories; structural errors add sanitized
line/column information. Parser/I/O messages and offending values are withheld.

## Envelope and account semantics

| Source case | Staging behavior |
| --- | --- |
| No source | Empty registry, no active world; nothing created |
| `{world,tokens}` | World ID `legacy`, name `My first world`, active `legacy`; tokens validated and preserved |
| `{worlds,active_world}` | Preserve every entry/name/world in recorded order; unknown selected ID or null gives no active world |
| Missing required envelope field / wrong shape | Reject with location; no defaults hiding malformed data |
| Both `world` and `worlds` | Reject as ambiguous rather than choose an envelope |
| Main tokens / players | Preserve exact keys, account names, town ownership, salts and hashes; reject unknown owners/empty keys |
| Optional provisioning | Known worlds/towns only; existing saved login key wins; add only missing keys |
| Unknown provisioning world/town or overridden account | Do not apply it; retain its original bytes and count it as ignored |
| Unknown envelope/account/gameplay fields | Preserve privately and report preservation, without claiming semantic compatibility |

These rules follow `server.load` and `player_login` storage shapes. No trimming,
case folding, PIN hashing or token reissuance occurs. Existing normalized keys are
not normalized again. Python's legacy envelope does not promote an unknown
top-level `players` field into accounts; it remains preserved source data here too.
Provisioning records for unknown worlds are not interpreted, and overridden or
unknown-town records are not hydrated as effective credentials. Duplicate JSON
keys are rejected even inside otherwise ignored/opaque payloads; they are never
silently overwritten as a dictionary parse might do.

## Ownership, identity and quantities

Each imported world has one counter starting at zero. Traverse towns in recorded
JSON object order; within each town allocate buildings in list order, villagers
in list order, then a founder actor only if `player` exists. Every allocation
includes its world/town owner. No names or coordinates determine identity. The
overlay stores the resulting IDs and high-water mark; identical source bytes
produce identical bindings. Serial numbers span all towns in a world, and the
same opaque town key in another world remains a different owner.

Worker `building_index` and building `farm_index`/`orchard_index` receive explicit
typed target bindings. Indices must be integral, local and in range; farm/orchard
links must target their matching building kind. Null links remain absent links,
without rewriting the source. Building tiles must be exact i32 integers.

Saved canonical trade `target_town` values must name a different town in the same
world. Adventure `quarry.members` and `lanterns.members` keys must belong to that
world and their records must be objects. Offer IDs are nonempty and unique within
each town's offer list; craft request IDs are nonempty and unique within each
building's `craft_queue`. IDs remain opaque and are never regenerated. Historical
or unknown offer aliases (such as `target`) are preserved without guessed binding;
missing canonical target fields are not evidence of a validated trade target.
The report counts only canonical town links actually checked. These checks do not
validate trade terms, recipes, rewards or adventure phase consistency.

This is an immutable staging overlay, not a partially rewritten gameplay save.
Other task/coordinate references remain in the preserved payload. Do not reorder
or hydrate raw entities until *all* relevant references have been mapped. NPCs,
individual animals, trip/claim references and world-owned NPC identity need their
remaining binding/ownership coverage before playable conversion; preserved data
is not evidence those references are already validated or converted. Later
durable conversion must persist the allocated bindings, not allocate again from
a reordered source.

Amounts/inventories/positions/cargo are retained in the exact source bytes, with
no f64/fixed-point rewrite, aggregation, normalization, refunds or rate changes.
Large integer stocks/seeds (including integers beyond u64) and existing opaque
offer/request IDs survive as source data. Fractional lexemes such as
`1.2345600e-3` remain identical. This deliberately does not pass unported quantities
through R05's smaller model DTOs. Gameplay accounting/phase validation and typed
inventory hydration remain required before a playable Rust import.

## Landscapes and versions

Independent town maps stay independent; a shared nonchunked map stays one shared
source record. Map aliases are not rebound into mutable copies. All terrain,
resources, stumps, NPCs, dimensions, seeds and activity data remain in the source.
No generator, Python chunk loader, migration/ensure function or resource catch-up runs.

Explicit `terrain_version` must be an integer 1–7. Absence remains absence (the
later generator's legacy default is 1). Recognized migration markers permit
integer 0 through their audited maximum: forage/mineral/NPC placement/wood/grain/
worker capacity/acre upgrade/inventory/market storage 1; industry/food balance 2;
NPC roster 3. Zero denotes pending legacy state, not a completed migration.
Unknown explicit values of these known markers are rejected, including in
inactive worlds. Unknown other fields/markers are preserved as unsupported
payload, not approved as compatible. Marker acceptance does not run or validate
the migration's economic effects.

`worlds[].versions` lists explicit audited markers with ordinal schema paths,
saved/supported versions and a status. Terrain versions use `supported_generator;
never upgrade implicitly`; an older terrain algorithm is not a pending upgrade.
Other older markers use `older_marker; migration not applied`, while current
markers use `current_marker; effects unverified`. Missing markers are not inferred:
applicability and effects remain unverified. Unknown marker names remain privately
preserved fields, without copying their names into diagnostics. The inspector
never applies refunds, reassignments, inventory normalization or other migration
effects merely because a marker is absent or old.

Malformed shapes, duplicate keys, nonfinite JSON constants/exponent values and
excessive nesting are rejected before exposing any imported registry. Arbitrary
integer lexemes remain lossless; there is no lossy cast to check their magnitude.

## External chunk copies (slice 2)

The Python reference is `<save-stem>_chunks/<str(terrain_seed)>/<cx>,<cy>.json`.
Callers supply each seed directory explicitly; the library never infers a path
from private saved metadata. The leaf directory name must match the seed namespace.
Integer seeds retain Python's arbitrary-length decimal representation (`-0` maps
to directory `0`); string seeds remain exact, including leading zeros. Other seed
types and empty, dot, separator/NUL/colon-containing namespaces fail explicitly.
The seed directory itself must be a real directory, its entries regular files;
direct directory/file symlinks, shared canonical directories across worlds,
unexpected files/subdirectories, temporary files and noncanonical names are rejected.
No entry is silently skipped. These are explicit synthetic copies, not an atomic
snapshot mechanism for concurrently changing live saves.

Supported chunk metadata is `shared_map.chunked` true or integer 1, `chunk_size`
40, `map_size` 480 or 1920, and a safe seed. World-level and independent-town
chunked flags are rejected. Boolean false/integer 0 retain ordinary-map semantics;
other flag values fail. Missing terrain version resolves to 1 for validation;
missing starter offset resolves to the historical 880 (even on a 480-size record),
never silently to current 160. Explicit offsets must be nonnegative u32 multiples
of 40. Optional `chunk_clock` must be finite and nonnegative. Missing metadata
stays absent in the original bytes; these validation defaults do not rewrite it.
Inline terrain/resources/stumps on a chunked shared map are rejected as ambiguous.

Every supplied file, including sleeping/evicted chunks and inactive-world data,
is read and retained byte for byte in private `ImportedChunk` records. Keys are
canonical nonnegative decimal pairs within the saved dimensions. Records require
40 rows of 40 characters, resource/stump arrays, integer positions inside the
owning chunk, nonempty resource kinds and finite nonnegative stump timers. Optional
record clocks are validated without advancement; absent clocks stay absent.
Known explicit version bounds apply to chunk records too; an explicit terrain
version must match its world. Unknown fields remain private raw data. Supply,
regeneration, elapsed time, unknown large integers and fractional lexemes are not
normalized or passed through gameplay DTOs. No regeneration, tiny-water repair,
starter/NPC overlay, generation, loading into core terrain or ID allocation for
resource/animal payloads occurs. Stored chunks sort by numeric key; diagnostics
follow registry order regardless of binding argument order.

Python saves a sparse store and generates absent chunks on demand. There is no
persisted complete-key manifest, so an absent file cannot generally be classified
as lost or unvisited. Missing supplied directories and corrupt files always fail.
`ChunkDirectory::requiring(&keys)` validates an optional caller-declared required
key set; missing, duplicate or out-of-bounds required keys fail. The report gives
file counts, required keys checked and `sparse_copy_only; absent files unverified`;
it never claims that an unmanifested sparse copy is complete. An empty explicit
directory is valid sparse input. Neither success nor failure creates missing files.

## Inspection command (slice 3)

From `rust/`, inspect explicit synthetic copies:

```sh
cargo run -p lantern-host --locked --offline -- inspect-copy \
  --source /tmp/lantern-copy/registry.json \
  --provision /tmp/lantern-copy/private.players.json \
  --chunks current /tmp/lantern-copy/current/18446744073709551615 \
  --chunks old /tmp/lantern-copy/old/text-seed \
  --require-chunk current 0 0
```

`--source` is required exactly once; `--provision` is optional and not discovered.
For ordinary saves omit `--chunks` and `--require-chunk`. Repeat `--chunks WORLD
SEED_DIRECTORY` once per chunked world; repeat `--require-chunk WORLD X Y` for a
caller-supplied required-key set. Required options may precede their bindings.
`inspect-copy --help` documents the command. Unknown options, duplicate source/
provision options, invalid owner/key arguments and missing option values fail
without echoing arguments. Path arguments use OS strings. Unlike the library's
missing-save initialization semantics, a nonexistent CLI source is an error.
There is no default save root, provisioning discovery or automatic chunk discovery.

Exit 0 means read-only staging/inspection succeeded, **not** playable conversion.
Stdout contains one JSON `InspectionReport`; stderr is empty on success. Exit 1
means argument, read, validation or output failure; input/validation errors leave
stdout empty and emit only a static reason or sanitized `BoundaryError` on stderr.
Report fields never include raw input, credential keys/values, account/NPC/display
names, seeds, unknown field names or filesystem paths. Public world IDs are
intentionally included. `preserved` paths use ordinals for unsupported fields,
including fields directly under worlds and shared maps; their reason is `field
retained privately; not hydrated`. A whole-record preservation marker additionally
covers nested unported payloads. Unknown data is retained, not approved as valid
Rust gameplay.

`capabilities` lists checked reference/schema categories separately from
`preserved_only` categories. It explicitly sets `migrations_applied`,
`simulation_advanced`, `writes_enabled` and `playable` to false. Counts and identity
high-water marks are repeatable; private resource/cargo/inventory values remain
in exact source bytes instead of being summed through floating point for output.
The command calls the staging library only; it never starts the replay runner,
Python loader, gameplay clocks or an HTTP listener. There are no output-save,
conversion, generation or production-write options. Source bytes/mtime and copied
directory contents remain unchanged on successful and rejected inspection.

## Validation and remaining work

`rust/crates/lantern-host/tests/legacy_import.rs` uses only committed synthetic
fixtures under that crate's `tests/fixtures/` plus disposable filesystem copies.
Tests cover old/current envelopes, independent/shared maps, inactive-world
retention, active selection, exact private payload/stock/cargo retention, repeated
ID allocation, local link ownership, provisioning precedence, private report/error
exclusion, known versions, malformed references, duplicate keys and nonfinite data.
Read-only copies keep their bytes and modification times on successful and failed
imports; directory inventory confirms no output/chunk/temporary files were made.

`tests/legacy_chunks.rs` adds a committed synthetic chunked registry and generated
synthetic records on disposable copies. It covers current/legacy dimensions and
defaults, versions 1–7, exact chunk lexemes/quantities/clocks, repeated identity and
report stability, explicit owner/namespace bindings, active/evicted/inactive chunks,
missing/required/sparse copies, malformed metadata/records, unexpected files and
symlinks, private report/error exclusion and unchanged source bytes/mtime/entries.

`tests/legacy_inspect.rs` adds a synthetic migration/gameplay fixture and end-to-end
CLI checks. Coverage includes pending/current/generator marker diagnostics,
canonical trade/adventure ownership, duplicate offer/craft IDs, cross-world
rejection, opaque field locations, preserved construction/trip cargo and exact
stocks, repeatable identities/reports, explicit provisioning/chunks/required keys,
argument and malformed-input privacy, unchanged filesystem state and OS-string
error handling. Existing replay transport tests remain applicable.

R06 is complete as a read-only inspection/staging stage. Core hydration, applied
feature migrations, complete NPC/animal/task bindings, terrain/RNG behavior,
durable copy-based destination writes and runtime authentication remain their
named later stages. No existing gameplay feature is marked verified or removed by
this staging importer.
