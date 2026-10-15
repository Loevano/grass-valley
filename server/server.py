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
import resource_activity
import logistics
import barns
import work_cycles
import farming
import orchards
import equipment
import npcs
import trading
import horses
import quests
import player_login
from navigation import routes, RoutePending
import lanterns
import village_life
from spatial import SpatialIndex, BUILDING_FOOTPRINTS, footprint_tiles, occupied_tiles

from terrain import ensure_terrain, remove_invalid_trees, ensure_industry
from npcs import ensure_npcs
from shared_world import bind_map, building_range, export_world, new_world, validate_placement, walking_map
from chunk_world import CHUNK_SIZE, ChunkTerrain, attach as attach_chunks, chunk_key, parse_keys, parse_versions

ROOT = Path(__file__).resolve().parents[1]
SAVE = ROOT / 'savegame.json'
LOCK = threading.RLock()
COSTS = {'home': 20, 'woodcutter': 15, 'farm': 15, 'storehouse': 25, 'fishing_hut': 15, 'acre': 5}
JOBS = {'idle', 'building', 'woodcutting', 'farming', 'fishing', 'hauling'}
SETTLER_SPACES = {'hall': 1, 'home': 2, 'storehouse': 1, 'woodcutter': 1,
                  'fishing_hut': 1, 'farm': 1, 'acre': 1}
BUILDING_JOBS = {'storehouse': 'hauling', 'woodcutter': 'woodcutting', 'farm': 'farming',
                 'fishing_hut': 'fishing', 'acre': 'farming'}
TICK_SECONDS = 3
NEXT_TICK_AT = None
BUILD_SECONDS = 30
ROAD_BUILD_SECONDS = 10
# New towns develop professions instead of repeating wood-only upgrades.
HALL_UPGRADES = economy.HALL_UPGRADES
COSTS.update({k: v['wood'] for k, v in economy.BUILDINGS.items()})
JOBS.update(economy.JOB_BUILDINGS)
SETTLER_SPACES.update({k: v['slots'] for k, v in economy.BUILDINGS.items()})
BUILDING_JOBS.update({k: v['job'] for k, v in economy.BUILDINGS.items() if 'job' in v})

IDLE_FOOD = economy.IDLE_FOOD
WORKER_FOOD = economy.WORKER_FOOD
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


ANIMAL_NAMES = (
    'Poppy', 'Clover', 'Hazel', 'Biscuit', 'Mabel', 'Pip', 'Daisy', 'Fern',
    'Buttercup', 'Pebble', 'Maple', 'Honey', 'Moss', 'Nellie', 'Pumpkin', 'Coco',
    'Dottie', 'Juniper', 'Pickle', 'Rosie', 'Bramble', 'Olive', 'Chestnut', 'Sunny',
    'Bonnie', 'Acorn', 'Ginger', 'Marigold', 'Basil', 'Bluebell', 'Toffee', 'Pearl',
)


def ensure_animal_names(town):
    """Name barn animals once, keeping names aligned with their saved ages."""
    used = {name for barn in town.get('buildings', []) if barn['kind'] == 'barn'
            for names in barn.get('animal_names', {}).values() for name in names
            if isinstance(name, str) and name.strip()}
    seen = set()
    sequence = town.get('animal_name_sequence', 0)
    for barn in town.get('buildings', []):
        if barn['kind'] != 'barn' or not barn.get('animals'):
            continue
        names = barn.setdefault('animal_names', {})
        for animal, count in barn['animals'].items():
            saved = names.get(animal, [])[:count]
            saved += [''] * (count - len(saved))
            for index, name in enumerate(saved):
                if not isinstance(name, str) or not name.strip() or name in seen:
                    while True:
                        base = ANIMAL_NAMES[sequence % len(ANIMAL_NAMES)]
                        generation = sequence // len(ANIMAL_NAMES) + 1
                        name = base if generation == 1 else f'{base} {generation}'
                        sequence += 1
                        if name not in used:
                            break
                    saved[index] = name
                used.add(name)
                seen.add(name)
            names[animal] = saved
    if sequence != town.get('animal_name_sequence', 0):
        town['animal_name_sequence'] = sequence


def housing_capacity(town):
    # Existing cottages keep their current housing while an upgrade is built.
    return 5 + sum(max(2 * (building_level(b) + ('timber_frame' in b.get('profession_upgrades', []))),
                      settler_capacity(b) if town.get('sandbox') else 0)
                   for b in town['buildings'] if b['kind'] == 'home'
                   and (completed(b) or 'upgrade' in b['construction']))


def housing_loyalty(town):
    # Housing is shared, so cottage comfort protects workers in every profession.
    population = len(town['villagers'])
    comfort = []
    for building in town['buildings']:
        if building['kind'] != 'home' or (not completed(building) and 'upgrade' not in building['construction']):
            continue
        level = building_level(building) + ('timber_frame' in building.get('profession_upgrades', []))
        comfort.extend([min(.75, .25 * (level - 1))] * (2 * level))
    return sum(sorted(comfort, reverse=True)[:population]) / population if population else 0


def feed_settlers(town):
    needed = len(town['villagers']) * IDLE_FOOD + WORKER_FOOD * sum(
        v['job'] != 'idle' or bool(v.get('construction_target')) for v in town['villagers'])
    shortage = town['food'] + 1e-9 < needed
    economy.eat(town, needed)
    town['food_shortage_seconds'] = (town.get('food_shortage_seconds', 0) + TICK_SECONDS
                                     if shortage else 0)
    town['housing_shortage_seconds'] = (town.get('housing_shortage_seconds', 0) + TICK_SECONDS
                                       if len(town['villagers']) > housing_capacity(town) else 0)
    town['arrival_delay_seconds'] = max(0, town.get('arrival_delay_seconds', 0) - TICK_SECONDS)
    if town.get('sandbox'):
        return
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
    logistics.cancel_trip(town, villager)
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


def walkable(town, x, y, blocked=None, water=None, fences=None):
    size = town.get('map_size', 12)
    radius = .18
    if not (radius - .5 <= x <= size - .5 - radius
            and radius - .5 <= y <= size - .5 - radius):
        return False
    if barns.blocked(x, y, barns.fences(town['buildings'], size) if fences is None else fences):
        return False
    if blocked is None:
        blocked = occupied_tiles(b for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and town.get('player', {}).get('boat')))
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
        blocked = occupied_tiles(b for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and player.get('boat')))
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
        blocked = occupied_tiles(b for b in
                   town['buildings'] + town['map_resources']
                   if b['kind'] not in {'tree', 'berry_bush'} and not (b['kind'] == 'water' and player.get('boat')))
        # Small steps prevent tunnelling through obstacles; slide along walls.
        water = {(r['x'], r['y']) for r in town['map_resources'] if r['kind'] == 'water'}
        fences = barns.fences(town['buildings'], town.get('map_size', 12))
        for _ in range(5):
            tile = (math.floor(player['x'] + .5), math.floor(player['y'] + .5))
            speed = (1 + .25 * max(0, economy.player_boat_level(player) - 1)) if tile in water else 1.5 if tile in roads else 1
            if tile not in water and player.get('mounted'):
                speed *= 1.75
            x = player['x'] + dx / length * .06 * speed
            if walkable(town, x, player['y'], blocked, water, fences):
                player['x'] = x
            y = player['y'] + dy / length * .06 * speed
            if walkable(town, player['x'], y, blocked, water, fences):
                player['y'] = y
        if dx != dy:
            player['facing'] = 1 if dx > dy else -1
    return {'player': dict(player)}


def map_resources(buildings):
    occupied = occupied_tiles(buildings)
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
    for building in town['buildings']:
        building.pop('grain_flow', None)
        building.pop('grain_delivery_pending', None)
        if (building['kind'] == 'acre' and 'construction' in building
                and 'upgrade' not in building['construction']):
            building.pop('construction')
            farming.field_state(building)
    # Older saves could assign residents to the founder's town hall.
    for villager in town['villagers']:
        index = villager.get('building_index')
        if (type(index) is int and 0 <= index < len(town['buildings'])
                and town['buildings'][index]['kind'] == 'hall'):
            villager.pop('building_index')
            villager['job'] = 'idle'
            villager.pop('work_target', None)
            logistics.cancel_trip(town, villager)
        elif (type(index) is int and 0 <= index < len(town['buildings'])
              and town['buildings'][index]['kind'] == 'market' and villager['job'] == 'idle'):
            # Residents assigned to markets before merchants existed start trading.
            villager['job'] = 'trading'
    if 'town_origin' in town:
        migrate_wood_chain(town)
        migrate_grain_chain(town)
        migrate_acre_upgrades(town)
        migrate_worker_capacities(town)
        return
    if 'map_resources' not in town:
        town['map_resources'] = map_resources(town['buildings'])
    if town.get('map_size') != MAP_SIZE:
        occupied = occupied_tiles(town['buildings'])
        occupied.update((r['x'], r['y']) for r in town['map_resources'])
        town['map_resources'].extend(r for r in map_resources(town['buildings'])
                                     if (r['x'] >= 12 or r['y'] >= 12)
                                     and (r['x'], r['y']) not in occupied)
        town['map_size'] = MAP_SIZE
    ensure_terrain(town)
    ensure_npcs(town)
    migrate_wood_chain(town)
    migrate_grain_chain(town)
    migrate_acre_upgrades(town)
    migrate_worker_capacities(town)


