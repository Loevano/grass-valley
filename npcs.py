"""Friendly locals placed on open ground in their home biome."""
import math
import random

import progression as economy


THEMES = {
    'g': [('Tobin', 'Valley traveler', 'The hall lantern is a welcome sight after a long walk.')],
    'f': [('Sylva', 'Forest ranger', 'Leave the young trees standing and the forest will always provide.'),
          ('Bram', 'Forest toolsmith', 'A well-balanced axe saves your woodcutters a lot of effort. I forge steel and masterwork blades.')],
    'm': [('Clover', 'Meadow beekeeper', 'Follow the flowers. The bees know every quiet corner of this meadow.'),
          ('Fern', 'Meadow grower', 'Take my spare sickle for one of your farmers. A clean cut makes a generous harvest.'),
          ('Runa', 'Harvest toolsmith', 'My steel and masterwork sickles help farmers bring in more from every harvest.')],
    's': [('Saffron', 'Dune wanderer', 'Keep water close and watch how the wind shapes the sand.')],
    'r': [('Flint', 'Rock surveyor', 'Every outcrop tells a story. I am still learning to read this one.')],
    'b': [('Moss', 'Swamp herbalist', 'Willow roots love the wet ground. Look for herbs among the reeds.'),
          ('Reed', 'Marsh fisher', 'The pools are quiet, but there is plenty of life beneath the lily pads.')],
}


# Each newcomer has an offer, using the same saved visit progress as older locals.
ADDITIONAL_THEMES = {
    'g': [('Pip', 'Valley provisioner', 'A little firewood keeps my bread oven warm. I can share fresh loaves.'),
          ('Wren', 'Traveling angler', 'Take my spare rod. There are good fishing waters beyond the valley.')],
    'f': [('Hazel', 'Forest forager', 'I trade woodland berries for firewood.'),
          ('Alder', 'Forest woodworker', 'My spare axe deserves a home with a hardworking woodcutter.')],
    'm': [('Poppy', 'Meadow seed keeper', 'A spare sickle is a fine way to start a new field.'),
          ('Mabel', 'Meadow poultry keeper', 'My hens lay more eggs than I need. Bring wood for their shelter.')],
    's': [('Dara', 'Dune outfitter', 'Here is a light desert outfit for your travels. Wear it from your character panel.'),
          ('Kestrel', 'Dune tool trader', 'A sharp axe is useful even far from the forest. I sell steel and masterwork tools.')],
    'r': [('Orin', 'Ridge provisioner', 'The ridge gets cold at night. I swap vegetables for fuel.'),
          ('Petra', 'Ridge trail guide', 'Wet trails call for good boots. Take this spare pair for the darkest swamp ground.')],
    'b': [('Nell', 'Marsh rod maker', 'I craft steel and masterwork fishing rods for a more generous catch.'),
          ('Willow', 'Marsh fish trader', 'Bring firewood and I will share fresh fish from the marsh.')],
}
for biome, people in ADDITIONAL_THEMES.items():
    THEMES[biome].extend(people)

ROSTER_VERSION = 3
NEW_LOCALS = {'Bram', 'Fern', 'Runa'} | {
    name for people in ADDITIONAL_THEMES.values() for name, _, _ in people}



