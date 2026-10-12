# Improvement references

These four areas organize what we want to improve in Lantern Vale. Each reference
starts with the current implementation, then records proposals and open questions.
Tables and drawings are working references that we can expand as decisions are made.

| Area | Goal | Reference |
| --- | --- | --- |
| Development structure and prioritization | Understand the request flow, where calculations and rendering happen, and which work takes priority. | [Development](DEVELOPMENT.md) |
| Balancing | Set satisfying progression times, production ratios, and rewards for manual work. | [Balancing](BALANCING.md) |
| Game design | Make town building and exploration fun, with connected stories, rewarding items, and cosmetics. | [Game design](GAME_DESIGN.md) |
| Artwork | Give the game a cohesive, attractive appearance with readable characters, buildings, and landscapes. | [Artwork](ARTWORK.md) |

## Reading and updating the references

| Label | Meaning |
| --- | --- |
| Current | Observed in the code; provisional game values can still change. |
| Confirmed | An agreed design direction recorded in [DESIGN.md](DESIGN.md), or a goal requested for these improvement areas. |
| Proposed | A starting idea or target to discuss and try; it is not an implemented feature or final decision. |
| Open | A decision or measurement still needed. |

The priority tables suggest development order. They do not change runtime
scheduling. Keep runtime order, visual layering, and development priorities distinct.
When adding an improvement, record its purpose, status, priority, and what would
show that it worked. Replace proposals with decisions when agreed, and update
current values alongside their implementation.

[DESIGN.md](DESIGN.md) retains the agreed design and unresolved questions.
[ROADMAP.md](ROADMAP.md) covers town and profession progression.
[IDEAS.md](IDEAS.md) collects brainstorms. These references organize improvement
work without replacing those documents. Code remains the source of truth for
implemented behavior; older prose may lag behind it.
