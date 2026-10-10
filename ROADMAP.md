# Town hall progression roadmap

The hall now upgrades from level 1 through level 10. The table distinguishes
current rewards from proposed future unlocks. Proposed unlocks are design ideas,
not implemented features or fixed delivery promises. Higher hall levels currently
do not raise the population cap, storage capacity, or worker production rates.

| Level | Available now | Proposed future additions |
| --- | --- | --- |
| 1 — First settlement | Starting hall, 5 villagers; cottages, woodcutters, farms, fishing huts, acres, and storehouses available | Introductory tasks teaching housing, food, and worker assignment |
| 2 — Growing hamlet | +30 coins; building range grows from 10 to 15 tile steps | Paths, roads, fences, and basic village decorations |
| 3 — Resource village | +60 coins | Quarry and stone gathering; stone storage and stone-funded buildings |
| 4 — Crafting village | +90 coins | Workshop, timber processing, and craftable tools |
| 5 — Market town | +120 coins | Marketplace and player-to-player trading |
| 6 — Thriving town | +150 coins | Improved housing and a higher population cap; balance food upkeep before increasing population |
| 7 — Frontier town | +180 coins | Shared wilderness exploration and gathering expeditions |
| 8 — Adventurers' town | +210 coins | Adventure quests, equipment, and optional combat outside town |
| 9 — Enchanted town | +240 coins | Magic and magical professions connected to exploration |
| 10 — Valley capital | +270 coins; maximum hall level | Cooperative valley projects, major town customization, and repeatable endgame goals |

All previously unlocked features would remain available at later levels. Existing
buildings stay available at level 1; future progression should not remove them
from existing saves. Keep town progression possible without mandatory cooperative quests (a proposed
rule; this remains an open design question). Adventure quests require at least
two players to start, following the confirmed design. Solo exploration and
gathering remain available. Combat should not threaten saved towns.

## Current upgrade requirements

Harvest totals are cumulative across the life of the town. Spending wood does not
reduce milestone progress. Pay the wood cost when choosing an upgrade; receive
the coin reward when its timer completes. Only one upgrade can run at a time.

| Target level | Total wood harvested | Wood cost | Upgrade time | Coin reward |
| --- | --- | --- | --- | --- |
| 2 | 30 | 40 | 30 seconds | 30 |
| 3 | 90 | 80 | 60 seconds | 60 |
| 4 | 180 | 120 | 90 seconds | 90 |
| 5 | 300 | 160 | 120 seconds | 120 |
| 6 | 450 | 200 | 150 seconds | 150 |
| 7 | 630 | 240 | 180 seconds | 180 |
| 8 | 840 | 280 | 210 seconds | 210 |
| 9 | 1080 | 320 | 240 seconds | 240 |
| 10 | 1350 | 360 | 270 seconds | 270 |

Build storehouses to afford upgrades above the starting storage capacity of 100
wood. Timers pause when the host closes or a different world is opened. Existing
level 2 towns can continue upgrading, and saved upgrades retain their remaining
time. Building range stays at 15 steps from level 2 onward under the current rules.

## Suggested implementation order

1. Roads and decorations; stone gathering and storage.
2. Workshops and crafting, then trading once the resource economy is useful.
3. Housing expansion with food and performance balancing.
4. Shared wilderness and expeditions before quests, optional combat, and magic.
5. Cooperative projects and repeatable goals once the earlier systems work.

Resource requirements should expand to include stone, crafted goods, or other
resources only when those systems exist. Keep the current wood-only progression
playable until then. See [DESIGN.md](DESIGN.md) for the broader game direction.
