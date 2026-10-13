# Lantern Vale

A first playable village prototype for a cozy medieval cooperative game.
One Mac hosts; up to four players can build separate towns in the same saved world.

## Start the host

Requires Python 3.9 or newer. From this folder:

```sh
python3 server/server.py
```

Open **http://localhost:8000** in a browser. Use the Worlds menu to start a named
world or open a saved one, then enter your player name and four-digit PIN.
New players also choose a village name. Returning players recover their village
in that world using the same name and PIN from any browser.
This browser remembers your login for each world and reconnects after a refresh
or reopening the browser. It stores your player token, never your PIN. Use a
separate browser profile or private window to play as someone else.
Keep the server running while playing; stop it with Control-C.
Choose another port with `python3 server/server.py --port 8080` if needed.
Successful requests stay quiet in the terminal; failed requests are still logged.

## Manage worlds

Click **Worlds** to return to the world menu. Only the host using `localhost`
or `127.0.0.1` on this Mac can create, open, or destroy worlds. Friends join
whichever world the host opens; each world has its own towns and ownership tokens.
Inactive worlds stay saved and their production pauses.

To destroy an old world, click **Destroy world** and type its name to confirm.
This permanently removes all its towns and progress. Deleting the current world
returns everyone to the menu until the host opens or creates another world.
Your existing save appears as **My first world**, keeping its towns and ownership.

To experiment with supply chains, select **Sandbox** under **World mode** when
creating a world. Sandbox towns start with a level-10 hall, unlocking all buildings
and crops. Buildings, acres, roads, and building upgrades are free and finish
instantly; moving buildings is free too. Building or upgrading cottages immediately
adds villagers to fill the new housing capacity and settles available idle villagers
in them. Cottage capacity upgrades also provide enough housing for their residents.
Villagers can take any profession and use every recipe without training, and never
leave because of food or housing shortages. Their normal talents, practice, and
production rates still apply. Production ingredients, hauling, storage, animal
care, and food consumption run normally, so you can observe the chain's bottlenecks.
Sandbox worlds save and reopen with these rules and are labeled in the Worlds menu.

## Join from another Mac

Connect both Macs to the same local network. On the host, find its local IP address
in **System Settings → Network → your active connection → Details → TCP/IP**.
Friends open `http://HOST-IP:8000` in their browsers, replacing `HOST-IP` with that
address. Allow incoming Python connections if macOS asks. Guest Wi-Fi networks
may prevent devices from reaching one another.

This prototype is intended for a trusted LAN. It has no public account system or
encrypted transport; do not expose it through router port forwarding.

## Shared map in new worlds

New worlds use one shared 480×480 map, 16 times the area of the original
120×120 valley. Existing saves keep their map size and player towns. Terrain loads as
you explore. Everyone sees the same terrain, buildings, workers, and player
characters, and can walk between towns. Town halls start at randomized locations
at least 40 tiles apart, with nearby wood, water, and clear paths connecting the towns.

Build within 30 horizontal/vertical tile steps of your hall, adding 5 steps with
each completed upgrade (30, 35, 40, 45, 50 at levels 1–5). Construction also stays
within the same range of your original town site,
so moving a hall cannot extend that territory. Halls must stay at least
40 tiles apart when moved. Only a town's owner can build or manage its workers.
The building area is highlighted when building or inspecting a hall.

The host generates terrain in 40×40 chunks and keeps only the areas around
players and town sites in the simulation. Remote terrain is cached on demand;
the browser retains at most 128 terrain chunks. Resource regeneration catches
up when a sleeping area becomes active again. Changed chunks save separately.
Map refreshes send only terrain and resource chunks the browser needs, and walking
saves with the regular three-second simulation update.

Explorable worlds also have three small wilderness villages: Reedhaven in the
swamp, Sunwell in the desert, and Frostbrook on the snowy ice fields. Each has
eight themed cottages and workshops, an inn, a village hall, an animal shelter,
a three-stall market square, and eight residents. Willows, oasis palms, winter
pines, herb beds, berry shrubs, and a small fishing pool fill pockets between
the buildings.
Residents walk between home, work, the market, and their hall. Boatwrights and
winter woodworkers chop, fishers cast their rods, herbalists gather, and keepers
tend animals. Chickens and marsh pigs, caravan horses, or winter sheep roam beside
the village shelters. Residents pause beside visitors for conversation and trade.
These ambient routines are saved and run while the host is active; their work
does not consume shared resources or produce goods for player towns.
Marsh herbalists and boatwrights, desert caravan folk, and winter craftspeople
share connected rumors about a missing herb boat, an overdue caravan, and a silent
watchpost. These are story hooks for future quests; they do not yet offer objectives
or rewards. Open **Valley** and click a
village to find it, then walk there; click a resident or press **I** nearby to
talk and buy or sell travel equipment. The swamp village has safe clear paths;
dark marsh ground outside them still requires swamp boots. Desert and ice ground
are walkable without special gear. Existing explorable saves gain these villages
away from player buildings and roads. Existing wilderness villages expand when
their central square is free of player buildings, roads, and other locals; existing
traders and their progress stay in place. Villages with player work in the square
keep their smaller layout. Older 120×120 maps keep their layout.

**Valley** centers the camera on a town without changing maps. Existing saved
worlds keep their current maps and dimensions. Restart
the Python host, refresh both browsers, and create a new world to use the shared map.

## Play

- Window shortcuts follow the menu order: **1** Worlds, **2** Build, **3** Villagers,
  **4** Resources, **5** Items, **6** Valley, **7** Help, and **⌘,** Settings. Press a panel's number again to close it.
  **8–9** are reserved for future windows. Gameplay bindings can use letters or **0**.
