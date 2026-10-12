"""Lantern Vale: a small, authoritative LAN village prototype."""
import argparse
import ipaddress
import json
import math
import secrets
import shutil
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlsplit

import progression as economy
import logistics
import work_cycles
import equipment
import npcs
import trading
import horses
import quests
import player_login
from navigation import routes
import lanterns
import village_life
from spatial import SpatialIndex

from terrain import ensure_terrain, remove_invalid_trees, ensure_industry
from npcs import ensure_npcs
from shared_world import bind_map, building_range, export_world, new_world, validate_placement, walking_map
from chunk_world import CHUNK_SIZE, ChunkTerrain, attach as attach_chunks, chunk_key, parse_keys, parse_versions

ROOT = Path(__file__).resolve().parent
SAVE = ROOT / 'savegame.json'
LOCK = threading.RLock()
COSTS = {'home': 20, 'woodcutter': 15, 'farm': 15, 'storehouse': 25, 'fishing_hut': 15, 'acre': 5}
JOBS = {'idle', 'building', 'woodcutting', 'farming', 'fishing', 'hauling'}
SETTLER_SPACES = {'hall': 1, 'home': 2, 'storehouse': 1, 'woodcutter': 1,
                  'fishing_hut': 1, 'farm': 4, 'acre': 1}
BUILDING_JOBS = {'storehouse': 'hauling', 'woodcutter': 'woodcutting', 'farm': 'farming',
                 'fishing_hut': 'fishing', 'acre': 'farming'}
TICK_SECONDS = 3
BUILD_SECONDS = 30
ROAD_BUILD_SECONDS = 10
# New towns develop professions instead of repeating wood-only upgrades.
HALL_UPGRADES = economy.HALL_UPGRADES
COSTS.update({k: v['wood'] for k, v in economy.BUILDINGS.items()})
JOBS.update(economy.JOB_BUILDINGS)
SETTLER_SPACES.update({k: v['slots'] for k, v in economy.BUILDINGS.items()})
BUILDING_JOBS.update({k: v['job'] for k, v in economy.BUILDINGS.items() if 'job' in v})

WORKER_FOOD = .18
SHORTAGE_GRACE_SECONDS = 60
DEPARTURE_CHANCE = 5  # Percent per tick after the grace period, before housing.
VILLAGER_NAMES = (
    'Ada', 'Rowan', 'Wren', 'Bram', 'Hazel', 'Finn', 'Mira', 'Jasper',
    'Elara', 'Theo', 'Ivy', 'Silas', 'Nora', 'Felix', 'Willow', 'Owen',
    'Cora', 'Alden', 'Flora', 'Hugo', 'Lila', 'Emmett', 'Pearl', 'Arthur',
    'Clara', 'Tobin', 'Rose', 'Cedric', 'Iris', 'Elias', 'Mae', 'Oscar',
)


def next_villager_name(town):
    # Reuse available names, adding a suffix when a large town fills the pool.
    used = {v.get('name') for v in town['villagers']}
    used.add(town.get('last_departure', {}).get('name'))
    generation = 1
    while True:
        for base in VILLAGER_NAMES:
            name = base if generation == 1 else f'{base} {generation}'
            if name not in used:
                return name
        generation += 1


def ensure_villager_names(town):
    for villager in town['villagers']:
        name = villager.get('name', '')
        if not name.strip() or (name.startswith('Settler ') and name[8:].isdigit()):
            villager['name'] = next_villager_name(town)


def housing_capacity(town):
    return 5 + 2 * sum(building_level(b) + ('timber_frame' in b.get('profession_upgrades', []))
                       for b in town['buildings'] if b['kind'] == 'home' and completed(b))


def housing_loyalty(town):
    # Housing is shared, so cottage comfort protects workers in every profession.
    population = len(town['villagers'])
    comfort = []
    for building in town['buildings']:
        if building['kind'] != 'home' or not completed(building):
            continue
        level = building_level(building) + ('timber_frame' in building.get('profession_upgrades', []))
        comfort.extend([min(.75, .25 * (level - 1))] * (2 * level))
    return sum(sorted(comfort, reverse=True)[:population]) / population if population else 0


def feed_settlers(town):
    needed = len(town['villagers']) * .12 + WORKER_FOOD * sum(
        v['job'] != 'idle' or bool(v.get('construction_target')) for v in town['villagers'])
    shortage = town['food'] + 1e-9 < needed
    economy.eat(town, needed)
    town['food_shortage_seconds'] = (town.get('food_shortage_seconds', 0) + TICK_SECONDS
                                     if shortage else 0)
    town['housing_shortage_seconds'] = (town.get('housing_shortage_seconds', 0) + TICK_SECONDS
                                       if len(town['villagers']) > housing_capacity(town) else 0)
    town['arrival_delay_seconds'] = max(0, town.get('arrival_delay_seconds', 0) - TICK_SECONDS)
    reasons = [kind for kind in ('food', 'housing')
               if town[f'{kind}_shortage_seconds'] >= SHORTAGE_GRACE_SECONDS]
    if not reasons or len(town['villagers']) <= 1:
        return
    loyalty = 0 if 'housing' in reasons else housing_loyalty(town)
    chance = round(DEPARTURE_CHANCE * (1 - loyalty) * 100)
    if secrets.randbelow(10000) >= chance:
        return
    # Any resident can leave, including an experienced specialist.
    villager = town['villagers'].pop(secrets.randbelow(len(town['villagers'])))
    logistics.cancel_output(town, villager)
    if villager.get('worker_tool') in npcs.WORKER_TOOLS:
        tools = town['player'].setdefault('worker_tools', {})
        key = villager['worker_tool']
        tools[key] = tools.get(key, 0) + 1
    town['departure_count'] = town.get('departure_count', 0) + 1
    town['last_departure'] = {
        'name': villager['name'], 'job': villager['job'],
        'reason': ' and '.join(reasons),
        'practice': {job: villager.get(job, 0) for job in JOBS
                     if job not in {'idle', 'building'} and villager.get(job, 0)},
    }
    town['food_shortage_seconds'] = 0
    town['housing_shortage_seconds'] = 0
    town['arrival_delay_seconds'] = SHORTAGE_GRACE_SECONDS

MAP_SIZE = 120
RESOURCE_REACH = 3
WOODCUTTER_REACH = 10


def hall_upgrade(town):
    upgrade = next((u for u in HALL_UPGRADES if u['level'] == town['level'] + 1), None)
    if upgrade is None and town.get('upgrade_remaining') and town['level'] < 10:
        level = town['level'] + 1
        return {'level': level, 'seconds': 30 * (level - 1), 'coins': 30 * (level - 1)}
    return upgrade


def walkable(town, x, y, blocked=None, water=None):
    size = town.get('map_size', 12)
    radius = .18
    if not (radius - .5 <= x <= size - .5 - radius
            and radius - .5 <= y <= size - .5 - radius):
        return False
    if blocked is None:
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and town.get('player', {}).get('boat'))}
    terrain = town.get('terrain')
    if water is None:
        water = {(r['x'], r['y']) for r in town['map_resources'] if r['kind'] == 'water'}
    for tx in range(math.floor(x + .5) - 1, math.floor(x + .5) + 2):
        for ty in range(math.floor(y + .5) - 1, math.floor(y + .5) + 2):
            outside = (terrain and 0 <= tx < size and 0 <= ty < size
                       and terrain[ty][tx] == 'v')
            hazardous = (terrain and 0 <= ty < len(terrain) and 0 <= tx < len(terrain[ty])
                         and (tx, ty) not in water
                         and not equipment.protected(town.get('player', {}), terrain[ty][tx], tx, ty))
            if ((tx, ty) in blocked or outside or hazardous) and (abs(x - tx) < .5 + radius
                                       and abs(y - ty) < .5 + radius):
                return False
    return True


