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

Build within 10 horizontal/vertical tile steps of your hall, expanding to 15 after
its first upgrade. Construction also stays within 15 steps of your original town
site, so moving a hall cannot claim another town's land. Halls must stay at least
40 tiles apart when moved. Only a town's owner can build or manage its workers.
The building area is highlighted when building or inspecting a hall.

**Valley** centers the camera on a town without changing maps. **Find me** centers
on your character. Existing saved worlds keep their separate town maps. Restart
the Python host, refresh both browsers, and create a new world to use the shared map.

## Play

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
  Open **Settings** to rebind movement and camera lock keys or restore defaults.
  Preferences are saved in this browser. Scroll to pan; Option/Alt-scroll to zoom.
  Click **Find me** to return to your town, center your character, and start walking.
  Buildings, trees, water, and rocks block movement. Your position is saved per
  world. Movement pauses while typing, placing buildings, or using world menus.
- The town hall allows new buildings and moves within 10 horizontal/vertical tile
  steps (a diagonal counts as two). Completing its upgrade expands the range to 15.
  Click the hall to see its range; the map highlights it while inspecting the hall
  or opening Build. Existing buildings outside the range keep working. Moving the
  hall within its current range shifts the building area to its new location.
- Open **Build**, choose a building, hover over open grass to highlight its tile, then left-click
  to place it and spend wood. Keep clicking open tiles to place more of the selected building,
  or **Cancel selection** to clear it. Place woodcutters within three horizontal/vertical tile steps
  of trees and farms within three steps of water; workers need these nearby sources.
- Trees and water are renewable. Rock outcrops occupy tiles but cannot be mined yet.
- Select a farm and choose **Build acre · 5 wood**, then click open ground within
  three tile steps of that farm. Farmers tend its acres and deliver food back to
  the farm; water must still be within three steps of the farm. Farms without
  nearby acres continue their existing production. Acres are saved with their farm
  association; moving the farm out of reach pauses work on those fields.
- Click a building, moving villager, or resource to open its information panel.
  Click the town hall for milestone progress and its upgrade. Close panels with
  **Close panel** or Escape. On small screens, panels appear as a bottom drawer.
- Open **Villagers** for the full roster, or select one villager to assign a job.
  Their skills increase as they work. **Help** contains the basic instructions.
- Use arrow keys and Enter on the focused map to inspect tiles or choose build locations.
- Workers walk around obstacles to their resource spots, chop wood or tend crops,
  and carry supplies back to their workplace. These animations illustrate work;
  production still follows the host's three-second rhythm. If a workplace is out
  of reach, production and skill growth pause; move it closer to the source to resume.
- Build a fishing hut within three tile steps of water and assign Fishing to
  produce food and learn fishing skill. Farms now appear as buildings.
- Woodcutters produce half as much wood as before. Assigned workers consume
  0.30 food each tick, compared with 0.12 for resting villagers.
- The first hall upgrade costs 40 wood and takes 30 seconds in the open world; the hall
  panel shows remaining time. Closing the host pauses the upgrade.
- Build cottages to attract residents; maintain food and add storage as needed.
- Click a building, then **Destroy building** to remove it without a resource
  refund. The town hall cannot be destroyed. Removing workplaces pauses affected
  workers until another suitable workplace is available; existing residents stay.
- Click **Move** in a building’s information panel, then an empty tile.
  You can also use **Move a building** in the Build panel. Moving costs one coin,
  or is free if you have none, so rearranging is always possible.
- Upgrade the town hall through level 10. Each upgrade requires a cumulative
  wood-harvesting milestone, consumes saved wood, and awards coins on completion.
  Build storehouses to hold the larger costs. Upgrade timers run only in the open
  world while the host is running.

  | Hall level | Total wood harvested | Wood cost | Time | Coin reward |
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