def ensure_npcs(town, obstacles=()):
    placed = town.get('npc_placement_version', 0) >= 1
    roster_current = 'npcs' in town and town.get('npc_roster_version', 0) >= ROSTER_VERSION
    if roster_current and placed:
        return {}
    # Reserve space for the full canopy, including trees that will regrow.
    trees = [r for r in town['map_resources'] + town.get('tree_stumps', [])
             if r.get('kind') == 'tree']
    # In isometric projection, trees in front share the NPC's screen column.
    obscured = {(r['x'] + dx, r['y'] + dy) for r in trees
                for dx in range(-4, 1) for dy in range(-4, 1)
                if abs(dx - dy) <= 1 and -(dx + dy) <= (7 if r.get('species') == 'ancient' else 5)}
    hidden = [n for n in town.get('npcs', [])
              if not placed and (n['x'], n['y']) in obscured]
    existing = 'npcs' in town
    locals_ = town.setdefault('npcs', [])
    names = {npc['name'] for npc in locals_}
    rng = random.Random(str(town.get('terrain_seed', 0)) + ':npcs')
    occupied = {(obj['x'], obj['y']) for obj in
                town.get('buildings', []) + town['map_resources'] + locals_
                + town.get('roads', []) + list(obstacles)}
    player = town.get('player')
    if player:
        occupied.add((round(player['x']), round(player['y'])))
    blocked = occupied | obscured
    candidates = {biome: [] for biome in THEMES}
    terrain = town.get('terrain', [])
    tiles = (sorted(terrain.tiles(), key=lambda p: (p[1], p[0])) if hasattr(terrain, 'tiles')
             else ((x, y) for y, row in enumerate(terrain) for x in range(len(row))))
    for x, y in tiles:
        biome = terrain[y][x]
        if biome in candidates and (x, y) not in blocked and (x >= 12 or y >= 12):
            candidates[biome].append((x, y))
    moved = {}
    for npc in hidden:
        tiles = candidates.get(npc.get('biome'), [])
        if not tiles:
            continue
        x, y = min(tiles, key=lambda p: ((p[0] - npc['x']) ** 2 + (p[1] - npc['y']) ** 2, p[1], p[0]))
        old_key = local_key(npc)
        npc.update(x=x, y=y)
        tiles.remove((x, y))
        moved[old_key] = local_key(npc)
    for biome, people in (() if roster_current else THEMES.items()):
        tiles = candidates[biome]
        for name, role, dialogue in people:
            if name in names or (existing and name not in NEW_LOCALS):
                continue
            if not tiles:
                break
            x, y = tiles.pop(rng.randrange(len(tiles)))
            town['npcs'].append({'name': name, 'role': role, 'dialogue': dialogue,
                                 'biome': biome, 'x': x, 'y': y})
    town['npc_roster_version'] = ROSTER_VERSION
    town['npc_placement_version'] = 1
    move_local_visits(town, moved)
    return moved


def move_local_visits(town, moved):
    visits = town.get('local_visits', {})
    for old_key, new_key in moved.items():
        if old_key in visits:
            visits[new_key] = visits.pop(old_key)


WORKER_TOOLS = {
    'fine_axe': {'name': 'Fine axe', 'job': 'woodcutting', 'bonus': .25},
    'fine_fishing_rod': {'name': 'Fine fishing rod', 'job': 'fishing', 'bonus': .25},
    'fine_sickle': {'name': 'Fine sickle', 'job': 'farming', 'bonus': .25},
    'steel_fishing_rod': {'name': 'Steel fishing rod', 'job': 'fishing', 'bonus': .5, 'price': 30},
    'masterwork_fishing_rod': {'name': 'Masterwork fishing rod', 'job': 'fishing', 'bonus': .75, 'price': 60},
    'steel_axe': {'name': 'Steel axe', 'job': 'woodcutting', 'bonus': .5, 'price': 30},
    'masterwork_axe': {'name': 'Masterwork axe', 'job': 'woodcutting', 'bonus': .75, 'price': 60},
    'steel_sickle': {'name': 'Steel sickle', 'job': 'farming', 'bonus': .5, 'price': 30},
    'masterwork_sickle': {'name': 'Masterwork sickle', 'job': 'farming', 'bonus': .75, 'price': 60},
}
GIFTS = {'Sylva': 'fine_axe', 'Reed': 'fine_fishing_rod', 'Fern': 'fine_sickle',
         'Wren': 'fine_fishing_rod', 'Alder': 'fine_axe', 'Poppy': 'fine_sickle',
         'Dara': 'desert_clothing', 'Petra': 'swamp_boots'}
TOOL_SHOPS = {'Bram': ('steel_axe', 'masterwork_axe'),
              'Runa': ('steel_sickle', 'masterwork_sickle'),
              'Nell': ('steel_fishing_rod', 'masterwork_fishing_rod'),
              'Kestrel': ('steel_axe', 'masterwork_axe')}
BARTERERS = {
    'Tobin': (10, 'vegetables', 8), 'Clover': (10, 'vegetables', 8),
    'Pip': (10, 'bread', 5), 'Hazel': (6, 'berries', 5),
    'Mabel': (8, 'eggs', 6), 'Orin': (10, 'vegetables', 8),
    'Willow': (8, 'fish', 6),
}
TRADE_SECONDS = 300

