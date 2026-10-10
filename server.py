"""Lantern Vale: a small, authoritative LAN village prototype."""
import argparse
import ipaddress
import json
import math
import secrets
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

from terrain import ensure_terrain

ROOT = Path(__file__).resolve().parent
SAVE = ROOT / 'savegame.json'
LOCK = threading.RLock()
COSTS = {'home': 20, 'woodcutter': 15, 'farm': 15, 'storehouse': 25}
JOBS = {'idle', 'woodcutting', 'farming'}

RESOURCE_REACH = 3


def walkable(town, x, y, blocked=None):
    size = town.get('map_size', 12)
    radius = .18
    if not (radius - .5 <= x <= size - .5 - radius
            and radius - .5 <= y <= size - .5 - radius):
        return False
    if blocked is None:
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']}
    terrain = town.get('terrain')
    for tx in range(math.floor(x + .5) - 1, math.floor(x + .5) + 2):
        for ty in range(math.floor(y + .5) - 1, math.floor(y + .5) + 2):
            outside = (terrain and 0 <= tx < size and 0 <= ty < size
                       and terrain[ty][tx] == 'v')
            if ((tx, ty) in blocked or outside) and (abs(x - tx) < .5 + radius
                                       and abs(y - ty) < .5 + radius):
                return False
    return True


def ensure_player(town):
    player = town.setdefault('player', {'x': 5., 'y': 7., 'facing': 1})
    if not walkable(town, player['x'], player['y']):
        size = town.get('map_size', 12)
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']}
        tiles = ((x, y) for x in range(size) for y in range(size))
        for x, y in sorted(tiles, key=lambda p:
                           abs(p[0] - player['x']) + abs(p[1] - player['y'])):
            if walkable(town, x, y, blocked):
                player.update(x=float(x), y=float(y))
                break
    return player


def move_player(town, data):
    dx, dy = data.get('dx'), data.get('dy')
    if type(dx) is not int or type(dy) is not int or dx not in (-1, 0, 1) or dy not in (-1, 0, 1):
        raise ValueError('Choose a WASD direction.')
    player = ensure_player(town)
    length = math.hypot(dx, dy)
    if length:
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']}
        # Small steps prevent tunnelling through obstacles; slide along walls.
        for _ in range(5):
            x = player['x'] + dx / length * .06
            if walkable(town, x, player['y'], blocked):
                player['x'] = x
            y = player['y'] + dy / length * .06
            if walkable(town, player['x'], y, blocked):
                player['y'] = y
        if dx != dy:
            player['facing'] = 1 if dx > dy else -1
    return {'player': dict(player)}


def map_resources(buildings):
    occupied = {(b['x'], b['y']) for b in buildings}
    locations = {
        'tree': [(0, 2), (0, 3), (1, 4), (2, 0), (3, 0), (3, 1),
                 (0, 7), (1, 8), (2, 9), (3, 10), (7, 0), (8, 1),
                 (8, 2), (10, 6), (11, 7), (10, 9), (9, 10), (11, 11)],
        'water': [(9, 0), (10, 0), (11, 0), (9, 1), (10, 1), (11, 1),
                  (9, 2), (10, 2)],
        'rock': [(10, 3), (11, 4), (0, 10)],
    }
    return [{'kind': kind, 'x': x, 'y': y}
            for kind, tiles in locations.items() for x, y in tiles
            if (x, y) not in occupied]


def ensure_resources(town):
    if 'map_resources' not in town:
        town['map_resources'] = map_resources(town['buildings'])
    ensure_terrain(town)


def work_source(town, job, worker_index):
    building, source = {'woodcutting': ('woodcutter', 'tree'),
                        'farming': ('farm', 'water')}[job]
    workplaces = []
    for b in town['buildings']:
        if b['kind'] != building:
            continue
        nearby = [r for r in town['map_resources'] if r['kind'] == source
                  and abs(r['x'] - b['x']) + abs(r['y'] - b['y']) <= RESOURCE_REACH]
        if nearby:
            workplaces.append((b, min(nearby, key=lambda r:
                                      abs(r['x'] - b['x']) + abs(r['y'] - b['y']))))
    return workplaces[worker_index % len(workplaces)][1] if workplaces else None


def new_town(name):
    town = {'name': name, 'wood': 65, 'food': 40, 'coins': 20, 'level': 1,
            'harvested': 0, 'achievements': [],
            'player': {'x': 5., 'y': 7., 'facing': 1},
            'map_resources': map_resources([{'x': 5, 'y': 5}]),
            'buildings': [{'kind': 'hall', 'x': 5, 'y': 5}],
            'villagers': [{'name': n, 'job': 'idle', 'woodcutting': i % 3,
                           'farming': (i + 1) % 3} for i, n in enumerate(
                               ['Ada', 'Rowan', 'Wren', 'Bram', 'Hazel'])]}
    ensure_terrain(town, new=True)
    return town