def ensure_player(town):
    player = town.setdefault('player', {'x': 5., 'y': 7., 'facing': 1})
    if not walkable(town, player['x'], player['y']):
        size = town.get('map_size', 12)
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and player.get('boat'))}
        terrain = town.get('terrain')
        tiles = terrain.tiles() if hasattr(terrain, 'tiles') else ((x, y) for x in range(size) for y in range(size))
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
    # Normalize screen distance for the 2:1 isometric projection.
    length = math.hypot(dx - dy, (dx + dy) * .5) / math.sqrt(2)
    if length:
        roads = {(r['x'], r['y']) for r in town.get('roads', []) if completed(r)}
        player.pop('activity', None)
        player.pop('activity_status', None)
        blocked = {(b['x'], b['y']) for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and player.get('boat'))}
        # Small steps prevent tunnelling through obstacles; slide along walls.
        water = {(r['x'], r['y']) for r in town['map_resources'] if r['kind'] == 'water'}
        for _ in range(5):
            tile = (math.floor(player['x'] + .5), math.floor(player['y'] + .5))
            speed = (1 + .25 * max(0, economy.player_boat_level(player) - 1)) if tile in water else 1.5 if tile in roads else 1
            if tile not in water and player.get('mounted'):
                speed *= 1.75
            x = player['x'] + dx / length * .06 * speed
            if walkable(town, x, player['y'], blocked, water):
                player['x'] = x
            y = player['y'] + dy / length * .06 * speed
            if walkable(town, player['x'], y, blocked, water):
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
    # Older saves could assign residents to the founder's town hall.
    for villager in town['villagers']:
        index = villager.get('building_index')
        if (type(index) is int and 0 <= index < len(town['buildings'])
                and town['buildings'][index]['kind'] == 'hall'):
            villager.pop('building_index')
            villager['job'] = 'idle'
            villager.pop('work_target', None)
            villager.pop('logistics_trip', None)
            villager.pop('logistics_target', None)
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
    if building['kind'] == 'hall':
        return 0
    return SETTLER_SPACES[building['kind']] * building.get('settler_level', 1)


def settler_building(town, worker_index):
    villager = town['villagers'][worker_index]
    if 'building_index' in villager:
        return villager['building_index']
    # Preserve workplace selection in older saves until explicitly reassigned.
    kind = {'woodcutting': 'woodcutter', 'farming': 'farm',
            'fishing': 'fishing_hut', **economy.JOB_BUILDINGS}.get(villager['job'])
    source = 'tree' if kind == 'woodcutter' else 'water'
    workplaces = [i for i, b in enumerate(town['buildings'])
                  if b['kind'] == kind and (completed(b) or 'upgrade' in b['construction'])
                  and (kind == 'farm' or any(r['kind'] == source and
                          abs(r['x'] - b['x']) + abs(r['y'] - b['y']) <=
                          resource_reach(b)
                          for r in town['map_resources']))]
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
    if building['kind'] == 'hall':
        raise ValueError('Only your controllable character can settle in the town hall.')
    if not completed(building):
        raise ValueError('Finish construction before assigning settlers.')
    others = [i for i in assigned_settlers(town, building_index) if i != worker_index]
    if len(others) >= settler_capacity(building):
        raise ValueError('This building has no free settler spaces. Upgrade its capacity first.')
    villager = town['villagers'][worker_index]
    reason = economy.qualification(villager, BUILDING_JOBS.get(building['kind'], 'idle'))
    if reason:
        raise ValueError(reason)
    logistics.cancel_output(town, villager)
    villager['building_index'] = building_index
    villager['job'] = BUILDING_JOBS.get(building['kind'], 'idle')
    villager.pop('logistics_trip', None)
    villager.pop('logistics_target', None)


def fill_cottages(town):
    waiting = iter(i for i, villager in enumerate(town['villagers'])
                   if villager['job'] == 'idle' and 'building_index' not in villager)
    for building_index, building in enumerate(town['buildings']):
        if building['kind'] != 'home' or not completed(building):
            continue
        spaces = settler_capacity(building) - len(assigned_settlers(town, building_index))
        for _ in range(spaces):
            worker_index = next(waiting, None)
            if worker_index is None:
                return
            assign_settler(town, worker_index, building_index)


def work_assignment(town, job, worker_index, spatial=None):
    if job == 'hauling':
        index = town['villagers'][worker_index].get('building_index')
        if type(index) is int and 0 <= index < len(town['buildings']):
            building = town['buildings'][index]
            if building['kind'] == 'storehouse' and completed(building):
                return building, building
        return None
    if economy.qualification(town['villagers'][worker_index], job):
        return None
    if job in economy.JOB_BUILDINGS:
        index = town['villagers'][worker_index].get('building_index')
        if type(index) is int and 0 <= index < len(town['buildings']):
            b = town['buildings'][index]
            if b['kind'] == economy.JOB_BUILDINGS[job] and completed(b):
                assigned = assigned_settlers(town, index)
                if worker_index in assigned and assigned.index(worker_index) < settler_capacity(b):
                    return b, b
        return None
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
        boating = job == 'fishing' and work_cycles.fishing_boating(town, b, worker_index)
        reach = 2 + b.get('level', 1) if job == 'fishing' and not boating else resource_reach(b)
        nearby = (spatial.nearby(source, b['x'], b['y'], reach) if spatial is not None else
                  [r for r in town['map_resources'] if r['kind'] == source
                   and abs(r['x'] - b['x']) + abs(r['y'] - b['y']) <= reach])
        if boating:
            depths = (spatial or SpatialIndex(town['map_resources'])).water_depths()
            occupied = {(item['x'], item['y']) for item in town['buildings'] + town.get('npcs', [])}
            nearby = work_cycles.fishing_water(b, nearby, occupied, reach, depths=depths)
        if nearby or job == 'farming':
            if job == 'farming':
                acres = [a for a in town['buildings'] if a['kind'] == 'acre' and completed(a)
                         and a.get('farm_index') == farm_index
                         and abs(a['x'] - b['x']) + abs(a['y'] - b['y']) <= RESOURCE_REACH]
                if assigned_acre is not None:
                    if assigned_acre not in acres:
                        return None
                    acres = [assigned_acre]
                nearby = [acres[worker_index % len(acres)]] if acres else [b]
            workplaces.append((b, min(nearby, key=lambda r:
                                      (r.get('supply', 1) <= 0, -depths.get((r['x'], r['y']), 1) if boating else 0,
                                       (-1 if boating else 1) * (abs(r['x'] - b['x']) + abs(r['y'] - b['y']))))))
    if not workplaces:
        return None
    assignment = workplaces[worker_index % len(workplaces)]
    owner_index = next(i for i, b in enumerate(town['buildings']) if b is assignment[0])
    slot_index = town['villagers'][worker_index].get('building_index') if assigned_acre is not None else owner_index
    assigned = assigned_settlers(town, slot_index)
    if worker_index in assigned and assigned.index(worker_index) >= settler_capacity(assigned_acre or assignment[0]):
        return None
    return assignment


def work_source(town, job, worker_index):
    assignment = work_assignment(town, job, worker_index)
    return assignment[1] if assignment else None


def building_level(building):
    return building.get('level', 1)


