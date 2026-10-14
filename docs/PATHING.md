# Pathing decisions

Movement and routing decisions for Lantern Vale.

- **Current:** implemented behavior checked against the code.
- **Planned:** work listed in [ROADMAP.md](ROADMAP.md#scalable-villager-pathfinding).
- **Open:** a choice still needs to be made.
- Each topic separates **Host** authority from **Browser** presentation and ends with code references.
- Explanations describe the current rules; they do not introduce new requirements.
- Code remains the source of truth. Update this reference when movement,
  destination selection, route validity, or scheduling changes.
- [Backend](BACKEND.md) covers requests and animation; the roadmap tracks remaining work.

## Ownership and movement types

**Current:**

- **Host:** owns authoritative gameplay and saved state.
  - Gameplay movement and work completion.
  - Cargo transfers and saved worker state.
- **Browser:** owns derived presentation data.
  - Derived navigation indexes, collision overlays, and route caches.
  - Animation jobs and playback history.
  - Presentation must not mutate authoritative snapshots or award production or skills.

### Movement by actor

- **Founder (player):**
  - **Host:** validates direct WASD commands, checks collisions, and returns the authoritative position.
  - **Browser:** predicts movement immediately, follows buffered inputs, and reconciles host replies.
  - No destination route search.
  - **References:**
    - **Host:** `walkable`, `move_player` in [server/server.py](../server/server.py).
    - **Browser:** `predictPlayerStep`, `playerSprite` in [web/app.js](../web/app.js).
- **Workplace workers:**
  - **Host:** owns routes, work phases, and confirmed movement timelines.
  - **Browser:** follows confirmed timelines when available; otherwise prepares visual routes and phases.
  - **References:**
    - **Host:** `advance`, `_advance` in [server/work_cycles.py](../server/work_cycles.py).
    - **Browser:** `planSteps`, `playback` in [web/animations.js](../web/animations.js).
- **Builders:**
  - **Host:** retains a claimed site and waypoint route, advances walking, and applies construction progress.
  - **Browser:** animates the host's assignment; visual progress does not complete construction.
  - **References:**
    - **Host:** `construct`, `construction_route` in [server/server.py](../server/server.py).
    - **Browser:** `animatedVillagers` in [web/app.js](../web/app.js),
      `planSteps`, `advanceSteps` in [web/animations.js](../web/animations.js).
- **Haulers, merchants, and producers fetching or delivering goods:**
  - **Host:** selects and advances trips, validates pickup and delivery, and transfers cargo.
  - **Browser:** animates the host's trip and cargo; may plan an approach to its remaining path.
  - **References:**
    - **Host:** `start_trip`, `advance_trip`, `transfer` in [server/logistics.py](../server/logistics.py),
      `logistics_route` in [server/server.py](../server/server.py).
    - **Browser:** `planSteps`, `advanceSteps` in [web/animations.js](../web/animations.js).
- **Wilderness residents:**
  - **Host:** owns local cardinal routes and ambient routine state.
  - **Browser:** extrapolates the published route for display.
  - **References:**
    - **Host:** `route`, `advance` in [server/village_life.py](../server/village_life.py).
    - **Browser:** `npcSprites` in [web/app.js](../web/app.js).
- **Barn animals:**
  - **Host:** owns animal counts and growth state.
  - **Browser:** prepares short roaming routes and displays the animals along them.
  - **References:**
    - **Host:** `animal_growth`, `grow_animals` in [server/progression.py](../server/progression.py).
    - **Browser:** `animalRoutes` in [web/scene.js](../web/scene.js),
      `barnAnimalSprites` in [web/app.js](../web/app.js).

## Ground, obstacles, and access

**Current:**

- **Host:** supplies authoritative terrain and shared-town obstacles; enforces actor access rules.
- **Browser:** derives collision indexes and task overlays from snapshots for prediction and visual planning.

### Common rules

- Worker searches use tile coordinates.
- Buildings, map resources, and NPC locations form the default static obstacle set.
- Moving villagers do not block one another.
- Roads remain walkable and are separate from static obstacles.
- Shared-world movement includes every town's buildings and roads:
  - Other towns' structures obstruct routes.
  - Their completed roads can help them.
- Access exceptions belong to the actor or task and do not change the shared obstacle map.

### Access rules

- **Map edges:** stay inside the map and any supplied search bounds.
- **Void:**
  - Terrain marked `v` is impassable.
  - Bounded chunked worker searches also reject missing terrain.
- **Completed acres:**
  - Farmers can cross completed acres and enter their work acre.
  - Construction and logistics routes also allow completed acres.
  - Other professions do not all receive this exception.
  - Construction sites remain obstacles.
- **Ordinary buildings and resources:** usually stop on a cardinally adjacent
  open tile instead of entering the occupied target.
- **Fishing:**
  - Valid piers allow deck and endpoint access.
  - Boat trips use connected, permitted water tiles.
- **Founder:**
  - Trees and berry bushes permit passage; buildings and other resources block movement.
  - Water permits passage with a boat.
  - NPCs are outside the founder's blocking set.
- **Clothing:**
  - Only the darkest swamp tiles require equipped swamp boots.
  - Sand, desert dunes, and ice need no protective clothing.
  - Settler access is unaffected.

### Founder collision and recovery

- Character collision radius: **0.18 tiles**, checked against nearby tile footprints.
- Each command uses **five small substeps** to prevent tunnelling.
  - X and Y are checked separately so the character can slide along walls.
- Input distance is normalized for the **2:1 isometric screen projection**.
  - Worker routes measure distance in world tiles instead.
- Invalid starting positions trigger a search for nearby open ground before
  planning or continuing.
  - Recovery does not permit travel through blocked tiles.

**References:**

- **Host:** `walking_map` in [server/shared_world.py](../server/shared_world.py);
  `walkable`, `ensure_player`, `move_player`, `construction_blocked` in [server/server.py](../server/server.py);
  `protected` in [server/equipment.py](../server/equipment.py);
  `_advance` in [server/work_cycles.py](../server/work_cycles.py).
- **Browser:** `prepare`, `Overlay` in [web/scene.js](../web/scene.js);
  `planSteps`, `refreshNavigation` in [web/animations.js](../web/animations.js);
  `predictPlayerStep`, `biomeProtected` in [web/app.js](../web/app.js).

## Directions, arrival, and search algorithm

**Current:**

- **Host:** calculates routes used by gameplay movement.
- **Browser:** implements matching search rules for visual planning and fallback routes;
  confirmed workplace playback does not need a duplicate search.

### Common rules

- Land routes for workers, builders, and haulers allow **eight directions**.
  - Diagonal edges cost **`sqrt(2)` times** a straight edge.
  - Both orthogonal side tiles must be open; blocked corners cannot be crossed.
- Pier construction, worker sailing, ambient residents, and barn roaming use cardinal routes.

### Arrival and algorithm selection

| Query | Arrival rule | Search |
| --- | --- | --- |
| Ordinary destination | Cardinal neighbor: Manhattan distance exactly 1, even with diagonals enabled. | BFS on cardinal uniform ground; travel-time A* with roads or diagonals. |
| Exact destination | Requested tile, such as an acre or dock endpoint. | Same algorithm selection. |
| Set of destinations | Any member of the set, such as a usable water bank. | BFS on cardinal uniform ground; otherwise Dijkstra (weighted search with zero heuristic). |

- Optimize travel time to the supplied goal, rather than waypoint count.
- **A* lower bound:**
  - Manhattan distance for cardinal movement; octile distance for diagonals.
  - Minimum edge cost: **4 with roads**, **6 otherwise**.
  - Adjacent queries subtract one tile from the distance lower bound.
- Equal-cost discoveries keep the earlier route.
  - Neighbor and queue ordering are stable.

### Route result contract

- **Core results:**
  - Exclude the starting tile.
  - Empty path: arrival condition already satisfied.
  - Host `None` / browser `null`: unreachable.
- **Construction and logistics wrappers:** include the starting tile in trip paths.
- **Ambient residents:** use a separate contract.
  - An empty list can mean either no movement or a failed search.

**References:**

- **Host:** `RouteCache.route`, `RouteCache.search` in [server/navigation.py](../server/navigation.py);
  `construction_route`, `logistics_route` in [server/server.py](../server/server.py);
  `route` in [server/village_life.py](../server/village_life.py).
- **Browser:** `routeSteps` in [web/animations.js](../web/animations.js).

## Road preference and movement speed

**Current:**

- **Host:** chooses gameplay routes by travel time and applies speed bonuses as movement advances.
- **Browser:** uses road data for founder prediction, visual route costs, and walking animation;
  confirmed workplace playback uses host sample timing.

### Common rules

- Completed roads make players and settlers **50% faster**.
  - Unfinished roads provide no speed or routing bonus.
- A longer road route wins only when its travel time is lower.
  - Relative straight-edge cost: **6 on open ground**, **4 on road**.
- Road boundary timing differs by movement system; cache queries keep the cost modes separate.

### Road cost modes

| Movement | Mode | Edge cost before diagonal distance |
| --- | --- | --- |
| Continuous workplace walking | `smooth` | Departure + arrival half-edge costs: 3 for ground, 2 for road; mixed edge costs 5. |
| Builders | `departure` | Twice the departure half-edge cost; speed depends on the tile being left. |
| Logistics | `arrival` | Twice the arrival half-edge cost; speed depends on the tile being entered. |

### Speeds and road changes

- **Ordinary workers and builders:** 1.8 world tiles per second before road bonuses.
- **Logistics:** `1.8 * (1 + 0.25 * floor(hauling / 20))`, then the road bonus.
- **Founder:** base display speed of 3 in the normalized movement metric.
  - **Mounted land movement:** 1.75 multiplier.
  - **Player boats:** water speed multipliers of 1, 1.25, and 1.5 by tier.
- Actor speed differs from selecting the lowest-cost path to an already chosen destination.
- Road changes affect movement speed immediately but preserve valid active routes.
  - The next planned leg uses the new roads, avoiding replanning on every road edit.

**References:**

- **Host:** `RouteCache.search` in [server/navigation.py](../server/navigation.py);
  `move_player`, `construct` in [server/server.py](../server/server.py);
  `_advance` in [server/work_cycles.py](../server/work_cycles.py);
  `travel_speed`, `advance_trip` in [server/logistics.py](../server/logistics.py).
- **Browser:** `roadSpeed`, `playerTravelSpeed`, `playerSprite` in [web/app.js](../web/app.js);
  `routeSteps`, `advanceSteps`, `playback` in [web/animations.js](../web/animations.js).

## Destination selection and task reservations

**Current:**

- **Host:** chooses work targets, reserves tasks, selects logistics trips, and validates transfers.
- **Browser:** reads host targets and assignments to prepare visual movement;
  fallback planning does not reserve authoritative work or transfer goods.

### Common rules

- Choosing a job destination and finding its best route are separate decisions.
  - Destination selection does not globally optimize travel time across every
    possible workplace, tree, or storage building.
- Reservations allocate work targets; they do not turn villagers into movement obstacles.
- Full, disabled, or unreachable storage preserves buffered goods and can pause production.
  - The host validates transfers when deliveries occur.

### Woodcutting

- Retain a usable, unclaimed tree throughout the work and return trip.
  - Finish carried wood before retargeting.
- Choose new available trees in reach by Manhattan distance from the camp.
- Keep different targets per worker.
  - Wait when all trees are claimed.
  - Resolve duplicate saved claims in worker order.

### Farming and water

- Reserve an acre for one active tender.
- Follow the farm's work pass and field stage.
  - Retain active watering and cargo-return work.
- **Row selection:**
  - Prefer separate rows where available; stay on the existing row where possible.
  - Choose the nearer row endpoint from the worker's position.
- **Fetching water:**
  - Search all open cardinal banks beside water or completed wells in one pass.
  - Choose the reachable bank with the lowest route cost.
  - Water need not be beside the farm.

### Construction

- Dedicated builders claim sites before idle helpers.
- Keep the previous coordinate-based target ahead of newly inserted sites.
- **Site access:**
  - Allow one claim per site in each update.
  - Skip inaccessible sites.
- Workers helping a workplace upgrade stay assigned to that upgrade.

### Logistics and market sales

- **Producers:**
  - Retain an existing trip.
  - For new trips, filter storage by goods and capacity.
  - Sort by Manhattan distance from the workplace; accept the first reachable suitable trip.
- **Dedicated haulers:**
  - Prioritize input supply, then output collection.
  - Next prefer larger buffered stock, then distance.
  - Try candidates until a trip is reachable.
- **Automatic sales:**
  - Sort eligible sources by Manhattan distance from the market.
  - Use the first reachable trip.

**References:**

- **Host:** `woodcutting_source`, `construct` in [server/server.py](../server/server.py);
  `claimed`, `choose_field` in [server/farming.py](../server/farming.py);
  `_advance` in [server/work_cycles.py](../server/work_cycles.py);
  `worker`, `haul`, `merchant`, `transfer` in [server/logistics.py](../server/logistics.py).
- **Browser:** `planSteps`, `fetchWaterSteps` in [web/animations.js](../web/animations.js);
  `animatedVillagers` in [web/app.js](../web/app.js).

## Fishing, residents, and local roaming

**Current:**

- **Host:** owns fishing access, crew movement, resident routines, and animal state.
- **Browser:** displays confirmed fishing movement and resident routes;
  fallback fishing preparation and barn roaming are presentation work.

### Fishing

- **Piers:**
  - Reuse valid piers across ticks.
  - Start at the hut; extend at most **three cardinal deck steps** to nearby water.
- **Boat trips:**
  - Combine a land approach, boarding at a berth, and cardinal sailing within hut reach.
  - Eligible water must be connected and within the boat's depth limit.
  - Tiers 1, 2, and 3 permit depths up to 2, 3, and 4.
- **Crew:**
  - One hut owns one voyage.
  - Additional crew follow the captain's movement state.

### Ambient residents and barn animals

- **Residents:**
  - Use separate BFS routes inside a square around their village.
  - **Radius 9** for layout version 2, **radius 5** otherwise.
  - Do not use worker route caches or road weighting.
  - Read already active chunk obstacles; sleeping village life does not load terrain.
- **Barn animals:**
  - Use cached display routes of at most **two cardinal steps** away and back.
  - Avoid nearby obstacles, void, and missing chunk terrain.

**References:**

- **Host:** `fishing_water`, `fishing_pier`, `retained_pier`, `fishing_crew`, `_advance` in
  [server/work_cycles.py](../server/work_cycles.py);
  `route`, `blocked_tiles`, `advance` in [server/village_life.py](../server/village_life.py);
  `animal_growth`, `grow_animals` in [server/progression.py](../server/progression.py).
- **Browser:** `fishingPierSteps`, `travelSteps`, `planSteps`, `playback` in [web/animations.js](../web/animations.js);
  `animalRoutes` in [web/scene.js](../web/scene.js);
  `npcSprites`, `barnAnimalSprites` in [web/app.js](../web/app.js).

## Retaining routes and recovering from changes

**Current:**

- **Host:** preserves usable gameplay paths and saved claims; handles blocked work and cancelled trips.
- **Browser:** preserves usable visual paths, refreshes derived collision data, and cancels obsolete planning jobs.

### Keep a route when it remains usable

- Retain valid work, construction, and delivery paths across updates.
  - Builders preserve their claim and saved path across reloads.
- Refresh browser collision data for unrelated geometry changes without forcing a search.
- Handle farther-ahead blockages as the worker approaches them.

### Revalidate or recover when needed

- Task changes, moved/removed destinations, and impassable next steps can invalidate routes.
- Host and browser planners check diagonal side tiles during search.
  - Logistics also rechecks both diagonal side tiles during an existing trip.
  - Construction and workplace walking primarily validate the next waypoint;
    they do not fully revalidate every remaining edge on each geometry change.
- Unreachable workplace routes wait and retry on a later simulation tick.
  - Farmers retain harvest cargo and resume when acre or farm access returns.
  - Do not repeatedly retry an unchanged failure within one tick while holding the world lock.
- Cancel logistics trips when destinations disappear or the next edge is blocked.
  - Subsequent task selection can find another trip.

**References:**

- **Host:** `construct` in [server/server.py](../server/server.py);
  `_advance`, `retained_pier` in [server/work_cycles.py](../server/work_cycles.py);
  `advance_trip`, `cancel_trip` in [server/logistics.py](../server/logistics.py).
- **Browser:** `refreshNavigation`, `pathBlocked`, `advanceSteps` in [web/animations.js](../web/animations.js);
  `animatedVillagers` in [web/app.js](../web/app.js);
  `Queue` in [web/scene.js](../web/scene.js).

## Shared route caches and invalidation

**Current:**

- **Host:** maintains a derived route cache for gameplay searches.
- **Browser:** maintains a separate derived route cache tied to reusable collision bases.

### Common rules

- Each side shares its own cached answers between compatible worker queries.
  - Cache successful and failed searches.
  - Return copies so one worker cannot corrupt another worker's cached answer.
  - Return oversized answers without retaining them.
- Cache bounds limit retained data, not the work required by a single search.
- Derived route caches are not saved in world snapshots.

### Host cache

- Graph identity: map size, blocked tiles, terrain, bounds, and completed roads.
  - Chunk identity includes loaded keys and terrain rows to account for loading, eviction, and edits.
- Query identity: start, destination/goal set, adjacency, road cost mode, and diagonal permission.
- **LRU limits:**
  - At most **8 graphs**.
  - Per graph: **256 answers**, **32,768 waypoint cost units**.
  - Failed answers count as one unit; empty paths count as zero.

### Browser cache

- `WeakMap` keys use reusable base collision-set identity.
  - Unreferenced bases can be garbage-collected.
- Query identity adds overlay additions/removals, size, bounds, start, goals,
  adjacency, roads, cost mode, and diagonal permission.
- Plain mutable sets bypass caching.
- **LRU limits per base:**
  - **256 answers**, **32,768 waypoint cost units**.
  - Empty and failed answers count as one unit.

### Invalidation

- **Host:** graph identity covers supplied terrain and obstacles broadly.
  - **Planned:** finer invalidation limited to affected navigation areas.
- **Browser:** reuse collision sets when local geometry, terrain, and bounds match.
  - Crop growth, inventories, and ordinary work progress do not change navigation occupancy.
  - Completed-road edits change routing identity while keeping the collision base reusable.

**References:**

- **Host:** `RouteCache` in [server/navigation.py](../server/navigation.py).
- **Browser:** `cachedRoutes`, `routeSteps` in [web/animations.js](../web/animations.js);
  `prepare`, `Overlay` in [web/scene.js](../web/scene.js).

## Chunks, scheduling, and browser playback

**Current:**

### Search bounds

- **Host:** chunked workplace searches extend up to **60 tiles** from the town origin
  (or hall), clipped to the map.
  - Construction and logistics use supplied terrain without the same explicit workplace bounds.
  - Boat searches use their own reach bounds.
- **Browser:** town navigation uses the same local square for chunked maps.
  - Camera exploration outside the routing area must not reset town paths.
- These are local planning bounds, not routing between regions.

### Scheduling

- **Host:** searches remain synchronous; a host pathfinding queue is planned.
- **Browser:** prepared-scene searches use resumable generators.
  - Route searches yield every **128 inspected entries**.
  - Synchronous helper/fallback calls still exist.
  - `ClientScene.Queue` rotates unfinished jobs with an approximately **2 ms frame budget**.
  - Each run permits at most **2,048 generator advances**.
  - One generator advance can exceed the budget; it is not a strict deadline.
  - Replacing or cancelling a job key discards obsolete planning.
  - No urgency classes or fixed pending-job cap.

### Playback and prediction

- **Host:** publishes confirmed workplace movement samples and work phases.
  - Validates founder input and replies with the authoritative position.
- **Browser:** plays workplace samples and predicts founder movement.
  - Avoid duplicate work-route searches for confirmed movement.
  - Play host samples behind the simulation; retain at most **three playback batches**.
  - Join missed updates at walking speed rather than compressing a large position gap.
  - Farmer field presentation follows completed actions, including after switching acres.
  - Follow each buffered founder input step so turns do not cut across obstacles.
  - Reconcile founder position and pending inputs from host replies.

**References:**

- **Host:** Terrain loading in [server/chunk_world.py](../server/chunk_world.py);
  `RouteCache.search` in [server/navigation.py](../server/navigation.py);
  `_advance`, `_sample` in [server/work_cycles.py](../server/work_cycles.py);
  `move_player` in [server/server.py](../server/server.py).
- **Browser:** `prepare`, `Queue` in [web/scene.js](../web/scene.js);
  `routeSteps`, `playback`, `joinPlayback`, `fieldPresentation` in [web/animations.js](../web/animations.js);
  `animatedVillagers`, `predictPlayerStep`, `playerSprite` in [web/app.js](../web/app.js).

## Planned work and open decisions

These items remain unfinished, in [roadmap order](ROADMAP.md#scalable-villager-pathfinding).

### 1. Host pathfinding queue

- **Planned:**
  - Resumable searches, bounded work per update, cancellation, and protection against starvation.
  - Prioritize blocked workers and urgent deliveries.
- **Open:** work budget, urgency rules, fairness, waiting-state integration, and pending-request limits.

### 2. Cache refinement

- **Planned:** invalidate only affected navigation where practical; bound caches and future requests.
- **Open:** invalidation granularity and request/cache limits for larger towns.

### 3. Shared destination flow fields

- **Planned:**
  - Shared routing for common wells, storage, and workplaces.
  - Keep individual searches for unique destinations.
- **Open:** adoption threshold, sharing boundaries, road costs, and actor access rules.

### 4. Local crowd avoidance

- **Planned:** spacing, short waits, or avoidance without adding moving villagers to static obstacles.
- **Open:** spacing distances, collision priorities, and avoidance behavior.

### 5. Hierarchical routing

- **Planned:** route between connected regions, then calculate detailed local paths.
- **Open:** region definitions, chunk-boundary connections, unloaded terrain, and navigation updates.

### Acceptance criteria

- Measure increasing villager counts, simultaneous task changes, multiple towns,
  warm/cold caches, and obstacle changes before claiming a scaling improvement.
- Keep the host authoritative and awaiting workers in valid states.
- Preserve accessible destinations and road timing through scheduling changes.

## Verification references

- **Route costs, road modes, diagonals, cache isolation and bounds:**
  - [test_navigation.py](../tests/test_navigation.py)
  - [test_route_cache.js](../tests/test_route_cache.js)
  - [test_road_routing.js](../tests/test_road_routing.js)
  - [test_diagonal_routes.js](../tests/test_diagonal_routes.js)
- **Retention, recovery, and playback without duplicate planning:**
  - [test_path_retention.js](../tests/test_path_retention.js)
  - [test_construction.py](../tests/test_construction.py)
- **Target reservations and farmer recovery:**
  - [test_woodcutter_targets.py](../tests/test_woodcutter_targets.py)
  - [test_farming_destinations.py](../tests/test_farming_destinations.py)
  - [test_farming_cycle.py](../tests/test_farming_cycle.py)
- **Founder collision, prediction, roads, and equipment:**
  - [test_player.py](../tests/test_player.py)
  - [test_player_prediction.js](../tests/test_player_prediction.js)
  - [test_roads.py](../tests/test_roads.py)
  - [test_equipment.py](../tests/test_equipment.py)
- **Fishing, delivery movement, and presentation:**
  - [test_boats.py](../tests/test_boats.py)
  - [test_fishing_range.py](../tests/test_fishing_range.py)
  - [test_storage_movement.py](../tests/test_storage_movement.py)
  - [test_villager_playback.js](../tests/test_villager_playback.js)
- **Local preparation, queued jobs, and ambient routines:**
  - [test_scene.js](../tests/test_scene.js)
  - [test_village_life.py](../tests/test_village_life.py)
  - [test_animal_animations.js](../tests/test_animal_animations.js)

### Checks when changing pathing

- Inspect the affected checks: `python3 tools/check.py --list`.
- Validate that set: `python3 tools/check.py`.
- For routing performance work, record the starting commit and follow the
  comparison workflow in [AGENTS.md](../AGENTS.md).
- Documentation-only updates do not require gameplay benchmarks.
