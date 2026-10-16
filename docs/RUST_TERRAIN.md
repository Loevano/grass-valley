# R07 terrain compatibility

R07 slices 1–3a provide experimental **read-only saved-chunk loading**, **raw
versioned chunk generation**, and **nonchunked terrain-only hydration**, separate
from R06 staging and its unchanged
`preserved_only` inspection CLI. They do not create a playable authoritative world.
Python remains the normal host.

## Saved-copy reader

After explicit R06 copy import, construct
`lantern_host::legacy::ChunkReader::new(&archive.worlds()[index])`. Only chunked
worlds are accepted. `read(ChunkKey { x, y })` decodes the owning world's saved
record. Saved records always take precedence: a missing record returns an explicit
saved-only unavailable error.
No RNG, latest-version fallback, Python subprocess or reconstructed starter is used.
Unknown versions, conflicting record versions, dimensions, keys and positions are
rejected by the R06 boundary before a reader can be constructed.

`LoadedChunk` exposes immutable terrain rows, resources, stumps, optional saved
clock and explicitly private original bytes. Resource order, kind/species strings,
coordinates and optional supply/maximum/regeneration values are read without
clamping, replenishment or default insertion. Stump timers and missing clocks stay
as saved. Binary64 views do not rewrite the exact source numeric lexemes; all
unknown extensions, fields and arbitrary-size integers remain in original bytes.
No new resource/animal identity is allocated. Kind/species strings are source
metadata, **not** approval of corresponding gameplay rules or migrations.

Loading additionally rejects unsupported terrain codes (only `g f m s r b d i v p w c`
are accepted), nonstring explicit species and nonfinite/negative/nonnumeric
supply, maximum or regeneration values. Missing optional values remain `None`.
R06 can still preserve such unported data; a successful inspection is not a
promise that every record can be decoded or played. Diagnostics use static schema
paths/categories, never offending input values. Loaded records/resources have no
Debug or Serialize implementation and are not public snapshots.

The decoded view cache holds at most 49 records, matching Python's camera-view
limit. Successful `read` calls refresh recency; `tile` is a nonloading lookup.
Eviction and `unload` drop only decoded views; reload reads the same immutable
archive bytes. Failed reads neither evict nor reorder existing views. The archive
still retains **all** supplied files, so this bounds decoded views, not total
archive memory or disk I/O. A future runtime/disk streaming implementation is not
claimed here.

`tile(Tile { x, y })` distinguishes `OutsideWorld`, `Unloaded`, and `Saved(code)`.
Negative coordinates and the saved 480/1920 world edges are checked before chunk
arithmetic. Missing in-world terrain never becomes grassland; callers must treat
unloaded tiles as unavailable/impassable. Saved `v` remains a saved void code.
This reader has no mutable gameplay state, HTTP snapshot, activation or writer.

## Golden records and validation

`rust/crates/lantern-host/tests/fixtures/terrain/chunks.json` contains 58 records
from real Python `chunk_world.generate_chunk`, with the original generator version,
seed, dimensions, key and SHA-256 of Python's sorted-key record JSON. Versions 1–7
include seed 42 at `3,4`; v1/v2 retain the existing Python golden hashes. Fifty v7
chunks exercise adjacent tiles/water and cache overflow; additional records cover
text seeds, an arbitrary-size negative integer seed, legacy 1920 dimensions and
the far `47,47` boundary. The `3,4` sample is identical across versions 3–7;
slice 2 adds distinguishing generator vectors separately. These are raw generated
records, not village/starter/tiny-water/catch-up results. They establish saved decoding, not Rust generation.