def resource_reach(building):
    if building['kind'] == 'mine':
        return 10
    if building['kind'] == 'woodcutter':
        return WOODCUTTER_REACH
    if building['kind'] == 'fishing_hut':
        boat_level = economy.fishing_boat_level(building)
        return RESOURCE_REACH + building_level(building) - 1 + (3 + 2 * (boat_level - 1) if boat_level else 0)
    return RESOURCE_REACH


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
                               VILLAGER_NAMES[:5])]}
    if world and 'shared_map' in world:
        site = dict(world['town_sites'][len(world['towns'])])
        town.update(world['shared_map'])
        town['town_origin'] = site
        town['buildings'] = [{'kind': 'hall', **site}]
        town['player'] = {'x': float(site['x']), 'y': float(site['y'] + 2), 'facing': 1}
    else:
        ensure_terrain(town, new=True)
        ensure_npcs(town)
    for index, villager in enumerate(town['villagers']):
        economy.ensure_talents(villager, index)
    return town


def capacity(town):
    return sum(economy.storage_size(b) for b in town['buildings'] if completed(b))


def completed(building):
    # Buildings from older saves are already finished.
    return 'construction' not in building


def construction_route(town, start, target, road_mode='departure'):
    size = town.get('map_size', MAP_SIZE)
    spatial = town.get('_spatial_index')
    if spatial is not None:
        blocked = spatial.blocked(town)
    else:
        blocked = {(b['x'], b['y']) for b in town['buildings'] + town['map_resources']}
        blocked.update((n['x'], n['y']) for n in town.get('npcs', []))
    # Completed fields are walkable, including around farms awaiting upgrades.
    # Subtraction leaves the spatial index's cached obstacles unchanged.
    blocked = blocked.difference((b['x'], b['y']) for b in town['buildings']
                                 if b['kind'] == 'acre' and completed(b))
    terrain = town.get('terrain', [])

    def open_tile(p):
        x, y = p
        return (0 <= x < size and 0 <= y < size and p not in blocked
                and (not terrain or terrain[y][x] != 'v'))

    if not open_tile(start):
        tiles = terrain.tiles() if hasattr(terrain, 'tiles') else ((x, y) for x in range(size) for y in range(size))
        start = min((p for p in tiles if open_tile(p)), default=None,
                    key=lambda p: abs(p[0] - start[0]) + abs(p[1] - start[1]))
    if start is None:
        return None
    roads = {(r['x'], r['y']) for r in town.get('roads', []) if completed(r)}
    path = routes.route(start, target, blocked, size, terrain, roads=roads, road_mode=road_mode)
    return None if path is None else [start] + [(p['x'], p['y']) for p in path]


def logistics_route(town, start, target):
    return construction_route(town, start, target, road_mode='arrival')


def construct(town, landscape, workers=None, elapsed=TICK_SECONDS):
    helpers = {}
    if workers is None:
        workers = {i for i, v in enumerate(town['villagers'])
                   if v['job'] in {'idle', 'building'}}
        for index in range(len(town['villagers'])):
            if index in workers:
                continue
            workplace = settler_building(town, index)
            if type(workplace) is not int or not 0 <= workplace < len(town['buildings']):
                continue
            building = town['buildings'][workplace]
            if building['kind'] == 'acre' and 'upgrade' not in building.get('construction', {}):
                workplace = building.get('farm_index')
                if type(workplace) is not int or not 0 <= workplace < len(town['buildings']):
                    continue
                building = town['buildings'][workplace]
            if 'upgrade' in building.get('construction', {}):
                helpers[index] = workplace
                workers.add(index)
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
        for site_index, site in enumerate(town['buildings'] + town.get('roads', [])):
            if completed(site) or site_index in claimed:
                continue
            if index in helpers and site_index != helpers[index]:
                continue
            path = construction_route(landscape, start, site)
            if path is None:
                continue
            claimed.add(site_index)
            # A stable coordinate target avoids stale indices after destruction.
            villager['construction_target'] = {'kind': site.get('kind', 'road'),
                                               'x': site['x'], 'y': site['y']}
            roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if completed(r)}
            remaining, reached = elapsed, 0
            for tile in path[:-1]:
                duration = 1 / (1.8 * (1.5 if tile in roads else 1))
                if remaining < duration:
                    break
                remaining -= duration
                reached += 1
            step = path[reached]
            villager['construction_position'] = {'x': step[0], 'y': step[1]}
            if len(path) > 1:
                break
            villager['construction_arrived'] = True
            construction = site['construction']
            construction['progress'] = min(construction['total'],
                                           construction['progress'] + elapsed)
            if site.get('kind') == 'hall' and 'upgrade' not in construction:
                upgrade = hall_upgrade(town)
                town['upgrade_remaining'] = construction['total'] - construction['progress']
            if construction['progress'] == construction['total']:
                del site['construction']
                if 'upgrade' in construction:
                    site.update(construction['upgrade'])
                elif site.get('kind') == 'hall':
                    if upgrade:
                        town['level'] = upgrade['level']
                        town['coins'] += upgrade['coins']
                villager.pop('construction_target', None)
                villager.pop('construction_arrived', None)
            break


def prepare_landscapes(world, regenerate=False):
    if world.get('shared_map', {}).get('chunked'):
        attach_chunks(world, SAVE.with_name(SAVE.stem + '_chunks'))
        bind_map(world)
    buildings = [b for t in world['towns'].values() for b in t['buildings']]
    obstacles = buildings + [r for t in world['towns'].values() for r in t.get('roads', [])]
    obstacles += [dict(x=math.floor(t['player']['x'] + .5), y=math.floor(t['player']['y'] + .5))
                  for t in world['towns'].values() if 'player' in t]
    landscapes = [world['shared_map']] if 'shared_map' in world else list(world['towns'].values())
    for landscape in landscapes:
        if 'shared_map' not in world:
            ensure_resources(landscape)
        if not landscape.get('chunked'):
            ensure_industry(landscape, obstacles + landscape.get('npcs', []))
        remove_invalid_trees(landscape)
        npc_map = landscape
        if landscape.get('chunked') and not landscape.get('npc_placement_version'):
            # Saved NPCs may live in sleeping chunks outside the active towns.
            manager = world['_chunks']
            records = dict(manager.records)
            for npc in landscape.get('npcs', []):
                for dx in (-CHUNK_SIZE, 0, CHUNK_SIZE):
                    for dy in (-CHUNK_SIZE, 0, CHUNK_SIZE):
                        x, y = npc['x'] + dx, npc['y'] + dy
                        if 0 <= x < manager.size and 0 <= y < manager.size:
                            key = chunk_key(x, y)
                            records[key] = manager.read(key)
            npc_map = dict(landscape, terrain=ChunkTerrain(records, manager.size),
                           map_resources=[r for record in records.values() for r in record['map_resources']],
                           tree_stumps=[r for record in records.values() for r in record['tree_stumps']])
        moved = ensure_npcs(npc_map, obstacles)
        if npc_map is not landscape:
            for field in ('npcs', 'npc_roster_version', 'npc_placement_version'):
                landscape[field] = npc_map[field]
        if 'shared_map' in world:
            for town in world['towns'].values():
                npcs.move_local_visits(town, moved)
        economy.ensure_forage(landscape, buildings if 'shared_map' in world else landscape['buildings'])
        if regenerate:
            local_obstacles = obstacles if 'shared_map' in world else landscape['buildings'] + landscape.get('roads', [])
            economy.renew(landscape, local_obstacles + landscape.get('npcs', []))
        else:
            for _, _, members in economy.supply_groups(landscape):
                maximum, regeneration = economy.source_limits(landscape, members[0])
                for resource in members:
                    if resource['kind'] == 'tree' and resource.get('supply_max', maximum) != maximum:
                        resource['supply'] = resource.get('supply', maximum) * maximum / resource['supply_max']
                    resource.setdefault('supply', maximum)
                    resource.update(supply_max=maximum, regeneration=regeneration)
    if 'shared_map' in world:
        bind_map(world)