def migrate_acre_upgrades(town):
    """Keep fields and stock, refund queued upgrades, and retire extra acre slots."""
    if town.get('acre_upgrade_version') == 1:
        return
    for index, building in enumerate(town['buildings']):
        if building['kind'] != 'acre':
            continue
        construction = building.get('construction', {})
        pending = construction.get('upgrade')
        if pending:
            if not town.get('sandbox'):
                cost = (20 * building.get('settler_level', 1) if 'settler_level' in pending
                        else COSTS['acre'] * building.get('level', 1))
                economy.ensure_inventory(town)
                hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
                stock = hall.setdefault('stored_goods', {})
                stock['wood'] = stock.get('wood', 0) + cost
                economy.add_stock(town, 'wood', cost)
            building.pop('construction')
        building.pop('level', None)
        building.pop('settler_level', None)
        assigned = assigned_settlers(town, index)
        for worker_index in assigned[1:]:
            villager = town['villagers'][worker_index]
            logistics.cancel_output(town, villager)
            logistics.cancel_trip(town, villager)
            villager['job'] = 'idle'
            for key in ('building_index', 'work_target', 'watering_round', 'work_animation'):
                villager.pop(key, None)
    town['acre_upgrade_version'] = 1


def migrate_grain_chain(town):
    """Convert extra grain slots to equipment, keeping cargo and queued partial work."""
    if town.get('grain_chain_version') == 1:
        return
    for index, building in enumerate(town['buildings']):
        if building['kind'] not in {'mill', 'bakery'}:
            continue
        assigned = assigned_settlers(town, index)
        for worker_index in assigned:
            town['villagers'][worker_index]['building_index'] = index
        first = 'improved_millstones' if building['kind'] == 'mill' else 'larger_oven'
        pending = building.get('construction', {}).get('upgrade', {})
        old_slots = (2 if town.get('worker_capacity_version') != 1 else
                     min(2, building.get('settler_level', 1)) if building['kind'] == 'mill' else 1)
        if old_slots > 1 or len(assigned) > 1:
            upgrades = building.setdefault('profession_upgrades', [])
            if first not in upgrades:
                upgrades.append(first)
        if 'settler_level' in pending:
            if pending['settler_level'] > 1:
                upgrades = pending.setdefault('profession_upgrades', list(building.get('profession_upgrades', [])))
                if first not in upgrades:
                    upgrades.append(first)
            pending['settler_level'] = 1
        building['settler_level'] = 1
        for worker_index in assigned[1:]:
            worker = town['villagers'][worker_index]
            if worker.get('production_work'):
                building.setdefault('grain_work_queue', []).append(dict(worker['production_work']))
            logistics.cancel_output(town, worker)
            logistics.cancel_trip(town, worker)
            worker['job'] = 'idle'
            for key in ('building_index', 'work_target', 'construction_target',
                        'construction_arrived', 'production_status'):
                worker.pop(key, None)
    town['grain_chain_version'] = 1


def migrate_wood_chain(town):
    """Apply wood staffing limits once, preserving residents, cargo and equipment."""
    if town.get('wood_chain_version') == 1:
        return
    economy.ensure_inventory(town)
    for worker_index, villager in enumerate(town['villagers']):
        if 'building_index' not in villager:
            kind = {'woodcutting': 'woodcutter', 'woodworking': 'sawpit', 'crafting': 'workshop'}.get(villager['job'])
            places = [i for i, b in enumerate(town['buildings']) if b['kind'] == kind]
            if places:
                villager['building_index'] = places[worker_index % len(places)]
    for index, building in enumerate(town['buildings']):
        kind = building['kind']
        if kind not in economy.WOOD_WORKER_CAPACITIES:
            continue
        tiers = economy.WOOD_WORKER_CAPACITIES[kind]
        # An existing base sawpit already had two spaces.
        building['settler_level'] = min(len(tiers), max(2 if kind == 'sawpit' else 1,
                                                     building.get('settler_level', 1)))
        pending = building.get('construction', {}).get('upgrade', {})
        if 'settler_level' in pending:
            pending['settler_level'] = min(len(tiers), pending['settler_level'])
        assigned = [v for v in town['villagers'] if v.get('building_index') == index]
        for villager in assigned[settler_capacity(building):]:
            logistics.cancel_output(town, villager)
            villager['job'] = 'idle'
            for key in ('building_index', 'work_target', 'logistics_trip', 'logistics_target',
                        'construction_target', 'construction_arrived', 'production_status'):
                villager.pop(key, None)
        # Replace old workplace bonuses with tools that can be assigned and recovered.
        upgrade_lists = (building.get('profession_upgrades', []), pending.get('profession_upgrades', []))
        converted = {key for upgrades in upgrade_lists for key in upgrades if key in {'axe', 'tools', 'saw'}}
        for upgrades in upgrade_lists:
            upgrades[:] = [key for key in upgrades if key not in converted]
        for key in sorted(converted):
            tool = 'crafted_saw' if key == 'saw' else 'crafted_axe'
            tools = town.setdefault('player', {}).setdefault('worker_tools', {})
            workers = assigned[:settler_capacity(building)]
            for villager in workers:
                if not villager.get('worker_tool'):
                    villager['worker_tool'] = tool
                else:
                    tools[tool] = tools.get(tool, 0) + 1
            if not workers:
                tools[tool] = tools.get(tool, 0) + 1
    town['wood_chain_version'] = 1


def migrate_worker_capacities(town):
    """Retain unlocked spaces up to the new caps without losing residents or goods."""
    if town.get('worker_capacity_version') == 1:
        return
    legacy_tiers = {'woodcutter': (1, 2, 4), 'sawpit': (1, 2), 'workshop': (1,)}
    legacy_spaces = {'acre': 1, 'farm': 4, 'mill': 2, 'bakery': 2, 'mine': 2,
                     'smelter': 2, 'steelworks': 2}
    for index, building in enumerate(town['buildings']):
        kind = building['kind']
        tiers = economy.WORKER_CAPACITIES.get(kind)
        if not tiers:
            continue
        assigned = assigned_settlers(town, index)
        for worker_index in assigned:
            town['villagers'][worker_index]['building_index'] = index
        pending = building.get('construction', {}).get('upgrade', {})
        for state in (building, pending):
            if state is pending and 'settler_level' not in state:
                continue
            level = state.get('settler_level', 1)
            old_tiers = legacy_tiers.get(kind)
            spaces = old_tiers[min(level, len(old_tiers)) - 1] if old_tiers else legacy_spaces[kind] * level
            state['settler_level'] = next((i for i, capacity in enumerate(tiers, 1)
                                          if capacity >= spaces), len(tiers))
        for worker_index in assigned[settler_capacity(building):]:
            villager = town['villagers'][worker_index]
            logistics.cancel_output(town, villager)
            logistics.cancel_trip(town, villager)
            villager['job'] = 'idle'
            for key in ('building_index', 'work_target', 'watering_round',
                        'construction_target', 'construction_arrived', 'production_status'):
                villager.pop(key, None)
    town['worker_capacity_version'] = 1


def settler_capacity(building):
    if building['kind'] in ('hall', 'orchard_tree'):
        return 0
    tiers = economy.WORKER_CAPACITIES.get(building['kind'])
    if tiers:
        return tiers[min(len(tiers), building.get('settler_level', 1)) - 1]
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
        if building['kind'] in {'mill', 'bakery'}:
            raise ValueError('This building has one worker. Upgrade its equipment for more production.')
        raise ValueError('This building has no free settler spaces. Upgrade its capacity first.')
    villager = town['villagers'][worker_index]
    reason = economy.qualification(villager, BUILDING_JOBS.get(building['kind'], 'idle'))
    if reason:
        raise ValueError(reason)
    logistics.cancel_output(town, villager)
    logistics.cancel_trip(town, villager)
    villager.pop('watering_round', None)
    villager['building_index'] = building_index
    villager['job'] = BUILDING_JOBS.get(building['kind'], 'idle')


def fill_cottages(town):
    if town.get('sandbox'):
        while len(town['villagers']) < housing_capacity(town):
            villager = {'name': next_villager_name(town), 'job': 'idle',
                        'woodcutting': 0, 'farming': 0, 'fishing': 0, 'sandbox': True}
            economy.ensure_talents(villager, len(town['villagers']))
            town['villagers'].append(villager)
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


