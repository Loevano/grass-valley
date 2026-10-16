# R07 terrain compatibility

R07 slice 1 provides experimental **read-only saved-chunk loading**, separate from
R06 staging and its unchanged `preserved_only` inspection CLI. It does not create
playable core terrain or port a generator. Python remains the normal host.

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
distinguishing generator vectors remain slice 2 work. These are raw generated
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

## Remaining R07 slices

- **Slice 2:** reproduce all supported Python generators/RNG/seed semantics for
  unexplored chunks, including boundaries and old seeds. Goldens are available
  as compatibility inputs; Rust generator parity remains unimplemented.
- **Slice 3:** new-world creation, authored starter/town/village placement and
  separation, post-overlay tiny-water repair, active/sleeping membership, mutable
  resources/dirty tracking, regrowth and clock catch-up without offline production.
  These must operate on authoritative state with independent snapshot views.

Ordinary/shared nonchunked terrain hydration also remains unimplemented. No whole
R07 completion, playable-world readiness, performance gain, runtime memory bound,
durable save conversion or HTTP integration is claimed by slice 1.
