# R05 state and DTO boundary audit

R05 is a foundation checkpoint, not a playable Rust host. The host library exposes
offline conversion of the model built in R05 slices 1–2. Python launch, HTTP
responses, action bodies, browser behavior, rates and saves remain unchanged.

## Formats and conversion

`lantern-host::dto::encode_save` / `decode_save` use the explicit discriminator
`{"format":"lantern-model","version":1,...}`. This format is only for the
experimental R05 model. It is neither the Python format nor the durable Rust save
format promised by R19. No disk reader/writer, migration or HTTP endpoint is added.
The decoder rejects Python saves, unsupported versions, missing fields, unknown
fields at every modeled level, duplicate object keys and unsupported task variants.
There is no generic extension bag that could silently drop gameplay or leak data.

`public_snapshot` creates a distinct Serialize-only `PublicWorldDto` with format
`lantern-model-observation-v1`. It is an experimental observation, **not** a partial
implementation of `/api/world`. Its list/coordinate fields use familiar
`buildings`, `villagers`, `x`, `y`, `building_index`, and `pending_output` names.
R20–21 must implement the complete existing browser contract, including omitted
catalogs/playback/state, before serving the browser. This observation cannot be
loaded as a save. Neither DTO automatically serializes internal structures.

The host's `ModelWorld` validates ownership and unique town/entity identities
across towns before restoring the single world-wide allocator. Credentials must
reference existing towns. Per-town reconstruction validates task references and
rejects missing/foreign buildings. Save conversion retains the high-water mark
even for deleted IDs. Hosts must retire old jobs before reconstruction/rollback;
the saved route/task counters cannot protect jobs from a different simulation
instance after a rollback to older counters.

All new serials, generations, high-water marks and numeric terrain seeds are
canonical unsigned decimal **strings**, including small values. No conversion
passes through f64/JavaScript numbers. Opaque world/town/login/token keys are
preserved verbatim; the decoder performs no name normalization or authentication.
The model schema currently accepts numeric u64 terrain seeds only. The seed
encoding below also defines text seeds for the later legacy importer/generator;
unsupported legacy seed types must be preserved explicitly or rejected by R06.

Coordinates, goods, work and elapsed seconds remain finite binary64. Conversion
validates units without rounding or clamping. `serde_json`'s existing pinned
version enables `float_roundtrip`: tests showed its default parser changed some
values by multiple ULPs. Its `raw_value` feature inspects numeric lexemes before
conversion: integer quantity/position/time literals outside ±2^53 are rejected,
rather than rounded into f64. Existing binary64 floats (including large values
written with a decimal point/exponent) retain their binary64 semantics. Integer
tiles remain exact i32 values. Negative goods,
work and elapsed time, nonfinite numbers and null numeric fields are rejected.
Signed zero, subnormals and large finite values roundtrip without bit changes.

Task/destination/construction DTOs use external tags, for example
`{"idle":{}}` or `{"in_progress":{"remaining":0.125}}`. External tags allow
numeric fields to be read directly from their original JSON lexemes, without
Serde's internally tagged enum buffering converting them first. Empty variants
use empty structs so unknown fields are rejected there too.

Semantic diagnostics contain schema paths; structural JSON/schema failures
contain `$` plus line/column. Raw Serde messages and offending values are withheld
so malformed private account material cannot appear in diagnostics.

## Field coverage

