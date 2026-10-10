# Lantern Vale

A first playable village prototype for a cozy medieval cooperative game.
One Mac hosts; up to four players can build separate towns in the same saved world.

## Start the host

Requires Python 3.9 or newer. From this folder:

```sh
python3 server.py
```

Open **http://localhost:8000** in a browser. Name your town to begin.
Keep the server running while playing; stop it with Control-C.
Choose another port with `python3 server.py --port 8080` if needed.

## Join from another Mac

Connect both Macs to the same local network. On the host, find its local IP address
in **System Settings → Network → your active connection → Details → TCP/IP**.
Friends open `http://HOST-IP:8000` in their browsers, replacing `HOST-IP` with that
address. Allow incoming Python connections if macOS asks. Guest Wi-Fi networks
may prevent devices from reaching one another.

This prototype is intended for a trusted LAN. It has no public account system or
encrypted transport; do not expose it through router port forwarding.

## Play

- Build on open grass. Place woodcutters within three horizontal/vertical tile steps
  of trees and farms within three steps of water; workers need these nearby sources.
- Trees and water are renewable. Rock outcrops occupy tiles but cannot be mined yet.
- Give villagers jobs using the selectors on the right. Their skills increase as they work.
- Workers appear at their resource spots. If a workplace is out of reach, production
  and skill growth pause; move it closer to the source to resume.
- Build cottages to attract residents; maintain food and add storage as needed.
- Click **Move a building**, its grid tile, then an empty tile. Moving costs one coin,
  or is free if you have none, so rearranging is always possible.
- Harvest 30 wood to unlock a milestone; save 40 wood and choose the hall upgrade.
- Click another town's name to visit. Only its owner can issue commands for it.

The host saves all towns automatically in `savegame.json`, which is excluded from
Git. Your browser stores your town's ownership token. Use the same browser and
host address to return to your town; clearing site storage loses that association.
There is no recovery screen yet. Back up the save file while the server is stopped.
Towns keep producing while owners are away if the host is running. No production
occurs while the host is stopped.
Existing saves gain resource spots on unoccupied tiles when the host restarts;
existing buildings stay in place. Move older workplaces near trees or water as needed.

Explore mode, combat, magic, trading, and cooperative quests are not implemented yet.
See [DESIGN.md](DESIGN.md) for agreed decisions, provisional values, and questions
you can answer later.

## Check the simulation

```sh
python3 -m unittest discover -s tests -v
```
