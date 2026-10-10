"""Lantern Vale: a small, authoritative LAN village prototype."""
import argparse
import json
import secrets
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

ROOT = Path(__file__).resolve().parent
SAVE = ROOT / 'savegame.json'
LOCK = threading.RLock()
COSTS = {'home': 20, 'woodcutter': 15, 'farm': 15, 'storehouse': 25}
JOBS = {'idle', 'woodcutting', 'farming'}

RESOURCE_REACH = 3


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
    return {'name': name, 'wood': 65, 'food': 40, 'coins': 20, 'level': 1,
            'harvested': 0, 'achievements': [],
            'map_resources': map_resources([{'x': 5, 'y': 5}]),
            'buildings': [{'kind': 'hall', 'x': 5, 'y': 5}],
            'villagers': [{'name': n, 'job': 'idle', 'woodcutting': i % 3,
                           'farming': (i + 1) % 3} for i, n in enumerate(
                               ['Ada', 'Rowan', 'Wren', 'Bram', 'Hazel'])]}


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
    if action == 'build':
        kind, x, y = data.get('kind'), data.get('x'), data.get('y')
        if kind not in COSTS or type(x) is not int or type(y) is not int or not (0 <= x < 12 and 0 <= y < 12):
            raise ValueError('Choose a building and a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
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
                self.reply(200, WORLD)
        else:
            super().do_GET()

    def do_POST(self):
        try:
            size = int(self.headers.get('Content-Length', 0))
            if not 0 < size <= 4096:
                raise ValueError('Invalid request size.')
            data = json.loads(self.rfile.read(size))
            if not isinstance(data, dict):
                raise ValueError('Invalid request.')
            with LOCK:
                if self.path == '/api/join':
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
                elif self.path == '/api/command':
                    town_id = TOKENS.get(self.headers.get('Authorization', ''))
                    if town_id not in WORLD['towns']:
                        self.reply(401, {'error': 'Join this world first.'})
                        return
                    command(WORLD['towns'][town_id], data)
                    save()
                    self.reply(200, {'ok': True})
                else:
                    self.reply(404, {'error': 'Unknown endpoint.'})
        except (ValueError, TypeError) as error:
            self.reply(400, {'error': str(error) or 'Invalid request.'})


def save():
    temporary = SAVE.with_suffix('.json.tmp')
    temporary.write_text(json.dumps({'world': WORLD, 'tokens': TOKENS}))
    temporary.replace(SAVE)


def simulation():
    while True:
        time.sleep(3)
        with LOCK:
            tick(WORLD)
            save()


WORLD, TOKENS = {'towns': {}}, {}
if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=8000)
    args = parser.parse_args()
    if SAVE.exists():
        stored = json.loads(SAVE.read_text())
        WORLD, TOKENS = stored['world'], stored['tokens']
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