- Each new town has an irregular outline and random forests, ponds, meadows,
  sandy ground, and rocky areas. Ponds have shallow shores and deeper centers.
  The starting clearing has nearby wood and water. Terrain and resources are saved,
  so returning to a town keeps the same landscape. Open ground can be built on;
  trees, water, and rocks still reserve their tiles.
  Low-poly rocks mix large, broad boulders with medium rocks and small scattered
  stones, with varied silhouettes, flat shaded faces, occasional moss, and earthy colors.
  Neighboring rock tiles join into continuous larger outcrops. Their shapes remain
  stable when returning to a saved landscape.
- Swamp regions in new towns have muddy ground, green pools, reeds, lily pads,
  and willow trees. Swamp trees supply wood and swamp water supports farming.
  Lighter swamp ground is walkable; only the darkest muddy tiles require equipped swamp boots. Existing saved terrain stays as it is.
- Friendly locals live in each biome, with matching outfits and dialogue. Look for
  Moss the swamp herbalist and Reed the marsh fisher. Click a local to read their
  introduction. Sylva gives each town a fine axe, Reed a fine fishing rod, and
  Fern the meadow grower a fine sickle once.
  Walk beside them to accept, then give the tool to a worker from that worker’s
  panel or the **Items** tab next to Resources. Gift tools add 25% output in their
  matching job. Bram the forest toolsmith sells steel and masterwork axes; Runa
  the harvest toolsmith sells steel and masterwork sickles. Steel tools cost 30
  coins and add 50% output; masterwork tools cost 60 coins and add 75%. Each
  villager carries one tool, and its bonus only applies to the matching job.
  Twelve more locals now live across the six biomes: Pip and Wren, Hazel and
  Alder, Poppy and Mabel, Dara and Kestrel, Orin and Petra, Nell and Willow.
  Wren, Alder, and Poppy give spare worker tools; Dara gives desert clothing and
  Petra gives swamp boots. Nell sells steel/masterwork fishing rods with
  +50%/+75% fishing output, and Kestrel sells upgraded axes. Pip, Hazel, Mabel,
  Orin, and Willow trade firewood for bread, berries, eggs, vegetables, and fish.
  Their panels show exact costs; trades need storage and restock every five
  minutes. Gifts are once per town and clothing appears in your character inventory.
  Existing saves gain these new locals without moving the original ones.
  Matching workers appear first in the item picker. Select a villager to assign
  or transfer a tool immediately, or select **Nobody** to return it to inventory.
  The Villagers overview shows equipped tool icons beside names; hover or focus an
  icon to see its bonus and whether it matches the current job.
  Tobin and Clover each exchange 10 wood for 8 food once every five minutes of
  running game time, provided you have food storage room. Gifts and restocking
  are saved per town, so each player can collect their own rewards.
  Locals are separate from town workers; their positions are saved.
- Moss, Tobin, and Flint offer playable stories. Walk beside them and open their
  panel to accept a request, see its supplies and objectives, and continue the story.
  Moss needs herbs and food for **A Remedy Remembered**. Tobin needs provisions,
  three completed town road tiles, and wood for **The Lantern Road**. Flint asks you
  to examine three different rock outcrops for **Marks Beneath the Stone**: accept
  his survey, walk beside a rock, select it, and choose **Examine markings**.
  Return to Flint with your notes, then supply wood for a charcoal map.
  Each story awards coins and a keepsake once per town. Progress, examined rocks,
  and rewards survive reloads; shared-world players complete their own stories.
  Select **You** to review active objectives and collected keepsakes. Flint’s map
  leads into **The Sealed Quarry**, the first cooperative puzzle prototype.
- Select **You**, then **The Sealed Quarry · Puzzle prototype** to open its separate
  adventure board. Entry is available immediately for playtesting; completing
  Flint’s survey is optional. At least two founders must each choose **Enter quarry**
  before movement unlocks. Use two browser profiles or a normal and private window
  to test locally with separate towns in the same world.
  One founder holds the floor plate while a companion crosses the gate, securing
  it open for everyone. Beyond the gate, read the inscription and touch the three
  runes in the hinted order, then recover the lantern stone from the chest.
  Incorrect sequences reset without a penalty. Every expedition member receives
  20 coins and a lantern-stone keepsake once, including members who already left.
  **Close map** keeps you inside; reopen from You. **Leave expedition** returns
  control to the valley, and triggers a safety counterweight if only one founder
  remains, allowing them to finish. Members can rejoin an unfinished expedition.
  Positions, gate state, rune progress, and completion save with the world. Town
  production continues during the adventure. There is one expedition per world;
  replay it in a new world. Browser disconnection keeps your place in the expedition;
  use **Leave expedition** before disconnecting if your companion still needs the gate.
- Select **You**, then **The Falling Lanterns · Movement quest** for a real-time
  cooperative courier course. At least two founders must enter and stay inside;
  movement and hazards pause whenever fewer than two remain. Watch amber **!**
  warning tiles and dodge red **×** debris with your movement keys or direction
  buttons. Column **C** is a safe checkpoint; hits return you there (or to the
  entrance before reaching it) without consuming resources. Two founders must
  reach **T** together before either can recover the lost satchel. Every expedition
  member earns 25 coins and a courier lantern badge once per world. Closing the
  map leaves the expedition; rejoining preserves your position and checkpoint.
  Progress saves with the world, and hazards do not run while the host is stopped.
  A disconnected browser remains inside until its founder rejoins and leaves.