def tick(world):
    prepare_landscapes(world, regenerate=True)
    towns = list(world['towns'].values())
    if 'shared_map' in world:
        horses.wander(world['shared_map'], towns, TICK_SECONDS)
        if world.get('_chunks'):
            world['_chunks'].sync()
    else:
        for town in towns:
            horses.wander(town, [town], TICK_SECONDS)
    spatial = SpatialIndex(world['shared_map']['map_resources']) if 'shared_map' in world else None
    if world.get('shared_map', {}).get('chunked'):
        world['shared_map']['chunk_clock'] += TICK_SECONDS
    for town in world['towns'].values():
        ensure_villager_names(town)
        ensure_resources(town)
        economy.ensure_inventory(town)
        npcs.advance_trades(town, TICK_SECONDS)
        if town.get('upgrade_remaining'):
            # Resume upgrades from saves made before construction required builders.
            hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
            upgrade = hall_upgrade(town)
            if upgrade and completed(hall):
                hall['construction'] = {'total': upgrade['seconds'],
                                        'progress': upgrade['seconds'] - town['upgrade_remaining']}
        town_spatial = spatial if spatial is not None else SpatialIndex(town['map_resources'])
        walking = dict(walking_map(world, town) if 'shared_map' in world else town,
                       _spatial_index=town_spatial)
        construct(town, walking)
        landscape = world.get('shared_map', town)
        economy.player_tick(town, landscape, capacity(town))
        barn_cycles = set()
        butcher_cycles = set()
        field_cycles = {}
        # Haulers supply workplaces before this cycle's production consumes inputs.
        for index in sorted(range(len(town['villagers'])), key=lambda i: town['villagers'][i]['job'] != 'hauling'):
            villager = town['villagers'][index]
            job = villager['job']
            if villager.get('construction_target'):
                villager['work_target'] = None
                villager['production_status'] = 'Helping with construction'
                continue
            assignment = work_assignment(town, job, index, town_spatial) if job != 'idle' else None
            finishing_output = False
            if not assignment and villager.get('pending_output') and villager.get('work_animation', {}).get('task', [None])[0] == job:
                spec = villager['pending_workplace']
                workplace = next((b for b in town['buildings'] if completed(b) and all(b.get(k) == v for k, v in spec.items())), None)
                if workplace:
                    task = villager['work_animation']['task']
                    assignment = workplace, {'kind': task[4], 'x': task[5], 'y': task[6]}
                    finishing_output = True
            source = assignment[1] if assignment else None
            pending_workplace = villager.get('pending_workplace')
            if pending_workplace and (not assignment or any(assignment[0].get(k) != v for k, v in pending_workplace.items())):
                logistics.cancel_output(town, villager)
            villager['work_target'] = source
            villager.pop('production_status', None)
            if not source:
                villager.pop('work_animation', None)
                villager.pop('logistics_trip', None)
                villager.pop('logistics_target', None)
                reason = economy.qualification(villager, job)
                if reason:
                    villager['production_status'] = reason
                elif job == 'woodcutting':
                    camp_index = settler_building(town, index)
                    if camp_index is not None:
                        camp = town['buildings'][camp_index]
                        if camp['kind'] == 'woodcutter' and completed(camp):
                            nearby = lambda r: abs(r['x'] - camp['x']) + abs(r['y'] - camp['y']) <= WOODCUTTER_REACH
                            if not town_spatial.nearby('tree', camp['x'], camp['y'], WOODCUTTER_REACH):
                                regrowing = any(nearby(r) for r in landscape.get('tree_stumps', []))
                                villager['production_status'] = ('Waiting for trees to regrow' if regrowing
                                                                 else 'Waiting for trees within 10 tiles')
                continue
            building = assignment[0]
            if job == 'hauling':
                logistics.haul(town, walking, villager, building, logistics_route, TICK_SECONDS)
                continue
            boating = job == 'fishing' and work_cycles.fishing_boating(town, building, index)
            if boating:
                crew = work_cycles.fishing_crew(town, building)
                captain = town['villagers'][crew[0]]
                voyage = captain.get('work_animation', {})
                task = voyage.get('task', [])
                if len(task) >= 7 and task[:4] == ['fishing', building['kind'], building['x'], building['y']] and (
                        work_cycles.boat_load(town, building) or voyage.get('phase') in ('returning', 'depositing')):
                    previous_source = town_spatial.nearby('water', task[5], task[6], 0)
                    source = previous_source[0] if previous_source else {'kind': 'water', 'x': task[5], 'y': task[6], 'supply': 0}
                    villager['work_target'] = source
                depth = town_spatial.water_depths().get((source['x'], source['y']), 1)
                boat_output = economy.fishing_catch(building, depth)
                villager['boat_return_requested'] = bool(work_cycles.boat_load(town, building) and (
                    villager.get('boat_return_requested') or finishing_output or source.get('supply', 1) <= 0
                    or logistics.buffered(building) >= logistics.BUFFER_SIZE - 1e-9
                    or economy.production_room(town, boat_output) <= 0))
            else:
                villager.pop('boat_return_requested', None)
            if villager.get('logistics_trip'):
                villager.pop('work_animation', None)
            elif work_cycles.advance(town, walking, villager, building, source, index, TICK_SECONDS):
                if boating:
                    for crew_index in work_cycles.fishing_crew(town, building):
                        logistics.drop_off(town, building, town['villagers'][crew_index])
                else:
                    logistics.drop_off(town, building, villager)
            if finishing_output:
                villager['production_status'] = 'Returning gathered goods'
                continue
            if not boating or villager.get('logistics_trip') or not work_cycles.boat_load(town, building):
                if logistics.worker(town, walking, villager, building, logistics_route, TICK_SECONDS):
                    continue
            if boating and villager.get('work_animation', {}).get('phase') != 'working':
                villager['production_status'] = 'Unloading boat' if villager.get('work_animation', {}).get('phase') == 'depositing' else 'Sailing'
                continue
            if job in economy.JOB_BUILDINGS:
                if building['kind'] == 'butcher' and building.get('butchering'):
                    marker = id(building)
                    reason = economy.qualification(villager, job)
                    if reason:
                        villager['production_status'] = reason
                    elif marker in butcher_cycles:
                        villager['production_status'] = 'Animal already being butchered this cycle'
                    elif villager.get('work_animation', {}).get('phase') != 'working':
                        villager['production_status'] = 'Walking to butcher shop'
                    else:
                        butcher_cycles.add(marker)
                        villager['production_status'] = logistics.butcher_animal(town, building, villager, TICK_SECONDS)
                    continue
                if building['kind'] in {'barn', 'stable'}:
                    marker = id(building)
                    if marker in barn_cycles:
                        villager['production_status'] = 'Barn already tended this cycle'
                        continue
                    barn_cycles.add(marker)
                villager['production_status'] = logistics.produce(town, building, villager, TICK_SECONDS)
                if building['kind'] in {'barn', 'stable'} and villager['production_status'] == 'Working':
                    economy.grow_animals(building, TICK_SECONDS)
                continue
            output = 'wood' if job == 'woodcutting' else 'fish' if job == 'fishing' else source.get('crop', 'vegetables')
            if job == 'fishing':
                depth = town_spatial.water_depths().get((source['x'], source['y']), 1)
                output = economy.fishing_catch(building, depth) if work_cycles.fishing_boating(town, building, index) else 'fish'
            cap_room = economy.production_room(town, output)
            if cap_room <= 0:
                villager['production_status'] = 'Production cap reached'
                continue
            room = min(logistics.BUFFER_SIZE - logistics.buffered(building), cap_room)
            if boating:
                room = min(room, economy.fishing_boat_capacity(building) - work_cycles.boat_load(town, building))
            if room <= 1e-9:
                villager['production_status'] = 'Workplace buffer full'
                continue
            if job == 'farming':
                marker = id(source) if source['kind'] == 'acre' else id(building)
                remaining = (1 if source['kind'] == 'acre' else 1) - field_cycles.get(marker, 0)
                if remaining <= 0:
                    villager['production_status'] = 'Field already tended this cycle'
                    continue
            caught = True
            if job == 'fishing':
                caught = secrets.randbelow(10) != 0
                villager['fishing_catch'] = {'attempt': villager.get('fishing_catch', {}).get('attempt', 0) + 1, 'caught': caught}
            rate = .5 if job == 'woodcutting' else 1
            bonus = production_bonus(building) + npcs.tool_bonus(villager, job)
            if source['kind'] == 'acre':
                bonus += production_bonus(source)
            upgrades = building.get('profession_upgrades', [])
            bonus += .25 * sum(u in upgrades for u in ('axe', 'nets', 'wool_nets', 'tools'))
            if job == 'fishing' and work_cycles.fishing_boating(town, building, index):
                bonus += .25 * max(0, economy.fishing_boat_level(building) - 1)
            amount = min(room, rate * (1 + bonus) * economy.efficiency(villager, job))
            if job == 'farming':
                # Acres bound farming output; manure improves each field's yield.
                amount = min(amount, remaining * (1 + bonus) * economy.efficiency(villager, job))
                field_cycles[marker] = field_cycles.get(marker, 0) + amount / ((1 + bonus) * economy.efficiency(villager, job))
                if economy.quantity(town, 'manure') >= .1:
                    economy.spend(town, {'manure': .1})
                    amount = min(room, amount * 1.25)
            else:
                amount = economy.take_supply(landscape, source, amount)
            if amount <= 0:
                villager['production_status'] = 'Waiting for supply to regenerate'
                continue
            if caught:
                orphan_room = max(0, economy.fishing_boat_capacity(building) - work_cycles.boat_load(town, building) - amount) if boating else None
                logistics.record_output(town, building, output, amount, villager, orphan_room=orphan_room)
            if boating and source.get('supply', 1) <= 1e-9:
                for crew_index in work_cycles.fishing_crew(town, building):
                    town['villagers'][crew_index]['boat_return_requested'] = True
            economy.practice(villager, job)
            villager['production_status'] = 'Working' if caught else 'No catch — trying again'
        feed_settlers(town)
        if (len(town['villagers']) < housing_capacity(town) and town['food'] >= 20
                and not town.get('arrival_delay_seconds')):
            economy.eat(town, 10)
            town['villagers'].append({'name': next_villager_name(town),
                                     'job': 'idle', 'woodcutting': 0, 'farming': 0, 'fishing': 0})
            economy.ensure_talents(town['villagers'][-1], len(town['villagers']) - 1)
        fill_cottages(town)
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


