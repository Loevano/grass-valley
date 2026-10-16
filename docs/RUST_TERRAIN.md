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

Loading additionally rejects unsupported terrain codes (only `g f m s r b d i v`
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
The saved-only reader still rejects those codes: complete saved-reader terrain
code support and corresponding real-swamp cases remain integration work in
3b.iii.b–c. Known swamp markers remain untouched for a later phase;
accepting one does not mean its geometry has been applied. Unknown extensions
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
layout/life/swamp migrations plus versioned swamp geometry. Then 3b.iii.c must
integrate complete immutable saved-first materialization and tiny-water repair
sampling post-overlay neighbors. Slice 3c still owns authoritative lifecycle and
regrowth. The ordinary helper alone is not old-world exploration readiness.
Performance checks were not run for this compatibility port.

## Compatibility matrix and remaining work

| Behavior | Coverage |
| --- | --- |
| Immutable saved chunks, quantities/extensions and decoded LRU | Slice 1 complete; unchanged |
| Raw generator versions 1–7, legacy seeds/dimensions and unexplored nonstarter selection | Slice 2 complete; exact 226-record and RNG parity |
| D05 byte seed material → compatible reference RNG draws | Complete; gameplay adoption/state serialization remain later work |
| Authored starter ground/resources and new-world town sites/separation | Slices 3b.i–ii complete; 49 authored starter and 86 raw selection Python goldens |
| Village/swamp placement and versioned overlays | 3b.iii.a ordinary phase complete; 150 Python paired cases; 3b.iii.b placement/migrations/swamp remaining |
| Post-overlay v7 tiny-water repair and boundaries | 3b.iii.c complete integration/neighbor repair remaining; raw and ordinary-phase boundaries covered |
| Active/sleeping chunks, dirty resources, regrowth and clock catch-up | Slice 3c remaining |
| Ordinary/independent/shared nonchunked terrain-only hydration and `generate_map` | Slice 3a complete; 35 real maps and six paired hydration goldens; gameplay preparation remains later stages |

No whole R07 completion, playable-world readiness, performance gain, runtime
memory bound, durable save conversion or HTTP integration is claimed.
