# Pathing decisions

This document brings together Lantern Vale's movement and routing decisions.
**Current** means implemented behavior checked against the code.
**Planned** means work already listed in [ROADMAP.md](ROADMAP.md#scalable-villager-pathfinding).
**Open** means a choice still needs to be made. Explanations describe the purpose
of the current rules; they do not introduce new gameplay requirements.

Code remains the source of truth. Update this reference when movement, destination
selection, route validity, or scheduling changes. [Backend](BACKEND.md) covers the
wider request and animation system; the roadmap tracks remaining implementation.

## Ownership and movement types

**Current:** the host owns gameplay movement, work completion, cargo transfers,
and saved worker state. Browser navigation indexes, collision overlays, route
caches, animation jobs, and playback history are derived presentation data.
They must not mutate authoritative world snapshots or award production or skills.

| Actor | Movement decision | Implementation |
| --- | --- | --- |
| Founder (the player) | Direct WASD movement with continuous collision checks, local prediction, and host acknowledgement. There is no destination route search for these controls. | `walkable`, `move_player` in [server/server.py](../server/server.py); `predictPlayerStep`, `playerSprite` in [web/app.js](../web/app.js). |
| Workplace workers | Host routes and work phases; browser follows confirmed movement timelines when available. | [server/work_cycles.py](../server/work_cycles.py), [web/animations.js](../web/animations.js). |
| Builders | Host retains a claimed construction site and waypoint route; browser animates the assignment. | `construct`, `construction_route` in [server/server.py](../server/server.py). |
| Haulers, merchants, and producers fetching or delivering goods | Host plans and advances trips, validating pickup and delivery on the host. | [server/logistics.py](../server/logistics.py), `logistics_route` in [server/server.py](../server/server.py). |
| Wilderness village residents | Separate host-owned, local cardinal routes for ambient routines; browser extrapolates the published route. | [server/village_life.py](../server/village_life.py), `npcSprites` in [web/app.js](../web/app.js). |
| Barn animals | Browser prepares short local roaming routes for display; animal state remains host-owned. | `animalRoutes` in [web/scene.js](../web/scene.js), `barnAnimalSprites` in [web/app.js](../web/app.js). |

## Ground, obstacles, and access

**Current:** worker searches operate on tile coordinates. Buildings, map resources,
and NPC locations form the default static obstacle set. Moving villagers do not
block one another. Roads remain walkable and are separate from static obstacles.
Shared-world movement includes buildings and roads from every player's town, so
another town's structures obstruct routes and its completed roads can help them.
See `walking_map` in [server/shared_world.py](../server/shared_world.py).

Access exceptions belong to the actor or task, rather than changing the shared
obstacle map:

| Situation | Current rule |
| --- | --- |
| Map edges and void | Stay inside the map and any supplied search bounds. Terrain marked `v` is impassable. Bounded chunked worker searches also reject missing terrain. |
| Completed acres | Farmers may cross completed acres and enter their work acre. Construction and logistics routing also allow completed acres. Ordinary work routing does not apply this exception to every profession. Construction sites remain obstacles. |
| Ordinary buildings and resources | Usually stop on a cardinally adjacent open tile, rather than entering the occupied target tile. |
| Fishing | A valid pier permits access to its deck and endpoint. Boat trips use a separate set of connected, permitted water tiles. |
| Founder | Trees and berry bushes permit passage. Buildings and other resources obstruct movement; water permits passage when the player has a boat. NPCs are not included in the founder's blocking set. |
| Founder equipment | Only the darkest swamp tiles require equipped swamp boots. Sand, desert dunes, and ice are walkable without protective clothing. These clothing rules do not change settler access. |

Founder collision uses a radius of 0.18 tiles around the character and checks nearby
tile footprints. Commands move in five small substeps, checking X and Y separately
to prevent tunnelling and allow sliding along walls. Input distance is normalized
for the 2:1 isometric screen projection. Worker routes instead measure distance
in world tiles. See [server/equipment.py](../server/equipment.py) for clothing access.

When a starting position becomes invalid, movement setup seeks nearby open ground
before planning or continuing. This is recovery for changed geometry or saved
positions, rather than permission to travel through a blocked tile.

## Directions, arrival, and search algorithm

**Current:** land routes for workers, builders, and haulers allow eight directions.
A diagonal costs `sqrt(2)` times a straight edge and is allowed only when both
orthogonal side tiles are open. Villagers cannot cut across a blocked corner.
Pier construction and worker sailing use cardinal routes. Ambient village
residents and barn roaming also use cardinal routes.

| Query | Arrival rule | Search |
| --- | --- | --- |
| One ordinary destination | A cardinal neighbor of the target: Manhattan distance exactly 1, even when diagonal movement is allowed. | BFS on cardinal uniform ground; travel-time A* when roads or diagonals are enabled. |
| Exact destination | Reach the requested tile, such as an acre or dock endpoint. | Same algorithm selection. |
| Set of acceptable destinations | Reach any member of the set, such as a usable water bank. | BFS on cardinal uniform ground; weighted search with a zero heuristic otherwise (Dijkstra). |

The A* lower bound uses Manhattan distance for cardinal movement and octile
distance for diagonal movement. Its minimum edge cost is 4 when roads exist and
6 otherwise; adjacent queries subtract one tile from the distance lower bound.
Equal-cost discoveries keep their earlier route, with stable neighbor and queue
ordering. The search chooses travel time to the supplied goal, not merely the
fewest waypoints.

The core route result excludes the starting tile. An empty path means the arrival
condition is already satisfied; `None` on the host or `null` in the browser means
unreachable. Construction and logistics wrappers include the starting tile in
their trip paths. Ambient resident routing has its own contract and returns an
empty list for either no movement or a failed search.

Sources: [server/navigation.py](../server/navigation.py), `routeSteps` in
[web/animations.js](../web/animations.js).

## Road preference and movement speed

**Current:** completed roads make players and settlers 50% faster. Unfinished
roads grant no speed or routing bonus. A longer route can win if its travel time
is lower; a road detour is not automatically preferred.

Routing uses relative cost units: a straight open-ground edge costs 6 and a
straight road edge costs 4. Different movement systems time road boundaries
differently, so their queries must keep separate cost modes:

| Movement | Cost mode | Edge cost before diagonal distance is applied |
| --- | --- | --- |
| Continuous workplace walking | `smooth` | Sum of the departure and arrival half-edge costs: each is 3 on ground or 2 on road. A mixed edge costs 5. |
| Builder walking | `departure` | Twice the departure half-edge cost. Road speed depends on the tile being left. |
| Logistics trips | `arrival` | Twice the arrival half-edge cost. Road speed depends on the tile being entered. |

Ordinary workers and builders use a base walking speed of 1.8 world tiles per
second. Logistics speed is `1.8 * (1 + 0.25 * floor(hauling / 20))`, then the road
bonus applies. The founder's base display speed is 3 in the normalized movement
metric. Mounted land movement gains a 1.75 multiplier; player boat tiers use
water speed multipliers of 1, 1.25, and 1.5. These actor speed differences are
distinct from choosing the lowest-cost path for an already selected destination.

**Current:** changing a road changes movement speed immediately, but does not
discard an otherwise valid active route. The next planned leg accounts for the
new roads. This keeps workers moving without replanning every road edit.

## Destination selection and task reservations

Selecting a job destination and finding the best route to that destination are
separate decisions. **Current:** destination selection is not a global
travel-time optimization over every possible workplace, tree, or storage building.

| Task | Current destination decision |
| --- | --- |
| Woodcutting | Retain a usable, unclaimed tree throughout the work and return trip. Choose new trees by Manhattan distance from the camp among available trees in reach. Keep different tree targets per worker; wait when all are claimed. Resolve duplicate saved claims in worker order and finish carried wood before retargeting. |
| Farming | Reserve an acre for one active tender. Respect the farm's work pass and field stage; retain active watering and cargo-return work. Prefer separate rows when available, remain on the existing row where possible, and choose the nearer row endpoint from the worker's position. |
| Fetching water | Search the set of open cardinal banks beside water or completed wells in one pass. Choose the reachable bank with the lowest route cost; water need not be immediately beside the farm. |
| Construction | Dedicated builders claim sites before idle helpers. Keep the previous coordinate-based target ahead of newly inserted sites; each site has one claim in that update. Skip inaccessible sites. Workers helping a workplace upgrade stay assigned to that upgrade. |
| Producer logistics | Retain an existing trip. For a new trip, filter storage for the required goods and capacity, order by Manhattan distance from the workplace, and accept the first reachable suitable trip. |
| Dedicated hauling | Prioritize supplying inputs before collecting outputs, then larger buffered stock, then distance. Try candidates until a reachable trip is found. |
| Automatic market sales | Order eligible sources by Manhattan distance from the market; use the first reachable trip. |

Reservations allocate work targets; they do not make villagers into movement
obstacles. Full, disabled, or unreachable storage keeps goods buffered and can
pause production. Transfers remain host-validated when the delivery occurs.

Sources: `woodcutting_source` in [server/server.py](../server/server.py),
`claimed` and `choose_field` in [server/farming.py](../server/farming.py),
[server/logistics.py](../server/logistics.py).

## Fishing, residents, and local roaming

**Current:** fishers reuse a valid pier rather than rebuilding it each tick. A
pier starts at its hut and has at most three cardinal deck steps to nearby water.
Boat travel combines a land approach, boarding at a berth, and cardinal sailing
within the hut's fishing reach. The boat's eligible water is connected and limited
by depth: tiers 1, 2, and 3 allow depths up to 2, 3, and 4 respectively. One hut
owns one voyage; additional crew follow the captain's movement state.

Ambient village residents use a separate BFS bounded to a square around their
village (radius 9 for layout version 2, otherwise 5). This routine does not use
the worker travel-time cache or road weighting. It reads already active chunk
obstacles without loading terrain for sleeping village life. Barn animals use
cached display routes of at most two cardinal steps away from the barn and back,
avoiding nearby obstacles, void, and missing chunk terrain.

Sources: [server/work_cycles.py](../server/work_cycles.py),
[server/village_life.py](../server/village_life.py),
[web/scene.js](../web/scene.js).

## Retaining routes and recovering from changes

**Current:** retain valid work, construction, and delivery paths across updates.
Builders keep their claim and saved path across reloads. A task change, moved or
removed destination, or impassable next step can invalidate a route. An unrelated
geometry update refreshes browser collision data without forcing a search.
A blockage farther along the route is handled as the worker approaches it.

Search-time diagonal checks apply in the host and browser planners. During an
existing trip, logistics also rechecks both side tiles of a diagonal; construction
and workplace walking primarily validate the next waypoint. Retention therefore
does not mean every remaining edge is fully revalidated on every geometry change.

Unreachable workplace routes enter a waiting phase and retry on a later simulation
tick. In particular, farmers must retain harvest cargo and resume when acre or
farm access returns. They must not spin retrying an unchanged failed route within
the same tick while the host holds the world lock. A logistics trip is cancelled
when its destination disappears or its next edge is blocked, allowing subsequent
task selection to find another trip.

## Shared route caches and invalidation

**Current:** host and browser share cached answers between compatible worker
queries. Both retain failed searches as well as successful routes. They return
copies so consuming or editing one worker's path cannot corrupt another's answer.

| Cache | Identity and validity | Bounds |
| --- | --- | --- |
| Host | A graph includes map size, blocked tiles, terrain, search bounds, and completed roads. Chunk terrain includes loaded chunk keys and rows, so loading, eviction, or edits cannot reuse obsolete answers. A query adds start, destination or goal set, adjacency, road cost mode, and diagonal permission. | LRU: at most 8 graphs, each with at most 256 answers and 32,768 retained waypoint cost units. |
| Browser | A `WeakMap` keys caches by reusable base collision-set identity. Query keys also include actor overlay additions/removals, size, bounds, start, goals, adjacency, roads, cost mode, and diagonal permission. Plain mutable sets bypass caching. | LRU within each base: at most 256 answers and 32,768 retained waypoint cost units. Unreferenced bases can be garbage-collected. |

Oversized answers are returned without being retained. Browser empty and failed
answers count as one unit; host failed answers count as one and empty paths as zero.
Bounds limit retained cache data, not the amount of work a single search can take.
These derived caches are not serialized into saved worlds.

Browser scene preparation reuses local collision sets when geometry, terrain, and
bounds match. Crop growth, inventories, and ordinary work progress do not change
navigation occupancy. A completed-road edit changes routing identity while allowing
the underlying collision set to remain reusable. Host graph identity covers its
supplied terrain and obstacle data more broadly; finer affected-area invalidation
remains planned.

## Chunks, scheduling, and browser playback

**Current:** chunked workplace searches and browser town navigation use a square
up to 60 tiles in each direction from the town origin (or hall), clipped to the
map. This bounds local planning; it is not routing between regions. Construction
and logistics use the terrain supplied to them and do not pass this same explicit
workplace bound into the core search. Boat searches have their own reach bounds.
Browser camera exploration outside a town's routing area must not reset its paths.

Host searches remain synchronous. The browser's prepared-scene path uses
resumable generators: route searches yield every 128 inspected entries, and
`ClientScene.Queue` rotates unfinished jobs under an approximately 2 ms frame
budget, with a maximum of 2,048 generator advances per run. A generator advance
can overrun the time budget; it is not a strict deadline. Replacing or cancelling
a job key discards obsolete planning. Synchronous helper/fallback calls still
exist. The browser queue currently has no urgency classes or fixed pending-job cap.

Confirmed workplace playback does not perform a duplicate browser work-route
search. The browser plays host samples behind the simulation and retains at most
three playback batches. Missing updates are joined at walking speed rather than
compressing a large position gap into one short segment. Farmer field presentation
follows completed actions, including after assignment to another acre. Founder
prediction follows each buffered input step so turns do not cut across obstacles;
host replies reconcile the position and pending inputs.

Sources: [server/chunk_world.py](../server/chunk_world.py),
[web/scene.js](../web/scene.js), [web/animations.js](../web/animations.js),
[web/app.js](../web/app.js).

## Planned work and open decisions

The [scalable villager pathfinding roadmap](ROADMAP.md#scalable-villager-pathfinding)
retains these unfinished items in order. None is an implemented routing guarantee.

| Order | Planned direction | Decisions still open |
| --- | --- | --- |
| 1 | Host queue for resumable searches, with bounded work per simulation update, cancellation, and protection against starvation. Prioritize blocked workers and urgent deliveries. | Work budget, urgency rules, fairness policy, waiting-state integration, and pending-request limits. |
| 2 | Refine cache invalidation to affected navigation where practical; keep caches and future pending requests bounded. | Invalidation granularity and request/cache limits for larger towns. |
| 3 | Flow fields for common wells, storage, and workplaces, preserving individual searches for unique destinations. | Adoption threshold, sharing boundaries, and handling road costs and actor access rules. |
| 4 | Local spacing, waits, or avoidance for crowds without putting moving villagers in the static obstacle map. | Spacing distances, collision priorities, and avoidance behavior. |
| 5 | Hierarchical routing between connected regions, followed by detailed local paths. | Region definitions, chunk-boundary connections, unloaded-terrain behavior, and navigation updates. |

Increasing villager counts, simultaneous task changes, multiple towns, warm/cold
caches, and obstacle changes must be measured before claiming a scaling improvement.
The host must remain authoritative, awaiting workers must remain valid, and
accessible destinations and road timing must survive any scheduling change.

## Verification references

| Decision | Existing regression coverage |
| --- | --- |
| Shortest travel-time routes, road cost modes, diagonal corners, cache isolation and bounds | [test_navigation.py](../tests/test_navigation.py), [test_route_cache.js](../tests/test_route_cache.js), [test_road_routing.js](../tests/test_road_routing.js), [test_diagonal_routes.js](../tests/test_diagonal_routes.js). |
| Valid path retention, blocked-step recovery, host playback without duplicate planning | [test_path_retention.js](../tests/test_path_retention.js), [test_construction.py](../tests/test_construction.py). |
| Worker target reservations and unreachable farmer recovery | [test_woodcutter_targets.py](../tests/test_woodcutter_targets.py), [test_farming_destinations.py](../tests/test_farming_destinations.py), [test_farming_cycle.py](../tests/test_farming_cycle.py). |
| Founder collision, prediction, roads, and equipment | [test_player.py](../tests/test_player.py), [test_player_prediction.js](../tests/test_player_prediction.js), [test_roads.py](../tests/test_roads.py), [test_equipment.py](../tests/test_equipment.py). |
| Fishing access, delivery movement, and presentation | [test_boats.py](../tests/test_boats.py), [test_fishing_range.py](../tests/test_fishing_range.py), [test_storage_movement.py](../tests/test_storage_movement.py), [test_villager_playback.js](../tests/test_villager_playback.js). |
| Local navigation preparation, queued jobs, and ambient routines | [test_scene.js](../tests/test_scene.js), [test_village_life.py](../tests/test_village_life.py), [test_animal_animations.js](../tests/test_animal_animations.js). |

For changes, use `python3 tools/check.py --list` to inspect the affected checks,
then `python3 tools/check.py` to validate that set. A routing performance task
also requires a recorded starting commit and the comparison workflow in
[AGENTS.md](../AGENTS.md). Documentation-only updates do not require gameplay
benchmarks.
