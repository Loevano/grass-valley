# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement | Status | Goal |
| --- | --- | --- | --- |
| 1 | Cached zoomed-out town overview | Planned; not implemented | Show the landscape and city sizes without refreshing animated scene detail. |
| 2 | Coherent terrain regions and water | Planned; not implemented | Create recognizable large regions, rivers, and lakes while retaining smaller local patches. |

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
