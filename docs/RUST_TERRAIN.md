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
`saved chunk unavailable; generation and starter reconstruction unported` error.
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
nonstarter chunks use the saved seed, version and dimensions. Missing starter
chunks explicitly fail; using raw regional terrain there would erase authored
landscapes. The result is explicitly `RawChunk::Saved` or `RawChunk::Generated`,
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
expansion or overlays are returned. Existing imported raw selection still refuses
missing starter chunks. **3b.ii remains:** retain exact registry seed type and
integrate saved-first authored reconstruction with legacy/current dimensions and
offsets. **3b.iii remains:** village/swamp placement and versioned overlays,
followed by v7 tiny-water repair sampling neighbors after all edits. Slice 3c
still owns authoritative lifecycle/regrowth. Finite goldens retain the platform
libm release gate described above; this slice is not whole R07 completion.

## Compatibility matrix and remaining work

| Behavior | Coverage |
| --- | --- |
| Immutable saved chunks, quantities/extensions and decoded LRU | Slice 1 complete; unchanged |
| Raw generator versions 1–7, legacy seeds/dimensions and unexplored nonstarter selection | Slice 2 complete; exact 226-record and RNG parity |
| D05 byte seed material → compatible reference RNG draws | Complete; gameplay adoption/state serialization remain later work |
| Authored starter ground/resources and new-world town sites/separation | Slice 3b.i complete; 49 real Python goldens; saved reconstruction integration remains 3b.ii |
| Village/swamp placement and versioned overlays | Slice 3b.iii remaining |
| Post-overlay v7 tiny-water repair and boundaries | Slice 3b remaining; raw boundaries covered here |
| Active/sleeping chunks, dirty resources, regrowth and clock catch-up | Slice 3c remaining |
| Ordinary/independent/shared nonchunked terrain-only hydration and `generate_map` | Slice 3a complete; 35 real maps and six paired hydration goldens; gameplay preparation remains later stages |

No whole R07 completion, playable-world readiness, performance gain, runtime
memory bound, durable save conversion or HTTP integration is claimed.
