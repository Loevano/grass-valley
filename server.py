"""Lantern Vale: a small, authoritative LAN village prototype."""
import argparse
from collections import deque
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
from npcs import ensure_npcs
from shared_world import bind_map, export_world, new_world, validate_placement, walking_map

ROOT = Path(__file__).resolve().parent
SAVE = ROOT / 'savegame.json'
LOCK = threading.RLock()
COSTS = {'home': 20, 'woodcutter': 15, 'farm': 15, 'storehouse': 25, 'fishing_hut': 15, 'acre': 5}
JOBS = {'idle', 'building', 'woodcutting', 'farming', 'fishing'}
SETTLER_SPACES = {'hall': 1, 'home': 2, 'storehouse': 1, 'woodcutter': 1,
                  'fishing_hut': 1, 'farm': 4, 'acre': 1}
BUILDING_JOBS = {'woodcutter': 'woodcutting', 'farm': 'farming',
                 'fishing_hut': 'fishing', 'acre': 'farming'}
TICK_SECONDS = 3
BUILD_SECONDS = 30
# Requirements are cumulative harvested wood; costs are paid when upgrading.
HALL_UPGRADES = [
    {'level': level, 'harvested': 15 * level * (level - 1),
     'wood': 40 * (level - 1), 'seconds': 30 * (level - 1),
     'coins': 30 * (level - 1)}
    for level in range(2, 11)
]

WORKER_FOOD = .18

MAP_SIZE = 120
RESOURCE_REACH = 3


def hall_upgrade(town):
    return next((upgrade for upgrade in HALL_UPGRADES
                 if upgrade['level'] == town['level'] + 1), None)


def walkable(town, x, y, blocked=None):
    size = town.get('map_size', 12)
    radius = .18
    if not (radius - .5 <= x <= size - .5 - radius
            and radius - .5 <= y <= size - .5 - radius):
        return False
    if blocked is None:
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] != 'tree'}
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
                   town['buildings'] + town['map_resources']
                   if b['kind'] != 'tree'}
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
                   town['buildings'] + town['map_resources']
                   if b['kind'] != 'tree'}
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
            for bx in range(0, MAP_SIZE, 12) for by in range(0, MAP_SIZE, 12)
            for kind, tiles in locations.items() for tx, ty in tiles
            for x, y in [(bx + tx, by + ty)]
            if (x, y) not in occupied]


def ensure_resources(town):
    if 'town_origin' in town:
        return
    if 'map_resources' not in town:
        town['map_resources'] = map_resources(town['buildings'])
    if town.get('map_size') != MAP_SIZE:
        occupied = {(b['x'], b['y']) for b in town['buildings']}
        occupied.update((r['x'], r['y']) for r in town['map_resources'])
        town['map_resources'].extend(r for r in map_resources(town['buildings'])
                                     if (r['x'] >= 12 or r['y'] >= 12)
                                     and (r['x'], r['y']) not in occupied)
        town['map_size'] = MAP_SIZE
    ensure_terrain(town)
    ensure_npcs(town)


def settler_capacity(building):
    return SETTLER_SPACES[building['kind']] * building.get('settler_level', 1)


def settler_building(town, worker_index):
    villager = town['villagers'][worker_index]
    if 'building_index' in villager:
        return villager['building_index']
    # Preserve workplace selection in older saves until explicitly reassigned.
    kind = {'woodcutting': 'woodcutter', 'farming': 'farm',
            'fishing': 'fishing_hut'}.get(villager['job'])
    source = 'tree' if kind == 'woodcutter' else 'water'
    workplaces = [i for i, b in enumerate(town['buildings'])
                  if b['kind'] == kind and completed(b)
                  and any(r['kind'] == source and
                          abs(r['x'] - b['x']) + abs(r['y'] - b['y']) <= RESOURCE_REACH
                          for r in town['map_resources'])]
    return workplaces[worker_index % len(workplaces)] if workplaces else None


def assigned_settlers(town, building_index):
    return [i for i in range(len(town['villagers']))
            if settler_building(town, i) == building_index]


