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

Worlds created after this update use one shared 120×120 map. Everyone sees the
same terrain, buildings, workers, and player characters, and can walk between
towns. Town halls start at randomized locations at least 40 tiles apart, with
nearby wood, water, and clear paths connecting the towns.

Build within 10 horizontal/vertical tile steps of your hall, adding 5 steps with
each completed upgrade (10, 15, 20, 25, 30 at levels 1–5). Construction also stays
within the greater of 15 steps or the hall's range of your original town site,
so moving a hall cannot extend that territory. Halls must stay at least
40 tiles apart when moved. Only a town's owner can build or manage its workers.
The building area is highlighted when building or inspecting a hall.

**Valley** centers the camera on a town without changing maps. Existing saved
worlds keep their separate town maps. Restart
the Python host, refresh both browsers, and create a new world to use the shared map.

## Play

- Window shortcuts: **1** Build, **2** Villagers, **3** Valley, **4** Help,
  **5** Worlds, and **⌘,** Settings. Press a panel's number again to close it.
  **6–9** are reserved for future windows. Gameplay bindings can use letters or **0**.
- Each new town has an irregular outline and random forests, ponds, meadows,
  sandy ground, and rocky areas. Ponds have shallow shores and deeper centers.
  The starting clearing has nearby wood and water. Terrain and resources are saved,
  so returning to a town keeps the same landscape. Open ground can be built on;
  trees, water, and rocks still reserve their tiles.
- Swamp regions in new towns have muddy ground, green pools, reeds, lily pads,
  and willow trees. Swamp trees supply wood and swamp water supports farming.
  Open swamp ground can be walked on and built on. Existing saved terrain stays as it is.
- Friendly locals live in each biome, with matching outfits and dialogue. Look for
  Moss the swamp herbalist and Reed the marsh fisher. Click a local to read their
  introduction. Locals are separate from town workers; their positions are saved.
- Your gold-clad villager is marked **You**. Click the map and hold **WASD** to
  walk in screen directions. Press **Y** to toggle camera lock: when locked, the
  camera follows your movement; when unlocked, walking leaves the camera in place.
  Press **⌘,** to open **Settings** and rebind movement and camera lock keys or restore defaults.
  Preferences are saved in this browser. Scroll to pan; pinch on the touchpad or
  Option/Alt-scroll to zoom.
  Buildings, trees, water, and rocks block movement. Your position is saved per
  world. Movement pauses while typing, placing buildings, or using world menus.
- The town hall allows new buildings and moves within 10 horizontal/vertical tile
  steps (a diagonal counts as two). Each completed upgrade adds 5 steps to the range.
  Click the hall to see its range; the map highlights it while inspecting the hall
  or opening Build. Existing buildings outside the range keep working. Moving the
  hall within its current range shifts the building area to its new location.
- Open **Build**, choose a building, hover over open grass to highlight its tile, then left-click
  to place it and spend wood. Keep clicking open tiles to place more of the selected building,
  or **Cancel selection** to clear it. Place woodcutters within three horizontal/vertical tile steps
  of trees and farms within three steps of water; workers need these nearby sources.
- New buildings and acres start as construction sites. Idle villagers automatically
  help build; assign **Builder** in the villager panel to give a resident priority.
  Builders walk to reachable sites, then work through foundations, framing, and
  finishing over 30 seconds. Housing, storage, and workplaces activate only on
  completion. Progress survives saving, moving a site, and restarting the host.
- Trees and water regenerate finite shared supplies. Rock outcrops cannot be mined yet.
- Select a farm and choose **Build acre · 5 wood**, then click open ground within
  three tile steps of that farm. Farmers tend its acres and deliver food back to
  the farm; water must still be within three steps of the farm. Farms without
  nearby acres continue their existing production. Acres are saved with their farm
  association; moving the farm out of reach pauses work on those fields.
- Click a building, moving villager, or resource to open its information panel.
  Click the town hall for milestone progress and its upgrade. Close panels with
  **Close panel** or Escape. On small screens, panels appear as a bottom drawer.
- Every finished building has an **Add settler** control, an assigned roster, and
  removal controls. Woodcutters, fishing huts, halls, storehouses, and acres start
  with 1 space, cottages with 2, and farms with 4. Upgrade settler capacity twice
  for 20 then 40 wood to reach twice then three times the initial spaces. Capacity
  upgrades take 30 then 60 seconds of builder work and do not increase housing,
  storage, or the town population limit.
  Assignment transfers an existing resident; it does not create a new settler.
  Workplaces set the appropriate job; acres assign farmers to that specific field.
  Settlers assigned to halls, cottages, or storehouses rest and help construction
  automatically. Moving preserves assignments; destroying a building releases them.
- Settlers have permanent natural strengths and weaknesses: +25% or −25%
  output and practice gained in one profession family. Every 20 practice in
  their current job adds another base output multiplier. Their panel shows talents
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
- Open **Villagers** for the full roster, or select one villager to assign a job.
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
  of reach, production and skill growth pause; move it closer to the source to resume.
- Farmers plow with a hoe, sow seeds, water with a can, and harvest with a sickle,
  then carry grain back to the farm. Each action lasts two seconds; full food
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
- Build cottages to attract residents; maintain food and add storage as needed.
- Select any completed cottage, storehouse, woodcutter, farm, fishing hut, or acre
  to upgrade it through level 3. Builders work for 30 seconds to reach level 2
  and 60 seconds to reach level 3 after arriving. Each upgrade costs its original
  building price multiplied by its current level; the panel previews the benefit.
  Profession upgrades take 30 seconds of builder work. Materials are paid when
  starting; buildings pause their benefits and production until work finishes.
  Upgrade progress survives saving, moving, and restarting the host.
  Cottages provide 2/4/6 housing spaces (the town limit stays 16), and storehouses
  add 100/200/300 capacity for both wood and food. Woodcutters, farms, fishing huts,
  and acres gain 25% production per worker at level 2 and 50% at level 3.
  Farm and acre bonuses add together on that field. Fishing bonuses increase food
  from successful catches; the 80% catch chance stays the same. Workers still need
  nearby resources. Upgrades survive saving and moving the building.
  Completed building and settler-capacity upgrades also change the artwork:
  level 2 adds timber framing and roof trim; level 3 adds stonework and a dormer.
  Acres gain raised borders and irrigation, halls develop through five visual
  levels, and profession upgrades add visible tools, nets, boats, or framing.
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
- Forest trees and connected fishing waters have limited, regenerating supplies
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

The host saves all worlds automatically in `savegame.json`, which is excluded from
Git. Your browser stores your town's ownership token. Use the same browser and
host address to return to your town; clearing site storage loses that association.
There is no recovery screen yet. Back up the save file while the server is stopped.
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
