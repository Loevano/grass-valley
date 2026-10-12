# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement | Status | Goal |
| --- | --- | --- | --- |
| 1 | Cached zoomed-out town overview | Planned; not implemented | Show the landscape and city sizes without refreshing animated scene detail. |
| 2 | Coherent terrain regions and water | Planned; not implemented | Create recognizable large regions, rivers, and lakes while retaining smaller local patches. |
| 3 | Production line balancing | Planned; not implemented | Develop clear wood, farming, and fishing chains with capped staffing, useful crafted items, and simple upgrades. |

## Cached zoomed-out town overview

| Part | Planned behavior |
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

The zoom threshold, outline shape, and inclusion of acres, roads, and construction
sites still need decisions. A lightweight overview request/change mechanism is
proposed; the existing API does not yet provide it. When implementation begins,
follow the project's correctness and performance validation requirements.

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