- Your gold-clad villager is marked **You**. Click the map and hold **WASD** to
  walk in screen directions. Press **Y** to toggle camera lock: when locked, the
  camera follows your movement; when unlocked, walking leaves the camera in place.
  Press **⌘,** to open **Settings** and rebind movement and camera lock keys or restore defaults.
  Settings also offers **Low**, **Medium**, and **High** graphics quality. High
  (the default) uses full display resolution, Medium caps it at standard resolution,
  and Low uses reduced resolution to help slower devices. Changes apply immediately.
  Preferences are saved in this browser. Scroll to pan; pinch on the touchpad or
  Option/Alt-scroll to zoom.
  Zooming out switches to a cached overview with simple terrain blocks, named
  town-hall markers, and outlines of each town's buildings, acres, and construction
  sites. Click a hall marker to inspect its town. Roads do not extend the outline.
  The overview hides individual buildings and characters; zoom in to see them.
  Build and move tools keep their detailed map interactions at any zoom level.
  Buildings, trees, water, and rocks block movement. Your position is saved per
  world. Movement pauses while typing, placing buildings, or using world menus.
- Select **You** to inspect your equipment and equip or remove items. Swamp boots
  (15 coins) unlock the darkest swamp tiles. Sand is walkable without equipment;
  desert clothing (20 coins) gives +20% berries per gathering cycle while equipped.
  Swamp boots also give +20% herbs per gathering cycle. Desert dunes and ice fields are
  walkable without protective clothing. Boats allow sailing on water.
  Walk beside a local and press **I** (or click them) to discover their offers.
  Dara and Sunwell traders buy and sell desert clothing; Petra and Reedhaven traders
  buy and sell swamp boots. Clothing perks appear in their offers and your inventory.
  **I** selects the nearest local within trading distance; change it in Settings. Sale prices are half
  the purchase price, rounded down. Remove worn gear before selling your last copy,
  and return to safe ground before removing protective clothing. Island treasure
  includes an equipment item alongside its 25 coins. Items and worn gear are saved
  per character; settlers keep their existing movement rules.
- The town hall allows new buildings and moves within 30 horizontal/vertical tile
  steps (a diagonal counts as two). Each completed upgrade adds 5 steps to the range.
  Click the hall to see its range; the map highlights it while inspecting the hall
  or opening Build. Existing buildings outside the range keep working. Moving the
  hall within its current range shifts the building area to its new location.
- Open **Build**, choose a building, hover over open grass to highlight its tile, then left-click
  to place it and spend wood. Keep clicking open tiles to place more of the selected building,
  or **Cancel selection** to clear it. Place woodcutters within ten horizontal/vertical tile steps
  of trees; farmers can fetch water farther away during the watering stage.
- Choose **Dirt road** in Build to place free road construction sites on open ground within
  your town’s building area. Each tile takes 10 seconds of builder work after arrival.
  Players and settlers walk 50% faster on completed road tiles.
  Settlers choose routes by travel time, taking a longer road path when it gets
  them there sooner. Unfinished roads provide no routing bonus; cached routes
  account for changes to completed roads.
  Everyone can use roads in the shared valley. Click a road to inspect it; its
  owner can remove it for free. Roads are saved with the town.
- New buildings and acres start as construction sites. Idle villagers automatically
  help build; assign **Builder** in the villager panel to give a resident priority.
  Builders walk to reachable sites, then work through foundations, framing, and
  finishing over 30 seconds. Housing, storage, and workplaces activate only on
  completion. Progress survives saving, moving a site, and restarting the host.
- Trees are felled when their remaining wood reaches zero, freeing their tile for
  building and leaving a visible stump. They regrow after about five minutes 34 seconds on all ground while
  the tile is unoccupied. Buildings and roads pause regrowth until removed.
  Swamp willows and rare ancient trees take five times as much chopping and
  about sixteen minutes 40 seconds to regrow.
  Water regenerates its shared supply. Rock outcrops cannot be mined yet.
- New shared worlds have a large lake with three treasure islands. At hall level 3,
  select **You** and build an **Exploration boat** for 20 wood and 10 planks.
  Walk onto water to sail automatically and step onto land to disembark. Stand
  beside an island chest and press **G** (or choose **Collect loot**) for 25 coins.
  Each chest can be claimed once across the whole world; boat ownership and
  collected loot are saved. Existing landscapes keep their layout.
  You can also **Buy exploration boat** for 20 coins in the **You** panel.
  Both exploration and fishing-hut boats have three tiers. Fishing-hut boats
  start with the existing 10-plank purchase at hall level 3 and 20 fishing practice;
  improved boats need hall level 4 and 30 fishing practice (15 planks + 20 coins),
  and masterwork boats need hall level 5 and 40 practice (20 planks + 40 coins).
  Fishing-hut boats add 3, 5, then 7 tiles of reach; the two upgrades add 25%
  then 50% fish per catch. Exploration upgrades add 25% then 50% sailing speed
  and fish per catch while afloat. Each upgrade requires the previous tier.
  Each fishing hut buys and owns its own boat. The initial purchase applies
  immediately and shows a full-size boat on nearby water; click it to manage its
  hut. Assigned fishers prefer deeper connected water within the hut's reach,
  share one voyage and carry 5, 10, or 20 fish across the three boat tiers. They
  fish offshore until the boat is full or the spot runs out, then return to unload
  at the dock while staying aboard. Cargo is shared by all crew and survives saving.
  Idle boats stay by the bank.
  Depth counts tile steps from the nearest bank, including island shores.
  Boat tiers can sail in depth 2, 3, and 4 water, respectively, unlocking trout
  (1.5 food / 2 coins), salmon (2 food / 3 coins), and sturgeon (3 food / 5 coins).
  Shallow water still produces ordinary fish (1 food / 1 coin). These fish share
  food storage, can be hauled and eaten, and can be sold at a market stall.
  Boat upgrades use builders; exploration purchases and upgrades apply
  immediately. Existing boats keep their ownership and start at tier 1.