Recheck the committed goldens against real generation from the repository root:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate.py --check
```

The script checks every record and hash byte for byte. Running it without
`--check` explicitly refreshes the fixture; generator differences need review and
must not be hidden by refreshing a failing baseline. Rust `tests/loaded_chunks.rs`
checks all terrain cells and ordered resources, cross-chunk water, outside/unloaded
boundaries, 49-view LRU eviction/reload, saved edits and private large extensions,
optional supply/clock preservation, stump timers, unsupported decoded fields,
nonchunked refusal, failure atomicity and the existing read-only report boundary.
Edited-resource tests begin with real generated records. Existing R06 tests still
cover malformed records/versions/keys, manifests and source filesystem immutability.

## Raw legacy generators (slice 2)

`lantern_core::terrain::generate_chunk` implements raw Python chunk generators
1–7 for 480 and legacy 1920 dimensions. It receives the exact Python `str(seed)`
namespace: canonical decimal arbitrary-size signed integers or unchanged Unicode
text. Chunk RNG seeds remain `{seed}:chunk-v{min(version,3)}:{cx},{cy}`;
region and river streams retain their original versioned text. Noise uses
BLAKE2b with an **eight-byte digest parameter**, not truncated BLAKE2b-512.
Integer v1 noise coordinates and float regional coordinates (`1` versus `1.0`)
remain distinct. Float floor division follows CPython's fmod-derived quotient,
including the `160 // (480 / 9)` boundary where `floor(x / scale)` is wrong.
Stable row/column traversal and conditional RNG consumption preserve ordered
water, tree/palm, rock, berry and herb records. Raw v6/v7 outputs intentionally
match: v7's tiny-water repair belongs to the later overlay/read stage.

`PythonRandom` implements version-2 SHA-512 byte/text seed mixing, absolute
arbitrary-size decimal integer seed conversion, MT19937 initialization/twists,
53-bit `random`, `uniform`, 0–64-bit `getrandbits`, rejection-based `randbelow`,
half-open signed `randrange`, `choice` and `shuffle`. It rejects invalid integers,
empty ranges and unsupported bit widths. It is not a claim of all Python Random
APIs (e.g. weighted choices, Gaussian draws, arbitrary-width return values or
stepped ranges). D05's three gameplay seed-material vectors now reproduce their
reference draws; gameplay streams and serialized RNG state are not yet adopted.

`ImportedWorld::raw_chunk` selects immutable saved bytes first. Only absent
nonstarter chunks use the saved seed, version and dimensions. Slice 3b.ii now
reconstructs absent starter chunks with the original typed seed and authored
landscape, as detailed below. The result is explicitly `RawChunk::Saved` or
`RawChunk::Generated`,
with no decoded-view cache, overlays, tiny-water repair, catch-up, supplies,
activation, identity allocation or save writes. The slice-1 saved-only reader
remains saved-only. An absent sparse copy is not proof that its original file
never existed: required-copy manifests remain the caller's responsibility.

The core stays `no_std`, with SHA-512 (`sha2` 0.10.9), variable BLAKE2b (`blake2`
0.10.6), and `libm` 0.2.16 for float floor, powers and sine. RustCrypto hash crates
use their no-default-feature configurations; these established MIT/Apache-2.0
hash/math implementations avoid inventing cryptographic primitives or host
subprocesses. Exact landscape comparisons pass on this development host. Python
uses its platform math library while Rust uses portable libm; these finite vectors
do not prove every seed/tile on every platform. Any observed platform threshold
mismatch must block that platform's cutover and be resolved, never hidden by a
tolerance or generator-version upgrade.

