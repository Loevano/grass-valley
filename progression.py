"""Town professions, finite renewable supplies, and founder activities (hall 1–5)."""
from collections import deque
import secrets

BUILDINGS = {
    'barn': {'label': 'Barn', 'level': 2, 'wood': 25, 'job': 'husbandry', 'slots': 2},
    'sawpit': {'label': 'Sawpit', 'level': 3, 'wood': 25, 'job': 'woodworking', 'slots': 2},
    'mill': {'label': 'Mill', 'level': 4, 'wood': 30, 'planks': 5, 'job': 'milling', 'slots': 2},
    'smokehouse': {'label': 'Smokehouse', 'level': 4, 'wood': 25, 'planks': 5, 'job': 'preserving', 'slots': 2},
    'workshop': {'label': 'Workshop', 'level': 4, 'wood': 30, 'planks': 5, 'job': 'crafting', 'slots': 2},
    'fish_pond': {'label': 'Fish pond', 'level': 5, 'wood': 25, 'planks': 10, 'job': 'breeding', 'slots': 2},
    'orchard': {'label': 'Orchard', 'level': 5, 'wood': 25, 'job': 'orcharding', 'slots': 2},
    'bakery': {'label': 'Bakery', 'level': 4, 'wood': 30, 'planks': 5, 'job': 'baking', 'slots': 2},
    'butcher': {'label': 'Butcher shop', 'level': 4, 'wood': 25, 'planks': 5, 'job': 'butchery', 'slots': 2},
    'market': {'label': 'Market stall', 'level': 5, 'wood': 20, 'planks': 5, 'slots': 1},
}
CROPS = {'vegetables': 1, 'wheat': 2, 'carrots': 3, 'fodder': 3}
ANIMALS = {
    'chicken': {'level': 2, 'coins': 3, 'output': 'eggs', 'feed': .15},
    'sheep': {'level': 3, 'coins': 6, 'output': 'wool', 'feed': .25},
    'pig': {'level': 4, 'coins': 8, 'output': 'meat', 'feed': .4},
    'cow': {'level': 5, 'coins': 12, 'output': 'milk', 'feed': .5},
}
PROFESSION_UPGRADES = {
    'axe': {'label': 'Improved axe', 'building': 'woodcutter', 'level': 2, 'skill': 'woodcutting', 'xp': 10, 'wood': 10, 'coins': 5},
    'nets': {'label': 'Fishing nets', 'building': 'fishing_hut', 'level': 2, 'skill': 'fishing', 'xp': 10, 'wood': 10, 'coins': 5},
    'boat': {'label': 'Fishing boat', 'building': 'fishing_hut', 'level': 3, 'skill': 'fishing', 'xp': 20, 'planks': 10},
    'wool_nets': {'label': 'Durable wool nets', 'building': 'fishing_hut', 'level': 3, 'skill': 'fishing', 'xp': 20, 'wool': 5},
    'tools': {'label': 'Crafted axe', 'building': 'woodcutter', 'level': 4, 'skill': 'woodcutting', 'xp': 30, 'tools': 3},
    'saw': {'label': 'Improved saw', 'building': 'sawpit', 'level': 5, 'skill': 'woodworking', 'xp': 20, 'tools': 3},
    'timber_frame': {'label': 'Timber framing', 'building': 'home', 'level': 5, 'planks': 10},
}
HALL_UPGRADES = [
    {'level': 2, 'wood': 40, 'seconds': 30, 'coins': 30, 'milestone': 'Housing and reliable food'},
    {'level': 3, 'wood': 60, 'seconds': 60, 'coins': 40, 'milestone': 'Develop two professions (10 practice each)'},
    {'level': 4, 'wood': 80, 'planks': 10, 'seconds': 90, 'coins': 50, 'milestone': 'Produce 10 planks'},
    {'level': 5, 'wood': 80, 'planks': 15, 'seconds': 120, 'coins': 60, 'milestone': 'Produce flour, smoked fish, or crafted tools'},
]
# Food is the aggregate pantry, including these goods and legacy untyped provisions.
EDIBLE = {'vegetables': 1, 'carrots': 1, 'fish': 1, 'berries': 1, 'eggs': 1,
          'meat': 1, 'milk': 1, 'fruit': 1, 'smoked_fish': 1.5, 'bread': 1.5, 'prepared_meat': 1.5}
RECIPES = {
    'bakery': ({'flour': 2, 'wood': .5}, 'bread', 2),
    'butcher': ({'meat': 2}, 'prepared_meat', 2),
    'sawpit': ({'wood': 2}, 'planks', 1),
    'mill': ({'wheat': 2}, 'flour', 1),
    'smokehouse': ({'fish': 2, 'wood': .5}, 'smoked_fish', 2),
    'workshop': ({'planks': 2, 'wood': 1}, 'tools', 1),
    'fish_pond': ({'fodder': .5}, 'fish', 1),
}
SUPPLIES = {'tree': (8, .5), 'water': (12, 1.6), 'berry_bush': (6, .3), 'herb_patch': (4, .2)}
_supply_cache = None