- Build a **Well · 15 wood** from **Food & farming** at any hall level. Once
  construction finishes, workers can fetch its water when watering crops, using
  the nearest reachable water source. Leave at least one adjacent tile accessible.
  Wells supply water automatically without assigned workers or upgrades.
- Select a farm and choose **Build acre · 5 wood**, then click open ground within
  three tiles of that farm in each direction, including diagonals. This creates a
  7×7 square aligned with the map tiles, with the farm at its center (up to 48 acres).
  Farmers tend its acres and deliver food back to
  the farm. Farmers fetch reachable water when watering; the farm can be farther
  than three steps from water. Farms without
  nearby acres use the same farming cycle on the farm's own patch. Acres are saved
  with their farm association. Moving a farm moves all its linked acres by the same offset,
  preserving crops, upgrades, stored goods, and assignments. The whole move costs
  one coin (free in sandbox or with no coins) and requires every destination tile
  to be clear and inside the building area. The move preview includes the acres.
  Destroying a farm also destroys its linked acres, including construction sites,
  releases their assigned workers, and salvages their stored goods at the hall.
  In the farm's **Production** section, use **Crop for all acres** to change every
  linked acre to one unlocked crop, including acres still under construction.
  The selector shows **Mixed crops** when the acres grow different types.
  Individual acres can still have their crop changed separately.
- Click a building, moving villager, or resource to open its information panel.
  Click the town hall for milestone progress and its upgrade. Close panels with
  Escape. On small screens, panels appear as a bottom drawer.
- Every finished building except the town hall has an **Add settler** control,
  an assigned roster, and removal controls. Woodcutters, fishing huts, storehouses,
  and acres start with 1 space, cottages with 2, and farms with 4. Most buildings upgrade settler capacity twice
  for 20 then 40 wood to reach twice then three times the initial spaces. Capacity
  upgrades take 30 then 60 seconds of builder work and do not increase housing
  or storage.
  The wood chain has its own worker limits: woodcutters upgrade from 1 to 2 to 4
  spaces, sawpits from 1 to 2, and woodworker's workshops stay at 1. The same
  costs and builder times apply to their available capacity upgrades.
  Assignment transfers an existing resident; it does not create a new settler.
  Workplaces set the appropriate job; acres assign farmers to that specific field.
  The town hall is reserved for your controllable character and has no resident
  assignment or settler-capacity upgrades. Settlers assigned to cottages rest
  and help construction automatically. Moving preserves assignments; destroying
  a building releases them.
- Every villager has a name, including new arrivals. Numbered settlers in older
  saves receive names when the host loads the world; existing names are preserved.
- Settlers have permanent natural strengths and weaknesses: +25% or −25%
  output and practice gained in one profession family. Every 20 practice in
  their current job adds another base output multiplier. Neutral settlers gain
  0.04 practice per active three-second work cycle: about 12½ minutes to reach
  10 practice and 25 minutes for each output bonus. Butchery gains 25% more
  practice from completed work, with the same talent modifiers. Travel and blocked work
  pause training. Their panel shows talents
  and current output; workplace selectors show qualifications and output too.
  Resting settlers are labeled and listed first, including cottage residents
  who can be reassigned directly to a workplace.
  Basic woodcutting, farming, fishing, and husbandry train anyone. Advanced roles
  require that settler's own practice: sawpit workers need 10 woodcutting,
  millers 10 farming, preservers 10 fishing, crafters 2 woodworking,
  fish breeders 20 fishing, orchard workers 20 farming, bakers 2 milling,
  and butchers 2 husbandry. Another settler or the player's skill cannot qualify them.
  Existing settlers gain saved talents when loading; unqualified advanced workers
  pause until trained in the prerequisite job.
- Hall level 4 unlocks bakeries (30 wood + 5 planks) and butcher shops
  (25 wood + 5 planks), each with two initial worker spaces. Bakeries use
  2 flour + 0.5 wood to make 2 bread; butcher shops use 2 meat from barns
  to make 2 prepared meat. Both foods supply 1.5 pantry food per unit and can
  be sold at a market. Processing skill increases batch size and ingredient use
  together. Farmers' skill improves field yields; animals still limit barn output,
  and only one keeper tends a barn each cycle.
- Workshops offer seven recipes; bakeries and butcher shops offer five, selected in the
  workplace panel. Level 1 is available to beginners; levels 2–5 require 2–5
  practice in crafting, baking, or butchery respectively. Unlocks use qualified
  workers assigned to that workplace. Each worker must meet the selected recipe's
  level to produce it. Existing workplaces retain their original recipe by default.

  | Recipe level | Workshop | Bakery | Butcher shop |
  | --- | --- | --- | --- |
  | 1 | Tools | Bread | Prepared meat |
  | 2 | Wooden bowls | Bread rolls | Herbed meat |
  | 3 | Crates | Herb bread | Smoked meat |
  | 4 | Furniture | Fruit pies | Sausages |
  | 5 | Carved furniture | Cakes | Meat pies |

  Workshops also make **Crafted axes** at 2 crafting practice
  (2 planks + 1 tool, 9 seconds) and **Crafted saws** at 3 practice
  (2 planks + 2 tools, 12 seconds). Use **Items** or the worker's panel to assign
  one tool per villager. Axes add 25% woodcutting output; saws add 25% plank output
  and wood input per batch. Tools can be returned or transferred. These replace
  workplace axe and saw upgrades; existing saved upgrades become reusable items.
  Existing sawpits keep two worker spaces, higher tiers are capped at two, and
  workshops are capped at one. Excess workers rest, keeping skills and equipment.

  **Build → Decorations** lets you place a wooden bench for 1 furniture or a
  carved wooden statue for 1 carved furniture. Placement is immediate on an open
  tile within the town's build range. Each occupies one tile and can be moved or
  removed like a building; removal gives no refund. Decorations have no production,
  worker, storage, or upgrade benefits.

  Recipes show ingredient costs in the selector and use existing town resources.
  New products can be stored, hauled, capped, and sold at the market. Prepared
  foods supply 1.75–4.5 pantry food per unit at recipe levels 2–5. Fruit pies,
  cakes, and meat pies supply 7, 9, and 8 food per basic batch respectively.
  Advanced foods sell for their per-unit food value rounded up in coins;
  basic bread and prepared meat remain 1 coin each. Storehouse
  switches include the new crafted goods. Switching recipes releases unused
  reserved ingredients for collection without losing them.