# Stages are shared with the inspector; progress and rewards belong to each town.
STORIES = {
    'Moss': {
        'title': 'A Remedy Remembered', 'reward': 12, 'keepsake': 'Moss’s pressed willow leaf',
        'stages': [
            {'dialogue': 'A traveler is feverish, and my mentor’s remedy has slipped from memory. Will you help me remember?',
             'button': 'Offer to help'},
            {'dialogue': 'Bring three herbs from the patches among the reeds. Their scent may bring the old lesson back. Allocate herb storage in a storehouse before gathering.',
             'costs': {'herbs': 3}, 'button': 'Bring the herbs'},
            {'dialogue': 'Willow, of course! My mentor brewed it gently and always served a meal afterward. Two more herbs and four food will let me care for our visitor.',
             'costs': {'herbs': 2, 'food': 4}, 'button': 'Deliver the remedy supplies'},
            {'dialogue': 'The traveler is resting comfortably. I remembered the recipe, but you reminded me why my mentor taught it. Keep this leaf from her willow.'},
        ],
    },
    'Tobin': {
        'title': 'The Lantern Road', 'reward': 15, 'keepsake': 'Tobin’s brass lantern token',
        'stages': [
            {'dialogue': 'My old friend kept a lantern burning for travelers. Her light went dark last winter. Help me prepare to look for her?',
             'button': 'Help Tobin search'},
            {'dialogue': 'Four food will see me through the walk. I will follow the old route and bring back word.',
             'costs': {'food': 4}, 'button': 'Pack provisions'},
            {'dialogue': 'She is safe! Her shelter fell apart, and she moved to stay with family. Let us keep her welcome alive: finish three road tiles in your town and bring six wood for a new lantern shelter.',
             'costs': {'wood': 6}, 'roads': 3, 'button': 'Supply the lantern shelter'},
            {'dialogue': 'The shelter has a light again, and your road welcomes weary feet. My friend sent this token: a promise that someone will leave a lantern burning.'},
        ],
    },
    'Flint': {
        'title': 'Marks Beneath the Stone', 'reward': 18, 'keepsake': 'Flint’s charcoal map',
        'stages': [
            {'dialogue': 'I found a carved line beneath the moss on an outcrop. It might be part of a map. Will you help survey the rocks?',
             'button': 'Join the survey'},
            {'dialogue': 'Examine three different rock outcrops and return with your notes. Walk beside each rock and select it to record its markings.',
             'rocks': 3, 'button': 'Compare the markings'},
            {'dialogue': 'The lines join! They mark a doorway beneath the ridge. Bring two wood for charcoal, and I will copy the route for you. We should gather companions before venturing inside.',
             'costs': {'wood': 2}, 'button': 'Make a charcoal map'},
            {'dialogue': 'Here is our map. The doorway remains sealed, but we now know where a future expedition should begin. Until then, keep the route safe.'},
        ],
    },
}


def advance_story(town, npc, progress):
    story = STORIES.get(npc['name'])
    if not story:
        raise ValueError('This local has no story request.')
    stage = progress.get('story_stage', 0)
    if stage >= len(story['stages']) - 1:
        raise ValueError('You already completed this story.')
    objective = story['stages'][stage]
    costs = objective.get('costs', {})
    if not economy.afford(town, costs):
        raise ValueError('Bring the supplies listed in this story request.')
    roads = sum(not road.get('construction') for road in town.get('roads', []))
    if roads < objective.get('roads', 0):
        raise ValueError('Finish three road tiles in your town first.')
    if len(progress.get('story_rocks', [])) < objective.get('rocks', 0):
        raise ValueError('Examine three different rock outcrops first.')
    if costs:
        economy.spend(town, costs)
    progress = town.setdefault('local_visits', {}).setdefault(local_key(npc), {})
    progress['story_stage'] = stage + 1
    if stage + 1 == len(story['stages']) - 1:
        town['coins'] += story['reward']
        town['player'].setdefault('keepsakes', []).append(story['keepsake'])
    return {}


