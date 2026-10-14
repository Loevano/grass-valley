# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement                        | Status                   | Goal                                                                                                             |
| ----- | ---------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| 1     | Cached zoomed-out town overview    | Implemented              | Show the landscape and city sizes without refreshing animated scene detail.                                      |
| 2     | Production line balancing          | Wood and grain implemented; fishing remains | Develop clear fishing chains with capped staffing and simple upgrades. |
| 3     | Scalable villager pathfinding       | Foundations implemented; host scheduling and shared navigation remain | Keep movement responsive as villager counts and towns grow, with bounded routing work. |

## Cached zoomed-out town overview

| Part | Behavior |
| --- | --- |
| Terrain | Display simple terrain blocks, loading missing chunks and caching unchanged artwork. |
| Town markers | Render only town halls as static markers, with town names. |
| City size | Outline the current building extent of each town. |
| Animated detail | Omit individual buildings, workers, animals, and player animations from the overview. |
| Refresh | Reuse cached layers while unchanged; update affected data when towns change or new terrain is needed. |
| Interaction | Keep panning, zooming, and town selection responsive using cached layers. |
| Simulation | Keep gameplay and saved state authoritative on the host while viewing the overview. |

## Completion criteria

- Zoomed-out views clearly show terrain, town locations, and city extents.
- An unchanged overview does not rebuild its layers or redraw for animation.
- Camera input responds promptly and reuses cached artwork.
- Building changes update the affected town outline, including changes by friends.
- New terrain loads as needed; creating and switching worlds displays the correct
  map and towns, including after waiting in the Worlds menu.
- Returning to detailed zoom shows the current game state.

The overview starts below a 32-pixel tile width. Town outlines are convex hulls
of whole building footprints, including acres and construction sites; roads are
excluded. Names and hall glyphs stay readable and selectable. Building placement
and move tools retain the detailed scene at every zoom level.

Terrain uses the bounded chunk cache and padded ground canvas. Each town reuses
its summary and outline until its name, hall, level, or building positions change.
Idle overview frames do not clear or redraw the map, prepare worker animation
jobs, or draw individual objects. Pan, zoom, resize, hover, selection, visible
terrain changes, and changed town summaries trigger a redraw. Detailed playback
restarts from current state on leaving the overview.

The existing world API still refreshes host state and interface data, including
collision indexes used by founder movement. A lightweight overview request/change
mechanism remains future work; this feature does not reduce snapshot traffic.

## Production line balancing