- Mills turn 2 wheat into 2 flour in 3 seconds. Bread, rolls, and herb bread
  take 6 seconds per batch; fruit pies take 9 seconds and cakes take 12.
  Prepared and herbed meat take 6 seconds, smoked meat and sausages 9,
  and meat pies 12. Planks and smoked fish take 6 seconds; workshop recipes
  take 6–15 seconds according to complexity. These are active production times,
  excluding transport and blocked work. Each worker has an independent saved
  batch timer. Ingredients are consumed and goods awarded only at completion;
  changing assignment or recipe discards unfinished work without ingredient loss.
  Successful batches award 0.04 practice per 3 seconds, adjusted by talent
  (a neutral baker gains 0.08 per bread batch). Blocked work earns no practice.
- Open **Villagers** for a screen-wide roster with every resident’s details and job
  controls visible together. The roster scrolls when needed and stacks on smaller
  screens. You can also select one villager on the map to assign a job.
  Their skills increase as they work. **Help** contains the basic instructions.
- A minute of consecutive missed meals puts settlers at risk of leaving (5% per
  three-second tick before housing benefits). Departures permanently remove their
  skills and stop their work. The Villagers panel warns about shortages and keeps
  the latest departure visible. Feeding everyone resets the shortage timer; each
  departure gives another minute of grace and delays new arrivals for a minute.
  The last settler stays to help recovery. Level 2/3 cottages provide 25%/50% lower
  departure risk per housing space, shared across the population (up to 75% with
  timber framing). Only finished cottages count; capacity upgrades add worker
  slots rather than comfort.
- There is no fixed population limit. The town hall shelters five settlers;
  each finished cottage provides two housing spaces per building level, plus
  two for timber framing. New arrivals wait for free housing and at least 20 food
  after meals, consuming 10 food when they arrive. If housing is lost, a minute
  without enough spaces puts settlers at a 5% departure risk per three-second tick.
  Restoring enough housing resets this timer; a departure resets both shortage
  timers and delays arrivals for a minute. The last settler stays.
- Use arrow keys and Enter on the focused map to inspect tiles or choose build locations.
- Workers walk around obstacles to their resource spots, chop wood or tend crops,
  and carry supplies back to their workplace. They retain valid paths until their
  task changes or their next step is blocked. These animations illustrate work;
  production still follows the host's three-second rhythm. If a workplace is out
  of reach of its wood or fishing source, production and skill growth pause;
  move it closer to the source to resume.
- Farmers plow with a hoe, sow seeds, water with a can, and harvest with a sickle,
  then carry crops back to the farm. New acres start as unplowed soil. Every acre
  linked to a farm must be plowed before its workers can start seeding. Each seeded
  acre sprouts independently for **15 seconds**, then waits for water. Its tender
  fills the can **once per farm watering round** at a reachable well or natural
  water source, then uses that fill on every acre it tends in that round. Each
  farmer collects their own water; the next crop round needs a fresh fill. The crop then
  grows for another **15 seconds** before harvest. Each field action takes two
  seconds; travel adds time. Farmers finish each pass in rows, turning at the end
  of a row and choosing separate rows when possible. All acres finish their oldest
  crop cycle before harvested acres start another, so distant tiles are not skipped.
  Each harvest yields **10 crops per acre** before skill, tool, upgrade, and manure
  bonuses. Only one worker tends an acre at a time. Growth continues without workers or storage room;
  crops become available after harvesting and delivery. Field progress survives
  saving and moving. Changing the crop selection applies at the next seeding.
  Field visuals follow the farmer's confirmed animation: furrows, seedlings,
  watering changes, and cleared crops appear when the corresponding action finishes.
  Missed updates and resumed work reconnect at walking speed rather than making
  the farmer rush to catch up.
- Build a fishing hut within three tile steps of water and assign Fishing to
  produce food and learn fishing skill. Fishers cast into the water, wait by a
  rippling float, reel in a fish, and carry their catch back to the hut.
  Assigned fishers use a wooden pier extending from their hut to nearby water,
  so a hut on the only accessible bank does not block their fishing spot.
  Casts reach up to three more tiles into connected water when space allows.
  Fishing attempts have a 90% chance of catching food; an unlucky attempt
  yields no food, but still builds skill. Empty casts reel in and try again.
- Woodcutters produce 0.75 wood per base three-second productive cycle:
  15 wood per active minute. Four base woodcutters supply two sawpit workers
  (40 wood/minute) with a 20 wood/minute margin before transport and other demand.
  Two crafted saws raise demand to 50 wood/minute. Assigned workers consume
  0.30 food each tick, compared with 0.12 for resting villagers.
- Hall upgrades also require a builder at the hall. The first requires housing and reliable food, costs 40 wood, and
  takes 30 seconds of construction after arrival; the hall
  panel shows remaining time. Closing the host pauses the upgrade.
