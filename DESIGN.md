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
| Choose town location? | No; randomized, separated town sites in new worlds. |
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
| Should roads be freely placed and movable? Do buildings need road access? | Dirt roads are free and instant, removable by their owner, and give players and settlers +50% walking speed. Buildings do not require roads. |
| Which services matter first: market, tavern, well, healer, or something else? | |
| Which town upgrades can players choose between? | Hall levels 1–5 unlock profession branches; individual profession improvements require practice and materials. See ROADMAP.md. |
| How do individual villager skills contribute to town achievements? | |
| How should better resource types and recipes unlock? | Hall-gated crops, animals, and workplaces, with profession practice gates for tools. Earlier resources remain useful. |
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
- New worlds share one 120×120 landscape, with randomized hall sites at least 40 tiles apart. Construction stays within 10 steps of the owner's hall, adding 5 steps per completed hall upgrade, and within the greater of 15 steps or the hall's range of its original site. Existing worlds keep their separate town maps.
- Cottages, woodcutters, farms, and storehouses occupy one tile each.
- Move a building instantly for one coin, preserving its identity. If you have no coins, moving is free so rearranging never becomes locked.
- Every three seconds, assigned workers produce into 50-unit workplace buffers and gain skill if the buffer has room. Local outputs are immediately included in usable town stock; hauling relocates them without adding them twice. Each completed building shows its own inventory. Only storehouses and warehouses can allocate incoming storage; other buildings have fixed capacities for relevant inputs and outputs. Ordinary buildings hold 50 units; processor inputs remain separately reserved.
- Town maps contain renewable forests and water, plus rock outcrops. Woodcutters
  need a tree within three horizontal/vertical tile steps; farmers fetch water during
  the watering stage without a farm-to-water distance limit. Workers animate
  walking around obstacles, gathering beside the source, and delivering supplies. Animation is cosmetic; the host still controls
  production every three seconds. Buildings use open grass tiles.
  Rock mining and resource depletion are not implemented. Existing saves gain
  resources only on unoccupied tiles, preserving their buildings.
- The hall starts with 100 wood and 100 food spaces. Storehouses add 100 shared spaces per level, allocated between resources; level 5 becomes a 500-space Warehouse. Storage workers collect outputs and distribute inputs along walkable routes. Producers deliver full buffers and fetch inputs themselves when needed. Full storage preserves buffered goods and eventually pauses work.
- Food is consumed over time. A full minute of missed meals puts settlers at risk
  of leaving with all their skills. Cottage upgrades reduce that risk; the last
  settler stays so the town can recover.
- Each cottage adds room for two villagers, up to 16 in the prototype.
- New villagers arrive when housing and sufficient food are available.
- Harvesting 30 wood still records “Timber roots”; hall upgrades now use housing, food, profession, and processing milestones. See ROADMAP.md.
- Hall levels 2–5 provide coins and unlock profession branches. Levels 6–10 and player trading remain planned.
- Simulation runs only while the host is running; no catch-up production while it is closed.
- Each browser keeps a private ownership token. Other players can view, but cannot edit, its town through the supported interface.

## Implementation status

**First village prototype:** LAN host, four persistent towns, host-side simulation,
building placement/movement, jobs, individual skill growth, housing/food needs,
storage, a town milestone and a chosen upgrade, view other towns, automatic saving.
Visible map resources, source-dependent production, and animated workers walking,
woodcutting, farming, and carrying supplies back to workplaces.

**Shared map:** new worlds show all towns and founders on one landscape, with
walkable paths connecting randomized town sites. Valley centers the camera on a
town; ownership remains separate. Existing saves retain separate town maps.

**Planned:** richer upgrade choices, trading, wilderness resource gathering,
equipment and magic, instanced cooperative quests,
personal loot, character progression, defeat/return-home flow, richer art and sound.

**Player character:** each town founder has a separate persistent villager, marked
“You”, with WASD movement in screen directions, obstacle collision, and a following
camera. The character explores the shared map in new worlds, or its town map in existing
worlds; workers keep their assigned jobs.

## Suggested development stages

1. Play and refine the village prototype; settle the unanswered town questions.
2. Add a shared valley with town entrances, movement, and resource gathering.
3. Add trading and one cooperative quest area with active combat and individual loot.
4. Expand villagers, services, specializations, quest variety, and presentation.

## Planned visual polish

- Add subtle ambient activity later: water ripples, chimney smoke, crop movement,
  and brief resource-delivery effects. These remain deferred; preserve the existing
  worker animations while refining the static village presentation.

## Confirmed progression and founder gathering

Implement hall levels 1–5 first and retain the level 6–10 design in ROADMAP.md.
Prevent production spam through shared renewable forest/fishing supplies, worker
slots, field output, and animal capacity rather than arbitrary building counts.
The founder fishes with F, chops with C, and gathers berries/herbs with G,
learning separate saved skills. Current materials, milestones, and production
rules are documented in ROADMAP.md; balance values remain adjustable.

## Confirmed settler talents and professional training

Settlers combine learned experience with permanent natural strengths and weaknesses.
One profession family gains 25% output and practice; another loses 25%. Related jobs
inherit the family's talent. Weaknesses slow training but never prohibit a career.
Settlers gain 0.1 practice per active three-second work cycle before talents,
so 10 practice takes five minutes and 20 takes ten minutes of active work.
Each 20 practice in the current job adds one base output multiplier. Traits persist
through saves and reassignment; older residents receive deterministic traits on load.

Basic gathering jobs and husbandry accept beginners. Advanced jobs require the
assigned settler's own practice, never the founder's or another resident's skill:

| Role | Required personal practice |
| --- | --- |
| Woodworking | 10 woodcutting |
| Milling | 10 farming |
| Preserving | 10 fishing |
| Crafting | 10 woodworking |
| Fish breeding | 20 fishing |
| Orcharding | 20 farming |
| Baking | 10 milling |
| Butchery | 10 husbandry |

Hall level 4 adds bakeries and butcher shops with two initial worker spaces each.
Bakeries cost 30 wood and 5 planks, processing 2 flour and 0.5 wood into 2 bread.
Butcher shops cost 25 wood and 5 planks, processing 2 barn-produced meat into
2 prepared meat. Both goods contribute 1.5 pantry food per unit and can be sold.
Processing expertise increases batch sizes and ingredient consumption together.
Farming expertise improves a field's yield while retaining its shared cycle limit;
barns still have one animal-limited production cycle regardless of worker count.

Both assignment controls show missing qualifications and prevent untrained workers
from taking advanced roles. Saved advanced assignments without prerequisites pause
until the resident trains in the required job. Balance values remain provisional.

## Confirmed next additions

Start with free, instant dirt roads that make players and settlers walk 50% faster.
Roads use the town building area, remain walkable, and can be removed by their owner.
NPC requests, stone mining, a tavern, and a first cooperative adventure are approved
future additions. Player-to-player trading requires a market; its exchange rules
still need design before implementation.
