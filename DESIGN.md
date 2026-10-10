# Lantern Vale — working game design

Working title only. This document tracks the design conversation, confirmed answers,
temporary prototype choices, and questions to answer later. Edit the answers here or
continue the conversation. An unanswered question is not an agreed requirement.

## Confirmed design

| Topic / question | Your answer |
| --- | --- |
| How many players? | 2–4 friends, cooperative. |
| Hosting? | One Mac hosts; other Macs join over LAN. |
| Setting and tone? | Medieval with magic; cozy, relaxed, occasional danger. |
| Perspective? | Angled 2D with depth. |
| Shared or separate towns? | Each player owns a separate town in the same world. |
| Can towns be attacked? | No. Towns are safe. |
| Town mode? | Control villagers, place buildings, assign work, improve the town. |
| Explore mode? | One persistent character, WASD movement, quests and challenges. |
| Can players switch independently? | Yes; one can manage a town while others explore. |
| How does population grow? | Villagers move in as the town improves. |
| How does a town improve? | A mix of homes, food, services, and resource-funded upgrades. |
| Building placement? | Freely on a grid; buildings must remain movable. |
| What happens when needs are unmet? | Growth pauses. |
| Choose town location? | No; preset town locations. |
| Town specialties? | Develop through activities over time, rather than choosing a fixed class. |
| Who gains skills? | Villagers have individual skills; the town unlocks achievements. |
| What raises woodcutting skill? | Assign villagers to woodcutting; skill rises as they work. |
| What do skills unlock? | A mix of efficiency, better resources, and recipes. |
| Can villagers change jobs? | Yes; any villager can learn any job. Some start with strengths. |
| Are town upgrades automatic? | No; choose upgrades after reaching milestones. |
| Can friends manage your town? | No; cooperate through trading and adventures. |
| World structure? | Gradually discover a shared world with specific adventure locations. |
| Challenge types? | A mix: ruins, dungeons, helping villages, and other challenges. Safe towns exclude town raids. |
| Adventure rewards? | Resources, money, building recipes, potential villagers: a mix. |
| Combat feel? | Active: aiming, dodging, and timing. |
| Weapons and classes? | Freely mix weapons and magic; no fixed class. |
| Defeat penalty? | Return home with all belongings. |
| Can a player explore alone? | Yes, explore and gather resources alone. |
| Can a player start a quest alone? | No. At least two players start together; other players can stay in town. |
| Quest progress if someone leaves? | Remaining player can finish. |
| Quest style? | Mix of handcrafted stories/puzzles and repeatable challenges. |
| Where do quests happen? | Separate adventure areas entered together. |
| How is loot divided? | Each participant gets their own loot. |
| Saving? | Host saves one persistent world, including towns and characters. |
| Absent players' towns? | Continue production while the host is playing. |
| First playable focus? | Village building first. |

## Questions to answer later

Fill in the **Answer** column when ready. These include unresolved questions from
the conversation and practical details needed for later development.

| Question | Answer |
| --- | --- |
| Start with five villagers and a hall, or just two settlers? | |
| Start with woodcutting → homes → villagers and farming → food? | |
| Should production pause when storage fills? | |
| Pixel art or soft storybook illustration? | |
| What should moving a building cost? You wanted a small loss; a coin fee was suggested but not confirmed. | |
| Should roads be freely placed and movable? Do buildings need road access? | |
| Which services matter first: market, tavern, well, healer, or something else? | |
| Which town upgrades can players choose between? | |
| How do individual villager skills contribute to town achievements? | |
| How should better resource types and recipes unlock? | |
| Are skills tied to a job, or also to tools and buildings? | |
| Should villagers have names, personalities, and preferences? | |
| What happens to completed quest progress for absent players? Shared world progress was asked about but not settled. | |
| Can players fight enemies outside quests alone? Solo exploration/gathering is confirmed; solo combat is still open. | |
| Do separate quest areas pause when the host disconnects? How is unfinished progress saved? | |
| How should trading work: gifts, direct swaps, market orders, or a mix? | |
| Can players leave and rejoin from a different Mac? How should town ownership be recovered? | |
| Do absent towns consume food and attract villagers, or only produce? | |
| Are quests required for town upgrades, or can a solo player still develop a town? | |
| What adventure should be built first? | |
| What would make the first village prototype feel successful to you? | |

## Provisional prototype choices

These are starting values to make the prototype playable, **not final design decisions**.

- Browser client and Python host using the standard library; no installed dependencies.
- Generated geometric art to try the angled perspective before choosing an art style.
- Five villagers, a town hall, 65 wood, 40 food, 20 coins.
- Separate fixed 12×12 town grids; switch views to visit friends. Shared wilderness comes later.
- Cottages, woodcutters, farms, and storehouses occupy one tile each.
- Move a building instantly for one coin, preserving its identity. If you have no coins, moving is free so rearranging never becomes locked.
- Every three seconds, assigned workers produce resources and gain skill if storage has room.
- Storage starts at 100 per resource; each storehouse adds 100. Full storage pauses that resource's production.
- Food is consumed over time. Shortages pause arrivals; nobody dies or leaves.
- Each cottage adds room for two villagers, up to 16 in the prototype.
- New villagers arrive when housing and sufficient food are available.
- Harvesting 30 wood unlocks “Timber roots”; the player chooses a hall upgrade for 40 wood.
- The hall upgrade provides coins. Other upgrade choices and trading come later.
- Simulation runs only while the host is running; no catch-up production while it is closed.
- Each browser keeps a private ownership token. Other players can view, but cannot edit, its town through the supported interface.

## Implementation status

**First village prototype:** LAN host, four persistent towns, host-side simulation,
building placement/movement, jobs, individual skill growth, housing/food needs,
storage, a town milestone and a chosen upgrade, view other towns, automatic saving.

**Planned:** roads, richer upgrade choices, trading, shared explorable wilderness,
WASD character controls, equipment and magic, instanced cooperative quests,
personal loot, character progression, defeat/return-home flow, richer art and sound.

## Suggested development stages

1. Play and refine the village prototype; settle the unanswered town questions.
2. Add a shared valley with town entrances, movement, and resource gathering.
3. Add trading and one cooperative quest area with active combat and individual loot.
4. Expand villagers, services, specializations, quest variety, and presentation.
