# Rust migration contracts — R01

Audit date: 2026-10-16. Source revision: `935db7b95885c101c62dfd6bf9e16f39b6a7a8fe`.
This is a documentation baseline for [R01](RUST_MIGRATION.md#r01--contracts-and-decisions).
No Rust implementation, save conversion, or production-save inspection occurred.

## Authority, status, and acceptance

**Required parity** means existing behavior or an explicit requirement in the
migration plan, AGENTS.md, [BACKEND.md](BACKEND.md), or [PATHING.md](PATHING.md).
**Design decision** selects an internal approach for later stages, with a reason
and acceptance test. **Recommendation** is a proposed future change, not user
approval of changed gameplay, compatibility, or data loss. Executing R01 approves
this audit, not the execution of later stages or removal of any existing feature.

Every coverage row below has a Rust migration status. `unported` means no Rust
implementation exists; `implemented` means code exists but parity has not been
proved; `verified` requires named Rust and cross-host evidence plus a revision;
`intentionally deferred` requires a reason, owner stage, and cutover consequence.
Python test references identify existing evidence to reuse, not tests run in R01
or proof that Rust works. All existing features are **unported**. None is accepted
for removal. Any unported, unverified, or deferred existing feature blocks R24;
a deferred existing feature can be removed only with explicit user acceptance.

Coverage is split into HTTP, actions, catalogs, saved state, and feature/test
matrices. Rows are cumulative: an action inherits its endpoint's authentication,
errors, save behavior, catalog definitions, and associated feature invariants.
An endpoint passing while one of its actions or catalogs is missing is not parity.
Tests listed are starting points; R03 must add missing negative, replay, import,
and boundary cases rather than assume file-level references prove all contracts.

## HTTP coverage matrix

The runtime boundary is `server/server.py:Handler`, not the development profiler.
R20 slice 1a implements only static assets and the no-active-world GET forms;
active-world and mutation coverage remains open. JSON replies carry `Content-Type: application/json`,
`Cache-Control: no-store`, and `Content-Length`. Successful mutation is HTTP 200.
POST bodies must be JSON objects of 1–4096 bytes. Caught ValueError/TypeError maps
to HTTP 400 with `{error: message}`; unknown POST routes return 404. Do not infer
transactional rollback for every legacy validation failure: R03 captures actual
mutation order. Invalid requests must never gain another town's authority.

| Method / route | Input, authorization, and observable result | Existing evidence | Rust status / stage |
| --- | --- | --- | --- |
| GET `/api/world` | Public active-world snapshot; query `world_id`, `chunks`, `known`, `resources`; mismatched world ID clears known caches. Includes towns, shared map, quests, clock, catalogs and readiness. JSON frozen under lock; transmission outside lock. Invalid chunk queries: 400. | `tests/test_http.py`, `tests/test_chunk_world.py`, `tests/test_shared_world.py` | no-world form/query validation paired / R20 slice 1a; active-world/chunk assembly remains |
| GET `/api/players` | Public `{world_id,sampled_at,players}`; current founder state by town, no ownership tokens. Exact raw path match. | `tests/test_http.py`, `tests/test_remote_players.js`, `tests/test_player_polling.js` | no-world form paired / R20 slice 1a; active founders remain |
| GET `/api/lanterns` | Public `{world_id,quest}`; quest nullable, hazards and playing flag derived. URL path match allows query. | `tests/test_lanterns.py`, `tests/test_lanterns.js` | null no-world form paired / R20 slice 1a; adventure snapshots remain |
| GET `/api/worlds` | Public `{active_world,can_manage,worlds:[{id,name,sandbox,town_count}]}`. Exact raw path match. | `tests/test_http.py`, `tests/test_world_creation.js` | empty registry/peer-Host-Origin metadata paired / R20 slice 1a; populated registry remains |
| POST `/api/worlds/create` | Host only; trimmed name 1–24 chars, boolean sandbox; random ID, fresh explorable world, activate and save. | `tests/test_http.py`, `tests/test_world_creation.js` | unported / R20 |
| POST `/api/worlds/open` | Host only; existing `world_id`; flush old chunks, switch state/tokens, save active selection. | `tests/test_http.py`, `tests/test_chunk_world.py` | unported / R20 |
| POST `/api/worlds/delete` | Host only; existing ID and exact name `confirmation`; delete only selected world and its chunks, clear active selection if necessary, save registry. | `tests/test_http.py`, `tests/test_chunk_world.py` | unported / R20 |
| POST `/api/worlds/regenerate` | Host only; sandbox and exact name confirmation required. New world ID invalidates caches/tokens; preserve registered account names/PIN hashes/town IDs and town names, recreate progress; other worlds unchanged. | `tests/test_http.py`, `tests/test_world_creation.js` | unported / R20 |
| POST `/api/login` | Active matching `world_id` or 409; `player_name,pin`, optional `town_name`, optional Authorization token for legacy claim. Returns `needs_town` or `{token,town_id}`; save after completed login. | `tests/test_http.py`, `tests/test_player_login.js` | unported / R20 |
| POST `/api/join` | Legacy public town creation; active world match required; `name` string conversion, trim/truncate to 24; four-town limit. Returns private token and town ID; save. Retain even if current UI prefers login. | `tests/test_http.py`, `tests/test_shared_world.py` | unported / R20 |
| POST `/api/command` | Active matching `world_id` or 409, raw Authorization token resolves owning town or 401; action rows below. Returns `{ok:true,...result}`. Saves after success except chunked `walk` (next tick persists it). | `tests/test_http.py`, `tests/test_command_response.js`, `tests/test_command_routing.py` | unported / R20–21 |
| GET static / fallback | Serve `web/` including `/`, HTML/JS/CSS and `/favicon.ico` from `icons/`; unknown GET delegates to SimpleHTTPRequestHandler. Its HEAD/static MIME/cache/path behavior must be characterized before replacement. Other unsupported methods inherit HTTP-server behavior. | `tests/test_http.py`, `tests/test_http_bootstrap.py` | static bytes/MIME/HEAD/304/ranges/error fallback paired / R20 slice 1a; directory listing/fallback edge cases remain |

Host-only means **all** of: connection peer is loopback, Host hostname is
`localhost` or a loopback IP, and Origin is absent or exactly `http://` plus Host.
A forged Host/Origin from a LAN peer grants no rights. Normal host listens on
`0.0.0.0:8000`; LAN players retain play access without world-management access.
Commands never trust a client-supplied town ID to select their owner.

Account parity includes Unicode `strip().casefold()` keys; display names 1–24
characters; exactly four ASCII digits for PINs; existing PBKDF2-HMAC-SHA256 hashes
with 200,000 iterations and hex salts; constant-time hash comparison; existing
accounts reconnect even to a full world; token reuse per town; token-authorized
legacy claiming; refusal to claim an already registered town; optional provisioning
merges only existing towns and never overwrites existing accounts. No tokens,
salts, PIN hashes, or provisioning records in public snapshots or fixture logs.
Cryptographic credentials are separate from deterministic gameplay randomness.

## Action coverage matrix

Every literal dispatched by `server.command` appears once below, including legacy
aliases and actions absent from today's panels. All inputs also include `action`
and the HTTP `world_id`; indices refer to the owning town's published lists unless
explicitly an NPC/shared target. Unknown actions are errors. Integers exclude
booleans wherever Python uses `type(value) is int`. Delegated functions retain
their validation and proximity rules; this inventory is not authorization to
simplify them. Empty command results still become `{ok:true}` at HTTP.

| Action | Boundary fields / required behavior | Owner and existing tests | Rust status / stage |
| --- | --- | --- | --- |
| `walk` | dx,dy or steps (1–5 validated input dictionaries); normalized movement, collision substeps, ordered batch, authoritative player reply | `server`; `tests/test_player.py`, `tests/test_player_prediction.js`, `tests/test_command_routing.py` | private-copy fixed-command movement/shared assembly/recovery implemented/paired / R08; HTTP R20 remains |
| `fish` | start/stop founder activity; availability/access checked; player reply; adventures prohibit valley actions | `progression`; `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_wilderness.py` | R17 fishing/boat rules and R18 slice 3 shared activity arbitration, explicit batch replay protection, supplies/practice and owner reload implemented/paired offline; adventure owner slice 4 and aggregate/HTTP R20–21 remain |
| `chop` | start/stop founder activity; availability/access checked; player reply; adventures prohibit valley actions | `progression`; `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_wilderness.py` | R18 slice 3 offline toggle/range, shared tree supplies/stumps, clothing/skill batches and owner reload implemented/paired; aggregate/HTTP R20–21 remain |
| `gather` | start/stop founder activity; availability/access checked; player reply; adventures prohibit valley actions | `progression`; `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_wilderness.py` | R18 slice 3 offline gathering/chest arbitration, supplies, practice and one-time rewards implemented/paired; aggregate/HTTP R20–21 remain |
| `mine` | start/stop founder activity; availability/access checked; player reply; adventures prohibit valley actions | `progression`; `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_wilderness.py` | R18 slice 3 offline immediate rock/biome/range/capacity checks and shared depletion implemented/paired; aggregate/HTTP R20–21 remain |
| `stop_activity` | start/stop founder activity; availability/access checked; player reply; adventures prohibit valley actions | `progression`; `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_wilderness.py` | R18 slice 3 offline activity toggles/stop and injected adventure gate implemented/paired; adventure owner slice 4 and aggregate/HTTP R20–21 remain |
| `build` | kind,x,y,orientation; optional farm_index/orchard_index; footprint/territory/cost/unlock validation; linked field/tree placement | `server`; `tests/test_construction.py`, `tests/test_building_footprints.py`, `tests/test_acres.py`, `tests/test_orchards.py` | ordinary and linked building placement, initial field/tree state and construction/reload implemented/paired / R12 slices 1–3; full ledger R13 and HTTP R20 remain |
| `move` | index,x,y; linked acres/trees move together; validate all before cost; cancellation and cargo accounting; known continuity bug below | `server`; `tests/test_construction.py`, `tests/test_building_range.py`, `tests/test_delivery_resume.py` | atomic stable-ID linked-group moves and worker continuity implemented/paired; explicit transport refunds tested / R12 slices 1–3; full production/reservation accounting R14–17 and HTTP remain |
| `destroy` | index; hall protected; linked removal, index remap, stock/pending-output salvage and trip cancellation | `server`; `tests/test_construction.py`, `tests/test_storage_movement.py`, `tests/test_destroy_tool.js` | linked removal, reference/helper release, stored/pending salvage and explicit transport refunds implemented/tested / R12 slices 1–3; full ledger/production policy and HTTP remain |
| `set_building_orientation` | index,orientation 0–3; completed target; rotate saved footprint, validate changed occupancy | `server`; `tests/test_building_rotation.py`, `tests/test_decoration_rotation.py` | completed ordinary/linked building turns implemented at synthetic boundary / R12; public dispatch remains R20 |
| `set_decoration_orientation` | index,orientation 0–3; completed target; rotate saved footprint, validate changed occupancy | `server`; `tests/test_building_rotation.py`, `tests/test_decoration_rotation.py` | completed decorations implemented/paired / R12 slice 1; HTTP remains |
| `set_fence_orientation` | index,orientation 0–3; completed barn, inward gate, no trapped founder/worker/NPC | `server/barns`; `tests/test_barn_fences.py`, `tests/test_barn_fences.js` | active/upgrade barn inward gate and actor/local safety implemented / R12; species/HTTP integration remains R17/R20 |
| `build_road` | x,y; road placement access/territory and construction; removal preserves unrelated roads | `server`; `tests/test_roads.py`, `tests/test_roads.js` | placement and builder claims/progression/reload implemented/paired / R12 slices 1–2; HTTP remains |
| `remove_road` | x,y; road placement access/territory and construction; removal preserves unrelated roads | `server`; `tests/test_roads.py`, `tests/test_roads.js` | owned removal and atomic builder-claim cancellation implemented/tested / R12 slices 1–2; HTTP remains |
| `assign_settler` | index,building_index; capacity, qualification, continuity and old-task release | `server`; `tests/test_settlers.py`, `tests/test_settler_professions.py`, `tests/test_builder_reassignment.js` | R18 slice 1 offline ordered resident assignment, owner/completion/capacity/qualification checks and ledger/trip cleanup implemented/tested; R12 upgrade-helper behavior retained; aggregate profession/builder/boat cancellation and HTTP/index adapters remain R20–21 |
| `remove_settler` | index; unassign rather than delete resident; cancel pending output/trip, job idle | `server`; `tests/test_settlers.py`, `tests/test_delivery_resume.py` | R18 slice 1 offline unassignment preserves resident identity/position, returns held trip cargo, orphans completed output and resets model intent; aggregate profession/builder/boat cancellation and HTTP/index adapters remain R20–21 |
| `job` | index,job; unlocked completed workplace/free slot or idle/building; assignment chosen in existing list order | `server`; `tests/test_settlers.py`, `tests/test_settler_professions.py` | R18 slice 1 offline current-workplace preference, saved-order free-slot selection, qualification and idle/building roles implemented/tested; R12 construction-role and aggregate profession integration plus HTTP remain R20–21 |
| `configure_storage` | index,enabled resource→boolean map; completed storehouse; omitted allowed resources disabled, legacy allocation fields cleared | `server`; `tests/test_storage_movement.py`, `tests/test_storage_controls.js` | offline typed ledger operation implemented/paired R13 slice 2; index/JSON/HTTP dispatch remains R20–21 |
| `configure_rule` | resource,cap finite nonnegative or null removes cap | `logistics`; `tests/test_production_rules.py`, `tests/test_production_rules.js` | offline typed production-cap operation implemented/tested R13 slice 2; JSON numeric validation/HTTP remains R20–21 |
| `configure_resource_trade` | resource,buy_below,sell_above; nullable finite nonnegative thresholds; buy < sell; atomic configuration | `logistics`; `tests/test_market_sales.py`, `tests/test_resource_overview.js` | R14 slice 3 offline rules/transactions implemented/paired; HTTP dispatch/import R19–21 |
| `configure_market_sale` | resource,cap,enabled,amount 1–10; completed market, finite cap ≥1; preserve disabled cap/strictness | `logistics`; `tests/test_market_sales.py`, `tests/test_market_sales.js` | R14 slice 3 offline rules/transactions implemented/paired; HTTP dispatch/import R19–21 |
| `upgrade_capacity` | index; independent staffing/storage/building upgrade rules, costs and construction; acres/trees/wells exclusions | `server`; `tests/test_worker_capacity.py`, `tests/test_building_upgrades.py`, `tests/test_acre_upgrades.py` | independent staffing upgrades/caps/costs and saved construction implemented/paired / R12 slice 3; full resident/storage integration R13 and HTTP remain |
| `upgrade_building` | index; independent staffing/storage/building upgrade rules, costs and construction; acres/trees/wells exclusions | `server`; `tests/test_worker_capacity.py`, `tests/test_building_upgrades.py`, `tests/test_acre_upgrades.py` | structure limits/materials and saved upgrade completion implemented/paired / R12 slice 3; R18 slice 1 offline housing/comfort integration implemented/paired; full production and HTTP integration remain |
| `upgrade` | hall milestone and costs, reward coins not a cost, maximum level 10, saved construction/upgrade_remaining | `server/progression`; `tests/test_hall_levels.py`, `tests/test_hall_water.py`, `tests/test_hall_refresh.js` | explicit milestone-fact checks, material costs and once-only saved hall rewards implemented/paired / R12 slice 3; R13 slice 3 ledger-derived progression evidence and hall rule queries implemented/paired; actor/building integration and public DTOs R19–21 remain |
| `set_crop` | index,crop; farm or acre; hall unlock; farm applies to linked acres | `server/farming`; `tests/test_acres.py`, `tests/test_farming_cycle.py` | R13 crop unlock and R16 slice 3 linked/direct crop mutation with sown-yield/cargo preservation implemented/paired offline; HTTP R20–21 remain |
| `set_recipe` | index,recipe output key; matching workplace, qualified tier; release obsolete ingredients, reset worker partial production | `server/logistics`; `tests/test_profession_recipes.py`, `tests/test_industry.py` | R13 slice 3 recipe/assigned-worker qualification and batch scaling implemented/paired; R15 slices 2–3 recipe selection cleanup, advanced timers and workshop request lifecycle implemented/paired offline; JSON/HTTP R20–21 remain |
| `queue_workshop` | index,recipe,amount 1–999; qualified recipe, max 50 requests, persistent request ID, ordered remaining counts | `server/logistics`; `tests/test_workshop_queue.py`, `tests/test_workshop_queue.js` | R13 rules and R15 slice 3 ordered workshop request/quantity/qualification lifecycle implemented/paired offline; JSON/HTTP R20–21 remain |
| `cancel_workshop` | index,request_id; remove exact order; active change releases unused inputs, completed goods preserved | `server/logistics`; `tests/test_workshop_queue.py`, `tests/test_workshop_queue.js` | R15 slice 3 head/tail cancellation, obsolete input release, trip restitution and finished-goods preservation implemented/paired offline; JSON/HTTP R20–21 remain |
| `profession_upgrade` | index,upgrade; matching kind, skill/hall/prerequisite/cost checks; boat immediate, others construction | `server/progression`; `tests/test_profession_recipes.py`, `tests/test_building_upgrades.py`, `tests/test_boats.py` | R12 construction plus R13 slice 3 pure equipment offers, costs and skill evidence implemented/paired; ledger/building integration and HTTP R19–21 remain |
| `set_animal_count` | index,animal,count; unlocked barn species, shared capacity; preserve surviving ages/names | `server/progression`; `tests/test_animal_growth.py`, `tests/test_animal_names.py`, `tests/test_livestock_balance.py` | R17 slice 3 offline barn species/unlocks/shared capacity, preserved age/name prefixes and legacy purchase implemented/paired; horses slice 4 and command/HTTP R20–21 remain |
| `buy_animal` | index,animal; legacy purchase excludes horse; breeding only horse, two adults plus supplies; capacity/unlocks | `server/progression`; `tests/test_wild_horses.py`, `tests/test_livestock_balance.py` | R17 slice 3 offline barn species/unlocks/shared capacity, preserved age/name prefixes and legacy purchase implemented/paired; horses slice 4 and command/HTTP R20–21 remain |
| `breed_animal` | index,animal; legacy purchase excludes horse; breeding only horse, two adults plus supplies; capacity/unlocks | `server/progression`; `tests/test_wild_horses.py`, `tests/test_livestock_balance.py` | unported / R17 |
| `butcher_animal` | index shop,barn_index,animal; qualified butcher; select first adult, preserve name in order; replacement only last species member; fetching stage before processing | `server/logistics`; `tests/test_butcher_animation.py`, `tests/test_animal_growth.py`, `tests/test_animal_butchery.js` | R17 slice 3 offline first-adult selection, named last-member replacement, pickup/drag/process, blocked/full-buffer waits, interruption/reload and exact-once rewards implemented/paired; legacy conversion/live integration R19–21 remain |
| `buy_boat` | founder boat tier, hall/fishing practice and costs; player reply | `server/progression`; `tests/test_boats.py`, `tests/test_player_controls.js` | R17 slice 2 offline exploration ownership/cost/requirement transactions and reload implemented/paired; command/HTTP R20–21 remain |
| `upgrade_boat` | founder boat tier, hall/fishing practice and costs; player reply | `server/progression`; `tests/test_boats.py`, `tests/test_player_controls.js` | R17 slice 2 offline exploration ownership/cost/requirement transactions and reload implemented/paired; command/HTTP R20–21 remain |
| `build_boat` | legacy founder boat action; hall ≥3, 20 wood +10 planks (sandbox exemption); player reply | `server`; `tests/test_boats.py` | R17 slice 2 offline exploration ownership/cost/requirement transactions and reload implemented/paired; command/HTTP R20–21 remain |
| `sell_goods` | resource,amount integer 1–200; completed market, free stock, existing sale values | `server/progression`; `tests/test_market_sales.py`, `tests/test_market_sales.js` | R14 slice 3 offline rules/transactions implemented/paired; HTTP dispatch/import R19–21 |
| `create_trade` | target_town,give_resource,give_amount,want_resource,want_amount; distinct goods, 1–100, both markets/hall ≥5, max 10 offers; no reservation on offer | `trading`; `tests/test_trading.py` | R18 slice 3 offline owner/target/terms/market/limit checks and unreserved persistent offers implemented/paired; aggregate/HTTP and secure ID adapter R19–21 remain |
| `accept_trade` | source_town,offer_id; targeted receiver only; recheck outgoing stock/incoming enabled capacity for both, exact-once atomic exchange | `trading`; `tests/test_trading.py` | R18 slice 3 offline atomic two-ledger exchange, outgoing-before-incoming capacity, stale/replay checks and once-only counts/reload implemented/paired; aggregate/HTTP R19–21 remain |
| `cancel_trade` | offer_id; only own offer, no stock exchange | `trading`; `tests/test_trading.py` | R18 slice 3 offline own-offer cancellation without stock exchange implemented/paired; aggregate/HTTP R19–21 remain |
| `lead_horse` | x,y; wild horse within 1.8, hall ≥8 and stable; remove shared wild resource, leading flag | `horses`; `tests/test_wild_horses.py` | unported / R17 |
| `stable_horse` | index; completed nearby stable, capacity/age/saddle checks; leading, riding, mounted flags and animal ages preserved | `horses`; `tests/test_wild_horses.py` | unported / R17 |
| `ride_horse` | index; completed nearby stable, capacity/age/saddle checks; leading, riding, mounted flags and animal ages preserved | `horses`; `tests/test_wild_horses.py` | unported / R17 |
| `return_horse` | index; completed nearby stable, capacity/age/saddle checks; leading, riding, mounted flags and animal ages preserved | `horses`; `tests/test_wild_horses.py` | unported / R17 |
| `toggle_mount` | owned riding horse required; toggle mounted, clear activity, player reply | `horses`; `tests/test_wild_horses.py`, `tests/test_player_controls.js` | unported / R17 |
| `buy_equipment` | item,npc_index; trader catalog and distance ≤1.8; buy price, sell half floor, equipped stock protected; player reply | `equipment`; `tests/test_equipment.py`, `tests/test_equipment_controls.js` | R18 slice 3 offline clothing shop/slot/inventory/proximity and safe swamp unequip rules implemented/paired; aggregate/public reply adapters R19–21 remain |
| `sell_equipment` | item,npc_index; trader catalog and distance ≤1.8; buy price, sell half floor, equipped stock protected; player reply | `equipment`; `tests/test_equipment.py`, `tests/test_equipment_controls.js` | R18 slice 3 offline clothing shop/slot/inventory/proximity and safe swamp unequip rules implemented/paired; aggregate/public reply adapters R19–21 remain |
| `equip_item` | item; owned slot equipment, unsafe swamp unequip prohibited; player reply | `equipment`; `tests/test_equipment.py`, `tests/test_equipment_controls.js` | R18 slice 3 offline clothing shop/slot/inventory/proximity and safe swamp unequip rules implemented/paired; aggregate/public reply adapters R19–21 remain |
| `unequip_item` | item; owned slot equipment, unsafe swamp unequip prohibited; player reply | `equipment`; `tests/test_equipment.py`, `tests/test_equipment_controls.js` | R18 slice 3 offline clothing shop/slot/inventory/proximity and safe swamp unequip rules implemented/paired; aggregate/public reply adapters R19–21 remain |
| `interact_local` | npc_index; nearby routine position; optional item; per-local met/gift/story/cooldown, goods/storage/cost and one-time rewards | `npcs`; `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_wilderness.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `claim_local_gift` | npc_index; nearby routine position; optional item; per-local met/gift/story/cooldown, goods/storage/cost and one-time rewards | `npcs`; `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_wilderness.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `advance_local_story` | npc_index; nearby routine position; optional item; per-local met/gift/story/cooldown, goods/storage/cost and one-time rewards | `npcs`; `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_wilderness.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `trade_local` | npc_index; nearby routine position; optional item; per-local met/gift/story/cooldown, goods/storage/cost and one-time rewards | `npcs`; `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_wilderness.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `buy_worker_tool` | npc_index; nearby routine position; optional item; per-local met/gift/story/cooldown, goods/storage/cost and one-time rewards | `npcs`; `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_wilderness.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `inspect_story_rock` | x,y; nearby story target; preserve quest flags/rewards | `npcs`; `tests/test_npc_stories.py` | R18 slice 3 offline current-position/per-town visits, gifts/stories/survey/supplies/rewards and separate barter restocks implemented/paired; aggregate/HTTP R19–21 remain |
| `give_worker_tool` | index,item; transfer adds target_index; own workers only, one tool each, crafted stock fallback, exact-once inventory | `npcs`; `tests/test_toolsmiths.py`, `tests/test_equipment.py` | R18 slice 3 offline owner-checked atomic inventory/resident/crafted-stock tool transfer and departure receipt drain implemented/paired; aggregate/HTTP R19–21 remain |
| `take_worker_tool` | index,item; transfer adds target_index; own workers only, one tool each, crafted stock fallback, exact-once inventory | `npcs`; `tests/test_toolsmiths.py`, `tests/test_equipment.py` | R18 slice 3 offline owner-checked atomic inventory/resident/crafted-stock tool transfer and departure receipt drain implemented/paired; aggregate/HTTP R19–21 remain |
| `transfer_worker_tool` | index,item; transfer adds target_index; own workers only, one tool each, crafted stock fallback, exact-once inventory | `npcs`; `tests/test_toolsmiths.py`, `tests/test_equipment.py` | R18 slice 3 offline owner-checked atomic inventory/resident/crafted-stock tool transfer and departure receipt drain implemented/paired; aggregate/HTTP R19–21 remain |
| `join_quarry` | shared members by town, two founders start; separate valley position; leaving secures gate to avoid stranding companion | `quests`; `tests/test_quests.py`, `tests/test_quests.js` | unported / R18 |
| `leave_quarry` | shared members by town, two founders start; separate valley position; leaving secures gate to avoid stranding companion | `quests`; `tests/test_quests.py`, `tests/test_quests.js` | unported / R18 |
| `quarry_walk` | dx,dy cardinal; member/start/completion/wall/gate checks | `quests`; `tests/test_quests.py`, `tests/test_quests.js` | unported / R18 |
| `quarry_interact` | target inscription/sun/leaf/moon/treasure; adjacency, ordered runes, all recorded members get reward once | `quests`; `tests/test_quests.py`, `tests/test_quests.js` | unported / R18 |
| `join_lanterns` | shared saved members; mutually exclusive quarry; rejoin preserves course position/checkpoint, clears valley activity | `lanterns`; `tests/test_lanterns.py`, `tests/test_lanterns.js` | unported / R18 |
| `leave_lanterns` | shared saved members; mutually exclusive quarry; rejoin preserves course position/checkpoint, clears valley activity | `lanterns`; `tests/test_lanterns.py`, `tests/test_lanterns.js` | unported / R18 |
| `lanterns_walk` | dx,dy cardinal; two active members, bounds, 0.12 simulation-second step limit, hazard hit/checkpoint | `lanterns`; `tests/test_lanterns.py`, `tests/test_lanterns.js` | unported / R18 |
| `lanterns_collect` | at least two inside finishers at x=11; complete once; every recorded member gets coins/badge | `lanterns`; `tests/test_lanterns.py`, `tests/test_lanterns.js` | unported / R18 |

## Browser-required catalog coverage matrix

Public values and nested identifiers must retain JSON shapes, ordering where the
client uses it, costs, rates, thresholds and labels from the source revision.
Numeric Python dictionary keys become JSON strings. These tables include catalogs
sent by HTTP and catalogs kept locally in the existing browser; porting the host
does not authorize replacing those local UI definitions.

R13 slice 1 exports the full `progression.public_catalog()` into reviewed Rust
data, checked against current Python by `tests/resources.rs` and its read-only
fixture generator. Slice 2 adds offline ledger/storage/cap/reservation/flow rules,
checked against Python helpers and browser overview rows. Other metadata coverage
does not imply executable progression rules or public HTTP coverage.

| Catalog / fields | Source, consumer and verification obligation | Rust status / stage |
| --- | --- | --- |
| `progression.buildings` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.building_footprints` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.crops` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.animals` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.orchard_stage_seconds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.orchard_harvest_yield` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.orchard_reach` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.farming_growth_seconds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.farming_harvest_yield` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.farming_harvest_yields` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.animal_growth_seconds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.butcher_seconds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.animal_meat` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.animal_products` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.tradable_goods` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.job_requirements` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.talent_roots` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.profession_recipes` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.storage_keys` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.production_seconds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.sale_prices` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.sellable_goods` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.buyable_goods` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.manual_sellable_goods` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.boats` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.fish` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.smoked_fish` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.upgrades` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.recipes` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.edible` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.storage_goods` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.fixed_storage` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.wood_worker_capacities` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.storage_upgrade_kinds` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.workplace_storage_sizes` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slices 1–2 data + offline storage/valuation rules implemented/paired; live integration and wire remain R19–21 |
| `progression.worker_capacities` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.decorations` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.food_per_minute` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.crafted_worker_tools` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 3 offline rule queries implemented/paired; actor execution and live wire remain R15–21 |
| `progression.wood_per_cycle` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.stone_per_rock` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `progression.home_stone_cost` | `server/progression.py:public_catalog` → `web/app.js`; compare complete nested value to synthetic Python snapshot in R03/R21 | R13 slice 1 reviewed data + Python drift check; runtime rules R13–18, wire R21 |
| `hall_upgrades`, `hall_ready` | `server.HALL_UPGRADES`, `progression.milestone`; per-town eligibility and current upgrades; `tests/test_hall_levels.py`, `tests/test_hall_water.py` | R13 slice 3 hall offer/milestone queries implemented/paired; live building/ledger integration and wire R19–21 remain |
| `local_stories` | `npcs.STORIES`; stages/clues/rewards consumed by app; `tests/test_npc_stories.py` | R18 slice 3 compiled stories and Python drift check; public export R20–21 remains |
| `quarry_catalog` | `quests.CATALOG`; layout, runes, clue, reward, keepsake; `web/quarry.js`, `tests/test_quests.js` | unported / R18/R21 |
| Courier catalog / board | `lanterns.CATALOG` exists in Python but is **not** sent as an HTTP catalog. `web/lanterns.js` owns its board constants; `/api/lanterns` supplies quest and hazards; `tests/test_lanterns.js` | unported / R18/R21 |
| Equipment, worker tools and local shops | `equipment.ITEMS`, `npcs.WORKER_TOOLS`, `TOOL_SHOPS`, `GIFTS`, `BARTERERS`; local browser definitions and tests must agree with host acceptance/prices/slots; `tests/test_equipment_controls.js`, `tests/test_toolsmiths.py` | R18 slice 3 compiled equipment/tools/gifts/shops/barter metadata and Python drift check; browser/public export R20–21 remains |
| Core buildings/jobs/costs and browser labels | `server.COSTS/JOBS/BUILDING_JOBS/SETTLER_SPACES` extend progression; hall/home/storehouse/woodcutter/farm/acre/fishing hut plus all progression buildings; local app build/help/job/resource/art tables remain compatible; `tests/test_profession_recipes.js`, `tests/test_resource_visibility.js`, `tests/test_settler_professions.js` | unported / R12–18/R21 |
| Terrain and geometry conventions | `spatial.BUILDING_FOOTPRINTS`, biome codes, resource kind/species, orientations and barn fences; `web/scene.js`, `web/app.js`, `web/animations.js`; `tests/test_building_footprints.js`, `tests/test_terrain_rendering.js`, `tests/test_barn_fences.js` | geometry, founder movement and shared terrain/boundaries/recovery implemented/paired R08; construction commands R12 and browser wire R21 remain |

R08 slice 1 geometry evidence: `lantern-core/tests/spatial.rs` invariants and
`lantern-host/tests/spatial.rs` compare immutable private geometry to actual Python
`spatial`, `barns` and navigation vectors regenerated by
`rust/crates/lantern-host/tests/fixtures/spatial/generate.py`. Saved footprint defaults/orientation,
occupied/cardinal perimeter tiles, completed acre/road quirks, actor overlays,
54 clipped/fallback pen layouts, radius/crossing checks and all three road cost
modes are covered. Unknown private source fields remain byte-for-byte; unsupported
geometry rejects explicitly. R08 slice 2 adds local founder radius/sliding, screen
metric, ordered batches, clothes/boat/mount access and immutable private adapters:
95 actual Python cases (475 samples, 138 queries), 455 actual browser predictor
samples and core/host invariants. Frozen Python `--check` independently recomputes
the oracle; four zero/negative saved boat-level cases expose an existing JS/host
discrepancy and are compared against Python only. Shared-world/chunk assembly and blocked-start recovery are implemented by immutable
`PrivateFounderWorldCopy` and disposable `ChunkLifecycle::preview_founder_batch`
(R08 slice 3). Active terrain insertion order, stable recovery, per-command
activation/window refresh and bounded/unavailable search tile rules have actual
Python paired vectors in `fixtures/founder/world_generate.py` and
`lifecycle_generate.py`; source, owner records, camera caches and write intents
remain unchanged on success and failure. Construction commands remain R12; see the
[R08 slice scope](RUST_MIGRATION.md#r08-slice-1--footprints-occupancy-roads-and-fences).

The following concrete identifiers expand the catalog boundary (including nested
recipes and upgrades); omission of an individual kind is a missing feature:

- Production buildings: `well`, `barn`, `sawpit`, `mill`, `smokehouse`, `workshop`, `wooden_bench`, `wooden_statue`, `lantern`, `flower_box`, `wooden_table`, `signpost`, `garden_arch`, `fish_pond`, `orchard`, `orchard_tree`, `bakery`, `butcher`, `tannery`, `mine`, `smelter`, `stable`, `saddler`, `steelworks`, `tradehouse`, `market`.
- Profession recipe workplaces: `workshop`, `bakery`, `butcher`.
- Profession recipe outputs: `tools`, `wooden_bowls`, `crates`, `furniture`, `carved_furniture`, `crafted_axe`, `crafted_saw`, `lanterns`, `flower_boxes`, `wooden_tables`, `signposts`, `garden_arches`, `bread`, `bread_rolls`, `herb_bread`, `fruit_pies`, `cakes`, `prepared_meat`, `herbed_meat`, `smoked_meat`, `sausages`, `meat_pies`.
- Crops: `vegetables`, `wheat`, `carrots`, `fodder`.
- Animals: `chicken`, `sheep`, `pig`, `cow`, `horse`.
- Storage resources: `wood`, `stone`, `food`, `planks`, `wheat`, `fodder`, `flour`, `wool`, `tools`, `manure`, `herbs`, `wooden_bowls`, `crates`, `furniture`, `carved_furniture`, `crafted_axe`, `crafted_saw`, `lanterns`, `flower_boxes`, `wooden_tables`, `signposts`, `garden_arches`, `hides`, `leather`, `iron_ore`, `iron`, `saddles`, `steel`, `trade_crates`, `vegetables`, `carrots`, `fish`, `berries`, `eggs`, `meat`, `milk`, `fruit`, `smoked_fish`, `bread`, `prepared_meat`, `trout`, `salmon`, `sturgeon`, `smoked_trout`, `smoked_salmon`, `smoked_sturgeon`, `bread_rolls`, `herb_bread`, `fruit_pies`, `cakes`, `herbed_meat`, `smoked_meat`, `sausages`, `meat_pies`.
- Profession upgrades: `nets`, `boat`, `improved_boat`, `master_boat`, `wool_nets`, `steel_pickaxe`, `timber_frame`, `improved_millstones`, `geared_millstones`, `masterwork_millstones`, `larger_oven`, `double_hearth_oven`, `masterwork_oven`.
- Equipment: `swamp_boots`, `desert_clothing`.
- Worker tools: `crafted_axe`, `crafted_saw`, `fine_axe`, `fine_fishing_rod`, `fine_sickle`, `steel_fishing_rod`, `masterwork_fishing_rod`, `steel_axe`, `masterwork_axe`, `steel_sickle`, `masterwork_sickle`.
- Stories: `Moss`, `Tobin`, `Flint`.

## Save-version compatibility matrix

Legacy format names here are structural labels, **not invented schema numbers**.
The Python loader has no top-level format discriminator. Required Rust support is
an import contract, currently unported; it does not claim that any Rust loader
exists. Inspect/import must parse copies without invoking Python load/ensure
functions on production data. Preserve source bytes and report each conversion.

| Input / discriminator | Required interpretation and conversion | Existing evidence / future acceptance | Rust status / stage |
| --- | --- | --- | --- |
| No save file | Empty registry, no selected world; create explicitly | `tests/test_http.py`; fresh-root fixture | implemented read-only staging / R06 slice 1; runtime R20 unported |
| Legacy `{world,tokens}` | Wrap as world ID `legacy`, name `My first world`; ownership tokens preserved | `tests/test_http.py:test_legacy_save_migration_preserves_progress_and_ownership`; Rust `legacy_import.rs` | implemented private staging / R06 slice 1; playable hydration unported |
| Registry `{worlds,active_world}` | All entries, names, worlds, tokens, optional players; missing selected ID means no active world; inactive worlds retained | `tests/test_http.py`; Rust `legacy_import.rs` repeated-byte import and metadata/ownership tests | implemented private staging / R06 slice 1; durable conversion R19 unported |
| Independent town landscapes, no `shared_map` | Keep independent terrain/resources/NPCs; no automatic shared-map conversion | `tests/test_shared_world.py:test_old_world_is_not_converted`, `tests/test_terrain.py`; Rust `legacy_import.rs` | source preserved / R06 slice 1; terrain-only hydration and generate_map implemented / R07 slice 3a; other features R08+ |
| Shared nonchunked landscape | Single map persisted, town map fields rebound; retain 120-tile and saved legacy dimensions | `tests/test_shared_world.py`, `tests/test_chunk_world.py`; Rust `legacy_import.rs` | source preserved / R06 slice 1; terrain-only shared bindings, hydration and generate_map implemented / R07 slice 3a; other features R08+ |
| Chunked landscape (`chunked`, `chunk_size`) | Registry omits terrain/resources/stumps; require associated chunk directory, seed, dimensions, starter offset, clock; default missing starter offset 880, not current 160. Retain legacy 1920 and current 480 dimensions, 40-tile chunks | `tests/test_chunk_world.py`; Rust `legacy_chunks.rs` active/evicted/inactive raw copies, defaults, required keys and sanitized failures | implemented private staging / R06 slice 2 and immutable saved views / R07 slice 1; sparse completeness unverified without manifest; raw nonstarter generation implemented / R07 slice 2; typed-seed saved-first starter reconstruction / slice 3b.ii; ordinary/swamp metadata, overlays, water repair and private authoritative lifecycle/resource renewal implemented / R07; exact clocks complete 3c.iv; durable persistence R19 |
| Terrain version absent or 1 | Original starter/map generator and legacy chunks; Python seed/sample semantics. Never substitute latest | `tests/test_regional_terrain.py` v1 digest, `tests/test_terrain.py` | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 2 | First regional layout, connected rivers/lake, original per-chunk RNG | `tests/test_regional_terrain.py` v2 digest and connectivity | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 3 | `terrain_regions.sample_tile`, revised habitats and islands; chunk-v3 random seed prefix | `tests/test_regional_terrain.py`; Rust `generated_chunks.rs` exact cell/resource goldens distinguish v3 habitats | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 4 | v3 plus sparse grass | Rust `generated_chunks.rs` frozen vectors distinguish v3/v4 | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 5 | v4 plus open farmland | Rust `generated_chunks.rs` frozen vectors distinguish v4/v5 | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 6 | v5 plus green habitats, forest-only regrowth and meadow clearings | `tests/test_terrain.py`, reconstruction; preserve old grassland regrowth for older versions | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Terrain version 7 | v6 plus removal of water components smaller than three tiles, including seams **after** starter/village edits; current new explorable worlds | `tests/test_chunk_world.py`, `tests/test_regional_terrain.py` | saved-chunk decoding and raw generation implemented / R07 slices 1–2; nonchunked terrain-only hydration / slice 3a; authored starters and typed-seed saved-first reconstruction / slices 3b.i–ii; ordinary/swamp overlays, post-overlay water repair, private activation/renewal and exact clocks implemented / R07; playable host integration remains later stages |
| Unsupported/future versions or malformed references | Reject with location/reason before writes; never default an explicit unknown version to 1 or 7 | Rust `legacy_import.rs` and `legacy_chunks.rs` reject known versions, registry/account/assignment/farm/orchard links and chunk owners/keys/positions | implemented inspection validation / R06; canonical trade/adventure owners and offer/craft ID uniqueness also checked by `legacy_inspect.rs`; full gameplay hydration/reference coverage R08–18 unported |
| Optional `<save-stem>.players.json` | Private world→normalized account map; merge only known worlds/towns; existing account wins; archive with backup | `tests/test_http.py` provisioning and legacy claim cases; Rust `legacy_import.rs` | implemented explicit-copy staging / R06 slice 1; explicit inspection CLI R06 implemented; automatic discovery/authentication/durable backup unported |
| Read-only inspection | Explicit source/provision/chunk copies, capability/unsupported-field report, marker diagnostics; no private values, implicit root discovery, migrations, generation, simulation or writes | Rust `legacy_inspect.rs` CLI success/failure/privacy/read-only/repeatability tests; [command and limits](RUST_LEGACY_IMPORT.md#inspection-command-slice-3) | implemented / R06; preserved-only status is not playable compatibility |
| Rust schema | Explicit schema/RNG versions; native self-contained generations; source conversion to new roots, never overwritten | R19 persistence tests plus boat/hauler/construction disk-restart and chunk capture tests; [protocol/limits](RUST_SAVES.md) | model v1–3/construction v1–2 readers, aggregate registry/ledger v1 and generation manifests v1–2, exact named MT19937 state, atomic registry/chunks and flush/shutdown/fallback recovery implemented offline in slices 1–2; explicit native copy/recovery/cleanup and approved preserved-only Python archive/restore tools implemented in slice 3; complete playable Python hydration and native reverse export remain compatibility/cutover gates |

All versions 1–7 remain required even for **unvisited** chunks: importing only
already generated chunks cannot preserve a world that can still be explored.
R07 slice 1 loads saved copies through an [experimental immutable reader](RUST_TERRAIN.md);
slice 2 additionally establishes exact raw generator/RNG parity across 226 frozen
real Python records and independent RNG vectors. Raw unexplored selection now reconstructs missing starter terrain using the
original registry seed type; post-overlay playable exploration remains slices
3b.iii–3c.
Slice 3a implements pure legacy maps and immutable nonchunked terrain-only
hydration with exact source retention and shared/independent bindings; full
resource/bootstrap/industry/NPC preparation remains with its gameplay stages.
Slice 3b.i adds pure authored 120×120 ground/resources and ordered town sites
for versions 1–7, verified against 49 real Python new-world goldens; typed-seed
saved-first reconstruction integration in 3b.ii adds 86 real Python raw-read
goldens with legacy/current dimensions and offsets. Village/swamp and post-overlay
water seams remain 3b.iii.b–c. The isolated 3b.iii.a ordinary village phase
now matches 150 actual Python pairs, preserving record markers, ordered life
geometry, saved edits and private scalar lexemes/extensions. It does not change
raw reads or the inspection CLI; placement/swamp migrations and complete
post-overlay neighbor sampling remain required. Slice 3b.iii.b.i supplies an
independent private swamp-v1 phase (43 actual Python pairs) and valid saved
organic `p/w/c` decoding (four real saved chunks). Slice 3b.iii.b.ii supplies
complete explicit versioned swamp materialization including organic v2 geometry
and record upgrades (100 actual Python pairs plus exact layout), retaining mixed
v1/v2 order and Python repeat behavior. Slice 3b.iii.b.iii adds independent
ordinary obstacle-safe placement/dialogue/layout/life metadata (105 complete actual
Python worlds plus repeat/insertion-order checks), preserving private source and
scalar lexemes. It explicitly stops before swamp ensure; slice 3b.iii.b.iv adds
separate private `SwampVillageMetadata` for obstacle/bounds-safe swamp ensure,
NPC homes/routine repair and moved visit keys (264 complete actual Python worlds,
order/repeat checks and ordered deferred active/view cache plans). Runtime managers
reject; authoritative cache writes/rebinding/invalidation remain at the later
lifecycle boundary. Slice 3b.iii.c now composes immutable saved-first metadata,
ordinary/swamp overlays, ordered resident-copy plans and v7 post-overlay neighbor
water repair (35 actual Python worlds with complete record/insertion-order checks).
Resource catch-up integration, authoritative caches/rebinding and regrowth are
implemented by 3c below. NPC gameplay remains R18; the current completed R07
foundation is audited below.

R07 slice 3c.i adds independent private `CaughtUpChunk::prepare` for the actual
chunk-local Python `ChunkMap.catch_up` industry/mineral, invalid-tree, individual
supply and sleeping-stump rules (65 actual Python cases). It accepts a supplied
simulation-clock scalar and already expanded authoritative obstacle tiles, keeps
source/untouched numeric lexemes/extensions exact, and performs no production,
active tree regrowth, archive writes or lifecycle installation. Its supplied finite
nonnegative float/arbitrary-size integer clocks now use exact Python arithmetic
(3c.iv below). Slice 3c.ii adds a private authoritative `ChunkLifecycle` owner:
ordered resident swamp-plan installation, initial active aggregate ownership,
desired chunks/occupied footprints, active/camera precedence, 49-camera LRU,
active-plus-49 private snapshot copies and edited flush/evict/reload through
in-memory archive/write intents (19 actual Python traces). Failed operations
publish no partial state; imported sources remain immutable. R19 owns real
persistence/recovery and later protocol stages own public wire snapshots. Exact
clocks are complete in 3c.iv. Slice 3c.iii adds explicit transactional active
renewal, invalid-tree cleanup and immediate chunk membership sync. Fourteen new actual
Python traces cover versioned stumps, round-nine timers, occupied ordered conversion,
water groups/duplicate coordinates across seams, current limits and tree cap rescaling,
passive reads and active/sleeping transitions (33 lifecycle traces total). No clock or
production advances; metadata and private extensions remain owned and exact. Active
obstacles preserve Python origin/floor-founder behavior, separately from sleeping
footprints/ties-to-even founders. Slice 3c.iv removes the former 2^53 clock limit
through exact integer subtraction, correctly rounded integer true division,
Python mixed numeric conversion/equality, lossless scalar retention and explicit
arithmetic overflow errors. 3,096 independent Python arithmetic cases, 83 catch-up
cases and nine new clock lifecycle traces validate this boundary. All R07 criteria
are complete as a private migration foundation; see the [completion audit](RUST_MIGRATION.md#r07-slice-3civ-exact-clocks-and-completion-audit).
Finite platform-math vectors retain the R23 release gate. Python stays default.

Saved records take precedence over regeneration; seeded base generation and
idempotent legacy repairs/overlays are separate operations. `ChunkMap.read` can
catch up resources and apply village/terrain repair, so it is not a raw inspection
API. Preserve changes to mined resources, trees/stumps, and village ground.

R06 slice 1 evidence is `rust/crates/lantern-host/tests/legacy_import.rs` and its
three synthetic fixtures. [Staging scope and remaining validation](RUST_LEGACY_IMPORT.md)
distinguish preserved private source data from validated metadata and playable
core state. Original numeric lexemes and all unknown gameplay data remain in an
immutable archive; no inventories, phases, NPCs or animals are hydrated into the
minimal R05 model or silently discarded. The report always says `preserved_only`.
All existing gameplay subsystem rows below still require their runtime ports.

## Saved subsystem coverage and feature/test mapping

R05 slice 1 implements identity/unit foundations: see
`rust/crates/lantern-core/tests/primitives.rs` for world-wide allocation, owner
validation, reload-counter/collision rejection, reference stability in a synthetic
collection, exact large IDs and bit-preserving fractional units. Two compile-fail
doc tests enforce kind/unit separation. These are **implemented** primitives,
not cross-host feature parity. Saved subsystem rows below remain unported.
R05 slice 2 adds the **implemented** actor/building/task model foundation;
`rust/crates/lantern-core/tests/model.rs` covers independent fractional position
and cargo, ownership/dangling-reference checks, reorder/reconstruction continuity,
workplace relocation, task/request generations and stale/duplicate route results.
Route state is derived and excluded from internal `ActorState` reconstruction.
R05 slice 3 adds **implemented** explicit model save/public conversion and owned
world validation. `rust/crates/lantern-host/tests/boundary.rs` covers JSON
roundtrip/precision, private/derived-field exclusion, strict schema/map handling,
allocator continuity and expected-ID validation of delayed index references.
`rust/crates/lantern-core/tests/seed_material.rs` freezes D05 encoding vectors;
host doc tests disallow automatic internal-state serialization. The
[field and feature coverage audit](RUST_STATE_DTOS.md) specifies these experimental
formats and every remaining feature boundary. Full legacy import, durable saves,
browser HTTP DTOs and all gameplay subsystem rows below remain unported.

This is the semantic persistence inventory, including state nested below buildings,
workers, founders, landscapes, and adventures. It also covers autonomous behavior
without a command. All rows require import → play → save → reload comparisons.

| Subsystem / required state and behavior | Source | Existing test mapping / missing acceptance | Rust status / stage |
| --- | --- | --- | --- |
| Registry, active world, sandbox, town sites/origins, ownership and credentials | `server.py`, `shared_world.py`, `player_login.py` | `tests/test_http.py`, `tests/test_shared_world.py`, `tests/test_world_creation.js`, `tests/test_player_login.js`; inactive-world immobility, LAN isolation, shutdown save | unported / R06/R19–21 |
| Terrain seed/version/rows/resources/species, map size, chunk metadata/clock, resource supply/max/regeneration and stumps | `terrain.py`, `terrain_regions.py`, `chunk_world.py` | `tests/test_terrain.py`, `tests/test_regional_terrain.py`, `tests/test_chunk_world.py`, `tests/test_map_chunks.js`, `tests/test_scroll_chunks.js`; v1–7 seam/eviction/unvisited goldens; Rust terrain/catch-up/lifecycle evidence | implemented private terrain/chunk foundation / R07; playable integration and durable persistence remain later stages |
| Landscape migration markers and forage halls | `terrain.py`, `progression.py`, `server.prepare_landscapes` | `forage_version` absent/1, `industry_version` absent/1/2, `mineral_version` absent/1, `npc_roster_version` through 3 and `npc_placement_version`; `tests/test_wilderness.py`, `tests/test_npcs.py`, `tests/test_local_expansion.py`; import twice must not duplicate resources | chunk mineral/industry and village/swamp markers implemented / R07; full gameplay preparation, forage and NPC phases remain later feature/host stages |
| Buildings, levels, footprints/orientation, roads/construction, linked farm/orchard indices, barn gates/pens | `server.py`, `spatial.py`, `barns.py` | `tests/test_construction.py`, `tests/test_building_footprints.py`, `tests/test_building_rotation.py`, `tests/test_decoration_rotation.py`, `tests/test_barn_fences.py`, `tests/test_roads.py`, matching JS controls/geometry | unported / R12 |
| Town stock/goods/food valuation/coins, building stored goods, enabled filters, capacities, input/output buffers, pending output and reserved ingredients | `progression.py`, `logistics.py` | `tests/test_economy.py`, `tests/test_resource_precision.py`, `tests/test_storage_movement.py`, `tests/test_storage_unlocks.js`, `tests/test_logistics.py`, `tests/test_resource_visibility.js`; conservation includes cargo and reserved stock | R13 slice 2 offline single-owner ledger/storage/reservations/food accounting implemented/paired; ordinary-save migrations, model/logistics and HTTP integration remain R14–21 |
| Legacy balance/staffing conversions | `server.migrate_*`, `progression.ensure_inventory/relocate_market_stock` | `wood_chain_version`, `grain_chain_version`, `worker_capacity_version`, `acre_upgrade_version` absent→1; `inventory_version`, `market_storage_version` absent→1; `food_balance_version` absent→1→2. `tests/test_wood_chain.py`, `tests/test_grain_chain.py`, `tests/test_worker_capacity.py`, `tests/test_acre_upgrades.py`, `tests/test_economy.py`; preserve partial grain work queue, refunds, tools, displaced residents and cargo; idempotence | unported ordinary-save balance/staffing migrations / R19; R13 models current explicit facts |
| Hall milestones/upgrade_remaining, achievements, harvested/produced counters and skills | `server.py`, `progression.py` | `tests/test_hall_levels.py`, `tests/test_hall_water.py`, `tests/test_progression.py`, `tests/test_progression_controls.js`; water radius, completion eligibility, reward once | R13 slice 3 milestone rules, local ledger-produced counters and personal skill queries implemented/paired; saved clocks/migrations and integrated building/host lifecycle remain R15/R19–21 |
| Residents: names/counters, talents, skill practice, housing, hunger/shortage, arrivals/departures and assignment/capacity | `server.py`, `progression.py` | `tests/test_settlers.py`, `tests/test_settler_professions.py`, `tests/test_villager_names.py`, `tests/test_departures.py`, `tests/test_cottage_autofill.py`, `tests/test_worker_capacity.py` | R18 slice 1 offline persistent roster/names/talents, housing/loyalty, meals/shortages, departures/replacements, sandbox/autofill, assignments and versioned owner reload implemented/paired; explicit legacy workplace resolution and returned-tool receipts tested; ordinary-save hydration R19 and aggregate profession/builder/boat/navigation cleanup, live invocation of the slice-3 atomic founder receipt drain and playable host remain R20–21 |
| Founder continuous position/facing, activity/status, practice, boat tiers, inventory/equipment/tools, horse/mount/saddle, keepsakes | `server.py`, `progression.py`, `equipment.py`, `horses.py`, `npcs.py` | `tests/test_player.py`, `tests/test_player_mining.py`, `tests/test_boats.py`, `tests/test_equipment.py`, `tests/test_player_prediction.js`, `tests/test_player_polling.js`, `tests/test_remote_players.js` | movement/facing, boat/mount/clothing access and shared recovery implemented/paired R08; R18 slice 3 offline activity/status/practice, inventory/equipment/tools/keepsakes and reload implemented/paired; horse/mount gameplay R17 slice 4 and adventure arbitration R18 slice 4 remain; durable/live integration R19–21 |
| Worker work_animation task/phase/elapsed/position/path/playback, work_target/claims, watering_round, pending leg/route_waiting, cargo/pending workplace | `work_cycles.py`, `server.py`, `farming.py`, `logistics.py` | `tests/test_simulation.py`, `tests/test_delivery_resume.py`, `tests/test_workplace_dropoff.py`, `tests/test_woodcutter_targets.py`, `tests/test_barn_workers.py`, `tests/test_villager_playback.js`, `tests/test_path_retention.js`; relocate/reassign/reload continuity | R11 generic movement/recovery and v2 model reload implemented; R15 slice 1 offline woodcutting claims, productive clocks, supply clipping, tools/practice, pending/drop-off/orphan accounting and claim reload implemented/paired; R15 slices 2–3 processor approach/batch/unload/pause, advanced/workshop lifecycle and v1/v2 processor reload implemented/paired; R16 slices 1–2 field/water/harvest trip actors implemented/paired offline; R16 slice 3 orchard trips, crop edits, relocation and validated owner reload implemented/paired; R17 slices 1–2 land-fishing and boat/crew/species-cargo owners implemented/paired; other worker actors remain R17–18, live wire R21 and integrated W2/W3 acceptance remain open |
| Builder construction_target/position/path/arrival, upgrade helpers and claimed sites | `server.construct` | `tests/test_construction.py`, `tests/test_builder_reassignment.js`, `tests/test_construction.js`; retain progress/claim across reload | unported / R11–12 |
| Logistics trips/legs/positions/targets, pickup state, held sale goods, producer output, reservations, cancellations | `logistics.py` | `tests/test_delivery_resume.py`, `tests/test_hauling.py`, `tests/test_logistics.py`, `tests/test_delivery_animation.js`, `tests/test_hauling.js`; exact-once transfer/refund and unused tick continuation | R14 slices 1–3 offline producer/hauler/merchant trips implemented/paired: saved IDs/endpoints/phase/cargo, ordered rules/receipts, ledger reservations, blocked/full/partial delivery, returns and cancellation/reload; durable/public integration R19–21 |
| Production caps, market purchases/sales thresholds/strictness/batches; resource_activity seconds and 60-second buckets | `logistics.py`, `resource_activity.py` | `tests/test_production_rules.py`, `tests/test_market_sales.py`, `tests/test_resource_activity.py`, `tests/test_resource_overview.js`; imports do not advance production milestones, hauling/reservation is not production | R13 slice 2 offline caps/minute activity and R14 slice 3 merchant thresholds, batches, purchases/sales and cumulative trade totals implemented/paired; durable/public integration remains R19–21 |
| Profession production_work, recipe, upgrades, grain_work_queue, workshop craft_queue IDs/remaining/inputs | `progression.py`, `logistics.py` | `tests/test_profession_recipes.py`, `tests/test_industry.py`, `tests/test_recipe_balance.py`, `tests/test_wood_chain.py`, `tests/test_grain_chain.py`, `tests/test_workshop_queue.py`, `tests/test_toolsmiths.py`; every catalog recipe, qualification, fuel and cap | R15 slices 1–3 offline gathering/processor actors complete: all 13 recipe families/32 recipes, shared inputs, recipe changes, tools/practice, finite ore, grain partial work, ordered workshop IDs/remaining quantities, head/tail cancellation, output/trip recovery and v1/v2 model reload implemented/paired; ordinary saves, host/wire and integrated acceptance remain R19–23 |
| Farming field states/crop/pass/claims/water/growth/harvest cargo; orchard tree stages/yields | `farming.py`, `orchards.py`, `work_cycles.py` | `tests/test_farming_cycle.py`, `tests/test_farming_destinations.py`, `tests/test_acres.py`, `tests/test_orchards.py`, `tests/test_farming_animation.js`, `tests/test_farming_water.js`; multi-acre batch and first-task fairness | R16 slice 1 offline shared field work/growth, roster-aware claims, farm passes/rows, movement approaches, capped/fertilized yields and ledger harvest reservations implemented/paired; slice 2 water-source/round/collection/return, mixed-crop harvest continuation and blocked original-workplace deposit/reassignment implemented/paired; slice 3 orchard growth/restock/picking/returns, crop edits, linked relocation and validated owner reload/legacy agricultural facts implemented/paired; integrated legacy actor/durable saves R19, live playback/host R20–21 |
| Fishing piers, boat crew/captain trip/depth/reach, mixed species cargo, catch chance, XP and species-preserving smoking | `work_cycles.py`, `progression.py`, `logistics.py` | `tests/test_fishing_range.py`, `tests/test_fishing_luck.py`, `tests/test_boats.py`, `tests/test_smoking.py`, `tests/test_fishing_animation.js`; all fish/smoked fish catalog members | R17 slice 1 offline saved piers, land-fisher movement/cargo/catch batches, founder land activity, connected supply/depth and validated reload implemented/paired; R17 slice 2 offline captain/crew voyages, species catches/shared holds/berth returns, exploration purchases/afloat bonuses and strict reload implemented/paired; durable/live integration R19–21 remain; R15 slice 2 species-preserving base smokehouse actor implemented/paired |
| Barn/stable animal counts/ages/names, growth/output clocks, keeper rounds and butchering order ID/name/pickup/stage/remaining | `barns.py`, `progression.py`, `logistics.py`, `work_cycles.py` | `tests/test_animal_growth.py`, `tests/test_animal_names.py`, `tests/test_livestock_balance.py`, `tests/test_barn_workers.py`, `tests/test_butcher_animation.py`, `tests/test_animal_timers.js`; no animation-driven rewards | R17 slice 3 barn counts/ages/names, independent growth, keeper rounds and manual order stages implemented/paired with strict owner reload; stable/horse behavior slice 4 and durable/live integration R19–21 remain |
| Wild horse home/wander timer/from/start, capture, settlement, breeding, riding unlock | `horses.py` | `tests/test_wild_horses.py`; saved ecology separate from decorative horse display | unported / R17 |
| NPC roster/placement, villages/layout/life/swamp versions, saved routine paths/phase/timing, outdoor lanes/tavern, visits/stories/gifts/cooldowns | `npcs.py`, `villages.py`, `swamp_village.py`, `village_life.py` | `tests/test_npcs.py`, `tests/test_npc_stories.py`, `tests/test_villages.py`, `tests/test_village_life.py`, `tests/test_npc_routines.js`; layout versions 1/2, life 0/1, chunk village overlays through 3, swamp 1/2; preserve occupied player ground and moved visit keys | R07 ordinary/swamp placement/layout/life/home/visit migrations and ordered active/camera lifecycle effects implemented/tested; R18 slice 2 ambient routes/work/waits/visitor pauses/sleepers, organic lanes/fallback, active-only collision, stable visit anchors, separate restocks and owner reload implemented/paired; R18 slice 3 non-village roster/placement/canopy relocation/visit moves and NPC interactions/gifts/stories/local trades implemented/paired on private copies; durable/public aggregate integration R19–21 |
| Town trade_offers IDs/targets/terms and completed_trades | `trading.py` | `tests/test_trading.py`; owner/target isolation, outgoing/incoming capacity checked atomically, stale offer cannot double reward | R18 slice 3 offline offers, atomic exchanges, counts and world-checked reload implemented/paired; durable/live aggregate integration R19–21 remains |
| Quarry members/inside/positions, started/completed/gate_latched, rune sequence/solved/message, coins and keepsakes | `quests.py` | `tests/test_quests.py`, `tests/test_quests.js`; reconnect and departed participant reward eligibility | unported / R18 |
| Courier members/inside/positions/checkpoints/hits/last_step, elapsed/completed/message, hazards derived | `lanterns.py` | `tests/test_lanterns.py`, `tests/test_lanterns.js`; two-player pause/resume, 0.25s elapsed clamp/0.05s hit slices, one-time reward | unported / R18 |
| Navigation/collision and deterministic destination selection, roads, bounds and pending searches | `navigation.py`, `spatial.py`, `work_cycles.py`, `logistics.py` | `tests/test_navigation.py`, `tests/test_spatial.py`, `tests/test_host_search_queue.py`, `tests/test_command_routing.py`, `tests/test_road_routing.js`; PATHING actor-specific rules are normative, queues/caches derived and rebuilt | occupancy/acres, fences, corners and road costs implemented/paired; explicit pier/boat overlay interfaces in R08 slice 1; local founder collision/access/bounds implemented/paired R08 slice 2; shared assembly/unloaded terrain/recovery and terrain/search bounds implemented/paired R08 slice 3; selected no_std core search implemented/tested in R09 slice 2; actual Python route/access/cost parity implemented/paired R09 slice 3 (418 frozen routes, exact paths/outcomes, independent costs; experimental core only); scheduling/travel/shared navigation R10–11/R22 remains |
| Browser scene/playback/prediction, UI catalogs, panels/help, animations, chunk cache, selection/rotation, overview and keybindings | `web/app.js`, `web/scene.js`, `web/animations.js`, `web/quarry.js`, `web/lanterns.js` | `tests/test_scene.js`, `tests/test_villager_playback.js`, `tests/test_map_overview.js`, `tests/test_command_response.js`, `tests/test_keybindings.js`, plus action/catalog tests above; retain existing browser, no client port | unported (Rust integration) / R21 |
| Derived-only runtime state | `_chunks`, shared map aliases, spatial indexes, caches, route frontiers/admission reservations, scene overlays and jobs, host monotonic deadlines | `tests/test_host_search_queue.py`, `tests/test_shared_world.py`, `tests/test_scene.js`; rebuild from persistent task/position/cargo, do not serialize locks/frontiers or let client mutate authoritative objects | unported / R05/R10/R19 |

## Architecture decisions and consequences

These internal choices guide later stages. Recommendations that would change
observable behavior are explicitly called out; none removes an existing feature.

| ID / topic | Decision, reason and boundary | Testable consequence / owner |
| --- | --- | --- |
| D01 Ownership/layout | Use a Cargo workspace under `rust/`, core and host crates. One simulation owner orders commands, simulation steps and accepted worker results. HTTP/filesystem/wall-clock types stay outside rules. Existing Python remains default. Prevent races without a permanent Python subprocess dependency | Core runs offline with injected time/randomness; ordered traces cannot double apply transfers; R04/R05/R20 |
| D02 Persistent IDs | Preserve existing opaque world/town/offer/request IDs. Allocate typed persistent IDs for buildings, workers, individual animals and NPCs on copy-based import; durable world-scoped monotonic counters, never reused. Import in recorded list order, record old index/coordinate→ID mappings. Coordinates remain locations, not identity; names are display data | Moving buildings, deleting earlier list entries, worker departure and reload preserve links/cargo/claims. Browser indices and coordinate-based local visit keys remain boundary adapters; import remaps all saved references before any entities are reordered. No wire ID change required. R05/R06/R12 |
| D03 Quantities and position | Use finite IEEE-754 binary64 for fractional goods, food, work, practice and continuous tile positions to retain Python/browser behavior; typed wrappers distinguish units. Integer counts, orientations, tiles and IDs remain exact; 64-bit seeds/IDs travel without JS number truncation. Reject nonfinite external values; no blanket fixed-point rounding | Integer quantities and ownership exact. Comparison absolute error ≤1e-9 for fractional goods/work/practice/elapsed seconds, ≤1e-6 tiles for positions at matched times; no tolerance may mask lost items or boundary/phase errors. JSON roundtrip tests include >2^53 terrain seeds. R03/R05 |
| D04 Time | Inject explicit elapsed seconds into core, keep separate production, courier, resident and persisted chunk/activity clocks. Preserve 3s economic tick ordering (construction before work, haulers before consumers), 0.05s fast-loop intent, courier clamp/hit slices and no wall-clock offline production. Monotonic host deadlines are not saved | Legal subdivision equivalence applies to movement/integration between economic boundaries, not arbitrary reordering of meals/random draws/production ticks. Replay stalls, pauses, reloads and tick overrun; current courier clamp remains unless separately changed. R18 slice 2 ambient fast-loop time and economic restock intervals are explicit/separate and reload does not advance either; R18 slice 3 founder economic batches use monotonic replay IDs and explicit separate barter restocks with no reload production; courier semantics remain R18 slice 4. R03/R05/R18 |
| D05 Randomness | Legacy terrain uses a compatibility implementation of Python Random seed conversion/MT19937, random/randrange/getrandbits, shuffle/choice and stable iteration. Preserve generator-specific string seeds, trig/noise and rounding semantics; merely choosing MT19937 is insufficient. Gameplay legacy traces inject recorded random draws; version new-world gameplay streams and persist state, separate from secure account/ID entropy | Old unvisited chunks match v1–7 goldens including starter/NPC overlays. R03 records draw sites/order. Existing saves have no global gameplay RNG state, so import cannot promise continuation of the old process's random future; it preserves state/distributions. Use independently seeded, Python-compatible MT19937 gameplay streams by world and subsystem, with explicit algorithm/seed-derivation version and serialized state. Import seeds streams from the saved terrain seed plus persistent world ID and subsystem name; R05 froze the [exact encoding and golden vectors](RUST_STATE_DTOS.md#d05-seed-material-encoding-v1), with compatible RNG and reference draws implemented in R07 slice 2; R19 slice 2 serializes explicitly supplied named MT19937 continuation state; gameplay-stream adoption remains later host integration work. This replaces unsaved process-global randomness, not saved outcomes; R03/R05/R07 |
| D06 Terrain support | Support every currently accepted generator version 1–7 and legacy dimensions/offsets. New algorithm requires new version and new-world opt-in. Never upgrade a seed silently | Frozen vectors, boundary/chunk traversal and evicted resource tests per version; unsupported versions fail before write. R06/R07 |
| D07 Browser compatibility | Preserve HTTP/JSON, raw Authorization token semantics, existing local browser assets, action names, public catalogs, null/missing distinctions, error statuses and user-facing errors. No mandatory WebSocket/WASM/client replacement. Chunk cache version strings are opaque; content-change and world-invalidation behavior matters, not a specific hash algorithm | Existing browser fixtures run against Rust endpoint; founder prediction and worker timelines agree; 49 requested/128 known/resource entries bounded, malformed/out-of-world keys fail, unchanged resources omitted. R03/R20/R21 |
| D08 Toolchain/dependencies | R04 selects stable Rust, pins an exact reviewed toolchain in rust-toolchain.toml and commits Cargo.lock. No speculative crate versions here; adopt HTTP/JSON/crypto/Unicode libraries when justified, evaluate search library against resumability. Core has no network/runtime dependency. R09 slice 1 evaluates pathfinding 4.16.0 dev-only and selects custom incremental search, promoted to no_std core in slice 2; [source/measurement review](RUST_SEARCH_EVALUATION.md). Preserve Python/Node tools during migration; release host gameplay eventually needs no Python | Build locked on supported development platforms; fmt/tests/clippy selected by affected crate graph; license/MSRV/platform review at adoption, compile offline after dependency cache. Missing tools fail visibly. R04; no dependencies added in R01 |
| D09 Scheduling | Separate simulation, navigation, persistence and API revisions; one owner accepts only matching task/request generations and relevant navigation dependencies. Keep position/cargo while waiting. Independent experimental route service cadence is implemented without changing three-second balance | Stale route cannot replace a new assignment; unrelated output changes do not restart navigation. Preserve PATHING stable neighbor/tie order and actor-specific road cost modes, pending≠unreachable. R09 slices 1/2 verify library limitations and the selected no_std core incremental solver: stable ties, operation-metered pops/reconstruction, immutable derived queries and cancellation storage release. Slice 3 pairs 418 actual Python routes with exact deterministic paths/outcomes and independent optimal costs; R10 slice 1 adds model-owned request generations, bounded resident/query/identity/lifetime-work admission, explicit resource waits and 4/2/1 FIFO service with one leased retry reservation. Actor facts survive all waits/cancellation/stale results; terminal answers retain slots until consumed. R10 slice 2 adds bounded positive/negative LRU and executed-prefix dependency certificates (including failed edges, diagonal/fence checks and global heuristic policy), local immutable-query refresh/restarts and current-query guarded completions. Already accepted route safety ignores road costs but checks origin/edges/arrival. Relevant refresh resets the per-search-attempt work cap under the same request generation; there is no cumulative per-generation work cap across edits. R10 slice 3 supplies a single-owner host pump with injected monotonic clock, bounded intent traversal, fixed idle delay after finish, coalesced missed releases, retries/refresh and guarded answer adoption independent of production. Bounded dense node/observation backing on small maps preserves exact path/work semantics with sparse fallback for large domains or small configured budgets; eager backing is charged in retained units. R10 is complete as an experimental foundation with measured isolated release waits; R11/R19–R23 retain complete W1–W6 contention/movement acceptance, which remains inconclusive |
| D10 Saves/rollback | Explicit versioned DTOs separate from in-memory state; retain unknown legacy fields losslessly in an extension container or reject with a path before conversion. Atomic new-destination conversion with source manifest/checksums; one writer/root. Rust refuses normal Python root while experimental | Kill during conversion/write; reopen either prior or complete new state, never mixed chunk/registry generations. Python's per-file temp+replace is not a multi-file transaction or fsync durability guarantee; improve crash recovery without changing gameplay. R06/R19 |

Numeric tolerances above are comparison limits, not permission to round saved
values or permit negative balances. Field-specific existing precision expectations
remain stronger where applicable. Terrain tiles, discrete phases, reward counts,
claims, command acceptance, and deterministic seeded choices compare exactly.
For equally optimal routes preserve stable ordering initially; accept a different
route only through a documented comparison rule proving equal access, cost and
arrival with no changed work/reward order. Cross-platform terrain float differences
must be resolved, not hidden by positional tolerance.

## Latency contract and workloads (proposed acceptance targets)

No latency was measured in R01. These are recommendations for R03 fixtures,
R10 scheduling and R23 acceptance, not implemented guarantees or approval to alter
balance. Reference machine: this development Mac, native arm64, plugged into power,
no concurrent benchmarks, loopback HTTP and a separate LAN run with round-trip
latency recorded. R03 must record actual CPU model, memory, macOS, compiler/runtime,
Chromium, build mode and power mode before freezing its baseline; the sandbox did
not permit reading CPU/memory through sysctl during R01. Until that metadata is
recorded, hardware-normalized acceptance remains unmeasured. Pair Python and Rust
release binaries on the same machine/workload; do not compare different hardware.

| Workload | Fixture / target and rationale |
| --- | --- |
| W1 Commands under load | Four towns, 200 active workers (50 each), seeded 480×480 version-7 world, mixed production/hauling/farming/building. Four browsers: ordinary snapshots every 2s, remote positions up to 50ms, courier board 100ms while open. Submit walk, upgrade_capacity, butcher_animal and assignments while saving/ticking. Host response p95 <100ms, p99 <250ms, max <1s; avoids input queued behind long simulation/search/save. Measure login separately including PBKDF2 |
| W2 Local routes | Existing server_perf 64×64 wall-detour family, 16/64/256 total actors distributed over four towns; cold/warm caches, examined/unexamined obstacle edits and road changes. Ordinary reachable host route p95 <250ms, p99 <1s, max <2s; route-ready and actual first-movement delays measured separately |
| W3 Blocked/recovery | Seal a delivery/field/butcher destination then reopen it; retain cargo and exactly-once pickup. Within W1/W2 load, resume host movement within 1s of accepted geometry change; continuing unreachable status within 1s, never report pending as final failure. Bounded search exhaustion produces explicit unreachable result |
| W4 Saturation / long detours | 256 simultaneous orders exceeds today's 128 pending-search cap; add unreachable, chunk-boundary and long-detour requests. Every unchanged admitted or retrying request receives service within 1s; require terminal route/unreachable within 10s for the finite 64×64 fixtures. Report long/world-size cases separately; no fabricated universal completion deadline. Keep at most 128 pending searches and no overflow list initially, retain existing bounded LRU/continuation budgets until measurement justifies changes |
| W5 End-to-end/browser | Existing client_perf small (1 town/5 workers/2,000 resources) and large (4 towns/200 workers/50,000 resources) fixtures, 1280×800 Chromium viewport at device scale 1; frame work p95 ≤8ms, plus measured input→ack→snapshot→scene→movement milestones. Existing 2s poll and behind-host playback prevent a subsecond visible worker guarantee; record actual visible delay rather than treating host route-ready as visible movement |
| W6 Save/snapshot contention | W1 with active and evicted edited chunks, multiple saved inactive worlds, login/reconnect and concurrent slow snapshot receiver. Command targets still hold. Record save p50/p95/p99/max, snapshot serialization/bytes, peak memory, and lock/queue wait separately; crash recovery is mandatory regardless of speed |

For each workload record at least five paired runs, seeds 0/42/99 where generation
matters, warmup separated, at least 1,000 observations for p99, queue delay in ms
and simulation time, restarts/cancellations, expansions, cache hits, max queue,
retained waypoint units, peak memory, payload and save duration. Insufficient
samples, missing hardware metadata, skipped host adapters or noisy measurements
are **inconclusive**. R03/R23 must wire these metrics: current server_perf imports
Python, drains many routing updates without real three-second waits, and lacks
full route-start/end-to-end p99 measurements; client_perf uses synthetic HTTP.
Neither is evidence of Rust-host speed or actual LAN latency. Existing absolute
budgets in tools/check.py remain in force; do not relax them to meet these targets.

## Rollback and release gate

Backup unit is the complete registry (all worlds, active selection, credentials,
tokens), all `<save-stem>_chunks/<terrain_seed>/` records including sleeping chunks,
and optional `.players.json`. Also capture schema/generator versions and manifest
checksums. Never put real accounts or local saves in fixtures, commits or reports.

Before R24: separate roots/ports; stop experimental Rust and reopen Python's
untouched source for rollback. Copy-based imports never silently synchronize new
Rust progress into Python. R19 must commit a generation manifest or equivalent
atomic snapshot boundary spanning registry and chunks and rehearse crashes.
Only one authoritative host and writer may open a world at a time.

**Recommendation for R24:** provide a tested reverse export preserving all covered
state and account ownership before making Rust default. This is not a promise
that reverse export exists. If reverse export is impractical, the user must
explicitly accept one-way conversion and loss of post-backup Rust progress on
rollback. R01 does not accept that loss. Default cutover remains blocked until
R24 records the choice and restore rehearsal; an old backup alone is not lossless
rollback. Unknown fields must not disappear during reverse export either.

## Behavior, known bugs, and intentional differences

| Category | Disposition and verification |
| --- | --- |
| Required existing gameplay | Preserve costs, recipe tiers, consumption, XP, staffing/storage limits, hall milestones, species, free last-animal replacement, butchery travel, sandbox exceptions, proximity and territory. Current balance wins over aspirational roadmap text |
| Known relocation continuity bug | `server.command` move cancels matching trips/output and drops assigned work_animation; coordinate-based task identity can reset position. R02 owns reproduction/fix before R03 records normative traces. R01 neither fixes it nor blesses teleporting/cargo loss as parity. Other current cancellation/salvage rules remain until tested |
| Documentation drift | PATHING's later “Host pathfinding queue” planned subsection conflicts with its implemented scheduling section; current navigation code/tests and scheduling section govern. BACKEND overview proposal is older than implemented roadmap/client tests. Do not count those prose gaps as missing current features or change unrelated docs in R01 |
| Intended internal differences | Experimental foundations now cover stable IDs, versioned DTOs, explicit clocks/RNG and an independent navigation service; integrated gameplay and atomic multi-file saves remain later stages. These must preserve observable play. Timing changes are evaluated against W1–W6; production rate/order changes need a separate behavior decision |
| Deferred new scope | WASM/full Rust client R25–27, new balance work (including proposed fishing catch progression/storage), crowd avoidance, hierarchical routes and speculative flow-field optimization are intentionally deferred as additions. Existing browser integration remains unported and required; existing route behavior cannot be deferred with these additions |
| Unapproved behavior changes | No removal of legacy join/build_boat/buy_animal actions; no new authentication scheme; no old-world terrain regeneration; no rounding all goods to integers; no automatic forward-only save cutover. New RNG sequences after importing a save without RNG state are documented above, not exact future-outcome parity |

## Audit and completion evidence

Inspected `server/server.py` dispatcher/HTTP/loop/load/save/migrations; delegated
`logistics.py`, `equipment.py`, `npcs.py`, `trading.py`, `horses.py`, `quests.py`,
`lanterns.py`; terrain/chunk/shared-world and village implementations; progression
catalog and inventory migrations; PATHING/BACKEND; test inventory and relevant
HTTP, legacy save, terrain, numeric and persistence cases; performance harnesses
and affected-test selection. No local save or credentials were read.

R01 inventories **66 actions**, **11 API method/route pairs plus static fallback**,
and every key returned by `progression.public_catalog`, with separate hall,
story, quarry, courier and browser-local catalogs. Structural save formats,
terrain versions 1–7, migration markers and saved feature families are covered.
R01 validation checks dispatcher actions and catalog keys against source, route
literals against the HTTP matrix, all named test paths and local Markdown links,
and documentation-only affected-check selection. Cross-host replay/goldens,
static fallback characterization and Rust performance evidence are future-stage
work, not waived parity requirements. No unresolved product choice authorizes a
feature removal; dependency versions, RNG vectors, hardware baseline metadata and
R24 rollback acceptance have explicit later-stage gates.

Document validation: source inventory and reference checks passed (66 actions,
11 API paths and all public progression keys). `python3 tools/check.py --list`
and `python3 tools/check.py` selected zero applicable tests for these Markdown
changes; correctness and performance were reported `not_run`. No gameplay/browser
benchmarks were needed because this stage changes documentation only.
