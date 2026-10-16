# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement                        | Status                   | Goal                                                                                                             |
| ----- | ---------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| 1     | Cached zoomed-out town overview    | Implemented              | Show the landscape and city sizes without refreshing animated scene detail.                                      |
| 2     | Production line balancing          | Wood and grain implemented; fishing remains | Develop clear fishing chains with capped staffing and simple upgrades. |
| 3     | Scalable villager pathfinding       | Host queue implemented; cache refinement and shared navigation remain | Keep movement responsive as villager counts and towns grow, with bounded routing work. |
| 4     | Rust migration | R07 saved chunks, raw generators and nonchunked terrain-only hydration complete; authored starter geometry and typed-seed reconstruction complete; ordinary village and legacy/organic swamp phases complete; ordinary and swamp metadata plus immutable post-overlay water integration complete; private supplied-clock chunk catch-up preparation complete within its explicit clock domain; authoritative activation/regrowth remain | Follow the [staged plan and prompts](RUST_MIGRATION.md), preserving playable checkpoints and saved worlds. |

## Rust migration

The [Rust migration plan](RUST_MIGRATION.md) contains 24 host-migration stages and
three optional client stages, each with a copyable prompt and completion criteria.
The [R01 contracts and coverage audit](RUST_CONTRACTS.md) is complete; Rust now has an experimental workspace and transport smoke CLI without gameplay.
The proposed sequence keeps Python as the normal
host while the Rust core, feature coverage, save compatibility, and browser contract
are verified on disposable worlds. Default-host cutover follows the release gates.

R06 read-only staging and inspection are complete for explicit synthetic ordinary
saves and external chunk copies; see the [staging scope](RUST_LEGACY_IMPORT.md).
R07 slices 1–2 load immutable saved-chunk views with bounded decoded caching and
reproduce raw versioned Python chunk generators;
see the [terrain scope and evidence](RUST_TERRAIN.md). Slice 3a adds exact legacy
map generation and explicit ordinary/independent/shared nonchunked terrain-only
hydration, preserving saved resources and private source bytes. Slice 3b.i adds
pure authored starter terrain/resources and exact town-site selection/separation.
Slice 3b.ii integrates typed-seed saved-first starter reconstruction with exact
legacy/current offsets and source preservation. Slice 3b.iii.a adds the explicit
private ordinary village phase (150 paired Python cases), without changing raw
reads or claiming complete overlays. Slice 3b.iii.b.i adds private legacy swamp-v1
materialization (43 real Python pairs) and saved organic `p/w/c` decoding. Slice
3b.iii.b.ii adds complete explicit versioned swamp materialization, organic v2
geometry and record upgrades (100 real Python pairs plus exact layout). Slice
3b.iii.b.iii adds independent ordinary obstacle-safe placement/dialogue/layout/life
metadata (105 actual Python worlds with repeat and insertion-order checks). Slice
3b.iii.b.iv adds independent swamp world metadata, NPC home/routine relocation and
moved visit keys (264 actual Python worlds plus ordered deferred cache plans).
Slice 3b.iii.c composes saved-first metadata/overlays, honors ordered resident-copy
plans and repairs v7 water using post-overlay neighbors (35 actual Python worlds).
Slice 3c.i adds private supplied-clock chunk catch-up preparation (65 actual Python
cases), with exact source/quantity preservation and no production. Clocks above
2^53 explicitly reject pending exact integer-clock support. Slice 3c must still
complete authoritative activation/sleeping/dirty state, ordered swamp-plan writes
and resource rebind/invalidation, obstacle extraction, active versioned regrowth,
bounded caches and simulation-time resource recovery without offline production. The completed
[state/DTO boundary audit](RUST_STATE_DTOS.md) identifies unsupported features;
gameplay remains unported. Release hardware metadata, Rust gameplay RNG stream
adoption and R24 rollback acceptance remain gated as documented in the contracts. Later navigation stages cover bounded waiting, cache
validity, and shared destinations; the existing pathfinding work below remains unfinished until verified.
The later client stages separately evaluate WebAssembly sharing or a full Rust
client. Update the plan's progress table and this status as each stage completes.

## Cached zoomed-out town overview

