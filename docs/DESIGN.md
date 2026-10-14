# Lantern Vale — working game design

Working title only. This document tracks the design conversation, confirmed answers,
temporary prototype choices, and questions to answer later. Edit the answers here or
continue the conversation. An unanswered question is not an agreed requirement.

For improvement tables and drawings, see the [four area references](README.md):
backend structure and animation priorities, balancing, game design, and artwork.

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

## Confirmed terrain direction

**Implemented for new explorable worlds:** large, coherent terrain regions use
seeded world coordinates, with irregular edges and smaller surrounding patches.

- A smaller northern ice region around Frostbrook mixes snow, groves, pools,
  and rocky outcrops.
- A western swamp surrounds Reedhaven, with dense willow groves interspersed
  with pools and narrow water channels.
- A smaller eastern desert surrounds Sunwell, with rocky areas and palm-lined
  oases; small sand patches remain elsewhere.
- Winding northern and western rivers feed a southern lake with three wooded
  islands. Smaller tributaries branch through the ice, swamp, and eastern land.
- Green terrain has three types: light-green grassland with no trees and occasional
  bushes; darker grassland with sparse trees that do not regrow; and forest with
  regenerating trees, in patches and large woods. Riverbanks and lake shores have
  more trees, forming forest strips. Swamp willows keep their regrowth behavior.
- Broad light-green grassland leaves open space for larger farming areas;
  rock outcrops and scattered rocks are less frequent across the landscape.
- Isolated water bodies of one or two tiles are removed, including tiny starter
  pools. Water connected across chunk boundaries still counts as one body.
- Each starting town has a smaller fishing lake nearby. The central treasure
  lake, its islands, starter clearings, and paths between towns remain available.

