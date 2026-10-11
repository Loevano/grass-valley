"""Friendly locals placed on open ground in their home biome."""
import math
import random

import progression as economy


THEMES = {
    'g': [('Tobin', 'Valley traveler', 'The hall lantern is a welcome sight after a long walk.')],
    'f': [('Sylva', 'Forest ranger', 'Leave the young trees standing and the forest will always provide.')],
    'm': [('Clover', 'Meadow beekeeper', 'Follow the flowers. The bees know every quiet corner of this meadow.')],
    's': [('Saffron', 'Dune wanderer', 'Keep water close and watch how the wind shapes the sand.')],
    'r': [('Flint', 'Rock surveyor', 'Every outcrop tells a story. I am still learning to read this one.')],
    'b': [('Moss', 'Swamp herbalist', 'Willow roots love the wet ground. Look for herbs among the reeds.'),
          ('Reed', 'Marsh fisher', 'The pools are quiet, but there is plenty of life beneath the lily pads.')],
}


def ensure_npcs(town):
    if 'npcs' in town:
        return
    rng = random.Random(str(town.get('terrain_seed', 0)) + ':npcs')
    occupied = {(obj['x'], obj['y']) for obj in town['buildings'] + town['map_resources']}
    player = town.get('player')
    if player:
        occupied.add((round(player['x']), round(player['y'])))
    candidates = {biome: [] for biome in THEMES}
    for y, row in enumerate(town.get('terrain', [])):
        for x, biome in enumerate(row):
            if biome in candidates and (x, y) not in occupied and (x >= 12 or y >= 12):
                candidates[biome].append((x, y))
    town['npcs'] = []
    for biome, people in THEMES.items():
        tiles = candidates[biome]
        for name, role, dialogue in people:
            if not tiles:
                break
            x, y = tiles.pop(rng.randrange(len(tiles)))
            town['npcs'].append({'name': name, 'role': role, 'dialogue': dialogue,
                                 'biome': biome, 'x': x, 'y': y})


WORKER_TOOLS = {
    'fine_axe': {'name': 'Fine axe', 'job': 'woodcutting'},
    'fine_fishing_rod': {'name': 'Fine fishing rod', 'job': 'fishing'},
}
GIFTS = {'Sylva': 'fine_axe', 'Reed': 'fine_fishing_rod'}
BARTERERS = {'Tobin', 'Clover'}
TRADE_SECONDS = 300


def local_key(npc):
    return f"{npc['name']}:{npc['x']}:{npc['y']}"


def advance_trades(town, seconds):
    for progress in town.get('local_visits', {}).values():
        if progress.get('trade_remaining', 0):
            progress['trade_remaining'] = max(0, progress['trade_remaining'] - seconds)


def tool_bonus(villager, job):
    return .25 if WORKER_TOOLS.get(villager.get('worker_tool'), {}).get('job') == job else 0


def change(town, data):
    action = data['action']
    player = town['player']
    if action in {'give_worker_tool', 'take_worker_tool'}:
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['villagers']):
            raise ValueError('Choose one of your workers.')
        worker = town['villagers'][index]
        tools = player.get('worker_tools', {})
        if action == 'give_worker_tool':
            key = data.get('item')
            if not isinstance(key, str) or key not in WORKER_TOOLS:
                raise ValueError('Choose a known worker tool.')
            if tools.get(key, 0) < 1:
                raise ValueError('Collect this tool from a local first.')
            if worker.get('worker_tool'):
                raise ValueError('Take back this worker’s tool first.')
            tools[key] -= 1
            worker['worker_tool'] = key
        else:
            key = worker.get('worker_tool')
            if key not in WORKER_TOOLS:
                raise ValueError('This worker has no tool to return.')
            tools = player.setdefault('worker_tools', {})
            tools[key] = tools.get(key, 0) + 1
            del worker['worker_tool']
        return {}
    index = data.get('npc_index')
    locals_ = town.get('npcs', [])
    if type(index) is not int or not 0 <= index < len(locals_):
        raise ValueError('Choose a local.')
    npc = locals_[index]
    if math.hypot(player['x'] - npc['x'], player['y'] - npc['y']) > 1.8:
        raise ValueError('Walk beside the local to interact.')
    visit_key = local_key(npc)
    progress = town.get('local_visits', {}).get(visit_key, {})
    if action == 'claim_local_gift':
        key = GIFTS.get(npc['name'])
        if not key:
            raise ValueError('This local has no equipment gift.')
        if progress.get('gift_claimed'):
            raise ValueError('You already collected this local’s gift.')
        tools = player.setdefault('worker_tools', {})
        tools[key] = tools.get(key, 0) + 1
        town.setdefault('local_visits', {}).setdefault(visit_key, {})['gift_claimed'] = True
    else:
        if npc['name'] not in BARTERERS:
            raise ValueError('This local does not trade wood for food.')
        if progress.get('trade_remaining', 0) > 0:
            raise ValueError('This local is restocking. Come back later.')
        if town['wood'] < 10:
            raise ValueError('This trade needs 10 wood.')
        if economy.storage_room(town, 'vegetables') < 8:
            raise ValueError('Make storage room for 8 food first.')
        economy.spend(town, {'wood': 10})
        economy.deliver(town, 'vegetables', 8, 0, record=False)
        town.setdefault('local_visits', {}).setdefault(visit_key, {})['trade_remaining'] = TRADE_SECONDS
    return {}