**Status:** wood and grain chains and species-preserving smoking implemented.
Fishing catch progression, storage scaling, and staffing remain.
Implemented capacities, storage upgrades, tools, decorations, and base rates are
documented in [wood balance](BALANCING.md#wood) and
[grain balance](BALANCING.md#grains). The grain chain uses two farms, one miller,
and two bakeries, with downstream storage sized for harvest and hauling gaps.

The implemented [grain flow](GRAINFLOW.md) keeps one worker per mill and bakery.
Equipment upgrades scale throughput, storage, and producer carrying capacity.
Meals cost 1.5 food/minute per working resident and 0.6 per idle resident.
The compact host test sustains 200 working residents with one top-tier mill,
one bread bakery, 12 farms, 24 acres, and four haulers; fuel is supplied.
Additional bakeries provide variety. Building panels show recipes and upgrade
options without grain-flow summaries. Existing extra processor slots
become equipment upgrades while preserving goods, residents, and partial work.

Sandbox worlds are available for supply chain experiments: all buildings are
unlocked, construction and upgrades are free and instant, cottages add residents
automatically, and villagers need no training and never depart. Production inputs,
hauling, storage, food consumption, and normal output rates still apply. The
remaining fishing changes below remain planned.
Hosts can also regenerate sandbox worlds from the Worlds menu to try the latest
terrain layout; confirmation resets only the selected sandbox's towns and progress.
Registered players retain their names and PINs and reconnect to fresh villages.

### Fishing and smoking

- Fishermen remain fishermen as they develop; progression should not require
  changing them into a different profession.
- More experienced fishermen have a higher chance of catching fish.
- Higher fishing levels unlock additional, more valuable fish species.

The smokehouse now retains species through one-to-one smoking, adding 0.5 food
and 1 coin per fish. Storage and market panels show each raw and smoked species.
Catch progression, storage scaling, and smokehouse staffing remain planned.

### Simple production upgrades

The upgrade panel now uses compact rows with a short benefit, cost and work time,
and an Upgrade action. Completed upgrades show a status without a disabled button;
the purchase popup retains full benefits and requirements. The compact rows retain
the implemented worker limits and wood, mill, and bakery storage upgrades.
Fishing balancing and storage scaling below remain planned.

Production now respects each resource's existing storage allowance, including
pending deliveries. Each edible good and fish species now has an independent
item capacity in halls, storehouses, and workplaces. Storage filters and market
lists are organized by production trade; Option/Alt-clicking a storage resource
checkbox toggles all shown resources on or off. Legacy food filters expand without losing stock.
Storage rows now follow hall, assigned-worker recipe, and owned fishing-boat
unlocks while retaining goods already in stock. Hidden acceptance settings and
capacities are preserved; Option/Alt-click toggles only the shown resources.
Producers haul when an allowance fills;
existing excess stock is preserved. Workplace storage totals appear beside the
Storage heading. Wood, mill, and bakery upgrades increase these allowances by 25% per
level; farm crop storage scales with its building level. Individual acres have
no upgrades and retain a fixed crop allowance. Fishing
storage scaling remains planned.

Worker drop-offs now preserve the rest of the simulation tick for the next trip,
removing the extra pause after unloading. Stock accounting remains on the host;
fishing production balancing and storage upgrades below remain planned.

- Production building upgrades increase internal storage or worker capacity.
- Items and worker skill levels improve conversion rates and unlock available
  options, rather than production building levels providing output bonuses.
  Grain equipment upgrades increase mill and bakery throughput with
  matching ingredient demand, independently of worker capacity.
- Keep worker capacity and internal storage independently upgradeable and show
  their effects before purchase.

Completion criteria:

- Fishing experience improves catch chance and access to valuable species.
- Storage and worker upgrades have clear effects; items and skills govern
  production improvements and options.

For fishing, downstream storage sizes, skill thresholds, catch
probabilities, fish values, and smoke-station staffing remain open before
implementation. Their production rates also need review against the implemented
worker limits documented in README.md.
The [luxury meat balance](BALANCING.md#grain-staple-and-luxury-meat) is implemented:
pigs use 10 feed and yield 32 food as prepared meat, with profitable advanced
recipes. Base bread is about 24% more efficient per active worker-second than
a full pig barn before travel and byproducts. Delivered livestock pacing and
its interaction with grain equipment still need play experience.
Sawpit work is labeled Sawing throughout job and skill controls, with a hand-driven
sawing animation during productive work. Saved practice and production rates are
preserved. The wood and grain rates still need play experience across town layouts
to assess delivered supply and pacing, including continuous forestry supply for the grain
chain. The grain chain has compact host fixtures for base rates and 200 residents.
The [hall upgrade path](HALL_PROGRESSION.md) now requires output from the previous
level's production unlocks, with tools and processed food at level 5 and saddle
production at level 9. Play experience should also assess these batch sizes and
material investments alongside the remaining fishing balance work.
Advanced woodcraft and metal sale values now cover ingredients, including fuel;
higher workshop recipe tiers preserve or improve profit per active work second.

Barns now use free young animals, fixed products during fed growth, and automatic
young replacements when adults are sent to the butcher. Farm-animal breeding is
removed. Play experience should assess the five-minute cycle and fixed 25 eggs,
wool, or milk per animal alongside feed supply, hauling, and manual butcher pacing.
The barn inspector now shows animal cards, fed-growth progress, remaining products,
capacity, and access to the butcher. The butcher panel previews meat and the free
replacement and shows active processing progress. Delivered pacing remains unmeasured.

## Scalable villager pathfinding

**Status:** partially implemented. Prioritize a host pathfinding queue before
adding shared destination routing or navigation between regions.

Current movement and routing decisions, implementation references, and open choices
are collected in [Pathing decisions](PATHING.md).

### Existing foundations

- Workers retain valid work, delivery, and construction paths until their task
  changes or a next step becomes blocked. Builders keep their claimed site and
  saved route across ticks and reloads; fishers reuse valid pier paths.
  Unreachable farmer routes wait until the next simulation tick before retrying,
  retaining harvest cargo and resuming when access is restored.
- Browser geometry updates refresh collision data without discarding valid paths.
  Roads change walking speed immediately and influence the next planned leg.
  Confirmed host playback does not perform a duplicate browser work-route search.
- Host and browser each maintain bounded route caches that workers reuse.
  Terrain, obstacles, roads, and search bounds affect validity; crop growth does not.
  Search uses BFS for cardinal routes on uniform ground and travel-time A*
  for roads or diagonal walking.
- Decorative barn, stable, and village animals follow nearby outdoor routes,
  avoiding their own shelter as well as other occupied tiles. Enclosed herds
  stay out of view until an adjacent tile becomes available.
  Barn animals roam throughout their fenced 7×7 range. Construction and moves
  keep other buildings out of the pen; selection and build tools show its range.
- Browser planning is resumable and runs through a queue with an approximately
  2 ms frame budget. The host still performs route searches synchronously.
- Gameplay movement remains authoritative on the host. Browser playback follows
  confirmed movement and reconnects missed updates at walking speed, with field
  changes aligned to completed farmer actions.
  Reloaded workers start at their active saved work, delivery, or construction
  position, including fishers offshore and between tiles; older task positions
  do not restart their approach from elsewhere.
  Farmers leave their work position directly along neighboring rows and columns,
  without walking back across the completed acre; diagonal departures retain
  their tile-center waypoint.
- Maps already support chunks and local search bounds. Farmers reserve work on
  acres and woodcutters retain separate tree targets through their work trips,
  waiting when nearby trees are all claimed. Villagers do not block one another's
  movement.

### Remaining work

| Order | Improvement | Planned behavior |
| --- | --- | --- |
| 1 | Host pathfinding queue | Spread resumable searches across simulation updates under a bounded work budget. Prioritize blocked workers and urgent deliveries, keep ordinary requests from starving, and cancel obsolete requests when tasks or navigation change. |
| 2 | Cache refinement | Limit shared-cache invalidation to affected navigation where practical. Existing route caches are bounded; bound pending requests when host scheduling is added. |
| 3 | Flow fields for shared destinations | Reuse a map of travel directions toward common wells, warehouses, and workplaces across many villagers. Account for road speeds and access rules, and retain individual searches for specific acres and other unique destinations. |

Completion criteria:

- Many simultaneous task changes do not require every route search to finish in
  one simulation update; workers awaiting routes remain in a valid waiting state.
- Requests cannot starve, obsolete routes are discarded, and caches and queues
  remain bounded as villagers and towns are added.
- Workers reach accessible destinations, honor road travel times, and recover
  when obstacles, buildings, or roads change.
- Crowds do not cause repeated full-map searches; region and chunk boundaries
  preserve valid routes and destination access.
- Browser movement stays at the intended speed and crop changes follow worker
  actions while the host remains authoritative for gameplay and saves.
- Compare routing cost and simulation responsiveness against a recorded baseline
  with increasing villager counts, multiple towns, warm and cold caches, and
  obstacle changes before claiming a scaling improvement.

Host work budgets, urgency rules, crowd spacing, and the villager counts that
justify flow fields or hierarchical routing remain open implementation decisions.