def inspect_story_rock(town, data):
    x, y = data.get('x'), data.get('y')
    if type(x) is not int or type(y) is not int:
        raise ValueError('Choose a rock outcrop.')
    if math.hypot(town['player']['x'] - x, town['player']['y'] - y) > 1.8:
        raise ValueError('Walk beside the rock to examine it.')
    if not any(r['kind'] == 'rock' and r['x'] == x and r['y'] == y
               for r in town['map_resources']):
        raise ValueError('Choose a rock outcrop.')
    npc = next((n for n in town['npcs'] if n['name'] == 'Flint'), None)
    progress = town.get('local_visits', {}).get(local_key(npc), {}) if npc else {}
    if progress.get('story_stage') != 1:
        raise ValueError('Start Flint’s survey before recording markings.')
    rocks = progress.get('story_rocks', [])
    key = f'{x}:{y}'
    if key in rocks:
        raise ValueError('You already recorded this outcrop.')
    if len(rocks) >= 3:
        raise ValueError('Your survey is ready. Return to Flint.')
    progress['story_rocks'] = [*rocks, key]
    return {}


def local_key(npc):
    return f"{npc['name']}:{npc['x']}:{npc['y']}"


def advance_trades(town, seconds):
    for progress in town.get('local_visits', {}).values():
        if progress.get('trade_remaining', 0):
            progress['trade_remaining'] = max(0, progress['trade_remaining'] - seconds)


def tool_bonus(villager, job):
    tool = WORKER_TOOLS.get(villager.get('worker_tool'), {})
    return tool.get('bonus', 0) if tool.get('job') == job else 0


def change(town, data):
    action = data['action']
    player = town['player']
    if action == 'inspect_story_rock':
        return inspect_story_rock(town, data)
    if action in {'give_worker_tool', 'take_worker_tool', 'transfer_worker_tool'}:
        index = data.get('index')
        if type(index) is not int or not 0 <= index < len(town['villagers']):
            raise ValueError('Choose one of your workers.')
        worker = town['villagers'][index]
        tools = player.get('worker_tools', {})
        if action == 'transfer_worker_tool':
            target_index = data.get('target_index')
            if (type(target_index) is not int
                    or not 0 <= target_index < len(town['villagers'])
                    or target_index == index):
                raise ValueError('Choose another worker to receive the tool.')
            key = worker.get('worker_tool')
            if key not in WORKER_TOOLS or data.get('item') != key:
                raise ValueError('This worker no longer carries that tool.')
            recipient = town['villagers'][target_index]
            if recipient.get('worker_tool'):
                raise ValueError('Take back the recipient’s tool first.')
            recipient['worker_tool'] = key
            del worker['worker_tool']
        elif action == 'give_worker_tool':
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
    if action == 'interact_local':
        town.setdefault('local_visits', {}).setdefault(visit_key, {})['met'] = True
        return {}
    if action == 'advance_local_story':
        return advance_story(town, npc, progress)
    if action == 'buy_worker_tool':
        key = data.get('item')
        if not isinstance(key, str) or key not in TOOL_SHOPS.get(npc['name'], ()):
            raise ValueError('This local does not sell that worker tool.')
        price = WORKER_TOOLS[key]['price']
        if town['coins'] < price:
            raise ValueError('Not enough coins for this tool.')
        town['coins'] -= price
        tools = player.setdefault('worker_tools', {})
        tools[key] = tools.get(key, 0) + 1
    elif action == 'claim_local_gift':
        key = GIFTS.get(npc['name'])
        if not key:
            raise ValueError('This local has no equipment gift.')
        if progress.get('gift_claimed'):
            raise ValueError('You already collected this local’s gift.')
        inventory = player.setdefault('worker_tools' if key in WORKER_TOOLS else 'items', {})
        inventory[key] = inventory.get(key, 0) + 1
        town.setdefault('local_visits', {}).setdefault(visit_key, {})['gift_claimed'] = True
    elif action == 'trade_local':
        if npc['name'] not in BARTERERS:
            raise ValueError('This local does not trade wood for food.')
        if progress.get('trade_remaining', 0) > 0:
            raise ValueError('This local is restocking. Come back later.')
        wood, item, amount = BARTERERS[npc['name']]
        if town['wood'] < wood:
            raise ValueError(f'This trade needs {wood} wood.')
        if economy.storage_room(town, item) < amount:
            raise ValueError(f'Make storage room for {amount} {item} first.')
        economy.spend(town, {'wood': wood})
        economy.deliver(town, item, amount, economy.storage_limit(town, item), record=False)
        town.setdefault('local_visits', {}).setdefault(visit_key, {})['trade_remaining'] = TRADE_SECONDS
    else:
        raise ValueError('Choose a known local action.')
    return {}
