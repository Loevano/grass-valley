# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement                        | Status                   | Goal                                                                                                             |
| ----- | ---------------------------------- | ------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| 1     | Cached zoomed-out town overview    | Implemented | Show the landscape and city sizes without refreshing animated scene detail.                                      |
| 2     | Production line balancing          | Planned; not implemented | Develop clear wood, farming, and fishing chains with capped staffing, useful crafted items, and simple upgrades. |

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

## Production line balancing

**Status:** planned; not implemented. Start with the wood chain, then develop
farming and fishing. This records the agreed direction; implementation is deferred.

### Worker capacity

These are maximum workers per building after worker-capacity upgrades, not
starting capacities. Upgrades unlock additional slots up to each limit.

| Production chain | Building | Maximum workers |
| --- | --- | --- |
| Wood | Woodcutter | 4 |
| Wood | Sawpit | 2 |
| Wood | Woodworker's workshop | 1 |
| Farming | Farm | 8 |
| Farming | Mill | 3 |
| Farming | Bakery | 2 |

### Wood and farming

- Woodcutters supply wood, sawpit workers make planks, and the woodworker crafts
  items that help workers work better or decorate the village.
- Higher woodworker skill levels unlock more craftable items.
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

- Production building upgrades increase internal storage or worker capacity.
- Items and worker skill levels improve conversion rates and unlock available
  options, rather than production building levels providing output bonuses.
- Keep worker capacity and internal storage independently upgradeable and show
  their effects before purchase.

Completion criteria:

- Worker upgrades respect the per-building limits above.
- Woodworker progression unlocks usable worker improvements and placeable village
  decorations.
- Fishing experience improves catch chance and access to valuable species.
- The smoke station displays stored fish by species, including smoked varieties.
- Storage and worker upgrades have clear effects; items and skills govern
  production improvements and options.

Starting worker counts, upgrade steps and costs, storage sizes, item recipes and
effects, skill thresholds, catch probabilities, fish values, smoke-station
staffing, and handling existing saves above the new worker limits remain open
before implementation. Existing production rates also need review against these
limits; this plan does not yet choose replacement rates.