`fixtures/terrain/parity.json` adds 168 real Python raw records: each generator
with distinguishing ice, swamp, farmland, river/seam, lake/island, desert/oasis and
world-corner cases, plus Unicode, leading-zero text, >64-bit negative integer and
maximum-u64 seeds in legacy dimensions. Tests compare every cell and every ordered
resource/metadata against these plus the original 58 records. Separate vectors
compare 630 random draws (crossing an MT twist), mixed bit widths, rejection
ranges, shuffle and choice for six integer/text seeds. Recheck without refreshing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_parity.py --check
```

## Nonchunked maps and terrain-only hydration (slice 3a)

`lantern_core::legacy_map::generate_map` reproduces Python `terrain.generate_map`,
including five sequential random fields, shape/swamp samples, conditional tree
species draws (even when subsequently rejected), the authored corner clearing,
starter trees, three independently sampled pond widths and ordered starter rocks.
`MapSeed::Integer` uses absolute arbitrary-size integer magnitude;
`MapSeed::Text` uses unchanged UTF-8 text. Numeric text, leading zeroes, empty text,
Unicode and strings unsuitable as chunk directory names retain their own meanings.
The generator supports integer dimensions 1–1920; zero/larger sizes and malformed
integer seeds explicitly fail. This is the legacy map generator, not a switch to
regional generation when a landscape carries a newer version marker.

`ImportedRegistry::hydrate_nonchunked(world_id)` is an explicit, immutable,
terrain-only operation. It accepts ordinary legacy and registry envelopes, keeps
independent maps separate, and constructs one shared map bound to all of its
towns using world-and-town `Owner` identity. Stale town-local shared fields are
ignored as in Python `bind_map`; foreign-world owners cannot resolve a binding.
Each call owns independent decoded data. No cache, clock, snapshot or authoritative
archive mutation is involved, and the inspection CLI remains unchanged.

Ground follows deterministic `ensure_terrain(new=False)` behavior: missing
`map_size` defaults to 12, saved rows of matching height win, and absent or
wrong-height rows regenerate from the supplied seed with void replaced by grass.
Matching-height saved rows must have supported codes and the declared width;
malformed rows explicitly fail. **Ground regeneration never replaces or inserts
resources.** Existing resource order, kind/species, optional quantities/limits and
stump timers remain unchanged, including missing supply fields. Missing stumps
produce an empty decoded view while absence remains in source bytes. Required
resources, missing seeds needed for reconstruction, unsupported seed types,
nonfinite/negative values, bad positions and chunked worlds are explicitly refused.
No entropy is invented. Seeds not needed for valid saved ground remain source-only.

All extensions, arbitrary integers, numeric spellings and other metadata remain
in the borrowed **whole private source registry bytes**, including credentials;
these bytes must never enter public snapshots. Hydration types have no Debug or
Serialize implementation. This is not `ensure_resources` or `prepare_landscapes`:
map expansion/bootstrap, resource/industry/forage/NPC migrations, invalid-tree
repair, supply initialization, town gameplay and offline production are not applied.
Those feature-specific preparations remain R08–18 as applicable; tree/regrowth
and chunk lifecycle rules needed by R07 remain slice 3c.

`fixtures/terrain/maps.json` freezes 35 real Python maps at sizes 1/12/13/40/120
for seven integer/text seeds, and six real `ensure_terrain` cases covering saved,
missing and wrong-height terrain at sizes 12/120. Eight Rust integration tests
compare every row and ordered resource, apply the hydration cases to independent
and shared layouts, check owner bindings and world selection, all saved version
markers 1–7, arbitrary-size integer reconstruction, exact extensions/lexemes,
missing values and failure/source immutability. These finite vectors share the
platform libm release limitation described above. Larger supported map sizes are
not exhaustively golden-covered. Recheck without refreshing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_maps.py --check
```

## Authored starter geometry (slice 3b.i)

`lantern_core::starter::generate_starter(MapSeed, version)` reproduces the
120×120 ground, ordered resources and four ordered town sites from Python
`shared_world.new_world` for versions 1–7. Integer and text seeds remain explicit;
integer namespaces are canonicalized as Python `str(int)` (including negative
zero/leading zero spelling), without changing text seeds. The caller supplies
seed/entropy; the pure core performs no random seed invention or filesystem work.

Version 1 uses the legacy map; versions 2–7 assemble the nine raw regional chunks
in Python insertion order, always sampling the current 480-wide landscape at
160–279 as Python `regional_starter` does. Authored edits preserve the shuffled
site order, 11×11 hall clearings, connecting three-wide paths, timber patches,
starter trees/water, fishing ponds, broad lake and three treasure islands/chests.
Ground and resource traversal order match Python; all four hall sites remain at
least 40 tiles apart. V6+ meadow/forest edits remain version-specific.