def capacity(town):
    return 100 + 100 * sum(b['kind'] == 'storehouse' for b in town['buildings'])


def tick(world):
    for town in world['towns'].values():
        ensure_resources(town)
        for index, villager in enumerate(town['villagers']):
            job = villager['job']
            resource = 'wood' if job == 'woodcutting' else 'food'
            source = work_source(town, job, index) if job != 'idle' else None
            villager['work_target'] = source
            if source and town[resource] < capacity(town):
                amount = min(1 + villager[job] // 20, capacity(town) - town[resource])
                town[resource] += amount
                villager[job] += 1
                if resource == 'wood':
                    town['harvested'] += amount
        # Shortages pause population growth; residents never leave.
        town['food'] = max(0, town['food'] - len(town['villagers']) * 0.12)
        homes = sum(b['kind'] == 'home' for b in town['buildings'])
        if len(town['villagers']) < min(5 + homes * 2, 16) and town['food'] >= 20:
            town['food'] -= 10
            town['villagers'].append({'name': 'Settler ' + str(len(town['villagers']) + 1),
                                     'job': 'idle', 'woodcutting': 0, 'farming': 0})
        if town['harvested'] >= 30 and 'Timber roots' not in town['achievements']:
            town['achievements'].append('Timber roots')


def command(town, data):
    ensure_resources(town)
    action = data.get('action')
    if action == 'walk':
        return move_player(town, data)
    if action in {'build', 'move'}:
        x, y = data.get('x'), data.get('y')
        terrain = town.get('terrain', [])
        if (type(x) is int and type(y) is int and 0 <= y < len(terrain)
                and 0 <= x < len(terrain[y]) and terrain[y][x] == 'v'):
            raise ValueError('Choose a tile inside the valley boundary.')
    if action == 'build':
        kind, x, y = data.get('kind'), data.get('x'), data.get('y')
        if kind not in COSTS or type(x) is not int or type(y) is not int or not (0 <= x < 12 and 0 <= y < 12):
            raise ValueError('Choose a building and a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before building here.')
        if town['wood'] < COSTS[kind]:
            raise ValueError('Gather more wood first.')
        town['wood'] -= COSTS[kind]
        town['buildings'].append({'kind': kind, 'x': x, 'y': y})
    elif action == 'move':
        index, x, y = data.get('index'), data.get('x'), data.get('y')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        if type(x) is not int or type(y) is not int or not (0 <= x < 12 and 0 <= y < 12):
            raise ValueError('Choose a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before moving a building here.')
        # Rearranging must remain possible even after spending the starting coins.
        town['coins'] = max(0, town['coins'] - 1)
        town['buildings'][index].update(x=x, y=y)
    elif action == 'job':
        index, job = data.get('index'), data.get('job')
        if type(index) is not int or not 0 <= index < len(town['villagers']) or job not in JOBS:
            raise ValueError('Choose a villager and a valid job.')
        required = {'woodcutting': 'woodcutter', 'farming': 'farm'}.get(job)
        if required and not any(b['kind'] == required for b in town['buildings']):
            raise ValueError('Build a ' + required + ' first.')
        town['villagers'][index]['job'] = job
    elif action == 'upgrade':
        if town['level'] >= 2:
            raise ValueError('Your hall is fully upgraded for this prototype.')
        if 'Timber roots' not in town['achievements'] or town['wood'] < 40:
            raise ValueError('Harvest 30 wood and save 40 wood to upgrade.')
        town['wood'] -= 40
        town['level'] = 2
        town['coins'] += 30
    else:
        raise ValueError('Unknown action.')
    for index, villager in enumerate(town['villagers']):
        villager['work_target'] = (work_source(town, villager['job'], index)
                                   if villager['job'] != 'idle' else None)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / 'web'), **kwargs)

    def reply(self, status, data):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == '/api/world':
            with LOCK:
                for town in WORLD['towns'].values():
                    ensure_resources(town)
                    ensure_player(town)
                self.reply(200, dict(WORLD, world_id=ACTIVE_WORLD,
                                     name=WORLDS[ACTIVE_WORLD]['name'] if ACTIVE_WORLD else None))
        elif self.path == '/api/worlds':
            with LOCK:
                self.reply(200, {'active_world': ACTIVE_WORLD, 'can_manage': self.is_host(),
                                 'worlds': [{'id': key, 'name': value['name'],
                                             'town_count': len(value['world']['towns'])}
                                            for key, value in WORLDS.items()]})
        else:
            super().do_GET()

    def is_host(self):
        try:
            hostname = urlsplit('http://' + self.headers.get('Host', '')).hostname
            local_host = hostname == 'localhost' or ipaddress.ip_address(hostname).is_loopback
        except ValueError:
            local_host = False
        origin = self.headers.get('Origin')
        same_origin = not origin or origin == 'http://' + self.headers.get('Host', '')
        return ipaddress.ip_address(self.client_address[0]).is_loopback and local_host and same_origin

    def do_POST(self):
        try:
            size = int(self.headers.get('Content-Length', 0))
            if not 0 < size <= 4096:
                raise ValueError('Invalid request size.')
            data = json.loads(self.rfile.read(size))
            if not isinstance(data, dict):
                raise ValueError('Invalid request.')
            with LOCK:
                if self.path in {'/api/worlds/create', '/api/worlds/open', '/api/worlds/delete'}:
                    if not self.is_host():
                        self.reply(403, {'error': 'Only the host at localhost can manage worlds.'})
                        return
                    if self.path == '/api/worlds/create':
                        name = data.get('name')
                        if not isinstance(name, str) or not name.strip() or len(name.strip()) > 24:
                            raise ValueError('Enter a world name of 1–24 characters.')
                        world_id = secrets.token_hex(8)
                        WORLDS[world_id] = {'name': name.strip(), 'world': {'towns': {}}, 'tokens': {}}
                        activate(world_id)
                    else:
                        world_id = data.get('world_id')
                        if not isinstance(world_id, str) or world_id not in WORLDS:
                            raise ValueError('That world no longer exists.')
                        if self.path == '/api/worlds/open':
                            activate(world_id)
                        else:
                            if data.get('confirmation') != WORLDS[world_id]['name']:
                                raise ValueError('Type the world name to confirm deletion.')
                            del WORLDS[world_id]
                            if ACTIVE_WORLD == world_id:
                                activate(None)
                    save()
                    self.reply(200, {'ok': True})
                elif self.path in {'/api/join', '/api/command'}:
                    if not ACTIVE_WORLD or data.get('world_id') != ACTIVE_WORLD:
                        self.reply(409, {'error': 'The host changed worlds. Refresh and try again.'})
                        return
                    if self.path == '/api/command':
                        town_id = TOKENS.get(self.headers.get('Authorization', ''))
                        if town_id not in WORLD['towns']:
                            self.reply(401, {'error': 'Join this world first.'})
                            return
                        if data.get('action') == 'walk':
                            move_key = (ACTIVE_WORLD, town_id)
                            now = time.monotonic()
                            if move_key in MOVE_TIMES and now - MOVE_TIMES[move_key] < .09:
                                self.reply(429, {'error': 'Walking too quickly. Try again.'})
                                return
                            MOVE_TIMES[move_key] = now
                        result = command(WORLD['towns'][town_id], data)
                        save()
                        self.reply(200, dict(result or {}, ok=True))
                        return
                    name = str(data.get('name', '')).strip()[:24]
                    if not name:
                        raise ValueError('Enter your town name.')
                    if len(WORLD['towns']) >= 4:
                        raise ValueError('This world already has four towns.')
                    token = secrets.token_urlsafe(24)
                    town_id = secrets.token_hex(8)
                    WORLD['towns'][town_id] = new_town(name)
                    TOKENS[token] = town_id
                    save()
                    self.reply(200, {'token': token, 'town_id': town_id})
                else:
                    self.reply(404, {'error': 'Unknown endpoint.'})
        except (ValueError, TypeError) as error:
            self.reply(400, {'error': str(error) or 'Invalid request.'})