- Completed workplaces hold storage: **50 units** in ordinary workplaces,
  200 in the hall, and 50–350 per enabled resource across eight storehouse levels.
  Its **Storage** section shows goods held there. Wood and other products become available for construction,
  meals, and sales when a villager finishes returning and unloading at their workplace.
  Goods still being gathered or carried are not available to spend. The host tracks
  these work cycles, including water trips, and preserves them in saves. Hauling relocates
  those goods without increasing the town total again. Processors and barns also
  reserve up to 50 input units locally; reserved ingredients are excluded from
  available stock. Old shared stock is preserved at the hall when loading, and
  old workplace outputs become available once. Demolition preserves available
  goods at the hall.
- Each storehouse has an **enable/disable switch for each resource** in its
  Storage section. Capacity per resource at levels 1–8 is **50/75/100/150/200/250/300/350**.
  Level 5 becomes a Warehouse. Upgrades increase every resource's capacity
  automatically; resources do not compete for shared space. Food groups edible
  goods in one capacity, weighted by pantry value. New storehouses start with all
  resources disabled. Disabling a resource stops incoming deliveries and keeps
  existing goods available for processing, construction, meals, and sales.
  Old positive allocations become enabled resources; saved stock remains intact.
- Open **Rules** (8) to set production caps per resource for your own town.
  Blank means unlimited; 0 pauses that resource. Workers and player gathering stop
  at the target and resume as stock is used. Stored goods, reserved inputs, and
  undelivered output count once. Recipe and animal batches wait until the full
  batch fits; hauling, purchases, and existing stock are preserved. Rules are saved
  with the town and do not affect other towns.
- Workplaces keep fixed storage for their relevant inputs and outputs within a
  50-space budget. The hall has 100 wood and 100 food spaces; homes have none.
  Production respects each resource's displayed limit, including goods still
  returning to the workplace and the shared food allowance. A sawpit holds at
  most 20 planks at level 1 (25 at level 2, 30 at level 3); its wood spaces cannot
  be used for extra planks. Existing excess stock stays available to spend or haul,
  and production waits for room.
  Processors fetch ingredients from buildings that actually hold them.
- Enable only wood in a storehouse near the woods, and only planks in another
  near the city. The first hauler collects wood from the woodcutter and supplies
  the sawpit; the second collects finished planks. Builders can use those planks
  for construction. Assign a hauler to each storehouse to run this chain.
- Assign settlers to a **Storehouse** as haulers. They collect small batches and
  distribute recipe ingredients and animal feed so producers can keep working.
  Without hauling support, producers fetch their own inputs and deliver outputs
  once their output's storage allowance fills. Transport takes time and follows
  open paths; roads speed it up. Full, disabled, or unreachable storage leaves outputs at
  their workplace and pauses production when the buffer fills. Moving, upgrading,
  or reassigning during transport preserves goods and cancels affected deliveries.
- Storehouses become a **Warehouse** at level 5 and upgrade through level 8,
  reaching 350 capacity per resource. Levels 6/7/8 cost 125/150/175 wood and
  require 150/180/210 seconds of builder work respectively.
  Building upgrades increase storage; settler-capacity upgrades add hauler slots.
- New landscapes contain denser forests, including timber patches beside towns.
  At distant zoom, trees merge into a forest canopy and rock artwork is hidden.
  Zoom in to see individual trees and rocks; their tiles remain selectable at any zoom.
- Build cottages to attract residents; maintain food and add storage as needed.
- Select any completed cottage, storehouse, woodcutter, farm, fishing hut, or acre
  to upgrade it through level 3 (storehouses through level 8). Builders work for 30 seconds to reach level 2
  and 60 seconds to reach level 3 after arriving. Each upgrade costs its original
  building price multiplied by its current level; the panel previews the benefit.
  Profession upgrades take 30 seconds of builder work. Materials are paid when
  starting; buildings pause their benefits and production until work finishes.
  Assigned workers automatically help upgrade their own workplace, then resume
  their jobs; farmers assigned to acres also help upgrade their parent farm.
  Dedicated builders keep priority, and resting villagers help automatically.
  Each site has one worker at a time. Remaining time counts work, excluding
  travel and waiting for an available worker or reachable route.
  Builders can cross completed acres to reach farms surrounded by fields.
  Upgrade progress survives saving, moving, and restarting the host.
  Cottages provide 2/4/6 housing spaces (the town limit stays 16), and storehouses
  hold 50/75/100/150/200/250/300/350 of each enabled resource. Level 5 becomes a Warehouse.
  Woodcutters, sawpits, and woodworker's workshops increase storage and production
  buffers from 50 to 62.5 to 75 spaces at levels 1/2/3; input buffers and resource
  allocations scale too. These levels provide no output bonus. Farms, fishing huts,
  and acres gain 25% production per worker at level 2 and 50% at level 3.
  Farm and acre bonuses add together on that field. Fishing bonuses increase food
  from successful catches; the 90% catch chance stays the same. Workers still need
  nearby resources. Upgrades survive saving and moving the building.
  Completed building upgrades change the artwork; settler-capacity upgrades add
  separate blue markers:
  level 2 adds timber framing and roof trim; level 3 adds stonework and a dormer.
  Acres gain raised borders and irrigation, halls develop through five visual
  levels, and profession upgrades add visible tools, nets, boats, or framing.
  Each workplace has its own roof color and equipment. Wall plaques show building
  level. Hover over or select a building to read its level, capacity tier, and
  profession improvements. Advanced workers saw, grind, smoke fish, hammer, feed
  animals and fish, pick fruit, knead dough, or prepare meat at their assigned
  workplace, then leave goods in its output buffer for collection. Work animations pause with production.
  Building, moving, demolition, animal purchases, crop changes, assignments, sales,
  and completed upgrades have brief visual feedback.