This table is exhaustive for current R05 internal model fields. Subsystems in the
[R01 saved-state matrix](RUST_CONTRACTS.md#saved-subsystem-coverage-and-featuretest-mapping)
still require feature ports and cross-host evidence; these DTO tests do not mark
any existing gameplay subsystem as verified.

| Internal fact | Model save v1 | Public observation | Boundary evidence |
| --- | --- | --- | --- |
| World ID, numeric seed | Opaque string / exact decimal string | Same | Unicode/large seed roundtrip |
| World-wide allocator high-water mark | Exact decimal string; validate all live IDs across all towns/kinds | Excluded | Deleted-ID gap, stale counter, cross-town collisions, post-reload allocation |
| Town ownership and record order | Town keys plus fully qualified entity IDs; order retained | Town map, ordered entity arrays | Reorder/reload/foreign ownership tests |
| Building ID, tile, construction state/work | Explicit ID/x/y/state DTO | Separate building DTO fields | Exact IDs >2^53; both construction variants; reference and quantity validation |
| Actor ID and fractional position | Explicit ID/position DTO | ID/x/y | Fractional/signed-zero/subnormal roundtrip |
| Actor cargo | Unique map of finite fractional goods | Copied `pending_output` map | Duplicate-key/negative rejection; no alias into authoritative state |
| Actor task, references, phases/legs, elapsed time | Exhaustive tagged DTOs with explicit phase mapping | Task DTO; derived optional `building_index` | All current task/phase variants roundtrip; foreign/dangling links rejected |
| Actor task/request generations | Exact decimal strings | Excluded | JSON reload clears pending route, increments subsequent request generation |
| Derived RouteState/request/path | Excluded; rebuild `NotRequested` | Explicit status string only | Sentinel path/request exclusion; stale-result rejection after reload |
| Token→town mapping | Separate private credentials DTO | Excluded | Preserve tokens, validate owners, reject duplicate keys |
| Account login key/name/town/salt/PIN hash | Separate private account DTO | Excluded | Credential roundtrip; allowlist/sentinel and sanitized-error tests |
| Search frontiers, caches, indexes, deadlines | No fields; decoder rejects attempted injection | No fields | Unknown-field tests, no flatten/Value passthrough, internal types have no Serialize |

Public DTO fields are explicit and detached from mutable simulation objects.
`PrivateAccount`/`PrivateCredentials` have no Debug/Serialize; internal world/town
serialization is also disallowed. Compile-fail tests guard the serialization
boundary. The save encoder is deliberately private-data-capable and must never
be used as a public response encoder.

## Delayed command references

The boundary implements the ID-validation alternative allowed by R05. A legacy
index arrives with `expected_id`, copied from that slot of the observed public
snapshot. Admission validates the authenticated world/town and the current
slot's exact ID, then creates a nonserializable typed queued reference containing
only that stable ID. A changed/missing slot or absent ID metadata is rejected.

At execution, resolve the ID and recheck the authenticated owner and current
entity existence. Reordering after admission keeps the intended target; deletion
returns an error. Reordering before admission rejects a mismatched index/ID.
Task or production changes do not invalidate an unchanged index/ID slot.
Actor and building adapters use separate types. The authenticated owner must
come from host credentials, never from the payload.

This refines slice 1's collection-revision proposal: per-slot expected-ID
validation provides the required stale-index rejection without introducing a
collection counter or trusting callers to bump one when rebuilding/reordering
collections. HTTP/queue dispatch and browser metadata integration remain R12/R20–21.
The current Python/browser contract is unchanged; an index-only request must not
be queued for deferred resolution by the future Rust boundary.

## D05 seed-material encoding v1

`lantern_core::seed_material::gameplay_seed_material_v1` freezes the bytes for
world/subsystem gameplay streams. It is not an RNG implementation or terrain
generator change, and it is not yet used for gameplay/replay draws.

Encoding, in order:

1. ASCII `lantern-gameplay-seed`, a zero byte, then version byte `01`.
2. Seed tag `00` followed by eight-byte unsigned big-endian integer, or tag `01`
   followed by a length-prefixed UTF-8 text seed.
3. Length-prefixed UTF-8 opaque world ID.
4. Length-prefixed UTF-8 subsystem label (nonempty).

Every length is an eight-byte unsigned big-endian UTF-8 **byte count**. No Unicode
normalization, trimming, case folding, native-endian numbers or platform hashes
are used. Feed the resulting bytes to Python-compatible
`random.seed(material, version=2)`: the bytes-seed conversion, SHA-512 mixing and
MT19937 behavior must all be reproduced by R07 before adoption. Seeding MT19937
directly from the raw byte integer is a different algorithm. Terrain generation
keeps its existing version-specific seed conversion; these are gameplay streams.

Three full hexadecimal material vectors are committed in
`rust/crates/lantern-core/tests/seed_material.rs`, independently generated with
Python `struct.pack('>Q', ...)`. Future RNG implementation must also reproduce
these Python reference first three `getrandbits(64)` draws (decimal strings):

| Seed/type | World / subsystem | Reference draws |
| --- | --- | --- |
| unsigned 0 | `legacy` / `economy` | `6330906786906379701`, `11366415972692216933`, `11378455115612559467` |
| unsigned 18446744073709551615 | `世界` / `residents` | `14625236343822065082`, `18176823067106685384`, `16569424490260922686` |
| text `0007` | `legacy` / `production` | `18143875931135448615`, `12762892325570383143`, `1493117759899797826` |

R05 tests check byte vectors, source-type separation, subsystem/world separation,
length framing and Unicode distinctions. The reference draws were generated
with Python, not a Rust RNG. R07 slice 2 now checks those exact draws with its
compatible RNG; gameplay stream adoption and serialized RNG state remain later subsystem work.

## Remaining feature coverage

All R01 HTTP/action/catalog rows and legacy/save-version rows remain unported.
No omitted feature is approved for removal. The saved subsystem matrix assigns:

- R06/R19–20: registry, active world, credentials/provisioning, inactive worlds,
  chunks, unknown legacy state, import/migrations, crash recovery and shutdown.
- R07–11/R22: terrain seeds/versions/resources/chunks, spatial/collision/access,
  navigation service, bounded search, legal paths, movement and confirmed playback.
- R12–13: complete buildings/footprints/roads/links, residents/assignments, stock,
  costs/capacities, skills, food/progression, hall unlocks and balance migrations.
- R14–17: trips/reservations/accounting, markets/resource activity, every production
  recipe/queue, farms/orchards/water/claims, species, boats, barn animals and horses.
- R18: remaining residents, village routines, trades/stories/gifts, equipment and
  adventure membership/clocks/rewards.
- R20–21: complete existing public DTOs/catalogs/auth/command dispatch, queued
  reference metadata, browser prediction/playback and full client acceptance.

The v1 model decoder rejects these extra fields rather than claiming a successful
import of them. R06 must inspect copies and preserve or explicitly reject every
unported legacy feature; it must never discard them into this minimal schema.
R06 now provides [separate private staging](RUST_LEGACY_IMPORT.md) for
ordinary saves and explicit chunk copies, retaining their source bytes instead
of hydrating model v1. Its explicit inspection CLI reports capabilities, audited
version markers and unsupported field locations without advancing simulation.
Public/API DTOs and private save DTOs must expand deliberately with each later
feature port and its tests. Durable save versioning/migrations remain R19.

R07 slice 3b.iii.a additionally exposes an independent private ordinary village
phase. Its materialized payload and original bytes have no Debug/Serialize;
retained resource/extension numbers keep exact scalar lexemes. It is neither a
public DTO nor authoritative hydrated gameplay state. Slice 3b.iii.b.i adds the
same private/source-preserving boundary for legacy swamp-v1 materialization;
its v2 records/metadata still explicitly fail. Slice 3b.iii.b.ii adds separate
private `SwampOverlay` for complete v1/v2 materialization and organic record
upgrades, preserving exact source/scalar extensions and mixed-order semantics.
Saved-only decoding accepts real organic `p/w/c` terrain. Slice 3b.iii.b.iii adds
private `OrdinaryVillageMetadata`, preserving complete world source, scalar lexemes
and ordered extensions while independently migrating ordinary placement/dialogue/
layout/life metadata. Its output is not prepared authoritative state or a public DTO;
it stops before swamp ensure. Slice 3b.iii.b.iv adds separate private
`SwampVillageMetadata` for swamp world metadata/NPC homes/routine repair/visit keys,
retaining world source/scalar/order and sanitized validation even on no-op worlds.
It exposes an ordinal-only ordered cache plan; runtime manager inputs reject and
actual active/view overlays, resource rebinding and index invalidation remain
deferred to the authoritative lifecycle boundary. Neither private payload has
Debug/Serialize or becomes a public DTO. Complete post-overlay water integration
remains 3b.iii.c; see the
[terrain compatibility boundary](RUST_TERRAIN.md#ordinary-village-overlay-phase-slice-3biiia).


R07 slice 3c.i exposes private `CaughtUpChunk` without Debug/Serialize. Explicit
preparation retains exact private source/untouched scalar extensions and returns
owned industry/resource/sleeping-stump catch-up output for a supplied simulation
clock and expanded obstacle tiles. It neither installs authoritative state nor
modifies archive/world/client DTOs or production. The clock domain is explicitly
finite/nonnegative ≤2^53. Slice 3c.ii adds private `ChunkLifecycle`,
`ChunkWriteIntent` and `PrivateChunkSnapshot` without Debug/Serialize: one owner
installs ordered resident cache-plan results, bootstraps active aggregates, extracts
desired chunks/occupied footprints and retains edited eviction/reload through an
in-memory archive plus explicit intents. Camera views are bounded to 49 and private
snapshots to active+49; returned copies never alias authority. Input archive/client
DTOs stay unchanged. Real persistence/acknowledgement/recovery remains R19; public
wire filtering/versioning remains later protocol work. Slice 3c.iii adds explicit
transactional active renewal with immediate aggregate/membership synchronization
and snapshot invalidation. Conversion preserves stump metadata and aggregate append
order; stocks/groups, versioned eligibility and timers match Python. No clock,
production, town inventory or client DTO is changed. Exact larger integer clocks
remain required by 3c.
