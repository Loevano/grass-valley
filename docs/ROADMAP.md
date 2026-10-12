# Roadmap

The roadmap starts with the cached zoomed-out map improvement. Design details
and cache rules are in [Backend](BACKEND.md#cached-zoomed-out-map).

| Order | Improvement | Status | Goal |
| --- | --- | --- | --- |
| 1 | Cached zoomed-out town overview | Planned; not implemented | Show the landscape and city sizes without refreshing animated scene detail. |

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