def woodcutting_source(town, building, trees, worker_index, walking=None):
    """Keep one tree per woodcutter until its work and delivery trip finishes."""
    worker = town['villagers'][worker_index]
    animation = worker.get('work_animation', {})
    task = animation.get('task', [])
    workplace = ['woodcutting', building['kind'], building['x'], building['y']]
    previous = tuple(task[4:7]) if len(task) >= 7 and task[:4] == workplace else None
    if previous is None and (worker.get('work_target') or {}).get('kind') == 'tree':
        previous = tuple(worker['work_target'][k] for k in ('kind', 'x', 'y'))
    claimed = set()
    for index, other in enumerate(town['villagers']):
        if (index == worker_index or other['job'] != 'woodcutting'
                or other.get('construction_target') or other.get('logistics_trip')):
            continue
        other_animation = other.get('work_animation', {})
        other_task = other_animation.get('task', [])
        if len(other_task) < 7 or other_task[0] != 'woodcutting':
            target = other.get('work_target')
            if target and target.get('kind') == 'tree' and other.get('production_status') == 'Waiting for a route':
                claimed.add(tuple(target[k] for k in ('kind', 'x', 'y')))
            continue
        owner = settler_building(town, index)
        if owner is None:
            continue
        camp = town['buildings'][owner]
        if not completed(camp) or other_task[:4] != ['woodcutting', camp['kind'], camp['x'], camp['y']]:
            continue
        if (other_animation.get('phase') not in ('walking', 'working', 'returning', 'depositing')
                and not other.get('pending_output') and not other_animation.get('route_pending')):
            continue
        target = tuple(other_task[4:7])
        # Older saves can contain duplicate targets; the first worker keeps it.
        if target != previous or index < worker_index:
            claimed.add(target)
    available = [tree for tree in trees if tree.get('supply', 1) > 0
                 and (tree['kind'], tree['x'], tree['y']) not in claimed]
    retained = next((tree for tree in available
                     if (tree['kind'], tree['x'], tree['y']) == previous), None)
    if retained is not None and (worker.get('pending_output') or animation.get('phase') != 'waiting'):
        return retained
    if previous is not None and worker.get('pending_output'):
        # The normal delivery fallback finishes carried wood before retargeting.
        return retained
    walking = walking if walking is not None else town
    spatial = walking.get('_spatial_index')
    blocked = (spatial.blocked(walking) if spatial is not None else
               occupied_tiles(walking['buildings'] + walking['map_resources'] + walking.get('npcs', [])))
    position = work_cycles.starting_position(town, walking, worker, worker_index, blocked)
    if position is None:
        return None
    bounds = work_cycles.walking_bounds(town, walking)
    roads = {(r['x'], r['y']) for r in walking.get('roads', []) if completed(r)}
    candidates = sorted(available, key=lambda tree: (
        tree is not retained, abs(tree['x'] - building['x']) + abs(tree['y'] - building['y'])))
    for tree in candidates:
        try:
            path = work_cycles.route(position, tree, blocked, walking.get('map_size', 120),
                                     walking.get('terrain', []), bounds=bounds, roads=roads,
                                     fences=barns.fences(walking['buildings'], walking.get('map_size', 120)))
        except RoutePending:
            worker['work_target'] = tree
            raise
        if path is not None:
            return tree
    return None


def work_assignment(town, job, worker_index, spatial=None, reservations=None, walking=None):
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
                    return b, orchards.choose_tree(town, b, worker_index) if job == 'orcharding' else b
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
            occupied = occupied_tiles(town['buildings'] + town.get('npcs', []))
            nearby = work_cycles.fishing_water(b, nearby, occupied, reach, depths=depths)
        if nearby or job == 'farming':
            if job == 'farming':
                acres = [a for a in town['buildings'] if a['kind'] == 'acre' and completed(a)
                         and a.get('farm_index') == farm_index
                         and max(abs(a['x'] - b['x']), abs(a['y'] - b['y'])) <= RESOURCE_REACH]
                if assigned_acre is not None:
                    if assigned_acre not in acres:
                        return None
                    acres = [assigned_acre]
                nearby = [farming.choose_field(town, b, acres or [b], worker_index, reservations)]
            target = (woodcutting_source(town, b, nearby, worker_index, walking) if job == 'woodcutting' else
                      min(nearby, key=lambda r:
                          (r.get('supply', 1) <= 0, -depths.get((r['x'], r['y']), 1) if boating else 0,
                           (-1 if boating else 1) * (abs(r['x'] - b['x']) + abs(r['y'] - b['y'])))))
            workplaces.append((b, target))
    if not workplaces:
        return None
    assignment = workplaces[worker_index % len(workplaces)]
    owner_index = next(i for i, b in enumerate(town['buildings']) if b is assignment[0])
    slot_index = town['villagers'][worker_index].get('building_index') if assigned_acre is not None else owner_index
    assigned = assigned_settlers(town, slot_index)
    if worker_index in assigned and assigned.index(worker_index) >= settler_capacity(assigned_acre or assignment[0]):
        return None
    return assignment if assignment[1] is not None else None


def work_source(town, job, worker_index, reservations=None):
    assignment = work_assignment(town, job, worker_index, reservations=reservations)
    return assignment[1] if assignment else None


def building_level(building):
    return 1 if building['kind'] == 'acre' else building.get('level', 1)


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
    if building['kind'] == 'acre' or building['kind'] in economy.STORAGE_UPGRADE_KINDS:
        return 0
    return .25 * (building_level(building) - 1)



def new_town(name, world=None):
    town = {'map_size': MAP_SIZE, 'name': name, 'wood': 65, 'food': 40, 'coins': 20, 'level': 1,
            'wood_chain_version': 1, 'worker_capacity_version': 1, 'grain_chain_version': 1, 'acre_upgrade_version': 1,
            'harvested': 0, 'achievements': [],
            'player': {'x': 5., 'y': 8., 'facing': 1},
            'map_resources': map_resources([{'x': 5, 'y': 5}]),
            'buildings': [{'kind': 'hall', 'x': 5, 'y': 5, 'footprint': list(BUILDING_FOOTPRINTS['hall'])}],
            'villagers': [{'name': n, 'job': 'idle', 'woodcutting': i % 3,
                           'farming': (i + 1) % 3, 'fishing': 0} for i, n in enumerate(
                               VILLAGER_NAMES[:5])]}
    if world and 'shared_map' in world:
        site = dict(world['town_sites'][len(world['towns'])])
        town.update(world['shared_map'])
        town['town_origin'] = site
        town['buildings'] = [{'kind': 'hall', **site, 'footprint': list(BUILDING_FOOTPRINTS['hall'])}]
        town['player'] = {'x': float(site['x']), 'y': float(site['y'] + BUILDING_FOOTPRINTS['hall'][1]), 'facing': 1}
    else:
        ensure_terrain(town, new=True)
        ensure_npcs(town)
    for index, villager in enumerate(town['villagers']):
        economy.ensure_talents(villager, index)
    if world and world.get('sandbox'):
        town['sandbox'] = True
        town['level'] = HALL_UPGRADES[-1]['level']
        for villager in town['villagers']:
            villager['sandbox'] = True
    return town


def capacity(town):
    return sum(economy.storage_size(b) for b in town['buildings'] if completed(b))


def completed(building):
    # Buildings from older saves are already finished.
    return 'construction' not in building


def building_costs(town, spec):
    return {} if town.get('sandbox') else economy.costs_of(spec)


def construction_blocked(town):
    spatial = town.get('_spatial_index')
    if spatial is not None:
        blocked = spatial.blocked(town)
    else:
        blocked = occupied_tiles(town['buildings'] + town['map_resources'])
        blocked.update((n['x'], n['y']) for n in town.get('npcs', []))
    # Completed fields are walkable, including around farms awaiting upgrades.
    # Subtraction leaves the spatial index's cached obstacles unchanged.
    return blocked.difference((b['x'], b['y']) for b in town['buildings']
                              if b['kind'] == 'acre' and completed(b))


def construction_route(town, start, target, road_mode='departure'):
    size = town.get('map_size', MAP_SIZE)
    blocked = construction_blocked(town)
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
    path = routes.route(start, target, blocked, size, terrain, roads=roads, road_mode=road_mode,
                        diagonal=True, fences=barns.fences(town['buildings'], size))
    return None if path is None else [start] + [(p['x'], p['y']) for p in path]


def logistics_route(town, start, target):
    return construction_route(town, start, target, road_mode='arrival')