def activate(world_id):
    global ACTIVE_WORLD, WORLD, TOKENS
    ACTIVE_WORLD = world_id
    entry = WORLDS.get(world_id)
    WORLD, TOKENS = (entry['world'], entry['tokens']) if entry else ({'towns': {}}, {})


def load():
    global WORLDS
    if not SAVE.exists():
        WORLDS = {}
        activate(None)
        return
    stored = json.loads(SAVE.read_text())
    if 'worlds' in stored:
        WORLDS = stored['worlds']
        activate(stored['active_world'] if stored['active_world'] in WORLDS else None)
    else:
        WORLDS = {'legacy': {'name': 'My first world', 'world': stored['world'],
                             'tokens': stored['tokens']}}
        activate('legacy')


def save():
    temporary = SAVE.with_suffix('.json.tmp')
    temporary.write_text(json.dumps({'worlds': WORLDS, 'active_world': ACTIVE_WORLD}))
    temporary.replace(SAVE)


def simulation():
    while True:
        time.sleep(3)
        with LOCK:
            tick(WORLD)
            save()


WORLD, TOKENS = {'towns': {}}, {}
MOVE_TIMES = {}
WORLDS, ACTIVE_WORLD = {}, None
if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=8000)
    args = parser.parse_args()
    load()
    server = ThreadingHTTPServer(('0.0.0.0', args.port), Handler)
    threading.Thread(target=simulation, daemon=True).start()
    print('Lantern Vale running at http://localhost:' + str(args.port), flush=True)
    print('Friends can join using http://YOUR-LAN-IP:' + str(args.port), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        with LOCK:
            save()
        server.server_close()