`fixtures/terrain/starters.json` freezes 49 actual Python `new_world` results:
every version for seven integer/text seeds, including arbitrary-size negative
integers, numeric and leading-zero text, empty text and Unicode. Two Rust tests
compare every ground cell, ordered resource/metadata and site, assert separation,
hall ground and chests, and test typed/canonical integer input and invalid inputs.
The independent verifier reruns actual Python without replacing frozen records:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_starters.py --check
```

This is geometry/resources only: no NPC roster/gameplay, registry hydration, chunk
expansion or overlays are returned. Slice 3b.ii integrates this base into explicit
raw imported selection below. **3b.iii remains:** village/swamp placement and
versioned overlays,
followed by v7 tiny-water repair sampling neighbors after all edits. Slice 3c
still owns authoritative lifecycle/regrowth. Finite goldens retain the platform
libm release gate described above; this slice is not whole R07 completion.

## Saved-first starter reconstruction (slice 3b.ii)

`ImportedWorld::raw_chunk` now reconstructs a missing starter record using the
registry's original integer or text seed type. `ChunkMetadata` privately retains
that type with the validated namespace; it has no Debug/Serialize implementation
and adds no seed to reports. Numeric and leading-zero text remain text. Original
saved chunks always win, including edited starter ground/resources/extensions.
The saved-only `ChunkReader`, inspection CLI and their capabilities do not change.

The host crops the matching 40×40 authored starter area, replaces `v` with `g`,
and filters/translates resources in their existing order. Generated raw data has
no saved clocks/stumps or private extensions; Python's corresponding raw records
start at clock zero with empty stumps. Current 480 and legacy 1920 dimensions
remain bounded before arithmetic; absent `starter_offset` stays 880 even in a
480 world. Explicit current offset 160 selects the modern starter. Regional
bases always sample 480 terrain at 160–279 even when placed at legacy 880.
Missing terrain versions remain v1; unsupported types/versions/offsets fail at
the import boundary with sanitized diagnostics. No namespace type inference,
RNG substitution, generator upgrade, Python subprocess or archive mutation occurs.

`fixtures/terrain/starter_chunks.json` freezes 86 actual Python `ChunkMap._read`
records across all versions and six integer/text seeds, all nine starter chunk
positions, current/legacy placement, missing version and offset defaults.
`loaded_chunks.rs` compares every row and ordered resource/species field twice,
partial saved siblings, saved starter edits, world bounds and immutable source
registry/archive/files. The fixture verifier does not refresh on `--check`:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_starter_chunks.py --check
```

This remains pre-overlay geometry: village/swamp edits and v7 neighbor-aware
water repair remain 3b.iii; authoritative activation/resources/regrowth remain
3c. No playable landscape or whole R07 completion is claimed. Existing finite
platform-math vectors retain the libm cutover gate above. Performance checks were
not run because this integrates compatibility rather than fixes performance.

## Ordinary village overlay phase (slice 3b.iii.a)

`OrdinaryVillageOverlay::materialize(source, villages, key, world_size)` is an
explicit independent private phase matching **`villages._apply_villages` only**.
It takes already selected/versioned village JSON and a real raw/saved chunk;
it does not select or migrate village metadata. The ordinary phase runs before
swamp edits in Python. This helper does not perform those swamp edits, integrate
with `ImportedWorld::raw_chunk`/`ChunkReader`, or claim `ChunkMap.read` semantics.

Missing/zero record marker paints the radius-24 biome circle and clears the
village square and approaches; marker 1 adds expanded layouts where applicable;
marker 2 adds life structures/plantings; marker 3 preserves player changes.
The exact conditional Python traversal/resource ordering, landmark names, palm
species and barn animal metadata are retained. Surviving resource quantities,
extensions, clock and stump values remain unchanged; scalar numeric lexemes
(including arbitrary-size integers) never pass through floating-point storage.
Materialized JSON container whitespace may change, but exact original source
bytes remain separately available. Payloads have no Debug/Serialize and must
never enter public snapshots. No inputs, imported archive or authoritative state
are mutated. No supply initialization, resource regrowth or NPC routines run.

Relevant unsupported marker values, themes/biomes, dimensions, record shapes,
resource quantities/species, clocks and owning-chunk positions fail with static
schema/ordinal paths. Valid saved organic road/canal/courtyard codes `p/w/c` survive this phase.
The saved-only reader now accepts these codes, verified using four actual
Python organic saved chunks (3b.iii.b.i). Known swamp markers remain untouched by
the ordinary phase; accepting one does not mean its geometry has been applied. Unknown extensions
remain private; they are not interpreted as new gameplay capabilities.

