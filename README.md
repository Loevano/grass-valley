# Lantern Vale

A first playable village prototype for a cozy medieval cooperative game.
One Mac hosts; up to four players can build separate towns in the same saved world.

## Start the host

Requires Python 3.9 or newer. From this folder:

```sh
python3 server.py
```

Open **http://localhost:8000** in a browser. Use the Worlds menu to start a named
world or open a saved one, then name your town.
Keep the server running while playing; stop it with Control-C.
Choose another port with `python3 server.py --port 8080` if needed.
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

## Join from another Mac

Connect both Macs to the same local network. On the host, find its local IP address
in **System Settings → Network → your active connection → Details → TCP/IP**.
Friends open `http://HOST-IP:8000` in their browsers, replacing `HOST-IP` with that
address. Allow incoming Python connections if macOS asks. Guest Wi-Fi networks
may prevent devices from reaching one another.

This prototype is intended for a trusted LAN. It has no public account system or
encrypted transport; do not expose it through router port forwarding.

## Shared map in new worlds

New worlds use one shared 1,920×1,920 map, 256 times the area of the original
120×120 valley. Terrain loads as you explore. Everyone sees the
same terrain, buildings, workers, and player characters, and can walk between
towns. Town halls start at randomized locations at least 40 tiles apart, with
nearby wood, water, and clear paths connecting the towns.

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

**Valley** centers the camera on a town without changing maps. Existing saved
worlds keep their current maps and dimensions. Restart
the Python host, refresh both browsers, and create a new world to use the shared map.

## Play

- Window shortcuts follow the menu order: **1** Worlds, **2** Build, **3** Villagers,
  **4** Resources, **5** Valley, **6** Help, and **⌘,** Settings. Press a panel's number again to close it.
  **7–9** are reserved for future windows. Gameplay bindings can use letters or **0**.
- Each new town has an irregular outline and random forests, ponds, meadows,
  sandy ground, and rocky areas. Ponds have shallow shores and deeper centers.
  The starting clearing has nearby wood and water. Terrain and resources are saved,
  so returning to a town keeps the same landscape. Open ground can be built on;
  trees, water, and rocks still reserve their tiles.
- Swamp regions in new towns have muddy ground, green pools, reeds, lily pads,
  and willow trees. Swamp trees supply wood and swamp water supports farming.
  Lighter swamp ground is walkable; only the darkest muddy tiles require equipped swamp boots. Existing saved terrain stays as it is.
- Friendly locals live in each biome, with matching outfits and dialogue. Look for
  Moss the swamp herbalist and Reed the marsh fisher. Click a local to read their
  introduction. Sylva gives each town a fine axe and Reed a fine fishing rod once.
  Walk beside them to accept, then give the tool to a worker from that worker’s
  panel. Each tool adds 25% output in its matching job; take it back to reassign it.
  Tobin and Clover each exchange 10 wood for 8 food once every five minutes of
  running game time, provided you have food storage room. Gifts and restocking
  are saved per town, so each player can collect their own rewards.
  Locals are separate from town workers; their positions are saved.
- Your gold-clad villager is marked **You**. Click the map and hold **WASD** to
  walk in screen directions. Press **Y** to toggle camera lock: when locked, the
  camera follows your movement; when unlocked, walking leaves the camera in place.
  Press **⌘,** to open **Settings** and rebind movement and camera lock keys or restore defaults.
  Preferences are saved in this browser. Scroll to pan; pinch on the touchpad or
  Option/Alt-scroll to zoom.
  Buildings, trees, water, and rocks block movement. Your position is saved per
  world. Movement pauses while typing, placing buildings, or using world menus.
- Select **You** to inspect your equipment and equip or remove items. Swamp boots
  (15 coins) unlock the darkest swamp tiles. Sand is walkable without equipment;
  desert clothing (20 coins) can still be worn. The planned desert biome will have
  windy patches deeper inside it that require goggles. Boats allow sailing on water.
  Walk beside any local and press **I** (or click them) to read dialogue and buy or sell gear.
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
  Everyone can use roads in the shared valley. Click a road to inspect it; its
  owner can remove it for free. Roads are saved with the town.
- New buildings and acres start as construction sites. Idle villagers automatically
  help build; assign **Builder** in the villager panel to give a resident priority.
  Builders walk to reachable sites, then work through foundations, framing, and
  finishing over 30 seconds. Housing, storage, and workplaces activate only on
  completion. Progress survives saving, moving a site, and restarting the host.
- Trees are felled when their remaining wood reaches zero, freeing their tile for
  building and leaving a visible stump. They regrow after five minutes on all ground while
  the tile is unoccupied. Buildings and roads pause regrowth until removed.
  Swamp willows and rare ancient trees take five times as much chopping and
  fifteen minutes to regrow.
  Water regenerates its shared supply. Rock outcrops cannot be mined yet.