The main swamp spans roughly 210 by 240 tiles across a 480-tile world; the desert
spans roughly 160 by 200 tiles and the southern lake about 140 by 110 tiles.
Seeds vary region centers, edges, and river bends. Existing saves retain their
original generator, including unexplored chunks; create a new world or use
**Regenerate world** on a sandbox world to get the latest layout. Regeneration
resets that sandbox world's towns and progress after host confirmation. Registered
players keep their names and PINs and receive fresh villages with the same names. See the
[game design reference](GAME_DESIGN.md#terrain-and-water).

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
| Should roads be freely placed and movable? Do buildings need road access? | Dirt roads are free, take 10 seconds of builder work per tile, are removable by their owner, and give players and settlers +50% walking speed when complete. Buildings do not require roads. |
| Which services matter first: market, tavern, well, healer, or something else? | |
| Which town upgrades can players choose between? | Hall levels 1–10 unlock profession branches; individual profession improvements require practice and materials. See the [progression catalog](../server/progression.py). |
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
- New worlds share one 120×120 landscape, with randomized hall sites at least 40 tiles apart. Construction stays within 30 steps of the owner's hall, adding 5 steps per completed hall upgrade, and within the same range of its original site. Existing worlds keep their separate town maps.
- Cottages, woodcutters, farms, and storehouses occupy one tile each.
- Move a building instantly for one coin, preserving its identity. If you have no coins, moving is free so rearranging never becomes locked.
- Every three seconds, assigned workers produce into 50-unit workplace buffers and gain skill if the buffer has room. Local outputs are immediately included in usable town stock; hauling relocates them without adding them twice. Each completed building shows its own inventory. Only storehouses and warehouses can enable or disable incoming resources; other buildings have fixed capacities for relevant inputs and outputs. Ordinary buildings hold 50 units; processor inputs remain separately reserved.
- Town maps contain renewable forests and water, plus rock outcrops. Woodcutters
  need a tree within ten horizontal/vertical tile steps; farmers fetch water during
  the watering stage without a farm-to-water distance limit. Each farmer fills
  the can once per farm watering round and reuses that water across its acres.
  The next crop round needs another fill. Workers animate
  walking around obstacles, gathering beside the source, and delivering supplies. Animation is cosmetic; the host still controls
  production every three seconds. Buildings use open grass tiles.
  Trees leave visible stumps when felled and regrow on any unoccupied ground
  tile after about five minutes 34 seconds (sixteen minutes 40 seconds for willows and ancient trees, which hold
  five times the wood). Buildings and roads pause regrowth. New shared worlds
  contain a large lake with three loot islands. Exploration boats unlock at
  hall level 3, cost 20 wood and 10 planks, and automatically embark/disembark
  when crossing shores. Each shared island chest awards 25 coins once.
  Rock mining is not implemented. Existing saves gain
  resources only on unoccupied tiles, preserving their buildings.
- The hall starts with 100 wood and 100 spaces per food type. Storehouses hold 50/75/100/150/200/250/300/350 of each enabled resource across eight levels; level 5 becomes a Warehouse. Each resource has independent capacity, including separate switches and item capacities for each edible good and fish species. Disabled resources keep existing stock available to use. Legacy Food switches enable each food type, while existing stock and untyped provisions are preserved. Option/Alt-clicking a storage checkbox enables every resource. Resource and market lists share production-trade categories. Storage workers collect outputs and distribute inputs along walkable routes. Producers deliver full buffers and fetch inputs themselves when needed. Full storage preserves buffered goods and eventually pauses work.
- Woodcutters have 1/2/4 worker spaces, sawpits 1/2, and woodworker's workshops 1.
  Their building levels upgrade storage and buffers to 50/62.5/75 spaces instead
  of output. Base chopping yields 0.75 wood per productive cycle. Workshops craft
  equipable axes and saws, while furniture places benches and carved furniture
  places statues. See [wood-chain balance](BALANCING.md#wood).
- Food is consumed over time. A full minute of missed meals puts settlers at risk
  of leaving with all their skills. Cottage upgrades reduce that risk; the last
  settler stays so the town can recover.
- Population has no fixed cap: the hall shelters five villagers, and finished
  cottages add two spaces per building level plus two for timber framing.
  A minute of insufficient housing puts settlers at risk of leaving; restoring
  housing resets that timer. Settler assignment capacity is separate from housing.
- New villagers arrive when housing and sufficient food are available.
- Harvesting 30 wood still records “Timber roots”; hall upgrades now use housing, food, profession, and processing milestones. See the [progression catalog](../server/progression.py).
- Hall levels 2–10 provide coins and unlock profession branches, including leather, ore, iron, stables, saddles, steel, and trade crates. Market owners can exchange goods through saved town trade offers.
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

The first puzzle prototype is **The Sealed Quarry**, opened from the founder’s
You panel on a separate tile board. Two founders explicitly enter before it starts.
A pressure plate opens a gate; crossing secures it permanently. A carved inscription
hints at a three-rune sequence, with harmless retries. Each enrolled founder earns
20 coins and a lantern-stone keepsake on completion, once per world. Party positions,
gate state, rune sequence, and completion persist. A departing member triggers a
safety counterweight if only one remains, so that founder can finish alone. Closing
the board preserves membership; leaving returns valley movement. Entry bypasses
Flint’s survey for playtesting. Combat and a valley entrance remain future work.

1. Play and refine the village prototype; settle the unanswered town questions.
2. Add a shared valley with town entrances, movement, and resource gathering.
3. Add trading and one cooperative quest area with active combat and individual loot.
4. Expand villagers, services, specializations, quest variety, and presentation.

## Planned visual polish

- Add subtle ambient activity later: water ripples, chimney smoke, crop movement,
  and brief resource-delivery effects. These remain deferred; preserve the existing
  worker animations while refining the static village presentation.

## Confirmed progression and founder gathering

Hall levels 1–10 are implemented; see the [progression catalog](../server/progression.py)
for milestones and industrial branches.
Prevent production spam through shared renewable forest/fishing supplies, worker
slots, field output, and animal capacity rather than arbitrary building counts.
The founder fishes with F, chops with C, and gathers berries/herbs with G,
learning separate saved skills. Current materials, milestones, and production
rules are defined in the [progression catalog](../server/progression.py);
current rate examples and proposed targets are in [BALANCING.md](BALANCING.md).
Balance values remain adjustable.

## Confirmed settler talents and professional training

Settlers combine learned experience with permanent natural strengths and weaknesses.
One profession family gains 25% output and practice; another loses 25%. Related jobs
inherit the family's talent. Weaknesses slow training but never prohibit a career.
Settlers gain 0.04 practice per active three-second work cycle before talents,
so 10 practice takes 12.5 minutes and 20 takes 25 minutes of active work.
Butchery uses its higher rate described below; see [BALANCING.md](BALANCING.md)
for current rates, production ratios, and proposed pacing targets.
Each 20 practice in the current job adds one base output multiplier. Traits persist
through saves and reassignment; older residents receive deterministic traits on load.

Basic gathering jobs and husbandry accept beginners. Advanced jobs require the
assigned settler's own practice, never the founder's or another resident's skill:

| Role | Required personal practice |
| --- | --- |
| Woodworking | 10 woodcutting |
| Milling | 10 farming |
| Preserving | 10 fishing |
| Crafting | 2 woodworking |
| Fish breeding | 20 fishing |
| Orcharding | 20 farming |
| Baking | 2 milling |
| Butchery | 2 husbandry |

Hall level 4 adds bakeries with one worker space and butcher shops with two.
Bakeries cost 30 wood and 5 planks, processing 2 flour and 0.5 wood into 2 bread.
Butcher shops cost 25 wood and 5 planks, processing 2 barn-produced meat into
2 prepared meat. Both goods contribute 1.5 pantry food per unit and can be sold.
Workshops have seven selectable recipes, including crafted axes and saws;
bakeries and butcher shops each have five. Beginners
can produce the first recipe; recipes 2–5 require 2–5 personal practice in the
current profession. Only qualified assigned workers unlock selection, and each
worker must meet the chosen recipe level. Recipes use existing ingredients and
new outputs participate in storage, hauling, production caps, food, and trading.

Milling converts 2 wheat into 2 flour per three-second batch. Baking takes six
seconds for bread, rolls, and herb bread, nine for fruit pies, and twelve for
cakes. Planks and smoked fish take six seconds; workshop batches take six to
fifteen. Butcher recipes take six to twelve seconds. Batch timers belong to each
worker and persist in saves. Missing inputs, caps, and full buffers pause progress;
ingredients, outputs, and practice are committed together when a batch completes.
Completed work grants 0.04 practice per three active seconds, adjusted by talent.
Butchery grants 25% more practice from completed work with the same talent modifiers.
Manually assigned animal butchery credits its full sixty seconds on completion:
1 practice per animal before talent modifiers.
Changing assignment or recipe resets unfinished work. Fruit pies, cakes, and meat
pies provide 7, 9, and 8 pantry food per basic batch; advanced food market prices
round up their per-unit food value. Basic bread and prepared meat remain one coin.
Existing available food is revalued once when loading the new balance.

Processing expertise increases batch sizes and ingredient consumption together.
Farming expertise improves a field's yield while retaining its shared cycle limit;
barns still have one animal-limited production cycle regardless of worker count.

The base grain chain is two farms (one farmer and at least two wheat acres each),
one miller, and two bakeries (one bread baker each). Mill and bakery levels increase
storage and separate ingredient buffers by 25%/50%, without increasing output;
worker capacity upgrades stay separate. Base milling supplies 40 flour per active
minute to two bakers consuming 40 flour and 10 wood. Delivered rates depend on
field layout, water, hauling, and room for finished goods. Current capacities and
the gameplay fixture are documented in [Balancing](BALANCING.md#grains).

Both assignment controls show missing qualifications and prevent untrained workers
from taking advanced roles. Saved advanced assignments without prerequisites pause
until the resident trains in the required job. Balance values remain provisional.

## Confirmed next additions

Start with free dirt roads requiring 10 seconds of builder work per tile that make players and settlers walk 50% faster when complete.
Roads use the town building area, remain walkable, and can be removed by their owner.
NPC requests, stone mining, a tavern, and a first cooperative adventure are approved
future additions. Player-to-player trading uses market offers accepted by the other
town owner, with stock and storage rechecked before exchanging both sides.