def assign_settler(town, worker_index, building_index):
    if type(worker_index) is not int or not 0 <= worker_index < len(town['villagers']):
        raise ValueError('Choose a settler first.')
    if type(building_index) is not int or not 0 <= building_index < len(town['buildings']):
        raise ValueError('Choose a building first.')
    building = town['buildings'][building_index]
    if not completed(building):
        raise ValueError('Finish construction before assigning settlers.')
    others = [i for i in assigned_settlers(town, building_index) if i != worker_index]
    if len(others) >= settler_capacity(building):
        raise ValueError('This building has no free settler spaces. Upgrade its capacity first.')
    villager = town['villagers'][worker_index]
    villager['building_index'] = building_index
    villager['job'] = BUILDING_JOBS.get(building['kind'], 'idle')


def work_assignment(town, job, worker_index):
    if job == 'building':
        return None
    building, source = {'woodcutting': ('woodcutter', 'tree'),
                        'farming': ('farm', 'water'),
                        'fishing': ('fishing_hut', 'water')}[job]
    explicit = town['villagers'][worker_index].get('building_index')
    assigned_acre = None
    if explicit is not None:
        assigned = town['buildings'][explicit]
        if assigned['kind'] == 'acre' and job == 'farming':
            assigned_acre = assigned
            explicit = assigned.get('farm_index')
            if explicit is None or not completed(assigned):
                return None
        elif assigned['kind'] != building:
            return None
    workplaces = []
    for farm_index, b in enumerate(town['buildings']):
        if b['kind'] != building or not completed(b) or (explicit is not None and farm_index != explicit):
            continue
        nearby = [r for r in town['map_resources'] if r['kind'] == source
                  and abs(r['x'] - b['x']) + abs(r['y'] - b['y']) <= RESOURCE_REACH]
        if nearby:
            if job == 'farming':
                acres = [a for a in town['buildings'] if a['kind'] == 'acre' and completed(a)
                         and a.get('farm_index') == farm_index
                         and abs(a['x'] - b['x']) + abs(a['y'] - b['y']) <= RESOURCE_REACH]
                if assigned_acre is not None:
                    if assigned_acre not in acres:
                        return None
                    acres = [assigned_acre]
                if acres:
                    nearby = [acres[worker_index % len(acres)]]
            workplaces.append((b, min(nearby, key=lambda r:
                                      abs(r['x'] - b['x']) + abs(r['y'] - b['y']))))
    return workplaces[worker_index % len(workplaces)] if workplaces else None


def work_source(town, job, worker_index):
    assignment = work_assignment(town, job, worker_index)
    return assignment[1] if assignment else None


def building_level(building):
    return building.get('level', 1)


def production_bonus(building):
    return .25 * (building_level(building) - 1)



def new_town(name, world=None):
    town = {'map_size': MAP_SIZE, 'name': name, 'wood': 65, 'food': 40, 'coins': 20, 'level': 1,
            'harvested': 0, 'achievements': [],
            'player': {'x': 5., 'y': 7., 'facing': 1},
            'map_resources': map_resources([{'x': 5, 'y': 5}]),
            'buildings': [{'kind': 'hall', 'x': 5, 'y': 5}],
            'villagers': [{'name': n, 'job': 'idle', 'woodcutting': i % 3,
                           'farming': (i + 1) % 3, 'fishing': 0} for i, n in enumerate(
                               ['Ada', 'Rowan', 'Wren', 'Bram', 'Hazel'])]}
    if world and 'shared_map' in world:
        site = dict(world['town_sites'][len(world['towns'])])
        town.update(world['shared_map'])
        town['town_origin'] = site
        town['buildings'] = [{'kind': 'hall', **site}]
        town['player'] = {'x': float(site['x']), 'y': float(site['y'] + 2), 'facing': 1}
    else:
        ensure_terrain(town, new=True)
        ensure_npcs(town)
    return town


def capacity(town):
    return 100 + 100 * sum(building_level(b) for b in town['buildings']
                           if b['kind'] == 'storehouse' and completed(b))


def completed(building):
    # Buildings from older saves are already finished.
    return 'construction' not in building


def construction_route(town, start, target):
    size = town.get('map_size', MAP_SIZE)
    blocked = {(b['x'], b['y']) for b in town['buildings'] + town['map_resources']}
    blocked.update((n['x'], n['y']) for n in town.get('npcs', []))
    terrain = town.get('terrain', [])

    def open_tile(p):
        x, y = p
        return (0 <= x < size and 0 <= y < size and p not in blocked
                and (not terrain or terrain[y][x] != 'v'))

    if not open_tile(start):
        start = min(((x, y) for x in range(size) for y in range(size)
                     if open_tile((x, y))), default=None,
                    key=lambda p: abs(p[0] - start[0]) + abs(p[1] - start[1]))
    if start is None:
        return None
    queue, previous = deque([start]), {start: None}
    while queue:
        p = queue.popleft()
        if abs(p[0] - target['x']) + abs(p[1] - target['y']) == 1:
            path = [p]
            while previous[p] is not None:
                p = previous[p]
                path.append(p)
            return list(reversed(path))
        for next_tile in [(p[0]+1, p[1]), (p[0], p[1]+1),
                          (p[0]-1, p[1]), (p[0], p[1]-1)]:
            if next_tile not in previous and open_tile(next_tile):
                previous[next_tile] = p
                queue.append(next_tile)
    return None