def construct(town, landscape, workers=None, elapsed=TICK_SECONDS):
    if town.get('sandbox'):
        for site in town['buildings'] + town.get('roads', []):
            construction = site.pop('construction', None)
            if construction is None:
                continue
            if 'upgrade' in construction:
                site.update(construction['upgrade'])
            elif site.get('kind') == 'hall':
                upgrade = hall_upgrade(town)
                if upgrade:
                    town['level'] = upgrade['level']
                    town['coins'] += upgrade['coins']
                    economy.record_activity(town, 'coins', upgrade['coins'], 'in')
                town['upgrade_remaining'] = 0
        for villager in town['villagers']:
            villager.pop('construction_target', None)
            villager.pop('construction_arrived', None)
            villager.pop('construction_walk', None)
        return
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
    sites = town['buildings'] + town.get('roads', [])
    if not workers or not any(not completed(site) for site in sites):
        for villager in town['villagers']:
            for key in ('construction_target', 'construction_arrived', 'construction_walk'):
                villager.pop(key, None)
        return
    blocked = construction_blocked(landscape)
    size, terrain = landscape.get('map_size', MAP_SIZE), landscape.get('terrain', [])
    roads = {(r['x'], r['y']) for r in landscape.get('roads', []) if completed(r)}
    fences = barns.fences(landscape['buildings'], size)
    def open_step(tile):
        x, y = tile
        return (0 <= x < size and 0 <= y < size and (x, y) not in blocked
                and (not terrain or (terrain[y][x] and terrain[y][x] != 'v')))
    for index, villager in enumerate(town['villagers']):
        villager.pop('construction_arrived', None)
        if index not in workers:
            villager.pop('construction_target', None)
            villager.pop('construction_walk', None)
    # Dedicated builders claim sites before residents who are helping while idle.
    for index in sorted(workers, key=lambda i: town['villagers'][i]['job'] != 'building'):
        villager = town['villagers'][index]
        with routes.worker(villager, 'construction'):
            position = villager.get('construction_position',
                                    {'x': hall['x'] - 2 + index % 5,
                                     'y': hall['y'] + 3 + index // 5})
            start = (position['x'], position['y'])
            previous = villager.pop('construction_target', None)
            walk = villager.get('construction_walk')
            # Keep the claimed task even when another site is inserted before it.
            ordered = sorted(enumerate(sites), key=lambda entry:
                             not (previous and all(entry[1].get(k, 'road') == v for k, v in previous.items())))
            for site_index, site in ordered:
                if completed(site) or site_index in claimed:
                    continue
                if index in helpers and site_index != helpers[index]:
                    continue
                target = {'kind': site.get('kind', 'road'), 'x': site['x'], 'y': site['y']}
                if 'footprint' in site:
                    target['footprint'] = site['footprint']
                retained = (walk and walk['target'] == target and walk.get('job') == villager['job']
                            and tuple(walk['path'][walk['step']]) == start)
                villager['construction_target'] = target
                claimed.add(site_index)
                path = walk['path'] if retained else construction_route(landscape, start, site)
                if path is None:
                    claimed.discard(site_index)
                    villager.pop('construction_target', None)
                    continue
                if not retained:
                    walk = {'target': target, 'job': villager['job'],
                            'path': [list(p) for p in path], 'step': 0}
                # Replan only when a step actually becomes impassable. Road edits
                # change walking time immediately but do not discard a valid route.
                if walk['step'] + 1 < len(path) and (not open_step(path[walk['step'] + 1])
                        or barns.crosses(start, path[walk['step'] + 1], fences)):
                    routes.priority = 0
                    path = construction_route(landscape, start, site)
                    if path is None:
                        continue
                    walk = {'target': target, 'job': villager['job'],
                            'path': [list(p) for p in path], 'step': 0}
                villager['construction_walk'] = walk
                claimed.add(site_index)
                # A stable coordinate target avoids stale indices after destruction.
                villager['construction_target'] = target
                path = walk['path']
                remaining, reached = elapsed, walk['step']
                travelling = reached + 1 < len(path)
                while reached + 1 < len(path):
                    if (not open_step(path[reached + 1])
                            or barns.crosses(path[reached], path[reached + 1], fences)):
                        # Leave the worker at the last reachable waypoint; the next
                        # update will find a detour from there.
                        break
                    tile = tuple(path[reached])
                    destination = path[reached + 1]
                    duration = math.hypot(destination[0]-tile[0], destination[1]-tile[1]) / (1.8 * (1.5 if tile in roads else 1))
                    if remaining < duration:
                        break
                    remaining -= duration
                    reached += 1
                walk['step'] = reached
                step = path[reached]
                villager['construction_position'] = {'x': step[0], 'y': step[1]}
                if travelling:
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
                            economy.record_activity(town, 'coins', upgrade['coins'], 'in')
                    villager.pop('construction_target', None)
                    villager.pop('construction_arrived', None)
                    villager.pop('construction_walk', None)
                break
            if 'construction_target' not in villager:
                villager.pop('construction_walk', None)


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


def harvest_crop(town, villager, building, source):
    """Reserve gathered crops once; they become available after the farm drop-off."""
    output = farming.field_state(source).get('crop', source.get('crop', 'vegetables'))
    room = farming.harvest_room(town, building, source)
    if room <= 1e-9:
        villager['production_status'] = ('Production cap reached' if economy.production_room(town, output) <= 0
                                         else 'Workplace buffer full')
        return False
    bonus = production_bonus(building) + npcs.tool_bonus(villager, 'farming')
    if source['kind'] == 'acre':
        bonus += production_bonus(source)
    field = farming.field_state(source)
    if 'yield_remaining' not in field:
        field['yield_remaining'] = farming.HARVEST_YIELDS.get(output, farming.HARVEST_YIELD) * (1 + bonus) * economy.efficiency(villager, 'farming')
        if economy.quantity(town, 'manure') >= .1:
            economy.spend(town, {'manure': .1})
            field['yield_remaining'] *= 1.25
    amount = min(room, field['yield_remaining'])
    field['yield_remaining'] -= amount
    if field['yield_remaining'] <= 1e-9:
        source['field'] = {'stage': 'unplowed', 'elapsed': 0, 'progress': 0,
                           'cycle': field['cycle'] + 1}
        work_cycles.finish_harvest(villager['work_animation'], source)
    logistics.record_output(town, building, output, amount, villager)
    villager['production_status'] = 'Working'
    return True


def advance_farmer(town, walking, villager, building, source, index, seconds, reservations=None):
    """Chain field actions within the tick and retain one continuous playback."""
    if villager.get('logistics_trip', {}).get('operation') == 'collect':
        logistics.cancel_trip(town, villager)
    remaining, timeline = seconds, []
    while remaining > 1e-9:
        villager['work_target'] = source
        animation = villager.get('work_animation', {})
        watering = farming.field_state(source)['stage'] == 'thirsty' and (
            villager.get('watering_round') == farming.watering_round(building, source)
            or animation.get('waterReady') or animation.get('phase') in
            ('fetching_water', 'collecting_water', 'returning_water'))
        if not villager.get('pending_output') and not watering:
            crop = farming.field_state(source).get('crop', source.get('crop', 'vegetables'))
            if timeline and economy.local_room(building, crop) <= 1e-9:
                # Wait for a hauler or consumption to free crop storage after
                # this confirmed farm drop-off.
                villager['production_status'] = 'Workplace buffer full'
                break
            if logistics.worker(town, walking, villager, building, logistics_route, remaining):
                if not villager.get('logistics_trip'):
                    work_cycles.pause_farming(villager, source, remaining)
                    paused = villager.get('work_animation', {}).get('playback', [])
                    timeline.extend(dict(sample, time=sample['time'] + seconds - remaining)
                                    for sample in paused)
                break
        if villager.get('logistics_trip'):
            villager.pop('work_animation', None)
            break
        delivered, unused = work_cycles.advance_farming(town, walking, villager, building, source, index, remaining)
        animation = villager.get('work_animation')
        if not animation:
            break
        if delivered:
            logistics.drop_off(town, building, villager)
            villager['logistics_position'] = {key: animation[key] for key in ('x', 'y')}
        if animation['phase'] == 'harvest_ready' and farming.field_state(source)['stage'] == 'ripe':
            harvested = harvest_crop(town, villager, building, source)
        else:
            harvested = False
            villager['production_status'] = farming.status(source, animation)
        timeline.extend(dict(sample, time=sample['time'] + seconds - remaining)
                        for sample in animation['playback'])
        if unused <= 1e-9 or (animation['phase'] == 'harvest_ready' and not harvested):
            break
        remaining = unused
        # The completed action has released its acre. Select the next eligible
        # field immediately, including another harvest while there is room.
        source = work_source(town, 'farming', index, reservations)
        if source is None:
            break
    animation = villager.get('work_animation')
    if timeline and animation and not villager.get('logistics_trip'):
        if timeline[-1]['time'] < seconds:
            timeline.append(dict(timeline[-1], time=seconds))
        animation['playback'] = timeline


def tick(world):
    with routes.update(world):
        _tick(world)


def _tick(world):
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
        ensure_animal_names(town)
        ensure_resources(town)
        economy.ensure_inventory(town)
        resource_activity.advance(town, TICK_SECONDS)
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
        farming.grow(town, TICK_SECONDS)
        orchards.grow(town, TICK_SECONDS)
        landscape = world.get('shared_map', town)
        economy.player_tick(town, landscape, capacity(town))
        for barn_index, barn in enumerate(town['buildings']):
            if barn['kind'] != 'barn' or ('construction' in barn and not barn['construction'].get('upgrade')):
                continue
            keepers = sum(town['villagers'][i]['job'] == 'husbandry'
                          and not town['villagers'][i].get('construction_target')
                          for i in assigned_settlers(town, barn_index)[:settler_capacity(barn)])
            logistics.advance_barn(town, barn, TICK_SECONDS, keepers)
        barn_cycles = {}
        butcher_cycles = set()
        farm_assignments, farm_reservations = {}, {}
        # Reserve every farmer's first task before anyone chains a second one.
        # Otherwise an early worker could claim an acre before its tender has
        # received any of this tick's time.
        for index, villager in enumerate(town['villagers']):
            if villager['job'] == 'farming' and not villager.get('construction_target'):
                assignment = work_assignment(town, 'farming', index, town_spatial, farm_reservations)
                farm_assignments[index] = assignment
                if assignment and not farming.claimed(town, assignment[1], index, farm_reservations):
                    farm_reservations[index] = assignment[1]
        # Haulers supply workplaces before this cycle's production consumes inputs.
        for index in sorted(range(len(town['villagers'])), key=lambda i: town['villagers'][i]['job'] != 'hauling'):
            villager = town['villagers'][index]
            with routes.worker(villager):
                job = villager['job']
                if villager.get('construction_target'):
                    villager['work_target'] = None
                    villager['production_status'] = 'Helping with construction'
                    continue
                assignment = (farm_assignments.get(index) if job == 'farming' else
                              work_assignment(town, job, index, town_spatial, walking=walking) if job != 'idle' else None)
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
                    animation = villager.get('work_animation')
                    if job == 'woodcutting' and animation:
                        villager['logistics_position'] = {key: animation[key] for key in ('x', 'y')}
                    villager.pop('work_animation', None)
                    villager.pop('watering_round', None)
                    logistics.cancel_trip(town, villager)
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
                                else:
                                    villager['production_status'] = 'Waiting for an available tree'
                    continue
                building = assignment[0]
                if job == 'orcharding':
                    orchards.advance(town, walking, villager, building, source, index, TICK_SECONDS)
                    continue
                if job == 'hauling':
                    logistics.haul(town, walking, villager, building, logistics_route, TICK_SECONDS)
                    continue
                if job == 'trading':
                    logistics.merchant(town, walking, villager, building, logistics_route, TICK_SECONDS)
                    continue
                if job == 'farming':
                    advance_farmer(town, walking, villager, building, source, index, TICK_SECONDS, farm_reservations)
                    farm_reservations.pop(index, None)
                    continue
                if job == 'butchery' and building.get('butchering'):
                    # One butcher owns the animal; helpers must not duplicate its trip.
                    crew = [i for i in assigned_settlers(town, villager['building_index'])[:settler_capacity(building)]
                            if town['villagers'][i]['job'] == 'butchery'
                            and not town['villagers'][i].get('construction_target')
                            and not economy.qualification(town['villagers'][i], 'butchery')]
                    if index != crew[0]:
                        animation = villager.pop('work_animation', None)
                        if animation:
                            villager['logistics_position'] = {k: animation[k] for k in ('x', 'y') if k in animation}
                        villager['production_status'] = 'Another butcher is handling the animal'
                        continue
                    logistics.cancel_trip(town, villager)
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
                        or economy.local_room(building, boat_output) <= 1e-9
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
                if routes.is_waiting(villager) and job != 'woodcutting':
                    villager['production_status'] = 'Waiting for a route'
                    continue
                if finishing_output:
                    villager['production_status'] = 'Returning gathered goods'
                    continue
                if (building['kind'] == 'barn' and barn_cycles.get(id(building)) == 'Working'
                        and not villager.get('logistics_trip')):
                    # Each keeper trains on its round; the herd clock advances separately.
                    villager['production_status'] = 'Working'
                    economy.practice(villager, job, TICK_SECONDS)
                    continue
                if (job != 'farming' and not (job == 'butchery' and building.get('butchering'))
                        and (not boating or villager.get('logistics_trip') or not work_cycles.boat_load(town, building))):
                    if logistics.worker(town, walking, villager, building, logistics_route, TICK_SECONDS):
                        continue
                productive_seconds = TICK_SECONDS
                wood_status = 'Working'
                if job == 'woodcutting':
                    animation = villager.get('work_animation', {})
                    playback = animation.get('playback', [])
                    productive_seconds = sum(b['time'] - a['time'] for a, b in zip(playback, playback[1:])
                                             if a['phase'] == 'working')
                    wood_status = {'walking': 'Walking to tree', 'returning': 'Returning gathered wood',
                                   'depositing': 'Unloading wood', 'waiting': 'Waiting for a route'}.get(animation.get('phase'), 'Working')
                    if productive_seconds <= 1e-9:
                        villager['production_status'] = wood_status
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
                            animation = villager.get('work_animation', {})
                            task = building['butchering']
                            villager['production_status'] = {'walking': 'Walking to butcher shop' if task.get('stage') == 'working' else 'Walking to barn',
                                'finding_animal': 'Finding the animal', 'dragging': 'Dragging animal to butcher shop',
                                'waiting': 'Waiting for a route'}.get(animation.get('phase'), 'Walking to butcher shop')
                        else:
                            butcher_cycles.add(marker)
                            playback = villager.get('work_animation', {}).get('playback')
                            seconds = sum(b['time'] - a['time'] for a, b in zip(playback, playback[1:])
                                          if a['phase'] == 'working') if playback else TICK_SECONDS
                            villager['production_status'] = logistics.butcher_animal(town, building, villager, seconds)
                        continue
                    if building['kind'] in {'barn', 'stable'}:
                        marker = id(building)
                        if marker in barn_cycles:
                            villager['production_status'] = (barn_cycles[marker] if building['kind'] == 'barn'
                                                             else 'Barn already tended this cycle')
                            if building['kind'] == 'barn' and villager['production_status'] == 'Working':
                                economy.practice(villager, job, TICK_SECONDS)
                            continue
                    villager['production_status'] = logistics.produce(town, building, villager, TICK_SECONDS)
                    if building['kind'] in {'barn', 'stable'}:
                        barn_cycles[marker] = villager['production_status']
                        if building['kind'] == 'stable' and villager['production_status'] == 'Working':
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
                storage_room = economy.local_room(building, output)
                room = min(storage_room, cap_room)
                if boating:
                    room = min(room, economy.fishing_boat_capacity(building) - work_cycles.boat_load(town, building))
                if room <= 1e-9:
                    if storage_room <= 1e-9:
                        building['buffer_full'] = True
                    villager['production_status'] = 'Workplace buffer full'
                    continue
                caught = True
                if job == 'fishing':
                    caught = secrets.randbelow(10) != 0
                    villager['fishing_catch'] = {'attempt': villager.get('fishing_catch', {}).get('attempt', 0) + 1, 'caught': caught}
                rate = economy.WOOD_PER_CYCLE * productive_seconds / TICK_SECONDS if job == 'woodcutting' else 1
                bonus = production_bonus(building) + npcs.tool_bonus(villager, job)
                if source['kind'] == 'acre':
                    bonus += production_bonus(source)
                upgrades = building.get('profession_upgrades', [])
                bonus += .25 * sum(u in upgrades for u in ('nets', 'wool_nets'))
                if job == 'fishing' and work_cycles.fishing_boating(town, building, index):
                    bonus += .25 * max(0, economy.fishing_boat_level(building) - 1)
                amount = min(room, rate * (1 + bonus) * economy.efficiency(villager, job))
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
                if job != 'farming':
                    economy.practice(villager, job, productive_seconds)
                villager['production_status'] = ('Waiting for a route' if routes.is_waiting(villager) else
                                                 wood_status if job == 'woodcutting' else 'Working' if caught else 'No catch — trying again')
        feed_settlers(town)
        if (not town.get('sandbox') and len(town['villagers']) < housing_capacity(town) and town['food'] >= 20
                and not town.get('arrival_delay_seconds')):
            economy.eat(town, 10)
            town['villagers'].append({'name': next_villager_name(town),
                                     'job': 'idle', 'woodcutting': 0, 'farming': 0, 'fishing': 0})
            economy.ensure_talents(town['villagers'][-1], len(town['villagers']) - 1)
        fill_cottages(town)
        if town['harvested'] >= 30 and 'Timber roots' not in town['achievements']:
            town['achievements'].append('Timber roots')


def linked_buildings(town, index):
    building = town['buildings'][index]
    return [(i, b) for i, b in enumerate(town['buildings'])
            if i == index or (building['kind'] == 'farm' and b['kind'] == 'acre'
                              and b.get('farm_index') == index)
            or (building['kind'] == 'orchard' and b['kind'] == 'orchard_tree'
                and b.get('orchard_index') == index)]


def validate_acre(town, farm_index, x, y):
    if (type(farm_index) is not int or not 0 <= farm_index < len(town['buildings'])
            or town['buildings'][farm_index]['kind'] != 'farm'
            or not completed(town['buildings'][farm_index])):
        raise ValueError('Select a farm before building an acre.')
    farm = town['buildings'][farm_index]
    if max(abs(x - farm['x']), abs(y - farm['y'])) > RESOURCE_REACH:
        raise ValueError('Place acres within 3 tiles of their farm in each direction.')


def validate_orchard_tree(town, orchard_index, x, y):
    if (type(orchard_index) is not int or not 0 <= orchard_index < len(town['buildings'])
            or town['buildings'][orchard_index]['kind'] != 'orchard'
            or not completed(town['buildings'][orchard_index])):
        raise ValueError('Select a completed orchard before planting a fruit tree.')
    orchard = town['buildings'][orchard_index]
    if max(abs(x - orchard['x']), abs(y - orchard['y'])) > orchards.REACH:
        raise ValueError('Plant fruit trees within 3 tiles of their orchard in each direction.')


def validate_building_range(town, x, y):
    hall = next((b for b in town['buildings'] if b['kind'] == 'hall'), None)
    reach = building_range(town)
    if hall is None or abs(x - hall['x']) + abs(y - hall['y']) > reach:
        raise ValueError(f'Build within {reach} tile steps of your town hall.')


def validate_footprint(town, building, moving=(), world=None):
    tiles = footprint_tiles(building)
    obstacles = [b for i, b in enumerate(town['buildings']) if i not in moving]
    pen_obstacles = obstacles + ([b for owner in world['towns'].values() if owner is not town
                                 for b in owner['buildings']]
                                if world and 'shared_map' in world else [])
    pens = [b for b in pen_obstacles if b['kind'] == 'barn']
    if any(max(abs(x - barn['x']), abs(y - barn['y'])) <= RESOURCE_REACH
           for x, y in tiles for barn in pens):
        raise ValueError('Keep buildings outside the barn\'s fenced range.')
    if building['kind'] == 'barn' and any(
            max(abs(x - building['x']), abs(y - building['y'])) <= RESOURCE_REACH
            for x, y in occupied_tiles(pen_obstacles)):
        raise ValueError('Choose a barn range without other buildings inside it.')
    occupied = occupied_tiles(obstacles)
    if len(tiles) > 1:
        owners = world['towns'].values() if world and 'shared_map' in world else [town]
        occupied.update(occupied_tiles(r for owner in owners for r in owner.get('roads', [])))
    resources = occupied_tiles(r for r in town['map_resources'] if r['kind'] not in {'berry_bush', 'herb_patch'})
    locals_ = occupied_tiles(town.get('npcs', []))
    size, terrain = town.get('map_size', MAP_SIZE), town.get('terrain', [])
    for x, y in tiles:
        if not (0 <= x < size and 0 <= y < size):
            raise ValueError('Choose a tile inside your town for the full building footprint.')
        if (x, y) in occupied:
            raise ValueError('That tile is already occupied.')
        if (x, y) in resources:
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        if (x, y) in locals_:
            raise ValueError('A local is standing here. Choose another tile.')
        if terrain and terrain[y][x] == 'v':
            raise ValueError('Choose a tile inside the valley boundary.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before building here.')
        validate_building_range(town, x, y)
        if world and 'shared_map' in world:
            validate_placement(world, town, {'x': x, 'y': y})


def clear_building_forage(town, buildings):
    tiles = occupied_tiles(buildings)
    # Keep the shared landscape list so every town and chunk sees the removal.
    town['map_resources'][:] = [r for r in town['map_resources']
                               if r['kind'] not in {'berry_bush', 'herb_patch'} or (r['x'], r['y']) not in tiles]


def migrate_footprints(world):
    """Expand clear saved sites, preserving crowded legacy sites until moved."""
    towns = list(world['towns'].values())
    for town in towns:
        landscape = world.get('shared_map', town)
        for building in town.get('buildings', []):
            if building['kind'] not in BUILDING_FOOTPRINTS:
                continue
            if 'footprint' in building and (building['kind'] != 'hall'
                    or tuple(building['footprint']) != (2, 2)):
                continue
            candidate = dict(building, footprint=BUILDING_FOOTPRINTS[building['kind']])
            others = [b for t in towns for b in t.get('buildings', []) if b is not building]
            obstacles = others + list(landscape.get('map_resources', [])) + list(landscape.get('npcs', []))
            obstacles += [r for t in towns for r in t.get('roads', [])]
            tiles = footprint_tiles(candidate)
            terrain, size = landscape.get('terrain', []), landscape.get('map_size', MAP_SIZE)
            clear = (not landscape.get('chunked') or bool(terrain)) and all(0 <= x < size and 0 <= y < size and (not terrain or terrain[y][x] and terrain[y][x] != 'v')
                        and not any(abs(t['player']['x'] - x) < .68 and abs(t['player']['y'] - y) < .68
                                for t in towns if 'player' in t)
                        for x, y in tiles)
            building['footprint'] = list(candidate['footprint'] if clear and not tiles.intersection(occupied_tiles(obstacles)) else building.get('footprint', (1, 1)))


def command(town, data, world=None):
    if world and world.get('shared_map', {}).get('chunked'):
        attach_chunks(world, SAVE.with_name(SAVE.stem + '_chunks'))
        bind_map(world)
    ensure_resources(town)
    action = data.get('action')
    if action == 'set_decoration_orientation':
        index, orientation = data.get('index'), data.get('orientation')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a decoration first.')
        building = town['buildings'][index]
        if (building['kind'] not in economy.DECORATIONS or building.get('construction')
                or type(orientation) is not int or orientation not in range(4)):
            raise ValueError('Choose a completed decoration and a direction.')
        building['orientation'] = orientation
        return {}
    if action == 'set_fence_orientation':
        index, orientation = data.get('index'), data.get('orientation')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a barn first.')
        building = town['buildings'][index]
        if not barns.active(building) or type(orientation) is not int or orientation not in range(4):
            raise ValueError('Choose a completed barn and a fence orientation.')
        size = town.get('map_size', MAP_SIZE)
        candidate = dict(building, fence_orientation=orientation)
        actual, _, _, segments = barns.pen(candidate, size)
        if actual != orientation:
            raise ValueError('The gate must face into the map.')
        landscape = world.get('shared_map', town) if world else town
        for npc in landscape.get('npcs', []):
            position = npc.get('routine') or npc
            if barns.blocked(position['x'], position['y'], segments):
                raise ValueError('Wait until everyone has cleared the fence before turning the gate.')
        towns = world['towns'].values() if world and 'shared_map' in world else [town]
        for owner in towns:
            positions = [owner.get('player', {})]
            for worker in owner.get('villagers', []):
                positions.append(worker.get('work_animation') or worker.get('logistics_position')
                                 or worker.get('construction_position') or {})
            if any('x' in p and barns.blocked(p['x'], p['y'], segments) for p in positions):
                raise ValueError('Wait until everyone has cleared the fence before turning the gate.')
        building['fence_orientation'] = orientation
        return {}
    if action in {'join_lanterns', 'leave_lanterns', 'lanterns_walk', 'lanterns_collect'}:
        return lanterns.change(town, data, world)
    if world and lanterns.inside(world, town) and action in {'walk', 'fish', 'chop', 'gather', 'mine', 'join_quarry'}:
        raise ValueError('Leave the courier path before exploring the valley or quarry.')
    if action in {'join_quarry', 'leave_quarry', 'quarry_walk', 'quarry_interact'}:
        return quests.change(town, data, world)
    if world and quests.inside(world, town) and action in {'walk', 'fish', 'chop', 'gather', 'mine'}:
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
    if action == 'configure_resource_trade':
        return logistics.configure_resource_trade(town, data)
    if action == 'configure_market_sale':
        return logistics.configure_market_sale(town, data)
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
        costs = building_costs(town, {'wood': 20, 'planks': 10})
        if not economy.afford(town, costs):
            raise ValueError('An exploration boat costs 20 wood and 10 planks.')
        economy.spend(town, costs)
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
        obstacles = (town['buildings'] + [r for r in town['map_resources'] if r['kind'] not in {'berry_bush', 'herb_patch'}]
                     + town.get('npcs', []))
        if (terrain and terrain[y][x] == 'v') or (x, y) in occupied_tiles(obstacles):
            raise ValueError('Place roads on open ground.')
        if not any(r['x'] == x and r['y'] == y for r in town.get('roads', [])):
            road = {'x': x, 'y': y}
            if not town.get('sandbox'):
                road['construction'] = {'progress': 0, 'total': ROAD_BUILD_SECONDS}
            town.setdefault('roads', []).append(road)
        clear_building_forage(town, [{'x': x, 'y': y}])
        return {}
    if world and 'shared_map' in world:
        if action == 'walk':
            return move_player(walking_map(world, town, local=True), data)
        if action in {'build', 'move'}:
            validate_placement(world, town, data)
    if action == 'walk':
        return move_player(town, data)
    if action in {'fish', 'chop', 'gather', 'mine', 'stop_activity'}:
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
        if any(r['kind'] not in {'berry_bush', 'herb_patch'} and r['x'] == x and r['y'] == y for r in town['map_resources']):
            raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
        player = ensure_player(town)
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('Walk away from this tile before building here.')
        validate_building_range(town, x, y)
        validate_footprint(town, {'kind': kind, 'x': x, 'y': y,
                                 'footprint': BUILDING_FOOTPRINTS.get(kind, (1, 1))}, world=world)
        spec = economy.BUILDINGS.get(kind, {'wood': COSTS[kind]})
        if town['level'] < spec.get('level', 1):
            raise ValueError(f"Requires town hall level {spec['level']}.")
        costs = building_costs(town, spec)
        if not economy.afford(town, costs):
            raise ValueError('Missing building materials: ' + ', '.join(costs))
        if kind == 'acre':
            validate_acre(town, data.get('farm_index'), x, y)
        if kind == 'orchard_tree':
            validate_orchard_tree(town, data.get('orchard_index'), x, y)
        economy.spend(town, costs)
        building = {'kind': kind, 'x': x, 'y': y,
                    'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        if kind in BUILDING_FOOTPRINTS:
            building['footprint'] = list(BUILDING_FOOTPRINTS[kind])
        if kind in ('acre', 'orchard_tree') or kind in economy.DECORATIONS:
            building.pop('construction')
        if kind == 'acre':
            building['farm_index'] = data['farm_index']
            farming.field_state(building)
        if kind == 'orchard_tree':
            building['orchard_index'] = data['orchard_index']
            orchards.tree_state(building)
        town['buildings'].append(building)
        clear_building_forage(town, [building])
    elif action == 'move':
        index, x, y = data.get('index'), data.get('x'), data.get('y')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        size = town.get('map_size', MAP_SIZE)
        if type(x) is not int or type(y) is not int or not (0 <= x < size and 0 <= y < size):
            raise ValueError('Choose a tile inside your town.')
        moved = town['buildings'][index]
        group = linked_buildings(town, index)
        moving_indices = {i for i, _ in group}
        dx, dy = x - moved['x'], y - moved['y']
        if dx == 0 and dy == 0:
            raise ValueError('That tile is already occupied.')
        player = ensure_player(town)
        terrain = town.get('terrain', [])
        # Validate every destination before charging or moving any part of the farm.
        for i, building in group:
            nx, ny = building['x'] + dx, building['y'] + dy
            validate_footprint(town, dict(building, x=nx, y=ny,
                                         footprint=BUILDING_FOOTPRINTS.get(building['kind'], (1, 1))), moving_indices, world)
            if not (0 <= nx < size and 0 <= ny < size):
                raise ValueError('Choose a tile inside your town for every linked acre.')
            if any(j not in moving_indices and b['x'] == nx and b['y'] == ny
                   for j, b in enumerate(town['buildings'])):
                raise ValueError('That tile is already occupied.')
            if any(r['kind'] not in {'berry_bush', 'herb_patch'} and r['x'] == nx and r['y'] == ny for r in town['map_resources']):
                raise ValueError('Choose open grass; trees, water, and rocks occupy this tile.')
            if any(n['x'] == nx and n['y'] == ny for n in town['npcs']):
                raise ValueError('A local is standing here. Choose another tile.')
            if terrain and terrain[ny][nx] == 'v':
                raise ValueError('Choose a tile inside the valley boundary.')
            if abs(player['x'] - nx) < .68 and abs(player['y'] - ny) < .68:
                raise ValueError('Walk away from this tile before moving a building here.')
            validate_building_range(town, nx, ny)
            if world and 'shared_map' in world:
                validate_placement(world, town, {'action': 'move', 'index': i, 'x': nx, 'y': ny})
        if moved['kind'] == 'acre':
            validate_acre(town, moved.get('farm_index'), x, y)
        if moved['kind'] == 'orchard_tree':
            validate_orchard_tree(town, moved.get('orchard_index'), x, y)
        # Rearranging must remain possible even after spending the starting coins.
        if not town.get('sandbox'):
            economy.record_activity(town, 'coins', min(1, town['coins']), 'out')
            town['coins'] = max(0, town['coins'] - 1)
        moved_specs = [{k: b[k] for k in ('kind', 'x', 'y')} for _, b in group]
        for villager in town['villagers']:
            trip = villager.get('logistics_trip', {})
            if any(spec in (trip.get('workplace'), trip.get('storage')) for spec in moved_specs):
                logistics.cancel_trip(town, villager)
            if villager.get('pending_workplace') in moved_specs:
                logistics.cancel_output(town, villager)
            if villager.get('building_index') in moving_indices:
                villager.pop('work_animation', None)
        for _, building in group:
            building.update(x=building['x'] + dx, y=building['y'] + dy)
            if building['kind'] in BUILDING_FOOTPRINTS:
                building['footprint'] = list(BUILDING_FOOTPRINTS[building['kind']])
        clear_building_forage(town, [b for _, b in group])
    elif action == 'destroy':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Select a building first.')
        if town['buildings'][index]['kind'] == 'hall':
            raise ValueError('The town hall cannot be destroyed.')
        economy.ensure_inventory(town)
        group = linked_buildings(town, index)
        removed_indices = {i for i, _ in group}
        removed_specs = [{k: b[k] for k in ('kind', 'x', 'y')} for _, b in group]
        for villager in town['villagers']:
            trip = villager.get('logistics_trip', {})
            if any(spec in (trip.get('workplace'), trip.get('storage')) for spec in removed_specs):
                logistics.cancel_trip(town, villager)
            if (villager.get('pending_workplace') in removed_specs
                    or villager.get('building_index') in removed_indices):
                logistics.cancel_output(town, villager)
            if villager.get('building_index') in removed_indices:
                logistics.cancel_trip(town, villager)
        remaining = [(i, b) for i, b in enumerate(town['buildings']) if i not in removed_indices]
        new_indices = {old: new for new, (old, _) in enumerate(remaining)}
        town['buildings'][:] = [b for _, b in remaining]
        hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
        for _, removed in group:
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
            if type(assigned) is int:
                if assigned in removed_indices:
                    villager.pop('building_index')
                    villager.pop('watering_round', None)
                    villager['job'] = 'idle'
                elif assigned in new_indices:
                    villager['building_index'] = new_indices[assigned]
        for building in town['buildings']:
            orchard_index = building.get('orchard_index')
            if type(orchard_index) is int:
                building['orchard_index'] = new_indices.get(orchard_index)
            farm_index = building.get('farm_index')
            if type(farm_index) is int:
                if farm_index in removed_indices:
                    building['farm_index'] = None
                elif farm_index in new_indices:
                    building['farm_index'] = new_indices[farm_index]
    elif action == 'assign_settler':
        assign_settler(town, data.get('index'), data.get('building_index'))
    elif action == 'remove_settler':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['villagers']):
            raise ValueError('Choose a settler first.')
        logistics.cancel_output(town, town['villagers'][index])
        logistics.cancel_trip(town, town['villagers'][index])
        town['villagers'][index].pop('building_index', None)
        town['villagers'][index]['job'] = 'idle'
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
        if building['kind'] == 'orchard_tree':
            raise ValueError('Fruit trees have no upgrades. Upgrade their orchard instead.')
        if building['kind'] == 'acre':
            raise ValueError('Acres have no upgrades. Upgrade their farm instead.')
        if building['kind'] == 'well':
            raise ValueError('Wells supply water automatically and need no assigned workers.')
        if building['kind'] == 'hall':
            raise ValueError('The town hall is reserved for your controllable character.')
        level = building.get('settler_level', 1)
        max_level = len(economy.WORKER_CAPACITIES.get(building['kind'], (1, 2, 3)))
        if not completed(building) or level >= max_level or building['kind'] in economy.DECORATIONS:
            raise ValueError('Capacity cannot be upgraded at this time.')
        cost = 0 if town.get('sandbox') else 20 * level
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
            logistics.cancel_trip(town, town['villagers'][index])
            town['villagers'][index].pop('building_index', None)
            town['villagers'][index].pop('watering_round', None)
        town['villagers'][index]['job'] = job
    elif action == 'upgrade_building':
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        if building['kind'] == 'orchard_tree':
            raise ValueError('Fruit trees have no upgrades. Upgrade their orchard instead.')
        if building['kind'] == 'acre':
            raise ValueError('Acres have no upgrades. Upgrade their farm instead.')
        if building['kind'] == 'well':
            raise ValueError('Wells already supply all the water workers need.')
        if building['kind'] not in COSTS:
            raise ValueError('Use the town hall upgrade instead.')
        if building['kind'] in economy.DECORATIONS:
            raise ValueError('Decorations have no upgrades.')
        if not completed(building):
            raise ValueError('Finish construction before upgrading.')
        level = building_level(building)
        if level >= (len(economy.STOREHOUSE_CAPACITIES) if building['kind'] == 'storehouse'
                     else 4 if building['kind'] == 'home' else 3):
            raise ValueError('This building is fully upgraded.')
        costs = {'wood': COSTS[building['kind']] * level}
        if building['kind'] == 'home' and level == 3:
            costs['stone'] = economy.HOME_STONE_COST
        costs = building_costs(town, costs)
        if not economy.afford(town, costs):
            required = ' + '.join(f'{amount} {key}' for key, amount in costs.items())
            raise ValueError(f'Gather {required} to upgrade this building.')
        economy.spend(town, costs)
        building['construction'] = {'progress': 0, 'total': BUILD_SECONDS * level,
                                    'upgrade': {'level': level + 1}}
    elif action in {'set_crop', 'set_recipe', 'queue_workshop', 'cancel_workshop', 'set_animal_count', 'buy_animal', 'breed_animal', 'butcher_animal', 'profession_upgrade'}:
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['buildings']):
            raise ValueError('Choose a building first.')
        building = town['buildings'][index]
        if not completed(building):
            raise ValueError('Finish construction first.')
        if action in {'queue_workshop', 'cancel_workshop'}:
            if building['kind'] != 'workshop':
                raise ValueError('Choose a workshop.')
            queue = building.get('craft_queue', [])
            if action == 'queue_workshop':
                spec = next((r for r in economy.PROFESSION_RECIPES['workshop']
                             if r['output'] == data.get('recipe')), None)
                if spec is None:
                    raise ValueError('Choose a workshop recipe.')
                amount = data.get('amount')
                if type(amount) is not int or not 1 <= amount <= 999:
                    raise ValueError('Request between 1 and 999 items.')
                if len(queue) >= 50:
                    raise ValueError('The workshop queue is full (50 requests).')
                workers = [v for v in town['villagers'] if v.get('building_index') == index
                           and v['job'] == 'crafting' and not economy.qualification(v, 'crafting')]
                if not town.get('sandbox') and spec['level'] > 1 and not any(v.get('crafting', 0) >= spec['level'] for v in workers):
                    raise ValueError(f"Requires an assigned worker with level {spec['level']} crafting.")
                building.setdefault('craft_queue', []).append({'id': secrets.token_hex(8),
                    'recipe': spec['output'], 'amount': amount, 'remaining': amount})
                if len(building['craft_queue']) == 1:
                    logistics.advance_workshop_queue(town, building)
            else:
                position = next((i for i, order in enumerate(queue) if order['id'] == data.get('request_id')), None)
                if position is None:
                    raise ValueError('Choose a queued workshop request.')
                queue.pop(position)
                if position == 0:
                    logistics.advance_workshop_queue(town, building)
        elif action == 'set_animal_count':
            animal, count = data.get('animal'), data.get('count')
            spec = economy.ANIMALS.get(animal)
            if (building['kind'] != 'barn' or animal not in economy.ANIMAL_MEAT
                    or spec is None or town['level'] < spec['level']):
                raise ValueError('Choose an unlocked animal and a barn.')
            animals = building.get('animals', {})
            others = sum(n for key, n in animals.items() if key != animal)
            if type(count) is not int or not 0 <= count <= 4 * building_level(building) - others:
                raise ValueError('Choose an animal count within the barn capacity.')
            ensure_animal_names(town)
            ages = economy.animal_growth(building, animal)
            building.setdefault('animal_growth', {})[animal] = (ages + [0] * count)[:count]
            building.setdefault('animals', {})[animal] = count
            ensure_animal_names(town)
        elif action == 'butcher_animal':
            if building['kind'] != 'butcher':
                raise ValueError('Choose a butcher shop.')
            if building.get('butchering'):
                raise ValueError('Finish butchering the current animal first.')
            barn_index, animal = data.get('barn_index'), data.get('animal')
            if type(barn_index) is not int or not 0 <= barn_index < len(town['buildings']):
                raise ValueError('Choose a barn first.')
            barn = town['buildings'][barn_index]
            if barn['kind'] != 'barn' or not completed(barn) or animal not in economy.ANIMAL_MEAT:
                raise ValueError('Choose an animal from a completed barn.')
            ages = economy.animal_growth(barn, animal)
            adult = next((i for i, age in enumerate(ages) if age >= economy.animal_duration(animal)), None)
            if adult is None:
                raise ValueError('Only fully grown animals can be butchered.')
            if not any(v.get('building_index') == index and v['job'] == 'butchery'
                       and not economy.qualification(v, 'butchery') for v in town['villagers']):
                raise ValueError('Assign a qualified butcher first.')
            ensure_animal_names(town)
            name = barn['animal_names'][animal].pop(adult)
            del ages[adult]
            if not ages:
                ages = [0]
            barn['animals'][animal] = len(ages)
            barn.setdefault('animal_growth', {})[animal] = ages
            ensure_animal_names(town)
            building['butchering'] = {'animal': animal, 'name': name, 'amount': economy.ANIMAL_MEAT[animal],
                                      'remaining': economy.BUTCHER_SECONDS, 'id': secrets.token_hex(8),
                                      'pickup': {'kind': 'barn', 'x': barn['x'], 'y': barn['y']},
                                      'stage': 'fetching'}
        elif action == 'set_recipe':
            if building['kind'] == 'workshop':
                raise ValueError('Add a workshop request with a quantity instead.')
            recipes = economy.PROFESSION_RECIPES.get(building['kind'], [])
            spec = next((r for r in recipes if r['output'] == data.get('recipe')), None)
            if spec is None:
                raise ValueError('Choose a matching workplace recipe.')
            job = economy.BUILDINGS[building['kind']]['job']
            workers = [v for v in town['villagers'] if v.get('building_index') == index
                       and v['job'] == job and not economy.qualification(v, job)]
            if not town.get('sandbox') and spec['level'] > 1 and not any(v.get(job, 0) >= spec['level'] for v in workers):
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
            if building['kind'] not in {'acre', 'farm'} or crop not in economy.CROPS:
                raise ValueError('Choose an acre or farm and a crop.')
            if town['level'] < economy.CROPS[crop]:
                raise ValueError('Upgrade the hall to unlock this crop.')
            acres = ([building] if building['kind'] == 'acre' else
                     [a for a in town['buildings'] if a['kind'] == 'acre'
                      and a.get('farm_index') == index])
            for acre in acres:
                acre['crop'] = crop
        elif action in {'buy_animal', 'breed_animal'}:
            animal = data.get('animal')
            if action == 'breed_animal' and animal != 'horse':
                raise ValueError('Barn animals are free; only the last of a species in a barn is replaced after selection for butchery.')
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
            ensure_animal_names(town)
            ages = economy.animal_growth(building, animal)
            ages.append(0)
            building.setdefault('animal_growth', {})[animal] = ages
            building.setdefault('animals', {})[animal] = animals.get(animal, 0) + 1
            ensure_animal_names(town)
        else:
            key = data.get('upgrade')
            spec = economy.PROFESSION_UPGRADES.get(key)
            if spec is None or spec['building'] != building['kind']:
                raise ValueError('Choose a matching profession upgrade.')
            if town['level'] < spec['level'] or (not town.get('sandbox') and economy.skill(town, spec.get('skill', '')) < spec.get('xp', 0)):
                raise ValueError('Develop the profession and upgrade the town hall first.')
            if spec.get('requires') and spec['requires'] not in building.get('profession_upgrades', []):
                raise ValueError('Purchase the previous equipment tier first.')
            if key in building.get('profession_upgrades', []):
                raise ValueError('Already unlocked.')
            if not economy.afford(town, building_costs(town, spec)):
                raise ValueError('Missing profession upgrade materials.')
            economy.spend(town, building_costs(town, spec))
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
        if key not in economy.MANUAL_SELLABLE_GOODS or type(amount) is not int or not 1 <= amount <= 200:
            raise ValueError('Choose goods and a quantity from 1 to 200.')
        if not economy.afford(town, {key: amount}):
            raise ValueError('Not enough goods to sell.')
        economy.spend(town, {key: amount})
        economy.record_sale(town, key, amount)
    elif action == 'upgrade':
        hall = next(b for b in town['buildings'] if b['kind'] == 'hall')
        if town.get('upgrade_remaining') or not completed(hall):
            raise ValueError('Your hall upgrade is already underway.')
        upgrade = hall_upgrade(town)
        if not upgrade:
            raise ValueError('Your town hall is fully upgraded (level 10).')
        if not town.get('sandbox') and not economy.milestone(town, upgrade['level']):
            raise ValueError(upgrade['milestone'] + ' required to upgrade.')
        costs = building_costs(town, upgrade)
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
    if town.get('sandbox'):
        fill_cottages(town)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(ROOT / 'web'), **kwargs)

    def translate_path(self, path):
        if urlsplit(path).path == '/favicon.ico':
            return str(ROOT / 'icons' / 'favicon.ico')
        return super().translate_path(path)

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
                    ensure_animal_names(town)
                    ensure_player(walking_map(WORLD, town) if 'shared_map' in WORLD else town)
                snapshot = export_world(WORLD)
                if WORLD.get('_chunks'):
                    try:
                        landscape = WORLD['_chunks'].snapshot(keys, known, versions)
                    except ValueError as error:
                        self.reply(400, {'error': str(error)})
                        return
                    snapshot = dict(snapshot, shared_map=landscape)
                sampled_at = time.monotonic()
                clock = {'tick_seconds': TICK_SECONDS,
                         'next_tick_in': max(0, NEXT_TICK_AT - sampled_at) if NEXT_TICK_AT is not None else TICK_SECONDS}
                snapshot = dict(snapshot, simulation_clock=clock, hall_upgrades=HALL_UPGRADES, progression=economy.public_catalog(), local_stories=npcs.STORIES, quarry_catalog=quests.CATALOG,
                                hall_ready={k: economy.milestone(t, t['level'] + 1) for k, t in WORLD['towns'].items()}, world_id=ACTIVE_WORLD,
                                name=WORLDS[ACTIVE_WORLD]['name'] if ACTIVE_WORLD else None,
                                players_sampled_at=sampled_at)
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
                                        'sandbox': value['world'].get('sandbox', False),
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
                if self.path in {'/api/worlds/create', '/api/worlds/open', '/api/worlds/delete', '/api/worlds/regenerate'}:
                    deleted_chunks = None
                    if not self.is_host():
                        self.reply(403, {'error': 'Only the host at localhost can manage worlds.'})
                        return
                    if self.path == '/api/worlds/create':
                        name = data.get('name')
                        if not isinstance(name, str) or not name.strip() or len(name.strip()) > 24:
                            raise ValueError('Enter a world name of 1–24 characters.')
                        sandbox = data.get('sandbox', False)
                        if type(sandbox) is not bool:
                            raise ValueError('Choose a valid world mode.')
                        world_id = secrets.token_hex(8)
                        WORLDS[world_id] = {'name': name.strip(), 'world': new_world(
                            explorable=True, chunk_directory=SAVE.with_name(SAVE.stem + '_chunks'),
                            sandbox=sandbox), 'tokens': {}}
                        activate(world_id)
                    else:
                        world_id = data.get('world_id')
                        if not isinstance(world_id, str) or world_id not in WORLDS:
                            raise ValueError('That world no longer exists.')
                        if self.path == '/api/worlds/open':
                            activate(world_id)
                        elif self.path == '/api/worlds/regenerate':
                            entry = WORLDS[world_id]
                            if not entry['world'].get('sandbox', False):
                                raise ValueError('Only sandbox worlds can be regenerated.')
                            if data.get('confirmation') != entry['name']:
                                raise ValueError('Type the world name to confirm regeneration.')
                            # Build the replacement before discarding any old state.
                            # A new ID invalidates every client's terrain/ownership cache.
                            replacement = {'name': entry['name'], 'tokens': {},
                                'world': new_world(explorable=True,
                                    chunk_directory=SAVE.with_name(SAVE.stem + '_chunks'), sandbox=True)}
                            # Keep salted credentials privately, but recreate each
                            # registered player's village with fresh sandbox state.
                            replacement['players'] = {}
                            for key, account in entry.get('players', {}).items():
                                town_id = account['town_id']
                                old_town = entry['world']['towns'].get(town_id)
                                if old_town is None:
                                    raise ValueError('A registered player has no village. Restore it before regenerating.')
                                if town_id not in replacement['world']['towns']:
                                    replacement['world']['towns'][town_id] = new_town(
                                        old_town['name'], replacement['world'])
                                replacement['players'][key] = dict(account)
                            replacement_id = secrets.token_hex(8)
                            WORLDS[replacement_id] = replacement
                            activate(replacement_id)
                            prepare_landscapes(WORLD)
                            landscape = entry['world'].get('shared_map', {})
                            if landscape.get('chunked'):
                                deleted_chunks = SAVE.with_name(SAVE.stem + '_chunks') / str(landscape['terrain_seed'])
                            del WORLDS[world_id]
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
    migrate_footprints(WORLD)


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
            ensure_animal_names(town)
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
    global NEXT_TICK_AT
    with LOCK:
        NEXT_TICK_AT = time.monotonic() + TICK_SECONDS
    while True:
        time.sleep(max(0, NEXT_TICK_AT - time.monotonic()))
        with LOCK:
            tick(WORLD)
            save()
            # Publish the next deadline with the state produced by this tick.
            # Overruns resume now without replaying a backlog of ticks.
            NEXT_TICK_AT = max(NEXT_TICK_AT + TICK_SECONDS, time.monotonic())


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