150 actual Python `_apply_villages` paired records start from real raw terrain
or earlier Python overlays with synthetic saved edits. Full cells and ordered
resource/metadata equality covers all three themes, markers 0–3, layout/life
combinations, overlap order, an entire 3×3 seam neighborhood, world corners and
legacy dimensions, mixed-layout terrain-buffer ordering, and four already
organic saved records. Four Rust tests also check idempotence, completed saved edits,
exact source scalar lexemes/extensions and sanitized rejection. Recheck without
refreshing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_village_overlays.py --check
```

**Remaining:** 3b.iii.b must supply actual obstacle-safe village placement and
layout/life/swamp world metadata migrations; the independent v1/v2 phases
below are complete. Then 3b.iii.c must integrate complete immutable saved-first materialization and tiny-water repair
sampling post-overlay neighbors. Slice 3c still owns authoritative lifecycle and
regrowth. The ordinary helper alone is not old-world exploration readiness.
Performance checks were not run for this compatibility port.

## Legacy swamp phase (slice 3b.iii.b.i)

`LegacySwampOverlay::materialize(source, villages, key, world_size)` independently
matches **`swamp_village.apply` for already selected v1 metadata only**. It clears
the old -19…18 square outside the retained center, keeps named village resources
and stumps, and appends the exact ordered legacy structures, tavern and decor.
The first expanded v1 village returns immediately in Python, even when it misses
this chunk; this ordering is preserved. Completed record marker 1 preserves saved
edits. Missing/zero markers and absent/ineligible metadata follow Python behavior.
Source bytes remain separate and exact; retained scalar lexemes, extensions,
quantities, clock and resource order survive. Unsupported metadata, record shapes,
coordinates and versions fail with sanitized ordinal paths. Explicit v2 records
or village markers are rejected even on a completed/no-op record.

43 actual Python paired records begin with real generator versions 1–7 or earlier
ordinary/swamp overlays, followed by saved edits. Full cells/resources/stumps
cover the 3×3 neighborhood, current/legacy bounds, typed seeds, eligibility,
overlap order and first-village/no-touch early returns. Rust additionally checks
idempotence, exact retained scalar spellings/source bytes and sanitized failures.
Four genuine saved organic records from the ordinary-phase corpus verify `p/w/c`
in immutable saved-only decoding, tile queries and unload/reload. Recheck without
refreshing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_legacy_swamp.py --check
```

This helper does not choose or migrate village metadata, check player obstacles,
apply organic v2 geometry, repair water, activate resources or integrate with raw
selection. Organic v2 and record upgrades are now complete in 3b.iii.b.ii below.
The remaining 3b.iii.b task is obstacle-safe placement/layout/life/swamp world
metadata migrations and moved visit keys. These must finish before 3b.iii.c
integrates all overlays and samples post-overlay neighbors for tiny-water repair.
R07 remains incomplete. Performance comparison was not run: this is compatibility
work, not an investigation or fix of a performance problem.

## Organic versioned swamp phase (slice 3b.iii.b.ii)

`SwampOverlay::materialize(source, villages, key, world_size)` independently
matches the complete **`swamp_village.apply` v1/v2 phase for already selected
metadata**. It composes the same legacy implementation with organic `layout()`:
ordered BFS roads, two-wide causeway, homes/access lanes, market/tavern courtyards,
canal water/bridges and reserved decoration placement/footprint. It upgrades
missing/zero/v1 record markers, re-wets unused old corners in Python's y/x order,
preserves the old center's harvested resources and stumps, and appends structures,
decor and sorted water in exact order. Roads overwrite terrain within the center
just as Python does. Retained source scalar lexemes and arbitrary extensions are
never converted through numeric JSON values. The boundary has no Debug/Serialize
and never mutates input archives/metadata or caches. Unsupported versions,
coordinates and malformed records fail with sanitized ordinal paths even when
completed metadata would otherwise produce no edits.

The original record marker is captured once; each organic village commits before
the next. The first eligible v1 village invokes legacy geometry and returns even
when outside the chunk. Mixed v2→v1 metadata can downgrade the result to marker 1,
then upgrade on the next call. These Python semantics are explicitly covered;
blanket idempotence is not imposed on such metadata. Normal completed v2 records
preserve saved edits on repeated calls.