| Part | Behavior |
| --- | --- |
| Terrain | Display simple terrain blocks, loading missing chunks and caching unchanged artwork. |
| Town markers | Render only town halls as static markers, with town names. |
| City size | Outline the current building extent of each town. |
| Main characters | Show every founder as a named, selectable marker that follows their movement, including founders without a town hall. |
| Animated detail | Omit individual buildings, workers, animals, and detailed player animations from the overview. |
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
terrain changes, founder movement, and changed town summaries trigger a redraw.
Named founder markers stay readable at every overview scale and reuse cached
terrain and town geometry when moving. Detailed playback restarts from current
state on leaving the overview.

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
Producers fetch only required inputs; storehouse haulers collect finished outputs.
Full workplace output storage pauses production until hauling or consumption frees room.
Hauling and supply trips now remove goods at pickup, keep a saved worker inventory
shown as Carrying, and credit the destination only on arrival. Carried goods remain
in resource totals but are unavailable in transit; workers retain undelivered loads
when destination storage fills or is disabled.
Advanced bakery recipes now require 5/10/15/20 baking practice for rolls, herb
bread, fruit pies, and cakes; basic bread remains available to beginners.
Ingredient affordability now uses the delivery checks' rounding tolerance, so
fractional fuel cannot strand a ready batch. Spending leaves no negative stock.
Meals cost 1.5 food/minute per working resident and 0.6 per idle resident.
The compact host test sustains 200 working residents with one top-tier mill,
one bread bakery, 12 farms, 24 acres, and four haulers; fuel is supplied.
Additional bakeries provide variety. Building panels show recipes and upgrade
options without grain-flow summaries. Recipe details use labeled rows for inputs,
output, batch time, practice, food value, and worker requirements. Production
storage shows totals and resource capacities together in Storage.
Existing extra processor slots
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
Boat cargo lists the quantity of each fish type aboard, including mixed catches.
Catch progression, storage scaling, and smokehouse staffing remain planned.

### Simple production upgrades

Building panels now lead with one combined activity status and average
host-reported cycle progress, then explain
blocked work before storage, worker assignments, and upgrades. Production and
trade controls sit with worker assignments, except Butchery, which stays expanded
directly below the status area. General instructions now live in the
Help tab under expandable topics; gameplay panels retain brief hints, current
status, requirements, and action warnings. Help groups production by sector, with
short steps, bullet lists, and reference tables. Its key table reflects custom
keybindings.
Barn panels retain their animal-first layout. Fishing huts sell the initial boat;
clicking a boat opens its own cargo and upgrade panel. Fishing balance and storage work below remain planned.

The upgrade panel now uses compact rows with a short benefit, cost and work time,
and an Upgrade action. Completed upgrades show a status without a disabled button;
the purchase popup retains full benefits and requirements. The compact rows retain
the implemented worker limits and wood, mill, and bakery storage upgrades.
Worker counts and capacity previews follow the host's staffing limits for every
profession.
Worker counts share their section heading. Villager inspectors keep one name,
job control, and talent description; shared housing appears once in the overview.
Blocked work uses its specific explanation, active animal butchery has one progress
display, and adult barn animals show a single readiness label. Markets retain the
quick-sale form without the duplicate single-item sale list; automatic sales use
their checkboxes to show enabled state. Build and move instructions appear once.
Fishing balancing and storage scaling below remain planned.

Production now respects each resource's existing storage allowance, including
pending deliveries. Each edible good and fish species now has an independent
item capacity in halls, storehouses, and workplaces. Storage filters and market
lists are organized by production trade; Option/Alt-clicking a storage resource
checkbox toggles all shown resources on or off. Category checkboxes toggle their
shown resources together and indicate partial selections. Legacy food filters expand without losing stock.
Markets also offer a quick-sale form at the top: choose 1, 5, 10, 50, 100, or
200 goods and type or choose a resource before selling at the usual price.
All individual stored resources support quick and automatic sales;
the form explains unsupported resources and insufficient stock.
The Resources popup now compares total, free, and reserved stock alongside editable
production caps, Buy at/Sell at thresholds, and actual activity over the last minute of running game time.
Basic resources, including vegetables, berries, carrots, fodder, manure, stone,
wheat, and wood, sell for 1 coin each.
Host-recorded flows exclude hauling and ingredient reservation. Merchants can buy
up to 10 goods per three seconds at twice their sale value, constrained by the
free-stock target, coins, and enabled storage. Imported goods do not advance
production milestones. Fishing balancing and storage scaling remain planned.

Storage rows now follow hall, assigned-worker recipe, and owned fishing-boat
unlocks while retaining goods already in stock. Hidden acceptance settings and
capacities are preserved; Option/Alt-click toggles only the shown resources.
Producers haul when an allowance fills;
existing excess stock is preserved. Production caps exclude reserved processor
ingredients so usable construction and upgrade stock can replenish; stored goods
and pending output still count toward the target. Workplace storage totals appear
beside the Storage heading. Wood, mill, and bakery upgrades increase these allowances by 25% per
level; farm crop storage scales with its building level. Individual acres are
available immediately on placement, have no upgrades, and retain a fixed crop
allowance. Fishing storage scaling remains planned.

Worker drop-offs now preserve the rest of the simulation tick for the next trip,
removing the extra pause after unloading. Stock accounting remains on the host;
fishing production balancing and storage upgrades below remain planned.