- New shared worlds have a large lake with three treasure islands. At hall level 3,
  select **You** and build an **Exploration boat** for 20 wood and 10 planks.
  Walk onto water to sail automatically and step onto land to disembark. Stand
  beside an island chest and press **G** (or choose **Collect loot**) for 25 coins.
  Each chest can be claimed once across the whole world; boat ownership and
  collected loot are saved. Existing landscapes keep their layout.
- Select a farm and choose **Build acre · 5 wood**, then click open ground within
  three tile steps of that farm. Farmers tend its acres and deliver food back to
  the farm. Farmers fetch reachable water when watering; the farm can be farther
  than three steps from water. Farms without
  nearby acres continue their existing production. Acres are saved with their farm
  association; moving the farm out of reach pauses work on those fields.
- Click a building, moving villager, or resource to open its information panel.
  Click the town hall for milestone progress and its upgrade. Close panels with
  Escape. On small screens, panels appear as a bottom drawer.
- Every finished building except the town hall has an **Add settler** control,
  an assigned roster, and removal controls. Woodcutters, fishing huts, storehouses,
  and acres start with 1 space, cottages with 2, and farms with 4. Upgrade settler capacity twice
  for 20 then 40 wood to reach twice then three times the initial spaces. Capacity
  upgrades take 30 then 60 seconds of builder work and do not increase housing,
  storage, or the town population limit.
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
  10 practice and 25 minutes for each output bonus. Travel and blocked work
  pause training. Their panel shows talents
  and current output; workplace selectors show qualifications and output too.
  Basic woodcutting, farming, fishing, and husbandry train anyone. Advanced roles
  require that settler's own practice: sawpit workers need 10 woodcutting,
  millers 10 farming, preservers 10 fishing, crafters 10 woodworking,
  fish breeders 20 fishing, orchard workers 20 farming, bakers 10 milling,
  and butchers 10 husbandry. Another settler or the player's skill cannot qualify them.
  Existing settlers gain saved talents when loading; unqualified advanced workers
  pause until trained in the prerequisite job.
- Hall level 4 unlocks bakeries (30 wood + 5 planks) and butcher shops
  (25 wood + 5 planks), each with two initial worker spaces. Bakeries use
  2 flour + 0.5 wood to make 2 bread; butcher shops use 2 meat from barns
  to make 2 prepared meat. Both foods supply 1.5 pantry food per unit and can
  be sold at a market. Processing skill increases batch size and ingredient use
  together. Farmers' skill improves field yields; animals still limit barn output,
  and only one keeper tends a barn each cycle.
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
- Use arrow keys and Enter on the focused map to inspect tiles or choose build locations.
- Workers walk around obstacles to their resource spots, chop wood or tend crops,
  and carry supplies back to their workplace. These animations illustrate work;
  production still follows the host's three-second rhythm. If a workplace is out
  of reach of its wood or fishing source, production and skill growth pause;
  move it closer to the source to resume.
- Farmers plow with a hoe, sow seeds, water with a can, and harvest with a sickle,
  then carry grain back to the farm. At the watering stage, they walk to a
  reachable water source, fill their can, and return to the field. Travel pauses
  the action timer. Each action lasts two seconds; full food
  storage pauses the animation until there is room again.
- Build a fishing hut within three tile steps of water and assign Fishing to
  produce food and learn fishing skill. Fishers cast into the water, wait by a
  rippling float, reel in a fish, and carry their catch back to the hut.
  Assigned fishers use a wooden pier extending from their hut to nearby water,
  so a hut on the only accessible bank does not block their fishing spot.
  Casts reach up to three more tiles into connected water when space allows.
  Fishing attempts have an 80% chance of catching food; an unlucky attempt
  yields no food, but still builds skill. Empty casts reel in and try again.
- Woodcutters produce half as much wood as before. Assigned workers consume
  0.30 food each tick, compared with 0.12 for resting villagers.
- Hall upgrades also require a builder at the hall. The first requires housing and reliable food, costs 40 wood, and
  takes 30 seconds of construction after arrival; the hall
  panel shows remaining time. Closing the host pauses the upgrade.
- Completed workplaces hold storage: **50 units** in ordinary workplaces,
  200 in the hall, and 100 per storehouse level. Its **Storage** section shows
  goods held there. Wood and other products become available for construction,
  meals, and sales immediately when produced at their workplace. Hauling relocates
  those goods without increasing the town total again. Processors and barns also
  reserve up to 50 input units locally; reserved ingredients are excluded from
  available stock. Old shared stock is preserved at the hall when loading, and
  old workplace outputs become available once. Demolition preserves available
  goods at the hall.