- Click a building, then **Destroy building** to remove it without a resource
  refund. The town hall cannot be destroyed. Removing workplaces pauses affected
  workers until another suitable workplace is available; existing residents stay.
- Click **Move** in a building’s information panel, then an empty tile.
  You can also use **Move a building** in the Build panel. Moving costs one coin,
  or is free if you have none, so rearranging is always possible.
- Town hall levels 1–5 now unlock profession branches. Level 2 brings nets, wheat,
  and barns/chickens; level 3 adds boats, sheep, carrots/fodder, and
  sawpits/planks; level 4 adds mills, smokehouses, pigs, and workshops/tools;
  level 5 adds fish breeding ponds, orchards, cows, timber framing,
  and market stalls. Each upgrade has a development milestone and material cost
  shown in the hall panel. Levels 6–10 add mining, metalworking, horses, and advanced trade.
- Barns show each purchased or bred chicken, sheep, pig, and cow with walking,
  pecking, and grazing animations. They roam up to two open tiles from their barn
  and return, avoiding buildings, resources, and impassable ground.
- Bought and bred animals start young and grow visibly over five minutes of full
  feeding by barn workers, with smooth size changes between growth updates.
  Growth pauses without a full ration of fodder or
  vegetables. Only adults produce eggs, wool, or milk and can breed. Existing
  animals from older saves remain adults. Pigs no longer produce recurring meat.
  At a butcher shop, select an adult from a barn and choose **Use animal for meat**.
  This permanently consumes one animal; a qualified butcher needs one minute of
  work at the shop to produce 2/4/8/10 raw meat for a chicken/sheep/pig/cow.
  Each completed animal grants 1 butchery practice, adjusted by talent, crediting
  the full minute of manually assigned work.
  Extra workers cannot duplicate or accelerate the batch. Processing pauses
  without a worker, and completed meat waits for buffer space or a production cap
  to allow it. Existing meat recipes resume afterward; hauling works as usual.
- Select an acre to choose its crop, a barn to buy/breed animals, or a workplace
  to unlock profession improvements. Assign settlers to new production buildings.
  Sawpits use wood; mills use wheat; smokehouses use any mix of fish, trout, salmon,
  or sturgeon plus wood (2 raw fish and 0.5 wood per base batch); workshops use
  planks and wood; breeding ponds use fodder. The Build panel shows stored goods.
- Forest trees and connected fishing waters have limited supplies
  shared by workers and players. Additional huts cannot duplicate a pond's supply.
  Worker slots, field output, barn animal capacity, and storage bound production.
  Source and worker panels show supply and pauses. Barns use fodder or vegetables;
  manure improves field yield and wool makes durable nets.
- Your character can fish with **F**, chop with **C**, and gather berries or herbs
  with **G** while standing beside the source. Actions repeat every three seconds
  until you move, press the same key again, or press Escape. Select **You** to see
  fishing, woodcutting, and gathering levels. Every 20 practice raises a level;
  work and practice pause when storage is full or supplies are empty. These keys
  can be changed in Settings without losing older movement preferences.
- Edible goods are included in the Food total. Eating, recipes, and sales consume
  the same stock; goods are not an extra copy of the pantry. Existing saved wood,
  food, buildings, and hall levels remain; paid hall upgrades still finish.
- Open **Valley** and click another town's name to find it. Only its owner can issue commands for it.

The host saves all worlds automatically in `savegame.json`; new worlds also save
changed map chunks in `savegame_chunks/`. Both are excluded from Git.
Player names and salted PIN hashes are saved privately per world. Enter your name
and PIN whenever you connect. Your browser retains an ownership token so an older
village can be linked to a player name on its first login; use its original browser
for that first login. PINs are never stored in browser storage or public snapshots.
The host can provision older villages with salted credentials in the ignored
`savegame.players.json`; these are imported on startup without replacing existing
accounts. Back up the save file, chunk directory, and this private provisioning file
while the server is stopped.
Towns in the open world keep producing while owners are away if the host is running.
No production occurs while the host is stopped or for worlds that are not open.
Existing saves keep their resource layout and gain varied ground. Saves without
resources gain resource spots on unoccupied tiles when the host restarts;
existing buildings stay in place. Move older workplaces near trees or water as needed.

Combat, magic, trading, and cooperative quests are not implemented yet.
See [ROADMAP.md](docs/ROADMAP.md) for planned improvements.
See [DESIGN.md](docs/DESIGN.md) for agreed decisions, provisional values, and questions
you can answer later.
See the [improvement references](docs/README.md) for backend structure and animation
priorities, balancing theory, game design, and artwork, with tables and diagrams.

## Check the simulation

Tests and performance comparisons do not run by default during development or
before committing. Run them only when investigating or fixing a performance-related
problem, or when explicitly requested. For a performance-related problem, run
targeted tests during development (individual commands are listed below), then
run every Python and JavaScript correctness test once before committing:

```sh
python3 tools/check.py
```

For browser performance comparisons, install the development-only browser harness once:

```sh
npm ci
npx playwright install chromium
```

Run performance comparisons when investigating or fixing a performance-related
problem involving simulation work, request frequency or payload size, scene
preparation, routing, animation scheduling, rendering, or caches. A change that
could affect runtime performance does not by itself require tests or a performance
comparison. Repeat validation only when subsequent edits or unresolved failures
warrant it.

Before working on a performance-related problem, record the current commit with
`git rev-parse HEAD`. After correctness checks pass for the final code, compare
against that commit on the same machine without repeating correctness checks:

```sh
python3 tools/check.py --performance-only --baseline <commit>
```