100 actual Python pairs use real terrain generators 1–7, typed seeds, a 3×3
neighborhood, current/legacy bounds and clipped corners, missing/0/1/2 record
markers, metadata eligibility, overlaps/mixed order and saved terrain/resources/
stumps/quantities/extensions. Repeated materialization compares against actual
Python again (four mixed cases differ on repetition). A separate exact layout
oracle checks ordered roads/materials/decor and the complete footprint/water.
Recheck the record and layout corpora independently without refreshing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_organic_swamp.py --check
```

This is record materialization, not obstacle-safe `ensure(world)` metadata
migration or placement. `ImportedWorld::raw_chunk` and saved-only `ChunkReader`
remain unchanged. Next: world metadata migrations/visit keys (3b.iii.b), complete
saved-first overlay integration and post-overlay neighbor water repair (3b.iii.c),
then activation/regrowth (3c). No performance comparison was run for this
compatibility port.

## Ordinary world metadata (slice 3b.iii.b.iii)

`OrdinaryVillageMetadata::materialize(world_bytes)` independently copies the
ordinary part of `ensure_villages`, ending explicitly before `swamp_village.ensure`.
It refreshes original trader dialogue, chooses obstacle-safe villages in Python
preferred/grid order and migrates layout/life metadata. Sizes 120/480/1920 and
absent-offset 880 are supported. Saved building/road/NPC footprints, exact margins,
starter exclusion, sequential NPC additions and final blocked markers match Python.
Marker 0 layout expands as Python does; unknown future markers and malformed
metadata reject even on no-op worlds. Independent worlds without shared_map and
runtime `_chunks` managers reject explicitly; no automatic map conversion occurs.

This private boundary retains exact source bytes, scalar lexemes, extensions,
private quantities/credentials and object/list order. It neither alters imported
snapshots nor exposes a public DTO, performs terrain reads, repairs routines/visits,
updates caches or applies swamp geometry. Existing NPC gameplay is retained.
105 complete actual Python ordinary-phase world pairs also check repeat behavior
and raw JSON insertion order. Terrain-independent metadata rules use isolated
fixtures; a real v7-generated record separately proves opaque terrain preservation.
Typed saved seeds and terrain markers 1–7 survive without conversion or RNG draws.
The fixed text catalog and frozen oracle independently verify against Python:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_village_metadata.py --check
```

Slice **3b.iii.b.iv** below completes the separate swamp ensure metadata phase
and exposes its ordered deferred cache effects.
Then 3b.iii.c integrates all phases and post-overlay neighbor water repair; 3c
retains lifecycle/resources/regrowth. These independent phases do not complete R07.
Performance comparison was not run: no performance issue was investigated.

## Swamp world metadata (slice 3b.iii.b.iv)

`SwampVillageMetadata::materialize(world_bytes)` independently copies the exact
`swamp_village.ensure` phase after ordinary metadata. Reedhaven upgrades only when
layout/life markers permit and the actual organic footprint/water (plus the old
square for swamp-v1 worlds) contains no occupied player building/road or foreign
NPC tile and fits the world bounds. Own residents, empty footprints and unchanged
organic villages follow Python condition order. Existing named locals retain saved
roles/dialogue/extensions while their homes/ambient stops move and routines reset;
missing locals and sleepers receive the Python names/roles/dialogue in Python order.
Other saved residents repair routines using ties-to-even rounding and the first
nearest road in insertion order. Duplicate names retain Python's last-value,
first-key-order behavior. Visit transfers preserve progress/opaque extensions,
destination overwrite, pop/reinsert order and repeated-village chaining.

Source bytes, original number lexemes, private quantities/credentials, unknown
extensions and complete object/list order survive. Relevant malformed metadata and
future versions reject at sanitized ordinal paths even on no-op worlds. Routine
coordinates must be finite with rounded values in the existing i32 tile domain;
unrepresentable coordinates reject explicitly. Sizes 120/480/1920 are supported.
Independent maps and runtime `_chunks` objects reject. This is a private copy,
without Debug/Serialize, not an automatic import migration or prepared gameplay.

`cache_plan().village_ordinals` records each upgraded village in iteration order.
At an authoritative lifecycle boundary, for **each** ordinal, apply the single
upgraded village to every active and camera-cached record, then rebind landscape
resources/stumps from active records and invalidate membership/resource indexes.
The returned plan performs none of these mutations. Applying all upgraded villages
in a single call is not an equivalent replacement for Python's ordered effects.
Actual mutable lifecycle integration remains 3c; no cache effect is silently dropped.