- Allocate incoming storage spaces only for goods relevant to each building.
  Workplaces accept their production inputs and outputs; farms and acres also
  accept manure. Homes do not store goods. Halls, storehouses, and markets accept
  all goods. Food spaces cover only the edible goods that building accepts, using
  their pantry value (a fishing hut accepts fish, for example). Food is consumed
  from available town stock. The hall starts
  with 100 wood and 100 food spaces. Each storehouse level adds 100 shared spaces;
  new and upgraded spaces remain unallocated until you assign them. Allocations
  cannot exceed a room's capacity or remove space occupied by its delivered goods.
  Only storehouses and warehouses can change allocations. Other buildings have
  fixed spaces for their relevant goods within a 50-space budget (the hall has
  100 wood and 100 food spaces; homes have none). A mill has 30 wheat and 20 flour
  spaces, for example. Each building holds its own stock. Processors fetch supplies from buildings that actually hold them.
  Older saved stocks remain intact, including
  goods above their fixed capacities; spend or haul them away to resume deliveries.
- In a storehouse or warehouse, select **Allocate** to adjust the wood slider's
  minimum and maximum. The maximum assigns wood storage space; **Enable reserve**
  keeps the minimum in that room, so producers and haulers take only surplus wood.
  Builders can always spend reserved wood on construction and upgrades. Disable
  the reserve to release it to production. Inputs already delivered to a workplace
  finish processing normally.
- Assign settlers to a **Storehouse** as haulers. They collect small batches and
  distribute recipe ingredients and animal feed so producers can keep working.
  Without hauling support, producers fetch their own inputs and deliver outputs
  once their 50-unit buffer fills. Transport takes time and follows open paths;
  roads speed it up. Full, unallocated, or unreachable storage leaves outputs at
  their workplace and pauses production when the buffer fills. Moving, upgrading,
  or reassigning during transport preserves goods and cancels affected deliveries.
- Storehouses upgrade through level 5, becoming a **Warehouse** with 500 spaces.
  Building upgrades increase storage; settler-capacity upgrades add hauler slots.
- New landscapes contain denser forests, including timber patches beside towns.
- Build cottages to attract residents; maintain food and add storage as needed.
- Select any completed cottage, storehouse, woodcutter, farm, fishing hut, or acre
  to upgrade it through level 3 (storehouses through level 5). Builders work for 30 seconds to reach level 2
  and 60 seconds to reach level 3 after arriving. Each upgrade costs its original
  building price multiplied by its current level; the panel previews the benefit.
  Profession upgrades take 30 seconds of builder work. Materials are paid when
  starting; buildings pause their benefits and production until work finishes.
  Upgrade progress survives saving, moving, and restarting the host.
  Cottages provide 2/4/6 housing spaces (the town limit stays 16), and storehouses
  add 100/200/300/400/500 shared storage spaces. Level 5 becomes a Warehouse. Woodcutters, farms, fishing huts,
  and acres gain 25% production per worker at level 2 and 50% at level 3.
  Farm and acre bonuses add together on that field. Fishing bonuses increase food
  from successful catches; the 80% catch chance stays the same. Workers still need
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
  barns/chickens, and improved axes; level 3 adds boats, sheep, carrots/fodder, and
  sawpits/planks; level 4 adds mills, smokehouses, pigs, and workshops/tools;
  level 5 adds fish breeding ponds, orchards, cows, improved saws, timber framing,
  and market stalls. Each upgrade has a development milestone and material cost
  shown in the hall panel. Levels 6–10 remain documented plans.
- Select an acre to choose its crop, a barn to buy/breed animals, or a workplace
  to unlock profession improvements. Assign settlers to new production buildings.
  Sawpits use wood; mills use wheat; smokehouses use fish and wood; workshops use
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
Your browser stores your town's ownership token. Use the same browser and
host address to return to your town; clearing site storage loses that association.
There is no recovery screen yet. Back up both the save file and chunk directory
while the server is stopped.
Towns in the open world keep producing while owners are away if the host is running.
No production occurs while the host is stopped or for worlds that are not open.
Existing saves keep their resource layout and gain varied ground. Saves without
resources gain resource spots on unoccupied tiles when the host restarts;
existing buildings stay in place. Move older workplaces near trees or water as needed.

Combat, magic, trading, and cooperative quests are not implemented yet.
See [ROADMAP.md](ROADMAP.md) for town hall rewards and proposed future unlocks.
See [DESIGN.md](DESIGN.md) for agreed decisions, provisional values, and questions
you can answer later.

## Check the simulation

```sh
python3 -m unittest discover -s tests -v
```

With Node.js available, check worker movement and animation transitions with
`node tests/test_animations.js`. Check hover previews and click-to-build placement with
`node tests/test_building_selection.js`. Check contextual panels, object selection,
keyboard controls, and visitor restrictions with `node tests/test_selection_panels.js`.
Check upgraded building artwork and selection with `node tests/test_building_visual_upgrades.js`.