JOB_BUILDINGS = {v['job']: k for k, v in BUILDINGS.items() if 'job' in v}


# Practice belongs to the individual; town/player skill cannot qualify a worker.
JOB_REQUIREMENTS = {
    'woodworking': {'woodcutting': 10}, 'milling': {'farming': 10},
    'preserving': {'fishing': 10}, 'crafting': {'woodworking': 10},
    'breeding': {'fishing': 20}, 'orcharding': {'farming': 20},
    'baking': {'milling': 10}, 'butchery': {'husbandry': 10},
}
TALENT_ROOTS = {'woodworking': 'woodcutting', 'crafting': 'woodcutting',
                'milling': 'farming', 'orcharding': 'farming', 'baking': 'farming',
                'preserving': 'fishing', 'breeding': 'fishing', 'butchery': 'husbandry'}
TALENTS = ('woodcutting', 'farming', 'fishing', 'husbandry')


def ensure_talents(villager, index):
    # Deterministic defaults for older saves; persisted traits never change on reload.
    villager.setdefault('talents', {TALENTS[index % len(TALENTS)]: 1,
                                    TALENTS[(index + 2) % len(TALENTS)]: -1})


def aptitude(villager, job):
    return villager.get('talents', {}).get(TALENT_ROOTS.get(job, job), 0)