264 frozen actual Python worlds check exact output/order and repeat behavior,
including ordinary-phase composition, typed seeds, terrain versions 1–7, obstacle
footprints/shape holes/legacy corners, bounds, duplicate residents, routines and
visit collisions/chaining. A Python manager-call trace independently verifies the
ordered plan and resource/index invalidation contract. Geometry metadata does not
sample terrain/RNG; a retained real v7 generated record proves opaque preservation.
Four Rust integration tests cover these oracles, scalar/source preservation and
sanitized rejection. The frozen oracle independently recomputes without writing:

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_swamp_metadata.py --check
```

Slice 3b.iii.c below integrates saved-first metadata/overlays, retaining and honoring
the ordered plan in immutable materialization, and post-overlay v7 neighbor water repair.
Slice 3c consumes actual authoritative cache effects and retains
lifecycle/resources/regrowth. R07 remains incomplete and Python remains the playable default. No performance comparison was
run: this bounded compatibility port does not investigate a performance problem.

## Immutable post-overlay terrain (slice 3b.iii.c)

`ImportedRegistry::materialize_chunked(&world_id, &resident_copies)` explicitly
prepares ordinary then swamp metadata on a private copy of that registry world's
payload. `PostOverlayTerrain::read(key)` selects supplied resident records first,
then saved archive bytes, then typed-seed raw/starter reconstruction. Fresh records
receive ordinary and swamp overlays, followed by v7 tiny-water repair. Public
inspection readiness and raw/saved-only reader behavior stay unchanged. Independent
and nonchunked worlds reject here rather than being converted.

`ResidentChunkCopy::new(&world_id, key, bytes)` declares an existing active OR
camera record copy; duplicate keys and foreign owners reject. Each ordinal in the
swamp metadata plan is applied separately to each resident copy before reads.
Combining all upgraded villages into one apply is observably different for
mixed/overlapping versions and is tested against the actual Python ensure path.
Residents bypass fresh-read overlays and water repair, matching Python's existing
active/view precedence. The ordinal plan is retained via `cache_plan()`; actual
authoritative cache writes, active-resource/stump rebinding and index invalidation
remain explicitly required in slice 3c. The private prepared world is the
manager-free metadata result; it does not claim those aggregate runtime effects
have happened. No resident input or imported archive is edited.

Tiny-water repair checks cardinal connectivity of at least three tiles. It first
checks the target's unchanged local water set, avoiding any neighbor reads when
three local tiles suffice. For smaller components it samples neighbors after
saved/starter/village/swamp edits and resident upgrades, without recursively
repairing them. Edge/corner crossings and world boundaries match Python. Removal
filters only target water resources: terrain, stumps, resource order, fractional
supply, extension scalars and clocks retain their values/lexemes. Versions 1–6
skip water repair. Relevant invalid neighbor records fail the read instead of
silently generating replacement terrain. Scratch water sets last only for one
read; no authoritative decoded/snapshot cache or runtime memory-bound claim is
introduced.

`TerrainRecord` and `PostOverlayTerrain` have no Debug/Serialize implementations;
explicit private-byte accessors are for later controlled hydration, not public
DTOs. Sources retain exact bytes. Materialized metadata/records preserve object
and list insertion order and retained scalar spellings, including arbitrary-size
integers. This integration corrected inherited ordering of swamp addition fields
and barn animal maps to match Python's resource snapshot order. Returns are not
automatically installed. Repeated calls on the same inputs are stable; no wall
clock, catch-up, regrowth, industry/supply migration, production or writes occur.

35 frozen actual Python worlds compare real generated ground/resources, saved
edits, all terrain versions, arbitrary-size integer/text seeds, legacy/current
bounds and missing version/offset defaults, ordinary/organic swamp composition,
active/camera plan effects, seam singleton/pair/three-tile bodies, diagonal-chunk
samples reached by cardinal paths, life-added water and neighbor clearing, world
edges and resident-over-saved precedence. Five Rust tests compare full records
and exact insertion order, repeated reads, preserved scalar/source data, private
output exclusion, failure atomicity and filesystem immutability. The oracle runs
actual `ChunkMap.read` with **only** catch-up explicitly disabled, not a replacement
water or terrain implementation. Its manager-free metadata output and actual
resident upgrade results are separately checked.

```sh
python3 rust/crates/lantern-host/tests/fixtures/terrain/generate_materialized.py --check
```

The verifier never refreshes fixtures in check mode. Activation/sleeping/dirty
resources, authoritative ordered cache effects and aggregate rebinding, cache
bounds/eviction/reload and supplied-time catch-up/regrowth remain slice 3c. Python
remains the playable default; R07 is incomplete. Performance comparison is not
needed for this compatibility port and no speedup is claimed.

## Compatibility matrix and remaining work

| Behavior | Coverage |
| --- | --- |
| Immutable saved chunks, quantities/extensions and decoded LRU | Slice 1 complete; unchanged |
| Raw generator versions 1–7, legacy seeds/dimensions and unexplored nonstarter selection | Slice 2 complete; exact 226-record and RNG parity |
| D05 byte seed material → compatible reference RNG draws | Complete; gameplay adoption/state serialization remain later work |
| Authored starter ground/resources and new-world town sites/separation | Slices 3b.i–ii complete; 49 authored starter and 86 raw selection Python goldens |
| Village/swamp placement and versioned overlays | 3b.iii.a ordinary and 3b.iii.b.i–ii legacy/organic swamp phases complete (150 + 43 + 100 Python pairs); 3b.iii.b.iii ordinary obstacle-safe placement/layout/life metadata complete (105 Python worlds); 3b.iii.b.iv swamp world metadata/home/routine/visit migration complete (264 Python worlds plus explicit ordered cache plans) |
| Post-overlay v7 tiny-water repair and boundaries | 3b.iii.c immutable integration complete; 35 actual Python worlds, including resident plans and post-overlay edges/corners; authoritative lifecycle remains 3c |
| Active/sleeping chunks, dirty resources, regrowth and clock catch-up | 3c.i private chunk catch-up preparation complete within finite nonnegative clock domain ≤2^53 (65 actual Python cases); authoritative lifecycle integration, exact larger integer clocks and active regrowth remain |
| Ordinary/independent/shared nonchunked terrain-only hydration and `generate_map` | Slice 3a complete; 35 real maps and six paired hydration goldens; gameplay preparation remains later stages |

No whole R07 completion, playable-world readiness, performance gain, runtime
memory bound, durable save conversion or HTTP integration is claimed.


## Explicit sleeping-chunk catch-up preparation (slice 3c.i)

`CaughtUpChunk::prepare` accepts private record bytes, validated generator/bounds,
a supplied JSON simulation-clock scalar and expanded obstacle tiles. It returns
private owned bytes while retaining the exact source. It independently ports
`ChunkMap.catch_up`: industry/mineral migration and captured-horse thinning,
obstacle/stump-aware horse additions, versioned invalid-tree removal, individual
sleeping resource limits/defaults and capped regeneration, and sleeping stump
round-nine timers floored at `3 * .9`. Future stored clocks produce zero elapsed
time but still receive the supplied clock. It never spawns regrown trees, runs
production, installs an authoritative record or mutates input/archive/source.

Known nonsupply chest/village records survive unchanged; unknown kinds/versions
and malformed records reject with sanitized paths. Untouched extensions/scalar
lexemes remain exact, including arbitrary-size private integers. The explicit
clock domain is finite nonnegative values ≤2^53; larger integer clocks reject
without lossy conversion and remain an R07 compatibility gate. Float spellings
use Python binary64 semantics; the supplied clock's original lexeme is retained.

65 actual Python cases use real generated chunks and authored chest-containing
starter chunks across versions 1–7, 480/1920 dimensions and typed seeds, with
isolated rule fixtures for migration/occupancy/supply/tree/timer boundaries.
`generate_catch_up.py --check` independently recomputes the oracle without
refreshing it. Three Rust tests cover full records, repeats, private source and
quantity preservation and sanitized rejection; a compile-fail doctest prevents
serialization of this private result.

This is an independent prerequisite, not a live chunk lifecycle. Remaining 3c
must extract authoritative obstacles/desired chunks, consume ordered swamp cache
plans with actual rebinding/invalidation, own active/sleeping/dirty resources,
bound derived caches/snapshots, preserve edits through eviction/reload/write
intents, integrate catch-up, support larger exact integer clocks, and implement
active versioned regrowth. No R07 completion or playable Rust load is claimed.
Performance comparison was not run because this is a compatibility port, not a
performance investigation or optimization.
