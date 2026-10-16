# R06 legacy staging import

Slices 1–2 implement read-only registry/account/ordinary-world and external-chunk
staging in `lantern_host::legacy`. It does not create a playable core world, invoke Python,
run migrations, generate terrain/names, advance clocks, authenticate players or
write a destination save. Python remains the normal playable host.

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
production-root inspection. CLI integration belongs to slice 3.

The result is an immutable `ImportedRegistry` with typed world/town ownership,
effective private credentials and an identity/reference overlay. Original source
and optional provisioning bytes are retained exactly, including whitespace,
object/list order, unknown fields and numeric lexemes. Explicitly named private
accessors allow later copy/export tools to obtain those bytes; they must not be
used for public output. No Debug/Serialize implementation exists for the archive,
imported worlds/towns, chunk metadata, raw chunks or credential records. Compile-fail
tests protect archive and chunk serialization boundaries.

`report()` returns a separate Serialize/Debug allowlist: effective active world,
public world IDs, landscape layout, counts, numeric chunk metadata, sparse coverage,
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

Slice 3 still owns broader migration/reference validation, capability diagnostics,
fixture coverage and the inspection CLI. Core hydration, feature migrations,
complete NPC/animal/task bindings, terrain/RNG behavior, durable copy-based destination writes and
runtime authentication remain their named later stages. No existing gameplay
feature is marked verified or removed by this staging importer.
