# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement                        | Status                   | Goal                                                                                                             |
| ----- | ---------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| 1     | Cached zoomed-out town overview    | Implemented              | Show the landscape and city sizes without refreshing animated scene detail.                                      |
| 2     | Coherent terrain regions and water | Planned; not implemented | Create recognizable large regions, rivers, and lakes while retaining smaller local patches.                      |
| 3     | Production line balancing          | Wood and field cycle implemented; farming and fishing chain balance remains | Develop clear farming and fishing chains with capped staffing and simple upgrades. |

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
- New terrain loads as needed; switching worlds displays the correct map and towns.
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

## Coherent terrain regions and water

**Status:** planned; not implemented. **Priority:** after the cached zoomed-out
town overview. Design direction is recorded in
[Game design](GAME_DESIGN.md#terrain-and-water) and the
[working design](DESIGN.md#confirmed-terrain-direction).

Make the landscape less eclectic by creating large, coherent terrain regions
with natural transitions.

| Part | Planned behavior |
| --- | --- |
| Ice | Consider an icy region in one corner of the world. |
| Swamp | Create a larger main swamp with smaller swamp spots around its edges. |
| Rivers and lakes | Include large, continuous rivers and a major lake, plus smaller lakes near cities. |
| Desert | Create a substantial, continuous desert while still allowing scattered small sand patches elsewhere. |

Completion criteria:

- Main terrain regions are recognizable while exploring and on the world map.
- The swamp and desert read as large regions, with smaller surrounding swamp
  spots and occasional sand patches retaining local variety.
- Large rivers and a major lake are visible, with smaller lakes near cities.

Exact region sizes, water layouts, and whether and where to place an icy corner
remain open design decisions before implementation.

## Production line balancing

**Status:** wood chain implemented. Farming and fishing remain.
Implemented wood capacities, storage upgrades, crafted worker tools, decorations,
and base rates are documented in [Balancing](BALANCING.md#wood).

Sandbox worlds are available for supply chain experiments: all buildings are
unlocked, construction and upgrades are free and instant, cottages add residents
automatically, and villagers need no training and never depart. Production inputs,
hauling, storage, food consumption, and normal output rates still apply. The
farming and fishing changes below remain planned.

### Worker capacity

These are maximum workers per building after worker-capacity upgrades, not
starting capacities. Upgrades unlock additional slots up to each limit.

| Production chain | Building | Maximum workers |
| --- | --- | --- |
| Farming | Farm | 8 |
| Farming | Mill | 3 |
| Farming | Bakery | 2 |

### Farming

Farm acre placement and work reach now cover a 7×7 square aligned with map tiles,
including its corners. Farms can change the crop of all their linked acres in one
action; individual acre crop selection remains available. Moving or destroying a
farm now includes its linked acres; moves preserve their layout and require all
destination tiles to be valid. Farming chain balancing below remains planned.

Completed wells now provide water for crop watering alongside natural water
sources. Farmers fetch from the nearest reachable source; wells need no staff.
The field cycle is implemented: acres start unplowed, seeding waits until every
linked acre is plowed, and each seeded acre sprouts for 15 seconds. One tender
fetches water once for that acre, waters it once, and it grows for another 15
seconds before harvest and delivery. Field progress is saved, growth needs no
worker, and farmers switch between acres needing work. Remaining farming work is
chain yields, ingredient demand, worker limits, and storage balancing below.

- Farmers supply crops to the mill, which supplies ingredients to the bakery.
- Balance the chains around their worker limits, ingredient demand, and delivered
  supply, including materials needed elsewhere in the village.

### Fishing and smoking

- Fishermen remain fishermen as they develop; progression should not require
  changing them into a different profession.
- More experienced fishermen have a higher chance of catching fish.
- Higher fishing levels unlock additional, more valuable fish species.
- A separate smoke station processes and stores smoked fish. Its panel shows all
  the fish species held there, retaining species identity through smoking.

### Simple production upgrades

The upgrade panel now uses compact rows with a short benefit, cost and work time,
and an Upgrade action. Completed upgrades show a status without a disabled button;
the purchase popup retains full benefits and requirements. The compact rows retain
the implemented wood worker limits and storage upgrades. Farming and fishing
balancing, storage scaling, and worker limits below remain planned.

Production now respects each resource's existing storage allowance, including
pending deliveries and shared food space. Producers haul when an allowance fills;
existing excess stock is preserved. Workplace storage totals appear beside the
Storage heading. Wood-building upgrades increase these allowances by 25% per
level; storage scaling for farming and fishing remains planned.

- Production building upgrades increase internal storage or worker capacity.
- Items and worker skill levels improve conversion rates and unlock available
  options, rather than production building levels providing output bonuses.
- Keep worker capacity and internal storage independently upgradeable and show
  their effects before purchase.

Completion criteria:

- Worker upgrades respect the per-building limits above.
- Fishing experience improves catch chance and access to valuable species.
- The smoke station displays stored fish by species, including smoked varieties.
- Storage and worker upgrades have clear effects; items and skills govern
  production improvements and options.

For farming and fishing, starting worker counts, upgrade steps and costs, storage
sizes, skill thresholds, catch probabilities, fish values, smoke-station staffing,
and handling existing saves above the new worker limits remain open before
implementation. Their production rates also need review against these limits.
The wood rates still need play experience to assess delivered supply and pacing.