def validate_building_range(town, x, y):
    hall = next((b for b in town['buildings'] if b['kind'] == 'hall'), None)
    reach = building_range(town)
    if hall is None or abs(x - hall['x']) + abs(y - hall['y']) > reach:
        raise ValueError(f'Build within {reach} tile steps of your town hall.')


def command(town, data, world=None):
    if world and world.get('shared_map', {}).get('chunked'):
        attach_chunks(world, SAVE.with_name(SAVE.stem + '_chunks'))
        bind_map(world)
    ensure_resources(town)
    action = data.get('action')
    if action in {'join_lanterns', 'leave_lanterns', 'lanterns_walk', 'lanterns_collect'}:
        return lanterns.change(town, data, world)
    if world and lanterns.inside(world, town) and action in {'walk', 'fish', 'chop', 'gather', 'join_quarry'}:
        raise ValueError('Leave the courier path before exploring the valley or quarry.')
    if action in {'join_quarry', 'leave_quarry', 'quarry_walk', 'quarry_interact'}:
        return quests.change(town, data, world)
    if world and quests.inside(world, town) and action in {'walk', 'fish', 'chop', 'gather'}:
        raise ValueError('Leave the quarry before exploring the valley.')
    if action == 'walk' and 'steps' in data:
        steps = data['steps']
        if (not isinstance(steps, list) or not 1 <= len(steps) <= 5
                or any(not isinstance(step, dict)
                       or type(step.get('dx')) is not int or step['dx'] not in (-1, 0, 1)
                       or type(step.get('dy')) is not int or step['dy'] not in (-1, 0, 1)
                       for step in steps)):
            raise ValueError('Choose up to five WASD steps.')
        # Process input in order, refreshing local obstacles after every step.
        # The entire batch is validated before any player movement is applied.
        for step in steps:
            result = command(town, {'action': 'walk', 'dx': step['dx'], 'dy': step['dy']}, world)
        return result
    if action in {'create_trade', 'accept_trade', 'cancel_trade'}:
        return trading.change(town, data, world)
    if action in {'lead_horse', 'stable_horse', 'ride_horse', 'return_horse', 'toggle_mount'}:
        return horses.change(town, data)
    if action == 'configure_rule':
        return logistics.configure_rules(town, data)
    if action in {'claim_local_gift', 'buy_worker_tool', 'trade_local', 'give_worker_tool', 'take_worker_tool', 'transfer_worker_tool',
                  'advance_local_story', 'inspect_story_rock', 'interact_local'}:
        return npcs.change(town, data)
    if action in {'buy_equipment', 'sell_equipment', 'equip_item', 'unequip_item'}:
        return equipment.change(town, data)
    if action in {'buy_boat', 'upgrade_boat'}:
        player = town['player']
        level = economy.player_boat_level(player)
        if action == 'buy_boat' and level:
            raise ValueError('You already have an exploration boat.')
        if action == 'upgrade_boat' and not level:
            raise ValueError('Buy or build an exploration boat first.')
        spec = economy.PLAYER_BOATS.get(level + 1)
        if spec is None:
            raise ValueError('Your boat is fully upgraded.')
        if town['level'] < spec['level'] or economy.skill(town, 'fishing') < spec['xp']:
            raise ValueError('Develop fishing and upgrade the town hall first.')
        if not economy.afford(town, economy.costs_of(spec)):
            raise ValueError('Missing boat purchase or upgrade materials.')
        economy.spend(town, economy.costs_of(spec))
        player.update(boat=True, boat_level=level + 1)
        return {'player': dict(player)}
    if action == 'build_boat':
        if town['level'] < 3:
            raise ValueError('Exploration boats require town hall level 3.')
        if town['player'].get('boat'):
            raise ValueError('You already have an exploration boat.')
        if not economy.afford(town, {'wood': 20, 'planks': 10}):
            raise ValueError('An exploration boat costs 20 wood and 10 planks.')
        economy.spend(town, {'wood': 20, 'planks': 10})
        town['player']['boat'] = True
        return {'player': dict(town['player'])}
    if action in {'build_road', 'remove_road'}:
        x, y = data.get('x'), data.get('y')
        size = town.get('map_size', MAP_SIZE)
        if type(x) is not int or type(y) is not int or not (0 <= x < size and 0 <= y < size):
            raise ValueError('Choose a tile inside your town.')
        if action == 'remove_road':
            town['roads'] = [r for r in town.get('roads', []) if (r['x'], r['y']) != (x, y)]
            return {}
        validate_building_range(town, x, y)
        if world and 'shared_map' in world:
            validate_placement(world, town, data)
        terrain = town.get('terrain', [])
        obstacles = town['buildings'] + town['map_resources'] + town.get('npcs', [])
        if (terrain and terrain[y][x] == 'v') or any(r['x'] == x and r['y'] == y for r in obstacles):
            raise ValueError('Place roads on open ground.')
        if not any(r['x'] == x and r['y'] == y for r in town.get('roads', [])):
            town.setdefault('roads', []).append({
                'x': x, 'y': y,
                'construction': {'progress': 0, 'total': ROAD_BUILD_SECONDS}})
        return {}
    if world and 'shared_map' in world:
        if action == 'walk':
            return move_player(walking_map(world, town, local=True), data)
        if action in {'build', 'move'}:
            validate_placement(world, town, data)
    if action == 'walk':
        return move_player(town, data)
    if action in {'fish', 'chop', 'gather', 'stop_activity'}:
        landscape = world.get('shared_map', town) if world else town
        ensure_player(walking_map(world, town) if world and 'shared_map' in world else town)
        economy.start_activity(town, landscape, action)
        return {'player': dict(town['player'])}
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
        size = town.get('map_size', MAP_SIZE)
        if kind not in COSTS or type(x) is not int or type(y) is not int or not (0 <= x < size and 0 <= y < size):
            raise ValueError('Choose a building and a tile inside your town.')
        if any(b['x'] == x and b['y'] == y for b in town['buildings']):
            raise ValueError('That tile is already occupied.')
        if any(r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before building here.')
        validate_building_range(town, x, y)
        spec = economy.BUILDINGS.get(kind, {'wood': COSTS[kind]})
        if town['level'] < spec.get('level', 1):
            raise ValueError(f"Requires town hall level {spec['level']}.")
        costs = economy.costs_of(spec)
        if not economy.afford(town, costs):
            raise ValueError('Missing building materials: ' + ', '.join(costs))
        if kind == 'acre':
            validate_acre(town, data.get('farm_index'), x, y)
        economy.spend(town, costs)
        building = {'kind': kind, 'x': x, 'y': y,
                    'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        if kind == 'acre':
            building['farm_index'] = data['farm_index']
        town['buildings'].append(building)
    elif action == 'move':
        index, x, y = data.get('index'), data.get('x'), data.get('y')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        size = town.get('map_size', MAP_SIZE)
        if type(x) is not int or type(y) is not int or not (0 <= x < size and 0 <= y < size):
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
        moved = town['buildings'][index]
        for villager in town['villagers']:
            if villager.get('pending_workplace') == {k: moved[k] for k in ('kind', 'x', 'y')}:
                logistics.cancel_output(town, villager)
        town['buildings'][index].update(x=x, y=y)
    elif action == 'destroy':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        if town['buildings'][index]['kind'] == 'hall':
            raise ValueError('The town hall cannot be destroyed.')
        economy.ensure_inventory(town)
        removed = town['buildings'][index]
        for villager in town['villagers']:
            if villager.get('pending_workplace') == {k: removed[k] for k in ('kind', 'x', 'y')}:
                logistics.cancel_output(town, villager)
        removed = town['buildings'].pop(index)
        hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
        for key, amount in economy.inventory(removed).items():
            stock = hall.setdefault('stored_goods', {})
            stock[key] = stock.get(key, 0) + amount
        # Demolition salvages unfinished goods at the hall as well.
        for key, amount in removed.get('pending_output', {}).items():
            stock = hall.setdefault('stored_goods', {})
            stock[key] = stock.get(key, 0) + amount
            economy.add_stock(town, key, amount)
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
        logistics.cancel_output(town, town['villagers'][index])
        town['villagers'][index].pop('building_index', None)
        town['villagers'][index]['job'] = 'idle'
        town['villagers'][index].pop('logistics_trip', None)
        town['villagers'][index].pop('logistics_target', None)
    elif action == 'configure_storage':
        index, enabled = data.get('index'), data.get('enabled')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a storage room first.')
        building = town['buildings'][index]
        if building['kind'] != 'storehouse':
            raise ValueError('Only storehouses and warehouses can configure storage.')
        if not completed(building):
            raise ValueError('Choose a completed storage room.')
        allowed = economy.building_storage_keys(building)
        if (not isinstance(enabled, dict)
                or any(k not in allowed or type(value) is not bool for k, value in enabled.items())):
            raise ValueError('Enable or disable valid storage resources.')
        building['storage_enabled'] = {k: enabled.get(k, False) for k in allowed}
        building.pop('storage_allocations', None)
        building.pop('wood_reserve', None)
        building.pop('wood_reserve_enabled', None)
    elif action == 'upgrade_capacity':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        if building['kind'] == 'hall':
            raise ValueError('The town hall is reserved for your controllable character.')
        level = building.get('settler_level', 1)
        if not completed(building) or level >= 3:
            raise ValueError('Capacity cannot be upgraded at this time.')
        cost = 20 * level
        if town['wood'] < cost:
            raise ValueError('Gather more wood first.')
        economy.spend(town, {'wood': cost})
        building['construction'] = {'progress': 0, 'total': BUILD_SECONDS * level,
                                    'upgrade': {'settler_level': level + 1}}
    elif action == 'job':
        index, job = data.get('index'), data.get('job')
        if type(index) is not int or not 0 <= index < len(town['villagers']) or job not in JOBS:
            raise ValueError('Choose a villager and a valid job.')
        required = {'hauling': 'storehouse', 'woodcutting': 'woodcutter', 'farming': 'farm', 'fishing': 'fishing_hut', **economy.JOB_BUILDINGS}.get(job)
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
            logistics.cancel_output(town, town['villagers'][index])
            town['villagers'][index].pop('building_index', None)
            town['villagers'][index].pop('logistics_trip', None)
            town['villagers'][index].pop('logistics_target', None)
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
        if level >= (5 if building['kind'] == 'storehouse' else 3):
            raise ValueError('This building is fully upgraded.')
        cost = COSTS[building['kind']] * level
        if town['wood'] < cost:
            raise ValueError(f'Gather {cost} wood to upgrade this building.')
        economy.spend(town, {'wood': cost})
        building['construction'] = {'progress': 0, 'total': BUILD_SECONDS * level,
                                    'upgrade': {'level': level + 1}}
    elif action in {'set_crop', 'set_recipe', 'buy_animal', 'breed_animal', 'butcher_animal', 'profession_upgrade'}:
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        if not completed(building):
            raise ValueError('Finish construction first.')
        if action == 'butcher_animal':
            if building['kind'] != 'butcher':
                raise ValueError('Choose a butcher shop.')
            if building.get('butchering'):
                raise ValueError('Finish butchering the current animal first.')
            barn_index, animal = data.get('barn_index'), data.get('animal')
            if type(barn_index) is not int or not 0 <= barn_index < len(town['buildings']):
                raise ValueError('Choose a barn first.')
            barn = town['buildings'][barn_index]
            if barn['kind'] != 'barn' or not completed(barn) or animal not in economy.ANIMALS:
                raise ValueError('Choose an animal from a completed barn.')
            ages = economy.animal_growth(barn, animal)
            adult = next((i for i, age in enumerate(ages) if age >= economy.ANIMAL_GROWTH_SECONDS), None)
            if adult is None:
                raise ValueError('Only fully grown animals can be butchered.')
            if not any(v.get('building_index') == index and v['job'] == 'butchery'
                       and not economy.qualification(v, 'butchery') for v in town['villagers']):
                raise ValueError('Assign a qualified butcher first.')
            ages.pop(adult)
            barn.setdefault('animal_growth', {})[animal] = ages
            barn['animals'][animal] -= 1
            building['butchering'] = {'animal': animal, 'amount': economy.ANIMAL_MEAT[animal],
                                      'remaining': economy.BUTCHER_SECONDS}
        elif action == 'set_recipe':
            recipes = economy.PROFESSION_RECIPES.get(building['kind'], [])
            spec = next((r for r in recipes if r['output'] == data.get('recipe')), None)
            if spec is None:
                raise ValueError('Choose a matching workplace recipe.')
            job = economy.BUILDINGS[building['kind']]['job']
            workers = [v for v in town['villagers'] if v.get('building_index') == index
                       and v['job'] == job and not economy.qualification(v, job)]
            if spec['level'] > 1 and not any(v.get(job, 0) >= spec['level'] for v in workers):
                raise ValueError(f"Requires an assigned worker with level {spec['level']} {job}.")
            economy.ensure_inventory(town)
            # Release obsolete reserved ingredients for collection by workers/haulers.
            for key in list(building.get('input_buffer', {})):
                if key not in spec['inputs']:
                    amount = building['input_buffer'].pop(key)
                    buffer = building.setdefault('output_buffer', {})
                    buffer[key] = buffer.get(key, 0) + amount
                    economy.add_stock(town, key, amount)
            building['recipe'] = spec['output']
            for villager in town['villagers']:
                if villager.get('building_index') == index:
                    villager.pop('production_work', None)
        elif action == 'set_crop':
            crop = data.get('crop')
            if building['kind'] != 'acre' or crop not in economy.CROPS:
                raise ValueError('Choose an acre and a crop.')
            if town['level'] < economy.CROPS[crop]:
                raise ValueError('Upgrade the hall to unlock this crop.')
            building['crop'] = crop
        elif action in {'buy_animal', 'breed_animal'}:
            animal = data.get('animal')
            spec = economy.ANIMALS.get(animal)
            if (spec is None or town['level'] < spec['level']
                    or building['kind'] != ('stable' if animal == 'horse' else 'barn')
                    or (animal == 'horse' and action == 'buy_animal')):
                raise ValueError('Choose an unlocked animal and a barn.')
            animals = building.get('animals', {})
            if sum(animals.values()) >= 4 * building_level(building):
                raise ValueError('Barn full. Upgrade it for more animal spaces.')
            costs = {'coins': spec['coins']} if action == 'buy_animal' else {'fodder': 3, 'herbs': 1}
            if action == 'breed_animal' and economy.mature_animals(building, animal) < 2:
                raise ValueError('Breeding requires two fully grown animals of the same kind.')
            if not economy.afford(town, costs):
                raise ValueError('Missing animal purchase or breeding supplies.')
            economy.spend(town, costs)
            ages = economy.animal_growth(building, animal)
            ages.append(0)
            building.setdefault('animal_growth', {})[animal] = ages
            building.setdefault('animals', {})[animal] = animals.get(animal, 0) + 1
        else:
            key = data.get('upgrade')
            spec = economy.PROFESSION_UPGRADES.get(key)
            if spec is None or spec['building'] != building['kind']:
                raise ValueError('Choose a matching profession upgrade.')
            if town['level'] < spec['level'] or economy.skill(town, spec.get('skill', '')) < spec.get('xp', 0):
                raise ValueError('Develop the profession and upgrade the town hall first.')
            if spec.get('requires') and spec['requires'] not in building.get('profession_upgrades', []):
                raise ValueError('Purchase the previous boat tier first.')
            if key in building.get('profession_upgrades', []):
                raise ValueError('Already unlocked.')
            if not economy.afford(town, economy.costs_of(spec)):
                raise ValueError('Missing profession upgrade materials.')
            economy.spend(town, economy.costs_of(spec))
            if key == 'boat':
                building.setdefault('profession_upgrades', []).append(key)
            else:
                building['construction'] = {'progress': 0, 'total': BUILD_SECONDS,
                                            'upgrade': {'profession_upgrades':
                                                        building.get('profession_upgrades', []) + [key]}}
    elif action == 'sell_goods':
        key, amount = data.get('resource'), data.get('amount')
        if not any(b['kind'] == 'market' and completed(b) for b in town['buildings']):
            raise ValueError('Build a market stall first.')
        if key not in {*economy.LATE_GOODS, *economy.FISH, 'planks', 'wool', 'flour', 'smoked_fish', 'tools', 'herbs', 'fruit', 'milk', 'eggs', 'meat', 'bread', 'prepared_meat', *(r['output'] for recipes in economy.PROFESSION_RECIPES.values() for r in recipes)} or type(amount) is not int or not 1 <= amount <= 10:
            raise ValueError('Choose goods and a quantity from 1 to 10.')
        if not economy.afford(town, {key: amount}):
            raise ValueError('Not enough goods to sell.')
        economy.spend(town, {key: amount})
        town['coins'] += amount * economy.FISH.get(key, {}).get('coins', economy.SALE_PRICES.get(key, 1))
        town['market_sales'] = town.get('market_sales', 0) + amount
    elif action == 'upgrade':
        hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
        if town.get('upgrade_remaining') or not completed(hall):
            raise ValueError('Your hall upgrade is already underway.')
        upgrade = hall_upgrade(town)
        if not upgrade:
            raise ValueError('Your town hall is fully upgraded (level 10).')
        if not economy.milestone(town, upgrade['level']):
            raise ValueError(upgrade['milestone'] + ' required to upgrade.')
        costs = economy.costs_of(upgrade)
        costs.pop('coins', None)  # Hall coins are rewards, not a material cost.
        if not economy.afford(town, costs):
            raise ValueError('Save the hall upgrade materials: ' + ', '.join(costs))
        economy.spend(town, costs)
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

    def handle(self):
        try:
            super().handle()
        except (BrokenPipeError, ConnectionResetError):
            # Browsers can disconnect while a request or response is in flight.
            self.close_connection = True

    def log_request(self, code='-', size='-'):
        # Polling and movement generate frequent successful requests.
        if code != '-' and int(code) >= 400:
            super().log_request(code, size)

    def reply(self, status, data):
        body = json.dumps(data, separators=(',', ':')).encode()
        self.reply_body(status, body)

    def reply_body(self, status, body):
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        route = urlsplit(self.path)
        if route.path == '/api/world':
            query = parse_qs(route.query)
            try:
                keys = parse_keys(query.get('chunks', [''])[0])
                # Clients retain a bounded terrain cache; validate each batch.
                known_value = query.get('known', [''])[0]
                known = set()
                parts = known_value.split(';') if known_value else []
                if len(parts) > 128:
                    raise ValueError('Too many cached map chunks.')
                for offset in range(0, len(parts), 49):
                    known.update(parse_keys(';'.join(parts[offset:offset + 49])))
                versions = parse_versions(query.get('resources', [''])[0])
            except ValueError as error:
                self.reply(400, {'error': str(error)})
                return
            with LOCK:
                if query.get('world_id', [ACTIVE_WORLD])[0] != ACTIVE_WORLD:
                    known, versions = set(), {}
                prepare_landscapes(WORLD)
                for town in WORLD['towns'].values():
                    ensure_resources(town)
                    ensure_player(walking_map(WORLD, town) if 'shared_map' in WORLD else town)
                snapshot = export_world(WORLD)
                if WORLD.get('_chunks'):
                    try:
                        landscape = WORLD['_chunks'].snapshot(keys, known, versions)
                    except ValueError as error:
                        self.reply(400, {'error': str(error)})
                        return
                    snapshot = dict(snapshot, shared_map=landscape)
                snapshot = dict(snapshot, hall_upgrades=HALL_UPGRADES, progression=economy.public_catalog(), local_stories=npcs.STORIES, quarry_catalog=quests.CATALOG,
                                hall_ready={k: economy.milestone(t, t['level'] + 1) for k, t in WORLD['towns'].items()}, world_id=ACTIVE_WORLD,
                                name=WORLDS[ACTIVE_WORLD]['name'] if ACTIVE_WORLD else None,
                                players_sampled_at=time.monotonic())
                # Freeze the mutable world while locked, then release it before
                # a slow LAN transfer can block movement or simulation ticks.
                body = json.dumps(snapshot, separators=(',', ':')).encode()
            self.reply_body(200, body)
        elif route.path == '/api/lanterns':
            with LOCK:
                body = json.dumps({'world_id': ACTIVE_WORLD, 'quest': lanterns.snapshot(WORLD)},
                                  separators=(',', ':')).encode()
            self.reply_body(200, body)
        elif self.path == '/api/players':
            with LOCK:
                snapshot = {'world_id': ACTIVE_WORLD, 'sampled_at': time.monotonic(),
                            'players': {key: dict(town['player'])
                                        for key, town in WORLD['towns'].items()
                                        if 'player' in town}}
            self.reply(200, snapshot)
        elif self.path == '/api/worlds':
            with LOCK:
                snapshot = {'active_world': ACTIVE_WORLD, 'can_manage': self.is_host(),
                            'worlds': [{'id': key, 'name': value['name'],
                                        'town_count': len(value['world']['towns'])}
                                       for key, value in WORLDS.items()]}
            self.reply(200, snapshot)
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
                    deleted_chunks = None
                    if not self.is_host():
                        self.reply(403, {'error': 'Only the host at localhost can manage worlds.'})
                        return
                    if self.path == '/api/worlds/create':
                        name = data.get('name')
                        if not isinstance(name, str) or not name.strip() or len(name.strip()) > 24:
                            raise ValueError('Enter a world name of 1–24 characters.')
                        world_id = secrets.token_hex(8)
                        WORLDS[world_id] = {'name': name.strip(), 'world': new_world(
                            explorable=True, chunk_directory=SAVE.with_name(SAVE.stem + '_chunks')), 'tokens': {}}
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
                            landscape = WORLDS[world_id]['world'].get('shared_map', {})
                            if landscape.get('chunked'):
                                deleted_chunks = SAVE.with_name(SAVE.stem + '_chunks') / str(landscape['terrain_seed'])
                            del WORLDS[world_id]
                            if ACTIVE_WORLD == world_id:
                                activate(None)
                    save()
                    if deleted_chunks and deleted_chunks.exists():
                        shutil.rmtree(deleted_chunks)
                    self.reply(200, {'ok': True})
                elif self.path in {'/api/login', '/api/join', '/api/command'}:
                    if not ACTIVE_WORLD or data.get('world_id') != ACTIVE_WORLD:
                        self.reply(409, {'error': 'The host changed worlds. Refresh and try again.'})
                        return
                    if self.path == '/api/login':
                        result = player_login.login(WORLDS[ACTIVE_WORLD], data,
                                                    self.headers.get('Authorization', ''),
                                                    lambda name: new_town(name, WORLD))
                        if not result.get('needs_town'):
                            prepare_landscapes(WORLD)
                            save()
                        self.reply(200, result)
                        return
                    if self.path == '/api/command':
                        town_id = TOKENS.get(self.headers.get('Authorization', ''))
                        if town_id not in WORLD['towns']:
                            self.reply(401, {'error': 'Join this world first.'})
                            return
                        result = command(WORLD['towns'][town_id], data, WORLD)
                        if data.get('action') != 'walk' or not WORLD.get('_chunks'):
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
                    prepare_landscapes(WORLD)
                    save()
                    self.reply(200, {'token': token, 'town_id': town_id})
                else:
                    self.reply(404, {'error': 'Unknown endpoint.'})
        except (ValueError, TypeError) as error:
            self.reply(400, {'error': str(error) or 'Invalid request.'})


def activate(world_id):
    global ACTIVE_WORLD, WORLD, TOKENS
    if world_id != ACTIVE_WORLD and WORLD.get('_chunks'):
        WORLD['_chunks'].flush()
        WORLD.pop('_chunks')
        for landscape in [WORLD['shared_map'], *WORLD['towns'].values()]:
            for field in ('terrain', 'map_resources', 'tree_stumps'):
                landscape.pop(field, None)
    ACTIVE_WORLD = world_id
    entry = WORLDS.get(world_id)
    WORLD, TOKENS = (entry['world'], entry['tokens']) if entry else ({'towns': {}}, {})
    attach_chunks(WORLD, SAVE.with_name(SAVE.stem + '_chunks'))
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
    # Optional private host provisioning for villages from older saves.
    provision = SAVE.with_suffix('.players.json')
    if provision.exists():
        for world_id, players in json.loads(provision.read_text()).items():
            if world_id in WORLDS:
                entry = WORLDS[world_id]
                for key, account in players.items():
                    if account['town_id'] in entry['world']['towns']:
                        entry.setdefault('players', {}).setdefault(key, account)
    for entry in WORLDS.values():
        bind_map(entry['world'])
        for town in entry['world']['towns'].values():
            ensure_villager_names(town)
            for index, villager in enumerate(town['villagers']):
                economy.ensure_talents(villager, index)


def save():
    if WORLD.get('_chunks'):
        WORLD['_chunks'].flush()
    temporary = SAVE.with_suffix('.json.tmp')
    temporary.write_text(json.dumps({'worlds': {key: dict(entry, world=export_world(entry['world']))
                                                for key, entry in WORLDS.items()},
                                     'active_world': ACTIVE_WORLD}))
    temporary.replace(SAVE)


def lantern_simulation():
    previous = time.monotonic()
    while True:
        time.sleep(.05)
        with LOCK:
            now = time.monotonic()
            lanterns.advance(WORLD, now - previous)
            village_life.advance(WORLD, now - previous)
            previous = now


def simulation():
    next_tick = time.monotonic() + TICK_SECONDS
    while True:
        time.sleep(max(0, next_tick - time.monotonic()))
        with LOCK:
            tick(WORLD)
            save()
        # Work and lock contention belong inside the three-second interval.
        # If a tick overruns, resume now without replaying a backlog of ticks.
        next_tick = max(next_tick + TICK_SECONDS, time.monotonic())


WORLD, TOKENS = {'towns': {}}, {}
WORLDS, ACTIVE_WORLD = {}, None
if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--port', type=int, default=8000)
    args = parser.parse_args()
    load()
    server = ThreadingHTTPServer(('0.0.0.0', args.port), Handler)
    threading.Thread(target=simulation, daemon=True).start()
    threading.Thread(target=lantern_simulation, daemon=True).start()
    print('Lantern Vale running at http://localhost:' + str(args.port), flush=True)
    print('Friends can join using http://YOUR-LAN-IP:' + str(args.port), flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        with LOCK:
            save()
        server.server_close()
