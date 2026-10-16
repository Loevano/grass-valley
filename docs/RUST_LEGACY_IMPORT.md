# R06 legacy staging import

Slice 1 implements read-only registry/account/ordinary-world staging in
`lantern_host::legacy`. It does not create a playable core world, invoke Python,
run migrations, generate terrain/names, advance clocks, authenticate players or
write a destination save. Python remains the normal playable host.

## Entry points and privacy

`import_bytes(source: Option<&[u8]>, provision: Option<&[u8]>)` imports explicit
synthetic input. `None` means no source save; empty or malformed bytes are errors.
`read_copies(source: &Path, provision: Option<&Path>)` only calls filesystem reads
on the explicit paths. A missing source produces an empty registry; a missing
explicit provisioning path is an error. There is no sidecar discovery, chunk
directory traversal, output directory creation or production-root inspection.
CLI integration/discovery belongs to slice 3.

The result is an immutable `ImportedRegistry` with typed world/town ownership,
effective private credentials and an identity/reference overlay. Original source
and optional provisioning bytes are retained exactly, including whitespace,
object/list order, unknown fields and numeric lexemes. Explicitly named private
accessors allow later copy/export tools to obtain those bytes; they must not be
used for public output. No Debug/Serialize implementation exists for the archive,
imported worlds/towns or credential records. A compile-fail test protects the
archive serialization boundary.

`report()` returns a separate Serialize/Debug allowlist: effective active world,
public world IDs, landscape layout, counts, preserved-field paths and ignored
provisioning counts. Its readiness is always `preserved_only`, never playable.
Raw JSON, tokens, account/login names, salts, hashes, inventory values and unknown
field names are excluded. Unknown names can themselves be secrets, so preservation
paths use source field ordinals (`fields[n]`) instead of echoing those names.
Semantic errors contain schema paths/categories; structural errors add sanitized
line/column information. Parser/I/O messages and offending values are withheld.

## Envelope and account semantics

| Source case | Slice 1 behavior |
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
No generator, chunk reader, migration/ensure function or resource catch-up runs.

Explicit `terrain_version` must be an integer 1–7. Absence remains absence (the
later generator's legacy default is 1). Recognized migration markers permit
integer 0 through their audited maximum: forage/mineral/NPC placement/wood/grain/
worker capacity/acre upgrade/inventory/market storage 1; industry/food balance 2;
NPC roster 3. Zero denotes pending legacy state, not a completed migration.
Unknown explicit values of these known markers are rejected, including in
inactive worlds. Unknown other fields/markers are preserved as unsupported
payload, not approved as compatible. Marker acceptance does not run or validate
the migration's economic effects.

`chunked:true` at world, shared-map or town level explicitly fails with
`external chunks require R06 slice 2`. No external path is read in slice 1.
Malformed shapes, duplicate keys, nonfinite JSON constants/exponent values and
excessive nesting are rejected before exposing any imported registry. Arbitrary
integer lexemes remain lossless; there is no lossy cast to check their magnitude.

## Validation and remaining work

`rust/crates/lantern-host/tests/legacy_import.rs` uses only committed synthetic
fixtures under that crate's `tests/fixtures/` plus disposable filesystem copies.
Tests cover old/current envelopes, independent/shared maps, inactive-world
retention, active selection, exact private payload/stock/cargo retention, repeated
ID allocation, local link ownership, provisioning precedence, private report/error
exclusion, known versions, malformed references, duplicate keys and nonfinite data.
Read-only copies keep their bytes and modification times on successful and failed
imports; directory inventory confirms no output/chunk/temporary files were made.

Slice 2 still owns external chunk paths/references, required metadata, original
chunk byte preservation and missing/corrupt chunk failures. Slice 3 still owns
broader migration/reference validation, capability diagnostics, fixture coverage
and the inspection CLI. Core hydration, feature migrations, complete NPC/animal/
task bindings, terrain/RNG behavior, durable copy-based destination writes and
runtime authentication remain their named later stages. No existing gameplay
feature is marked verified or removed by this staging importer.