def efficiency(villager, job):
    return (1 + villager.get(job, 0) // 20) * (1 + .25 * aptitude(villager, job))


def practice(villager, job):
    villager[job] = villager.get(job, 0) + 1 + .25 * aptitude(villager, job)


def qualification(villager, job):
    missing = [f"{amount} {skill} practice (has {villager.get(skill, 0):g})"
               for skill, amount in JOB_REQUIREMENTS.get(job, {}).items()
               if villager.get(skill, 0) < amount]
    return 'Requires ' + ', '.join(missing) if missing else None


def goods(town):
    return town.get('goods', {})


def quantity(town, key):
    return town.get(key, 0) if key in {'wood', 'food', 'coins'} else goods(town).get(key, 0)


def afford(town, costs):
    return all(quantity(town, k) >= v for k, v in costs.items())


def spend(town, costs):
    for key, amount in costs.items():
        if key in {'wood', 'food', 'coins'}:
            town[key] -= amount
        else:
            town.setdefault('goods', {})[key] = quantity(town, key) - amount
            town['food'] = max(0, town['food'] - amount * EDIBLE.get(key, 0))


def costs_of(spec):
    return {k: v for k, v in spec.items() if k in {'wood', 'coins', 'planks', 'wool', 'tools'}}


STORAGE_KEYS = ('wood', 'food', 'planks', 'wheat', 'fodder', 'flour', 'wool', 'tools', 'manure', 'herbs')


def storage_size(building):
    return 200 if building['kind'] == 'hall' else 100 * building.get('level', 1)


def storage_allocations(building):
    return building.get('storage_allocations', {'wood': 100, 'food': 100} if building['kind'] == 'hall' else {})


def storage_limit(town, key):
    key = 'food' if key in EDIBLE else key
    return sum(storage_allocations(b).get(key, 0) for b in town['buildings']
               if b['kind'] in {'hall', 'storehouse'} and 'construction' not in b)


def storage_room(town, key):
    group = 'food' if key in EDIBLE else key
    room = storage_limit(town, group) - quantity(town, group)
    return max(0, room / EDIBLE.get(key, 1))


def deliver(town, key, amount, capacity, record=True):
    room = (storage_room(town, key) if not town.get('_local_production')
            else capacity - quantity(town, key))
    if key in EDIBLE:
        room = min(room, (capacity - town['food']) / EDIBLE[key])
    amount = max(0, min(amount, room))
    if key == 'wood':
        town['wood'] += amount
        if record:
            town['harvested'] += amount
    else:
        town.setdefault('goods', {})[key] = quantity(town, key) + amount
        if key in EDIBLE:
            town['food'] += amount * EDIBLE[key]
    if record:
        totals = town.setdefault('produced', {})
        totals[key] = totals.get(key, 0) + amount
    return amount


def eat(town, amount):
    # Preserve some processed food for crafting/trading; consume fresh food first.
    remaining = min(town['food'], amount)
    for key, value in EDIBLE.items():
        take = min(quantity(town, key), remaining / value)
        if take:
            town['goods'][key] -= take
            remaining -= take * value
    town['food'] = max(0, town['food'] - amount)


def skill(town, job):
    return max([v.get(job, 0) for v in town['villagers']] + [town.get('player', {}).get(job, 0)])


def milestone(town, level):
    if level == 2:
        housing = any(b['kind'] == 'home' and 'construction' not in b for b in town['buildings'])
        food = any(b['kind'] in {'farm', 'fishing_hut'} and 'construction' not in b
                   and any(r['kind'] == 'water' and distance(b, r) <= 3 for r in town['map_resources'])
                   for b in town['buildings'])
        return housing and food and town['food'] >= 20
    if level == 3:
        return sum(skill(town, j) >= 10 for j in ('woodcutting', 'farming', 'fishing', 'husbandry', 'gathering')) >= 2
    if level == 4:
        return town.get('produced', {}).get('planks', 0) >= 10
    if level == 5:
        return any(town.get('produced', {}).get(k, 0) >= 1 for k in ('flour', 'smoked_fish', 'tools'))
    return False


def distance(a, b):
    return abs(a['x'] - b['x']) + abs(a['y'] - b['y'])


def ensure_forage(landscape, buildings):
    hall_buildings = [b for b in buildings if b['kind'] == 'hall']
    halls = landscape.get('forage_halls', [])
    if landscape.get('forage_version') == 1 and len(halls) >= len(hall_buildings):
        return
    occupied = {(b['x'], b['y']) for b in buildings + landscape['map_resources'] + landscape.get('npcs', [])}
    terrain = landscape.get('terrain', [])
    size = landscape.get('map_size', 120)
    candidates = []
    if landscape.get('forage_version') != 1:
        candidates = [(r['x'] + 1, r['y'], 'berry_bush' if i % 2 == 0 else 'herb_patch')
                      for i, r in enumerate(landscape['map_resources']) if r['kind'] == 'tree' and i % 17 == 0]
    halls = landscape.setdefault('forage_halls', [])
    for index, hall in enumerate(hall_buildings):
        # Town order is saved. Moving a hall must not create more free supplies.
        if index < len(halls):
            continue
        halls.append([hall['x'], hall['y']])
        candidates.extend((hall['x'] + dx, hall['y'] + dy, kind)
                          for dx, dy, kind in [(1, 4, 'berry_bush'), (2, 4, 'herb_patch')])
    for x, y, kind in candidates:
        if (0 <= x < size and 0 <= y < size and (x, y) not in occupied
                and (not terrain or terrain[y][x] != 'v')):
            landscape['map_resources'].append({'kind': kind, 'x': x, 'y': y})
            occupied.add((x, y))
    landscape['forage_version'] = 1


def supply_groups(landscape):
    """Connected water tiles share one pond; other sources have individual supplies."""
    global _supply_cache
    cache = _supply_cache
    resources = landscape['map_resources']
    if cache and cache[0] is resources and cache[1] == len(resources):
        return cache[2]
    waters = {(r['x'], r['y']): r for r in resources if r['kind'] == 'water'}
    groups = []
    while waters:
        start = next(iter(waters))
        queue = deque([start]); members = []
        while queue:
            p = queue.popleft()
            r = waters.pop(p, None)
            if r is None:
                continue
            members.append(r)
            x, y = p
            queue.extend(q for q in [(x+1,y),(x-1,y),(x,y+1),(x,y-1)] if q in waters)
        groups.append(('water', start, members))
    groups.extend((r['kind'], (r['x'], r['y']), [r]) for r in resources
                  if r['kind'] in SUPPLIES and r['kind'] != 'water')
    _supply_cache = (resources, len(resources), groups, {id(r): (kind, members) for kind, _, members in groups for r in members})
    return groups


def renew(landscape):
    for kind, _, members in supply_groups(landscape):
        maximum, regeneration = SUPPLIES[kind]
        root = min(members, key=lambda r: (r['x'], r['y']))
        available = min(maximum, root.get('supply', maximum) + regeneration)
        for r in members:
            r.update(supply=available, supply_max=maximum, regeneration=regeneration)


def take_supply(landscape, source, amount):
    # Supply metadata is stored on actual map resources, never client-provided tiles.
    supply_groups(landscape)
    group = _supply_cache[3].get(id(source))
    if group is None:
        return amount  # Acres do not consume fish stocks.
    kind, members = group
    available = min(r.get('supply', SUPPLIES[kind][0]) for r in members)
    taken = min(amount, available)
    for r in members:
        r['supply'] = available - taken
    return taken


def start_activity(town, landscape, action):
    player = town['player']
    if action == 'stop_activity':
        player.pop('activity', None); player.pop('activity_status', None)
        return
    job, kinds = {'fish': ('fishing', {'water'}), 'chop': ('woodcutting', {'tree'}),
                  'gather': ('gathering', {'berry_bush', 'herb_patch'})}[action]
    reachable = [r for r in landscape['map_resources'] if r['kind'] in kinds and distance(player, r) <= 1.8]
    if not reachable:
        raise ValueError('Walk next to water to fish, a tree to chop, or berries/herbs to gather.')
    source = min(reachable, key=lambda r: distance(player, r))
    activity = {'job': job, 'kind': source['kind'], 'x': source['x'], 'y': source['y']}
    if player.get('activity') == activity:
        player.pop('activity', None); player.pop('activity_status', None)
    else:
        player['activity'] = activity
        player['activity_status'] = 'Working'
        if source['x'] != player['x'] or source['y'] != player['y']:
            player['facing'] = 1 if source['x'] - player['x'] > source['y'] - player['y'] else -1


def player_tick(town, landscape, capacity):
    player = town.get('player', {})
    activity = player.get('activity')
    if not activity:
        return
    source = next((r for r in landscape['map_resources'] if all(r[k] == activity[k] for k in ('kind', 'x', 'y'))), None)
    if source is None or distance(player, source) > 1.8:
        player.pop('activity', None); return
    key = {'tree': 'wood', 'water': 'fish', 'berry_bush': 'berries', 'herb_patch': 'herbs'}[source['kind']]
    room = storage_room(town, key)
    if key in EDIBLE:
        room = min(room, (capacity - town['food']) / EDIBLE[key])
    if room <= 0:
        player['activity_status'] = 'Storage full'; return
    if source.get('supply', 0) <= 0:
        player['activity_status'] = 'Waiting for supply to regenerate'; return
    job = activity['job']
    amount = min(room, (1 if key != 'wood' else .5) * (1 + player.get(job, 0) // 20))
    amount = take_supply(landscape, source, amount)
    caught = job != 'fishing' or secrets.randbelow(5) != 0
    if caught:
        deliver(town, key, amount, capacity)
    player[job] = player.get(job, 0) + 1
    player['activity_status'] = 'Working' if caught else 'No catch — trying again'


def extra_worker(town, villager, building, capacity):
    """One worker cycle, bounded by materials, field/barn size, and storage."""
    reason = qualification(villager, villager['job'])
    if reason:
        return reason
    kind = building['kind']
    upgrades = building.get('profession_upgrades', [])
    speed = (1.25 if 'saw' in upgrades else 1) * efficiency(villager, villager['job'])
    if kind in RECIPES:
        inputs, output, amount = RECIPES[kind]
        amount *= speed
        costs = {k: v * speed for k, v in inputs.items()}
        if not afford(town, costs):
            return 'Waiting for ' + ', '.join(inputs)
        if quantity(town, output) + amount > capacity or (output in EDIBLE and town['food'] - sum(v*EDIBLE.get(k,0) for k,v in costs.items()) + amount*EDIBLE[output] > capacity):
            return 'Storage full'
        spend(town, costs)
        deliver(town, output, amount, capacity)
    elif kind == 'orchard':
        if deliver(town, 'fruit', speed, capacity) == 0:
            return 'Storage full'
    elif kind == 'barn':
        animals = building.get('animals', {})
        output_amounts = {}
        for animal, count in animals.items():
            if count:
                key = ANIMALS[animal]['output']; output_amounts[key] = output_amounts.get(key, 0) + count * .25 * speed
        if not output_amounts:
            return 'Buy animals first'
        # One barn cycle per tick regardless of the number of assigned workers.
        feed = sum(ANIMALS[a]['feed'] * n for a, n in animals.items())
        if quantity(town, 'fodder') < feed:
            feed_cost = {'vegetables': feed}  # Chickens before fodder unlocks.
        else:
            feed_cost = {'fodder': feed}
        if not afford(town, feed_cost):
            return 'Waiting for animal feed'
        if any(quantity(town, k) + v > capacity for k, v in output_amounts.items()) or town['food'] - sum(v*EDIBLE.get(k,0) for k,v in feed_cost.items()) + sum(v*EDIBLE.get(k,0) for k,v in output_amounts.items()) > capacity:
            return 'Storage full'
        spend(town, feed_cost)
        for key, amount in output_amounts.items():
            deliver(town, key, amount, capacity)
        deliver(town, 'manure', .1 * sum(animals.values()), capacity)
    else:
        return 'Resting'
    practice(villager, villager['job'])
    return 'Working'


def public_catalog():
    return {'buildings': BUILDINGS, 'crops': CROPS, 'animals': ANIMALS,
            'job_requirements': JOB_REQUIREMENTS, 'talent_roots': TALENT_ROOTS,
            'upgrades': PROFESSION_UPGRADES, 'recipes': RECIPES, 'edible': EDIBLE}