The runner uses five alternating baseline/candidate repetitions, fixed browser
fixtures, temporary baseline source, and disposable server saves. It measures real
Canvas rendering in Chromium, movement response, command acknowledgements,
snapshot-to-draw latency, worker readiness, and concurrent HTTP requests during
ticks and saves. The large client fixture has four towns, 200 idle workers,
50,000 resources, animals, and 128 cached chunks; it exercises a mass replan but
does not represent every possible late-game profession mix.

Results go into ignored `performance-results/` directories as Markdown, JSON, and
individual logs. A repeatable slowdown must exceed both 20% and its absolute
allowance: 2 ms for frame work, 5 ms for server work, or 20 ms for interaction and
snapshot latency. At least 80% of paired runs must agree. Candidate frame-work p95
also has an 8 ms absolute budget. Disagreeing/noisy samples or unavailable browser
support produce **inconclusive**, never a pass. Exit codes are 0 for pass, 1 for
test failures/regressions, and 2 for inconclusive measurements.

Use `--headed` for a visible browser, `--trace` to retain separate Chrome diagnostic
traces, `--cycles 30` for more server samples, or `--performance-only` when correctness
was already checked. Performance scenarios run sequentially. Callback intervals
are diagnostic scheduling measurements; headless results are not presented-frame
or dropped-frame measurements. For actual display smoothness, inspect the frame
events in a headed Chrome trace. Screenshots/profiling are excluded from scored
timings. The runner never overwrites or automatically promotes a baseline.

Profile a synthetic four-player starting world with ticks, saves, cached map
polls, and movement using `python3 server/profile_server.py`. Add `--profile` to show
the most expensive functions or `--ticks 50` for a longer run. It uses temporary
saves and reports median/p95 timings; it does not load or modify your game.
This measures server work, excluding network latency and browser rendering.
Use `--developed` for four towns with 50 workers each, or `--json` for structured
timings. This standalone profiler also uses temporary saves.

```sh
PYTHONPATH=server python3 -m unittest discover -s tests -v
```

With Node.js available, check worker movement and animation transitions with
`node tests/test_animations.js`. Check hover previews and click-to-build placement with
`node tests/test_building_selection.js`. Check contextual panels, object selection,
keyboard controls, and visitor restrictions with `node tests/test_selection_panels.js`.
Check upgraded building artwork and selection with `node tests/test_building_visual_upgrades.js`.

## Hall levels 5–10

- **5 — Leather and trade:** build a tannery. Butchering adult sheep, pigs, or cows
  also yields hides; tanning turns them into leather. Market stalls let town owners
  offer goods or coins to another town. Open your market to create, accept, or cancel
  offers. Both towns need completed markets and enabled storage for incoming goods.
  Offers use available stock when accepted, so spending it can make an offer unavailable.
  Assign a settler to the market to make them a merchant, then open **Automatic
  sales**. Enable a product with its checkbox, then set its **Sale cap**. Both
  changes save automatically. At or above the cap, the merchant walks to another
  building holding that product, collects a batch of up to 10 goods, and returns
  to sell it at the usual price. Existing configured batch sizes are preserved.
  Sales repeat while stock reaches the cap. Disabling a product keeps its cap for
  later. Rules are saved for the town and shared by its markets. Markets have no
  storage; any goods saved there move to the town hall without changing town totals.
  Stock and the sale trigger are checked at pickup. Collected goods leave storage
  and available town stock immediately, and the merchant carries them until the
  sale finishes at the market. Cargo survives saving and loading. Disabling the
  product before sale or cancelling the trip returns unsold goods to their source
  (or the town hall if the source is unavailable).
- **6 — Mining:** produce 5 leather to upgrade. Mines extract iron ore from a vein
  within 10 tile steps. Only some rock outcrops contain iron, marked by a few small mineral dots;
  their terrain and positions stay intact. Ore slowly regenerates.
- **7 — Iron:** mine 10 ore to upgrade. Smelters turn ore and wood into iron.
- **8 — Horses:** smelt 10 iron to upgrade, then build a stable and saddler.
  Wild horses are rare, wander near their grazing spot on open meadow or grass,
  and pause when you approach. Find one, walk beside it, select it, and choose
  **Lead horse to stable**. Walk beside your own stable and choose **Settle the horse
  you are leading**. Assign a worker with 2 husbandry practice and supply fodder or
  vegetables; horses need five minutes of feeding to settle or grow. Two adults can
  breed using 3 fodder and 1 herb. The saddler makes saddles from leather and iron.
  At the stable, saddle an adult to ride; riding adds 75% land speed and stacks with
  roads. Select **You** to mount or dismount. Return the horse at a stable to use it
  for breeding; your saddle is reusable. Wild horses, captured horses, riding status,
  growth, and trade offers survive saves.
- **9 — Steel:** saddle a settled horse to upgrade. Steelworks turn iron and wood
  into steel. Steel pickaxes improve mine output by 25%.
- **10 — Advanced trade:** produce 15 steel and either complete a town trade or
  sell 10 goods at a market. Trade houses pack planks, leather, and steel into trade
  crates worth 40 coins each. Hall level 10 reaches 75 tile steps of building range.

New professions use the existing delivery, production cap, worker practice, and
storehouse systems. Enable hides, leather, ore, iron, saddles, steel, and trade crates
inside your storehouses as needed. Restart the host and refresh the browser to use
these additions in existing worlds.

## Repository layout

- `server/`: Python host, simulation modules, and standalone server profiler.
- `web/`: browser client, styles, and artwork.
- `tests/`: Python and JavaScript checks.
- `tools/`: correctness and performance runners.
- `docs/`: [design decisions](docs/DESIGN.md), [roadmap](docs/ROADMAP.md),
  [ideas](docs/IDEAS.md), and [improvement references](docs/README.md).

`AGENTS.md` stays at the root to provide repository-wide contributor instructions.
Save files and terrain chunks stay at the root, outside version control.