Farmers carrying crops continue harvesting available ripe acres on their own farm
before unloading, even across different field cycles or unfinished farm passes.
Storage allowances, production caps, and other farmers' acre claims still apply.
Vegetable harvests now yield 4.8 per acre, a further 20% reduction from 6; other crops yield 10.
Play experience should assess vegetable supply alongside the remaining chain balancing.

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
The [luxury meat balance](BALANCING.md#grain-staple-and-luxury-meat) gives
8 raw pig meat a value of 32 food when prepared, with profitable advanced recipes.
Barns grow animals without feed, with additional keepers shortening growth.
Adults wait for manual butchery; only the last animal of a species in a barn
gets a free young replacement. Delivered livestock pacing and its interaction with
grain equipment still need play experience.
Settler and player skill gains are doubled, including butchery. Existing practice
and unlock thresholds are preserved. Husbandry now trains a further 60% faster;
delivered training pace still needs playtesting.
Fishing XP now accrues a further three times as fast for settlers and the player;
saved practice and unlock thresholds are preserved. Catch progression and the
remaining fishing balance work above are still planned.
Sawpit work is labeled Sawing throughout job and skill controls, with a hand-driven
sawing animation during productive work. Saved practice and production rates are
preserved. The wood and grain rates still need play experience across town layouts
to assess delivered supply and pacing, including continuous forestry supply for the grain
chain. The grain chain has compact host fixtures for base rates and 200 residents.
The [hall upgrade path](HALL_PROGRESSION.md) now requires output from the previous
level's production unlocks, with tools and processed food at level 5 and saddle
production at level 9. The level 2 milestone accepts natural water or a completed
well within 10 tile steps of a completed farm or fishing hut; the hall requirements
show this range and the completed-cottage and food requirements. Existing cottages,
farms, and fishing huts retain milestone eligibility while upgrades are in progress;
new buildings still need to finish their initial construction.
Play experience should also assess these batch sizes and
material investments alongside the remaining fishing balance work.
Advanced woodcraft and metal sale values now cover ingredients, including fuel;
higher workshop recipe tiers preserve or improve profit per active work second.
Workshop production now uses saved quantity requests in order and waits when its
queue is empty. Order controls and the request queue appear first in the workshop
inspector. Workshop recipes supply benches, carved statues, lanterns, flower boxes,
wooden tables, signposts, and garden arches. Recipe details and build buttons explain
the crafting, delivery, and placement path. Decorations share buildings’ four free
facing directions, available in Build before placement and on completed buildings,
with rotated footprints saved when moved or reloaded.
Request cancellation preserves finished goods and releases
unused ingredients. Delivered workshop pacing still needs play experience alongside
the wood-chain balancing above.

Barn animal counts are configurable per species within shared capacity.
Linked sliders sit below each species heading and above its individual animals.
Their limits share the barn capacity without automatically reducing other species.
Barn animals have persistent names, preserved through growth, count changes,
and reloads; each new animal or last-animal replacement gets a fresh name.
Base growth takes 60/120/240/360 seconds for chickens/sheep/pigs/cows without
feed or workers; each assigned keeper adds one base growth speed.
Growth is independent of animation, routes, output storage, and production caps.
Fully grown animals stop producing and stay until manually butchered. Selecting
an adult removes it; one young replacement appears only if no animals of that
species remain in that barn. Explicitly clearing a species leaves it unstocked.
The inspector shows growth countdowns, fully grown animals, and manual butcher
rewards, with replacement previews only for the last animal of that species.
Butchers now fetch the selected animal from its barn, drag it to the shop, and
process it with a blood-pool animation. The host saves each leg, handles blocked
routes, and starts the shared processing timer only after arrival.
Butchery orders start bounded departure planning immediately. Routes to shop
perimeters use a goal-directed search so fetching does not add unnecessary
host updates before the return trip; walking, pickup, and processing keep their
normal durations.
Countdowns follow the host update schedule, including while dropdowns are open;
overdue countdowns wait for host confirmation without advancing saved growth.
Butchery requests show immediate pending feedback. Command confirmations request
a fresh snapshot after any earlier poll and redraw the controls, including when
the animal dropdown retains focus, without waiting for the next two-second poll.
Animal drawings use species proportions, with larger cows and smaller chickens;
young animals stay visible and herd size no longer shrinks individual animals.
Play experience should assess delivered output, hauling, manual butchery pacing,
and herd depletion alongside the fixed 25 eggs, wool, or milk per growing animal.
Delivered pacing remains unmeasured.

## Scalable villager pathfinding

**Status:** host pathfinding scheduling implemented. Cache refinement and shared
destination routing remain before navigation between regions.

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
- Host work travel and woodcutter target selection reuse the tick's shared
  obstacle index, avoiding repeated landscape scans that delay player commands.
  Field and pier access use private obstacle copies; resource removal refreshes
  the shared index within the tick.
  Commands now share one spatial index and a 128-unit routing allowance instead
  of finishing builder and worker searches before acknowledging orders. Pending
  searches and other towns' admission reservations survive until a full tick
  revalidates them. Butchery and upgrade latency have dedicated host benchmarks.
- Host and browser each maintain bounded route caches that workers reuse.
  Terrain, obstacles, roads, and search bounds affect validity; crop growth does not.
  Search uses BFS for cardinal routes on uniform ground and travel-time A*
  for roads or diagonal walking.
- Decorative barn, stable, and village animals follow nearby outdoor routes,
  avoiding their own shelter as well as other occupied tiles. Enclosed herds
  stay out of view until an adjacent tile becomes available.
  Barn animals roam throughout their fenced 7×7 range. Construction and moves
  keep other buildings out of the pen; selection and build tools show its range.
  Barn workers now follow host-confirmed feeding rounds through the pen, returning
  beside the barn to unload; their routes and round progress persist in saves.
  Keepers reserve separate tending destinations so their rounds do not move
  in lockstep, including at the barn's full worker capacity.
  Assigned keepers tend and train on their rounds; growth and output advance once
  per barn on an independent clock, sped up by keeper count.
- Reedhaven's expanded marsh quarters have 56 buildings, 32 residents, custom
  stilt architecture, tavern patrons, lanterns, seating, and varied marsh plants.
  Giethoorn-inspired lanes, paved gathering places, and canal boardwalks connect
  irregular residential clusters, preserving marsh pockets between the branches.
  Residents follow saved lane routes; sleeping patrons stay beside buildings.
  Square-layout upgrades preserve visits and central harvests; existing player
  construction prevents reshaping occupied settlement ground.
- Idle town villagers take short strolls with staggered pauses near their hall,
  avoiding occupied and impassable tiles. These ambient browser routes leave
  authoritative state untouched and stop when work or construction is assigned.
- Browser planning is resumable and runs through a queue with an approximately
  2 ms frame budget. Host BFS/A* searches now share a 2,048-unit update budget
  across towns, with 128-unit quanta and at most 128 pending searches. Weighted
  service prioritizes blocked workers and urgent deliveries while ordinary work
  receives regular service; reserved admission prevents saturation starvation.
  Obsolete task/navigation requests are cancelled. Waiting workers retain their
  position, cargo, assignment, and movement leg; blocked deliveries retain their
  trip and pickup state. Queue data is derived and rebuilt after reload.
  Pending searches now survive obstacle changes outside their examined tiles,
  preventing distant tree removal or NPC movement from repeatedly restarting
  long routes. Future expansions use current obstacles; changes to examined tiles
  still restart the search. Completed-cache and other graph invalidation remain broad.
- Gameplay movement remains authoritative on the host. Browser playback follows
  confirmed movement and reconnects missed updates at walking speed, with field
  changes aligned to completed farmer actions.
  Reloaded workers start at their active saved work, delivery, or construction
  position, including fishers offshore and between tiles; older task positions
  do not restart their approach from elsewhere.
  Reassigning a worker to construction preserves the visible position and follows
  the new route, releasing the previous job's playback path and phase.
  Farmers leave their work position directly along neighboring rows and columns,
  without walking back across the completed acre; diagonal departures retain
  their tile-center waypoint.
- Maps already support chunks and local search bounds. Farmers reserve work on
  acres and woodcutters retain separate tree targets through their work trips,
  waiting when nearby trees are all claimed. Villagers do not block one another's
  movement.
  Woodcutters skip unreachable trees and retry blocked targets after navigation
  changes. Only productive chopping time earns wood and practice; saved waiting
  workers deliver carried wood before selecting another tree.

### Remaining work

| Order | Improvement | Planned behavior |
| --- | --- | --- |
| 1 | Cache refinement | Pending searches retain progress across unexamined obstacle changes. Refine completed-cache invalidation and other graph inputs where practical; keep caches and searches bounded. |
| 2 | Flow fields for shared destinations | Reuse a map of travel directions toward common wells, warehouses, and workplaces across many villagers. Account for road speeds and access rules, and retain individual searches for specific acres and other unique destinations. |

Completion criteria:

- Workers reach accessible destinations, honor road travel times, and recover
  when obstacles, buildings, or roads change.
- Crowds do not cause repeated full-map searches; region and chunk boundaries
  preserve valid routes and destination access.
- Browser movement stays at the intended speed and crop changes follow worker
  actions while the host remains authoritative for gameplay and saves.
- Compare routing cost and simulation responsiveness against a recorded baseline
  with increasing villager counts, multiple towns, warm and cold caches, and
  obstacle changes before claiming a scaling improvement.

Crowd spacing and the villager counts that justify flow fields or hierarchical
routing remain open implementation decisions. Host work budgets and urgency
rules are documented in [Pathing decisions](PATHING.md#scheduling).