def construct(town, landscape, workers=None, elapsed=TICK_SECONDS):
    if workers is None:
        workers = {i for i, v in enumerate(town['villagers'])
                   if v['job'] in {'idle', 'building'}}
    claimed = set()
    hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
    for index, villager in enumerate(town['villagers']):
        villager.pop('construction_target', None)
        villager.pop('construction_arrived', None)
    # Dedicated builders claim sites before residents who are helping while idle.
    for index in sorted(workers, key=lambda i: town['villagers'][i]['job'] != 'building'):
        villager = town['villagers'][index]
        position = villager.get('construction_position',
                                {'x': hall['x'] - 2 + index % 5,
                                 'y': hall['y'] + 3 + index // 5})
        start = (position['x'], position['y'])
        for site_index, site in enumerate(town['buildings']):
            if completed(site) or site_index in claimed:
                continue
            path = construction_route(landscape, start, site)
            if path is None:
                continue
            claimed.add(site_index)
            # A stable coordinate target avoids stale indices after destruction.
            villager['construction_target'] = {key: site[key] for key in ('kind', 'x', 'y')}
            step = path[min(len(path) - 1, int(elapsed * 1.8))]
            villager['construction_position'] = {'x': step[0], 'y': step[1]}
            if len(path) > 1:
                break
            villager['construction_arrived'] = True
            construction = site['construction']
            construction['progress'] = min(construction['total'],
                                           construction['progress'] + elapsed)
            if site['kind'] == 'hall':
                town['upgrade_remaining'] = construction['total'] - construction['progress']
            if construction['progress'] == construction['total']:
                del site['construction']
                if site['kind'] == 'hall':
                    upgrade = hall_upgrade(town)
                    if upgrade:
                        town['level'] = upgrade['level']
                        town['coins'] += upgrade['coins']
                villager.pop('construction_target', None)
                villager.pop('construction_arrived', None)
            break


def tick(world):
    for town in world['towns'].values():
        ensure_resources(town)
        if town.get('upgrade_remaining'):
            # Resume upgrades from saves made before construction required builders.
            hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
            upgrade = hall_upgrade(town)
            if upgrade and completed(hall):
                hall['construction'] = {'total': upgrade['seconds'],
                                        'progress': upgrade['seconds'] - town['upgrade_remaining']}
        construct(town, walking_map(world, town) if 'shared_map' in world else town)
        for index, villager in enumerate(town['villagers']):
            job = villager['job']
            resource = 'wood' if job == 'woodcutting' else 'food'
            assignment = work_assignment(town, job, index) if job != 'idle' else None
            source = assignment[1] if assignment else None
            villager['work_target'] = source
            if source and town[resource] < capacity(town):
                caught = True
                if job == 'fishing':
                    caught = secrets.randbelow(5) != 0
                    villager['fishing_catch'] = {
                        'attempt': villager.get('fishing_catch', {}).get('attempt', 0) + 1,
                        'caught': caught,
                    }
                rate = .5 if job == 'woodcutting' else 1
                bonus = production_bonus(assignment[0])
                if source['kind'] == 'acre':
                    bonus += production_bonus(source)
                rate *= 1 + bonus
                amount = min(rate * (1 + villager.get(job, 0) // 20),
                             capacity(town) - town[resource])
                if caught:
                    town[resource] += amount
                villager[job] = villager.get(job, 0) + 1
                if resource == 'wood':
                    town['harvested'] += amount
        # Shortages pause population growth; residents never leave.
        town['food'] = max(0, town['food'] - len(town['villagers']) * 0.12
                           - WORKER_FOOD * sum(v['job'] != 'idle' or bool(v.get('construction_target'))
                                               for v in town['villagers']))
        homes = sum(building_level(b) for b in town['buildings']
                    if b['kind'] == 'home' and completed(b))
        if len(town['villagers']) < min(5 + homes * 2, 16) and town['food'] >= 20:
            town['food'] -= 10
            town['villagers'].append({'name': 'Settler ' + str(len(town['villagers']) + 1),
                                     'job': 'idle', 'woodcutting': 0, 'farming': 0, 'fishing': 0})
        if town['harvested'] >= 30 and 'Timber roots' not in town['achievements']:
            town['achievements'].append('Timber roots')


def validate_acre(town, farm_index, x, y):
    if (type(farm_index) is not int or not 0 <= farm_index < len(town['buildings'])
            or town['buildings'][farm_index]['kind'] != 'farm'
            or not completed(town['buildings'][farm_index])):
        raise ValueError('Select a farm before building an acre.')
    farm = town['buildings'][farm_index]
    if abs(x - farm['x']) + abs(y - farm['y']) > RESOURCE_REACH:
        raise ValueError('Place acres within 3 tile steps of their farm.')


def building_range(town):
    return 15 if town['level'] >= 2 else 10


def validate_building_range(town, x, y):
    hall = next((b for b in town['buildings'] if b['kind'] == 'hall'), None)
    reach = building_range(town)
    if hall is None or abs(x - hall['x']) + abs(y - hall['y']) > reach:
        raise ValueError(f'Build within {reach} tile steps of your town hall.')


def command(town, data, world=None):
    ensure_resources(town)
    action = data.get('action')
    if world and 'shared_map' in world:
        if action == 'walk':
            return move_player(walking_map(world, town), data)
        if action in {'build', 'move'}:
            validate_placement(world, town, data)
    if action == 'walk':
        return move_player(town, data)
    if action in {'build', 'move'}:
        x, y = data.get('x'), data.get('y')
        if any(n['x'] == x and n['y'] == y for n in town['npcs']):
            raise ValueError('A local is standing here. Choose another tile.')
        terrain = town.get('terrain', [])
        if (type(x) is int and type(y) is int and 0 <= y < len(terrain)
                and 0 <= x < len(terrain[y]) and terrain[y][x] == 'v'):
            raise ValueError('Choose a tile inside the valley boundary.')
    if action == 'build':
        kind, x, y = data.get('kind'), data.get('x'), data.get('y')
        if kind not in COSTS or type(x) is not int or type(y) is not int or not (0 <= x < MAP_SIZE and 0 <= y < MAP_SIZE):
            raise ValueError('Choose a building and a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before building here.')
        validate_building_range(town, x, y)
        if town['wood'] < COSTS[kind]:
            raise ValueError('Gather more wood first.')
        if kind == 'acre':
            validate_acre(town, data.get('farm_index'), x, y)
        town['wood'] -= COSTS[kind]
        building = {'kind': kind, 'x': x, 'y': y,
                    'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        if kind == 'acre':
            building['farm_index'] = data['farm_index']
        town['buildings'].append(building)
    elif action == 'move':
        index, x, y = data.get('index'), data.get('x'), data.get('y')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        if type(x) is not int or type(y) is not int or not (0 <= x < MAP_SIZE and 0 <= y < MAP_SIZE):
            raise ValueError('Choose a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before moving a building here.')
        validate_building_range(town, x, y)
        if town['buildings'][index]['kind'] == 'acre':
            validate_acre(town, town['buildings'][index].get('farm_index'), x, y)
        # Rearranging must remain possible even after spending the starting coins.
        town['coins'] = max(0, town['coins'] - 1)
        town['buildings'][index].update(x=x, y=y)
    elif action == 'destroy':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        if town['buildings'][index]['kind'] == 'hall':
            raise ValueError('The town hall cannot be destroyed.')
        town['buildings'].pop(index)
        for villager in town['villagers']:
            assigned = villager.get('building_index')
            if type(assigned) is int and assigned >= index:
                if assigned == index:
                    villager.pop('building_index')
                    villager['job'] = 'idle'
                else:
                    villager['building_index'] = assigned - 1
        for building in town['buildings']:
            farm_index = building.get('farm_index')
            if type(farm_index) is int and farm_index >= index:
                building['farm_index'] = None if farm_index == index else farm_index - 1
    elif action == 'assign_settler':
        assign_settler(town, data.get('index'), data.get('building_index'))
    elif action == 'remove_settler':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['villagers']):
            raise ValueError('Choose a settler first.')
        town['villagers'][index].pop('building_index', None)
        town['villagers'][index]['job'] = 'idle'
    elif action == 'upgrade_capacity':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        level = building.get('settler_level', 1)
        if not completed(building) or level >= 3:
            raise ValueError('Capacity cannot be upgraded at this time.')
        cost = 20 * level
        if town['wood'] < cost:
            raise ValueError('Gather more wood first.')
        town['wood'] -= cost
        building['settler_level'] = level + 1
    elif action == 'job':
        index, job = data.get('index'), data.get('job')
        if type(index) is not int or not 0 <= index < len(town['villagers']) or job not in JOBS:
            raise ValueError('Choose a villager and a valid job.')
        required = {'woodcutting': 'woodcutter', 'farming': 'farm', 'fishing': 'fishing_hut'}.get(job)
        if required and not any(b['kind'] == required and completed(b) for b in town['buildings']):
            raise ValueError('Build a ' + required + ' first.')
        if required:
            current = town['villagers'][index].get('building_index')
            choices = [i for i, b in enumerate(town['buildings'])
                       if b['kind'] == required and completed(b)
                       and len([v for v in assigned_settlers(town, i) if v != index]) < settler_capacity(b)]
            if not choices:
                raise ValueError('No free settler spaces. Upgrade a building or remove a settler first.')
            assign_settler(town, index, current if current in choices else choices[0])
        else:
            town['villagers'][index].pop('building_index', None)
        town['villagers'][index]['job'] = job
    elif action == 'upgrade_building':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        if building['kind'] not in COSTS:
            raise ValueError('Use the town hall upgrade instead.')
        if not completed(building):
            raise ValueError('Finish construction before upgrading.')
        level = building_level(building)
        if level >= 3:
            raise ValueError('This building is fully upgraded.')
        cost = COSTS[building['kind']] * level
        if town['wood'] < cost:
            raise ValueError(f'Gather {cost} wood to upgrade this building.')
        town['wood'] -= cost
        building['level'] = level + 1
    elif action == 'upgrade':
        if town.get('upgrade_remaining'):
            raise ValueError('Your hall upgrade is already underway.')
        upgrade = hall_upgrade(town)
        if not upgrade:
            raise ValueError('Your hall has reached the maximum level of 10.')
        if town['harvested'] < upgrade['harvested'] or town['wood'] < upgrade['wood']:
            raise ValueError(f"Harvest {upgrade['harvested']} wood in total and save "
                             f"{upgrade['wood']} wood to upgrade.")
        town['wood'] -= upgrade['wood']
        town['upgrade_remaining'] = upgrade['seconds']
        hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
        hall['construction'] = {'progress': 0, 'total': upgrade['seconds']}
    else:
        raise ValueError('Unknown action.')
    for index, villager in enumerate(town['villagers']):
        villager['work_target'] = (work_source(town, villager['job'], index)
                                   if villager['job'] != 'idle' else None)
    construct(town, walking_map(world, town) if world and 'shared_map' in world else town,
              elapsed=0)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / 'web'), **kwargs)

    def log_request(self, code='-', size='-'):
        # Polling and movement generate frequent successful requests.
        if code != '-' and int(code) >= 400:
            super().log_request(code, size)

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
                    ensure_player(walking_map(WORLD, town) if 'shared_map' in WORLD else town)
                self.reply(200, dict(export_world(WORLD), hall_upgrades=HALL_UPGRADES, world_id=ACTIVE_WORLD,
                                     name=WORLDS[ACTIVE_WORLD]['name'] if ACTIVE_WORLD else None,
                                     players_sampled_at=time.monotonic()))
        elif self.path == '/api/players':
            with LOCK:
                snapshot = {'world_id': ACTIVE_WORLD, 'sampled_at': time.monotonic(),
                            'players': {key: dict(town['player'])
                                        for key, town in WORLD['towns'].items()
                                        if 'player' in town}}
            self.reply(200, snapshot)
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
                        WORLDS[world_id] = {'name': name.strip(), 'world': new_world(), 'tokens': {}}
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
                        result = command(WORLD['towns'][town_id], data, WORLD)
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
                    WORLD['towns'][town_id] = new_town(name, WORLD)
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
    bind_map(WORLD)


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
    temporary.write_text(json.dumps({'worlds': {key: dict(entry, world=export_world(entry['world']))
                                                for key, entry in WORLDS.items()},
                                     'active_world': ACTIVE_WORLD}))
    temporary.replace(SAVE)


def simulation():
    while True:
        time.sleep(TICK_SECONDS)
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
