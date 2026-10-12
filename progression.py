"""Town professions, finite renewable supplies, and founder activities (hall 1–10)."""
from collections import deque
from math import ceil
import secrets
from equipment import find_item, gathering_bonus

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
    'tannery': {'label': 'Tannery', 'level': 5, 'wood': 30, 'planks': 10, 'job': 'tanning', 'slots': 2},
    'mine': {'label': 'Mine', 'level': 6, 'wood': 40, 'planks': 15, 'tools': 3, 'job': 'mining', 'slots': 2},
    'smelter': {'label': 'Iron smelter', 'level': 7, 'wood': 40, 'planks': 15, 'job': 'smelting', 'slots': 2},
    'stable': {'label': 'Stable', 'level': 8, 'wood': 40, 'planks': 20, 'iron': 5, 'job': 'horsekeeping', 'slots': 2},
    'saddler': {'label': 'Saddler', 'level': 8, 'wood': 30, 'planks': 15, 'job': 'saddlery', 'slots': 2},
    'steelworks': {'label': 'Steelworks', 'level': 9, 'wood': 40, 'planks': 20, 'iron': 10, 'job': 'steelmaking', 'slots': 2},
    'tradehouse': {'label': 'Trade house', 'level': 10, 'wood': 50, 'planks': 25, 'steel': 10, 'job': 'packing', 'slots': 2},
    'market': {'label': 'Market stall', 'level': 5, 'wood': 20, 'planks': 5, 'slots': 1},
}
CROPS = {'vegetables': 1, 'wheat': 2, 'carrots': 3, 'fodder': 3}
ANIMALS = {
    'chicken': {'level': 2, 'coins': 3, 'output': 'eggs', 'feed': .15},
    'sheep': {'level': 3, 'coins': 6, 'output': 'wool', 'feed': .25},
    'pig': {'level': 4, 'coins': 8, 'output': 'meat', 'feed': .4},
    'cow': {'level': 5, 'coins': 12, 'output': 'milk', 'feed': .5},
}
ANIMALS['horse'] = {'level': 8, 'coins': 0, 'output': 'horse', 'feed': .5}
ANIMAL_GROWTH_SECONDS = 300
TREE_REGROWTH_RATE = .9
BUTCHER_SECONDS = 60
ANIMAL_MEAT = {'chicken': 2, 'sheep': 4, 'pig': 8, 'cow': 10}


def animal_growth(building, animal):
    """Fed seconds per animal; animals from older saves are already adults."""
    count = building.get('animals', {}).get(animal, 0)
    ages = building.get('animal_growth', {}).get(animal, [])
    return (ages + [ANIMAL_GROWTH_SECONDS] * count)[:count]


def mature_animals(building, animal):
    return sum(age >= ANIMAL_GROWTH_SECONDS for age in animal_growth(building, animal))


def grow_animals(building, seconds):
    building['animal_growth'] = {
        animal: [min(ANIMAL_GROWTH_SECONDS, age + seconds) for age in animal_growth(building, animal)]
        for animal in building.get('animals', {})}


PROFESSION_UPGRADES = {
    'axe': {'label': 'Improved axe', 'building': 'woodcutter', 'level': 2, 'skill': 'woodcutting', 'xp': 10, 'wood': 10, 'coins': 5},
    'nets': {'label': 'Fishing nets', 'building': 'fishing_hut', 'level': 2, 'skill': 'fishing', 'xp': 10, 'wood': 10, 'coins': 5},
    'boat': {'label': 'Fishing boat', 'building': 'fishing_hut', 'level': 3, 'skill': 'fishing', 'xp': 20, 'planks': 10, 'capacity': 5},
    'improved_boat': {'label': 'Improved fishing boat', 'building': 'fishing_hut', 'level': 4, 'skill': 'fishing', 'xp': 30, 'requires': 'boat', 'planks': 15, 'coins': 20, 'capacity': 10},
    'master_boat': {'label': 'Masterwork fishing boat', 'building': 'fishing_hut', 'level': 5, 'skill': 'fishing', 'xp': 40, 'requires': 'improved_boat', 'planks': 20, 'coins': 40, 'capacity': 20},
    'wool_nets': {'label': 'Durable wool nets', 'building': 'fishing_hut', 'level': 3, 'skill': 'fishing', 'xp': 20, 'wool': 5},
    'tools': {'label': 'Crafted axe', 'building': 'woodcutter', 'level': 4, 'skill': 'woodcutting', 'xp': 30, 'tools': 3},
    'saw': {'label': 'Improved saw', 'building': 'sawpit', 'level': 5, 'skill': 'woodworking', 'xp': 20, 'tools': 3},
    'steel_pickaxe': {'label': 'Steel pickaxes', 'building': 'mine', 'level': 9, 'skill': 'mining', 'xp': 10, 'steel': 5, 'leather': 3},
    'timber_frame': {'label': 'Timber framing', 'building': 'home', 'level': 5, 'planks': 10},
}
PLAYER_BOATS = {
    1: {'label': 'Exploration boat', 'level': 3, 'xp': 0, 'coins': 20},
    2: {'label': 'Improved exploration boat', 'level': 4, 'xp': 30, 'planks': 15, 'coins': 20},
    3: {'label': 'Masterwork exploration boat', 'level': 5, 'xp': 40, 'planks': 20, 'coins': 40},
}


def fishing_boat_level(building):
    upgrades = building.get('profession_upgrades', [])
    return 3 if 'master_boat' in upgrades else 2 if 'improved_boat' in upgrades else 1 if 'boat' in upgrades else 0


def fishing_boat_capacity(building):
    level = fishing_boat_level(building)
    return PROFESSION_UPGRADES[('boat', 'improved_boat', 'master_boat')[level - 1]]['capacity'] if level else 0


def player_boat_level(player):
    return player.get('boat_level', 1) if player.get('boat') else 0


HALL_UPGRADES = [
    {'level': 2, 'wood': 40, 'seconds': 30, 'coins': 30, 'milestone': 'Housing and reliable food'},
    {'level': 3, 'wood': 60, 'seconds': 60, 'coins': 40, 'milestone': 'Develop two professions (10 practice each)'},
    {'level': 4, 'wood': 80, 'planks': 10, 'seconds': 90, 'coins': 50, 'milestone': 'Produce 10 planks'},
    {'level': 5, 'wood': 80, 'planks': 15, 'seconds': 120, 'coins': 60, 'milestone': 'Produce flour, smoked fish, or crafted tools'},
    {'level': 6, 'wood': 90, 'planks': 20, 'leather': 5, 'seconds': 150, 'coins': 70, 'milestone': 'Produce 5 leather'},
    {'level': 7, 'wood': 100, 'planks': 25, 'iron_ore': 10, 'seconds': 180, 'coins': 80, 'milestone': 'Mine 10 iron ore'},
    {'level': 8, 'wood': 100, 'planks': 25, 'iron': 10, 'seconds': 210, 'coins': 90, 'milestone': 'Smelt 10 iron'},
    {'level': 9, 'wood': 100, 'planks': 30, 'iron': 15, 'leather': 10, 'seconds': 240, 'coins': 100, 'milestone': 'Saddle a settled horse from your stable'},
    {'level': 10, 'wood': 100, 'planks': 40, 'steel': 15, 'seconds': 270, 'coins': 120, 'milestone': 'Produce 15 steel and complete a town trade or sell 10 goods at your market'},
]
# Food is the aggregate pantry, including these goods and legacy untyped provisions.
FISH = {
    'fish': {'label': 'Fish', 'depth': 1, 'boat': 0, 'food': 1, 'coins': 1},
    'trout': {'label': 'Trout', 'depth': 2, 'boat': 1, 'food': 1.5, 'coins': 2},
    'salmon': {'label': 'Salmon', 'depth': 3, 'boat': 2, 'food': 2, 'coins': 3},
    'sturgeon': {'label': 'Sturgeon', 'depth': 4, 'boat': 3, 'food': 3, 'coins': 5},
}
EDIBLE = {'vegetables': 1, 'carrots': 1, 'fish': 1, 'berries': 1, 'eggs': 1,
          'meat': 1, 'milk': 1, 'fruit': 1, 'smoked_fish': 1.5, 'bread': 1.5, 'prepared_meat': 1.5,
          **{key: spec['food'] for key, spec in FISH.items()}}


def fishing_catch(building, depth):
    tier = fishing_boat_level(building)
    return next(key for key, spec in reversed(list(FISH.items())) if tier >= spec['boat'] and depth >= spec['depth'])


RECIPES = {
    'tannery': ({'hides': 2, 'wood': .5}, 'leather', 2),
    'mine': ({}, 'iron_ore', 2),
    'smelter': ({'iron_ore': 3, 'wood': 2}, 'iron', 1),
    'saddler': ({'leather': 5, 'iron': 1}, 'saddles', 1),
    'steelworks': ({'iron': 2, 'wood': 3}, 'steel', 1),
    'tradehouse': ({'planks': 3, 'leather': 2, 'steel': 1}, 'trade_crates', 1),
    'bakery': ({'flour': 2, 'wood': .5}, 'bread', 2),
    'butcher': ({'meat': 2}, 'prepared_meat', 2),
    'sawpit': ({'wood': 2}, 'planks', 1),
    'mill': ({'wheat': 2}, 'flour', 2),
    'smokehouse': ({'fish': 2, 'wood': .5}, 'smoked_fish', 2),
    'workshop': ({'planks': 2, 'wood': 1}, 'tools', 1),
    'fish_pond': ({'fodder': .5}, 'fish', 1),
}
# Recipe levels use the whole practice points shown in settler panels.
# Beginners can make the level-one recipe; levels two to five need 2–5 practice.
PROFESSION_RECIPES = {
    'workshop': [
        {'level': 1, 'inputs': {'planks': 2, 'wood': 1}, 'output': 'tools', 'amount': 1},
        {'level': 2, 'inputs': {'planks': 1, 'wood': 1}, 'output': 'wooden_bowls', 'amount': 2},
        {'level': 3, 'inputs': {'planks': 3}, 'output': 'crates', 'amount': 1},
        {'level': 4, 'inputs': {'planks': 4, 'wool': 1}, 'output': 'furniture', 'amount': 1},
        {'level': 5, 'inputs': {'planks': 5, 'wool': 2}, 'output': 'carved_furniture', 'amount': 1},
    ],
    'bakery': [
        {'level': 1, 'inputs': {'flour': 2, 'wood': .5}, 'output': 'bread', 'amount': 2},
        {'level': 2, 'inputs': {'flour': 2, 'milk': 1, 'wood': .5}, 'output': 'bread_rolls', 'amount': 3},
        {'level': 3, 'inputs': {'flour': 2, 'herbs': 1, 'wood': .5}, 'output': 'herb_bread', 'amount': 2},
        {'level': 4, 'inputs': {'flour': 2, 'fruit': 2, 'wood': .5}, 'output': 'fruit_pies', 'amount': 2},
        {'level': 5, 'inputs': {'flour': 2, 'eggs': 2, 'milk': 1, 'wood': .5}, 'output': 'cakes', 'amount': 2},
    ],
    'butcher': [
        {'level': 1, 'inputs': {'meat': 2}, 'output': 'prepared_meat', 'amount': 2},
        {'level': 2, 'inputs': {'meat': 2, 'herbs': 1}, 'output': 'herbed_meat', 'amount': 2},
        {'level': 3, 'inputs': {'meat': 2, 'wood': .5}, 'output': 'smoked_meat', 'amount': 2},
        {'level': 4, 'inputs': {'meat': 2, 'herbs': 1, 'flour': 1}, 'output': 'sausages', 'amount': 3},
        {'level': 5, 'inputs': {'meat': 2, 'flour': 2, 'eggs': 1, 'wood': .5}, 'output': 'meat_pies', 'amount': 2},
    ],
}
CRAFTED_GOODS = tuple(r['output'] for r in PROFESSION_RECIPES['workshop'][1:])
EDIBLE.update({r['output']: 1.5 + .25 * (r['level'] - 1)
               for kind in ('bakery', 'butcher') for r in PROFESSION_RECIPES[kind][1:]})
EDIBLE.update({'fruit_pies': 3.5, 'cakes': 4.5, 'meat_pies': 4})
PRODUCTION_SECONDS = {
    'leather': 9, 'iron_ore': 9, 'iron': 12, 'saddles': 15, 'steel': 15, 'trade_crates': 18,
    'flour': 3, 'planks': 6, 'smoked_fish': 6, 'fish': 3,
    'tools': 9, 'wooden_bowls': 6, 'crates': 9,
    'furniture': 12, 'carved_furniture': 15,
    'bread': 6, 'bread_rolls': 6, 'herb_bread': 6, 'fruit_pies': 9, 'cakes': 12,
    'prepared_meat': 6, 'herbed_meat': 6, 'smoked_meat': 9,
    'sausages': 9, 'meat_pies': 12,
}
SALE_PRICES = {r['output']: ceil(EDIBLE.get(r['output'], 1)) if r['level'] > 1 else 1
               for recipes in PROFESSION_RECIPES.values() for r in recipes}
SALE_PRICES['tools'] = 3
SALE_PRICES.update(hides=2, leather=4, iron_ore=2, iron=6, saddles=25, steel=12, trade_crates=40)
LATE_GOODS = ('hides', 'leather', 'iron_ore', 'iron', 'saddles', 'steel', 'trade_crates')
TRADABLE_GOODS = tuple(dict.fromkeys(('coins', 'wood', 'planks', 'wool', 'flour', 'tools',
                                     'wheat', 'fodder', 'manure', 'herbs', *EDIBLE,
                                     *CRAFTED_GOODS, *LATE_GOODS)))


def selected_recipe(building):
    recipes = PROFESSION_RECIPES.get(building['kind'], [])
    return next((r for r in recipes if r['output'] == building.get('recipe')), recipes[0] if recipes else None)


def recipe_requirement(villager, building):
    spec = selected_recipe(building)
    job = BUILDINGS[building['kind']]['job']
    if spec and spec['level'] > 1 and villager.get(job, 0) < spec['level']:
        return f"Requires level {spec['level']} {job} ({spec['level']} practice) for {spec['output'].replace('_', ' ')}"
    return None


def production_recipe(building):
    spec = selected_recipe(building)
    return (spec['inputs'], spec['output'], spec['amount']) if spec else RECIPES[building['kind']]


def production_seconds(building):
    return PRODUCTION_SECONDS[production_recipe(building)[1]] if building['kind'] in RECIPES else 3


SUPPLIES = {'tree': (8, .5), 'water': (12, 3.2), 'berry_bush': (6, .3), 'herb_patch': (4, .2), 'rock': (60, .15)}
_supply_cache = None

JOB_BUILDINGS = {v['job']: k for k, v in BUILDINGS.items() if 'job' in v}


# Practice belongs to the individual; town/player skill cannot qualify a worker.
JOB_REQUIREMENTS = {
    'smelting': {'mining': 2}, 'tanning': {'husbandry': 2},
    'horsekeeping': {'husbandry': 2}, 'saddlery': {'tanning': 2},
    'steelmaking': {'smelting': 2}, 'packing': {'crafting': 2},
    'woodworking': {'woodcutting': 10}, 'milling': {'farming': 10},
    'preserving': {'fishing': 10}, 'crafting': {'woodworking': 2},
    'breeding': {'fishing': 20}, 'orcharding': {'farming': 20},
    'baking': {'milling': 2}, 'butchery': {'husbandry': 2},
}
TALENT_ROOTS = {'tanning': 'husbandry', 'horsekeeping': 'husbandry', 'saddlery': 'husbandry',
                'mining': 'woodcutting', 'smelting': 'woodcutting', 'steelmaking': 'woodcutting', 'packing': 'woodcutting',
                'woodworking': 'woodcutting', 'crafting': 'woodcutting',
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


def practice(villager, job, seconds=3):
    # A neutral worker earns 20 practice in 500 active three-second cycles.
    gain = .04 * seconds / 3 * (1 + .25 * aptitude(villager, job))
    villager[job] = round(villager.get(job, 0) + gain, 3)


def qualification(villager, job):
    missing = [f"{amount} {skill} practice (has {villager.get(skill, 0):g})"
               for skill, amount in JOB_REQUIREMENTS.get(job, {}).items()
               if villager.get(skill, 0) < amount]
    return 'Requires ' + ', '.join(missing) if missing else None


def goods(town):
    return town.get('goods', {})


def quantity(town, key):
    return town.get(key, 0) if key in {'wood', 'food', 'coins'} else goods(town).get(key, 0)


def production_room(town, key):
    """Count available stock, reserved inputs and outputs still being gathered once."""
    cap = town.get('rules', {}).get('production_caps', {}).get(key)
    if cap is None:
        return float('inf')
    total = quantity(town, key)
    for building in town.get('buildings', []):
        total += building.get('pending_output', {}).get(key, 0)
        total += building.get('input_buffer', {}).get(key, 0)
    return max(0, cap - total)


def afford(town, costs):
    return all(quantity(town, k) >= v for k, v in costs.items())


def spend(town, costs):
    ensure_inventory(town)
    for key, amount in costs.items():
        if key == 'food':
            eat(town, amount)
            continue
        withdraw(town, key, amount)
        if key in {'wood', 'food', 'coins'}:
            town[key] -= amount
        else:
            town.setdefault('goods', {})[key] = quantity(town, key) - amount
            town['food'] = max(0, town['food'] - amount * EDIBLE.get(key, 0))


def costs_of(spec):
    return {k: v for k, v in spec.items() if k in {'wood', 'coins', 'planks', 'wool', 'tools', *LATE_GOODS}}


STORAGE_KEYS = ('wood', 'food', 'planks', 'wheat', 'fodder', 'flour', 'wool', 'tools', 'manure', 'herbs', *CRAFTED_GOODS, *LATE_GOODS)
STORAGE_GOODS = {
    **{kind: (*STORAGE_KEYS, *EDIBLE) for kind in ('hall', 'storehouse', 'market')},
    'stable': ('fodder', 'vegetables', 'manure'),
    'home': (),
    'woodcutter': ('wood',),
    'fishing_hut': tuple(FISH),
    'farm': (*CROPS, 'manure'),
    'acre': (*CROPS, 'manure'),
    'barn': ('fodder', 'vegetables', 'manure', *(a['output'] for a in ANIMALS.values())),
    'orchard': ('fruit',),
    **{kind: (*inputs, output) for kind, (inputs, output, _) in RECIPES.items()},
}
for kind, recipes in PROFESSION_RECIPES.items():
    STORAGE_GOODS[kind] = tuple(dict.fromkeys(k for r in recipes for k in (*r['inputs'], r['output'])))

STORAGE_GOODS['butcher'] = (*STORAGE_GOODS['butcher'], 'hides')
FIXED_STORAGE = {
    'hall': {'wood': 100, 'food': 100},
    'stable': {'fodder': 30, 'food': 15, 'manure': 5},
    **{kind: {key: 50 / len(set((*inputs, output))) for key in (*inputs, output)}
       for kind, (inputs, output, _) in RECIPES.items() if kind in {'tannery', 'mine', 'smelter', 'saddler', 'steelworks', 'tradehouse'}},
    'home': {},
    'woodcutter': {'wood': 50},
    'fishing_hut': {'food': 50},
    'farm': {'food': 20, 'wheat': 10, 'fodder': 10, 'manure': 10},
    'acre': {'food': 20, 'wheat': 10, 'fodder': 10, 'manure': 10},
    'barn': {'food': 20, 'fodder': 15, 'wool': 10, 'manure': 5},
    'orchard': {'food': 50},
    'bakery': {'flour': 15, 'wood': 5, 'herbs': 5, 'food': 25},
    'butcher': {'hides': 10, 'food': 20, 'flour': 10, 'herbs': 5, 'wood': 5},
    'sawpit': {'wood': 30, 'planks': 20},
    'mill': {'wheat': 30, 'flour': 20},
    'smokehouse': {'food': 45, 'wood': 5},
    'workshop': {'planks': 15, 'wood': 5, 'tools': 5, 'wool': 5, **{key: 5 for key in CRAFTED_GOODS}},
    'fish_pond': {'fodder': 15, 'food': 35},
    'market': {key: 3 if index < 8 else 2 for index, key in enumerate(STORAGE_KEYS)},
}


def accepts_storage(building, key):
    return key in STORAGE_GOODS.get(building['kind'], ())


def building_storage_keys(building):
    groups = {'food' if k in EDIBLE else k for k in STORAGE_GOODS.get(building['kind'], ())}
    return tuple(k for k in STORAGE_KEYS if k in groups)


def storage_size(building):
    return 0 if building['kind'] == 'home' else 200 if building['kind'] == 'hall' else (50, 75, 100, 150, 200)[building.get('level', 1) - 1] if building['kind'] == 'storehouse' else 50


def inventory(building):
    result = dict(building.get('stored_goods', {}))
    for key, amount in building.get('output_buffer', {}).items():
        result[key] = result.get(key, 0) + amount
    return result


def add_stock(town, key, amount):
    if key in {'wood', 'food'}:
        town[key] += amount
    else:
        town.setdefault('goods', {})[key] = quantity(town, key) + amount
        town['food'] += amount * EDIBLE.get(key, 0)


def ensure_inventory(town):
    """Locate old shared stock at the hall and count old workplace outputs once."""
    if town.get('food_balance_version') != 1:
        # Revalue available stock once; reserved inputs and pending outputs are not pantry food.
        town['food'] = town.get('food', 0) + sum(
            quantity(town, key) * (EDIBLE[key] - old)
            for key, old in {'fruit_pies': 2.25, 'cakes': 2.5, 'meat_pies': 2.5}.items())
        town['food_balance_version'] = 1
    if town.get('inventory_version') == 1 or 'buildings' not in town:
        return
    hall = next((b for b in town['buildings'] if b['kind'] == 'hall'), None)
    if hall is None:
        return
    stock = hall.setdefault('stored_goods', {})
    stock.update(town.get('goods', {}))
    stock['wood'] = town.get('wood', 0)
    stock['food'] = max(0, town.get('food', 0) - sum(n * EDIBLE.get(k, 0) for k, n in stock.items()))
    for building in town['buildings']:
        for key, amount in building.get('output_buffer', {}).items():
            add_stock(town, key, amount)
    town['inventory_version'] = 1


def withdraw(town, key, amount, source=None):
    """Remove available stock; processor input buffers are already reserved."""
    buildings = [source] if source is not None else town.get('buildings', [])
    remaining = amount
    for building in buildings:
        for field in ('stored_goods', 'output_buffer'):
            stock = building.get(field, {})
            take = min(remaining, stock.get(key, 0))
            if take:
                stock[key] -= take
                building.pop('buffer_full', None)
            remaining -= take
        if remaining <= 1e-9:
            break


def local_room(building, key):
    if not accepts_storage(building, key):
        return 0
    stock = inventory(building)
    for pending_key, pending_amount in building.get('pending_output', {}).items():
        stock[pending_key] = stock.get(pending_key, 0) + pending_amount
    group = 'food' if key in EDIBLE else key
    used = sum(n * EDIBLE.get(k, 1) for k, n in stock.items() if k == group or (group == 'food' and k in EDIBLE))
    allocated = max(0, storage_allocations(building).get(group, 0) - used) / EDIBLE.get(key, 1)
    if building['kind'] == 'storehouse':
        return allocated
    return min(allocated, max(0, storage_size(building) - sum(stock.values())))


def storage_allocations(building):
    if building['kind'] != 'storehouse':
        return dict(FIXED_STORAGE.get(building['kind'], {}))
    enabled = storage_enabled(building)
    return {k: storage_size(building) if enabled[k] else 0 for k in building_storage_keys(building)}


def storage_enabled(building):
    """Old positive allocations become switches without changing saved inventory."""
    enabled = building.get('storage_enabled')
    if enabled is None:
        enabled = {k: n > 0 for k, n in building.get('storage_allocations', {}).items()}
    return {k: enabled.get(k, False) for k in building_storage_keys(building)}


def storage_limit(town, key):
    group = 'food' if key in EDIBLE else key
    return sum(storage_allocations(b).get(group, 0) for b in town['buildings']
               if 'construction' not in b and (accepts_storage(b, key)
                   or (key == 'food' and 'food' in building_storage_keys(b))))


def storage_room(town, key):
    if town.get('inventory_version') == 1:
        return sum(local_room(b, key) for b in town['buildings'] if 'construction' not in b)
    group = 'food' if key in EDIBLE else key
    room = storage_limit(town, key) - quantity(town, group)
    return max(0, room / EDIBLE.get(key, 1))


def deliver(town, key, amount, capacity, record=True):
    if town.get('inventory_version') == 1:
        remaining = amount
        for building in town['buildings']:
            if 'construction' in building:
                continue
            take = min(remaining, local_room(building, key))
            if take > 0:
                stock = building.setdefault('stored_goods', {})
                stock[key] = stock.get(key, 0) + take
                remaining -= take
        amount -= remaining
        add_stock(town, key, amount)
        if record:
            totals = town.setdefault('produced', {})
            totals[key] = totals.get(key, 0) + amount
            if key == 'wood':
                town['harvested'] += amount
        return amount
    room = (storage_room(town, key) if not town.get('_local_production')
            else capacity - quantity(town, key))
    if key in EDIBLE and town.get('inventory_version') != 1:
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
    untyped = max(0, town['food'] - sum(quantity(town, k) * value for k, value in EDIBLE.items()))
    take = min(remaining, untyped)
    withdraw(town, 'food', take)
    remaining -= take
    for key, value in EDIBLE.items():
        take = min(quantity(town, key), remaining / value)
        if take:
            withdraw(town, key, take)
            town['goods'][key] -= take
            # Fractional servings can round slightly above the food still needed.
            remaining = max(0, remaining - take * value)
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
    produced = town.get('produced', {})
    if level in {6, 7, 8}:
        key, count = {6: ('leather', 5), 7: ('iron_ore', 10), 8: ('iron', 10)}[level]
        return produced.get(key, 0) >= count
    if level == 9:
        return bool(town.get('horse_ridden'))
    if level == 10:
        return produced.get('steel', 0) >= 15 and (town.get('completed_trades', 0) >= 1 or town.get('market_sales', 0) >= 10)
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


def source_limits(landscape, source):
    if source['kind'] == 'tree':
        swamp = landscape.get('terrain', [])[source['y']][source['x']] == 'b' if landscape.get('terrain') else False
        return (40 if swamp or source.get('species') == 'ancient' else 8), 0
    return SUPPLIES[source['kind']]


def renew(landscape, obstacles=()):
    stumps = landscape.get('tree_stumps', [])
    occupied = set()
    if stumps:
        occupied = {(r['x'], r['y']) for r in landscape['map_resources']}
        occupied.update((r['x'], r['y']) for r in obstacles)
    remaining = []
    for stump in stumps:
        x, y = stump['x'], stump['y']
        if (x, y) in occupied:
            remaining.append(stump)
            continue
        stump['remaining'] = round(stump['remaining'] - 3 * TREE_REGROWTH_RATE, 9)
        if stump['remaining'] <= 0:
            landscape['map_resources'].append({k: v for k, v in stump.items() if k != 'remaining'})
            occupied.add((x, y))
        else:
            remaining.append(stump)
    if 'tree_stumps' in landscape:
        landscape['tree_stumps'][:] = remaining
    for kind, _, members in supply_groups(landscape):
        maximum, regeneration = source_limits(landscape, members[0])
        root = members[0] if len(members) == 1 else min(members, key=lambda r: (r['x'], r['y']))
        available = min(maximum, root.get('supply', maximum) + regeneration)
        if kind == 'tree' and root.get('supply_max', maximum) != maximum:
            available = min(maximum, root.get('supply', maximum) * maximum / root['supply_max'])
        for r in members:
            if (r.get('supply') != available or r.get('supply_max') != maximum
                    or r.get('regeneration') != regeneration):
                r.update(supply=available, supply_max=maximum, regeneration=regeneration)


def take_supply(landscape, source, amount):
    global _supply_cache
    # Supply metadata is stored on actual map resources, never client-provided tiles.
    supply_groups(landscape)
    group = _supply_cache[3].get(id(source))
    if group is None:
        return 0 if source['kind'] in SUPPLIES else amount  # Acres do not consume fish stocks.
    kind, members = group
    available = min(r.get('supply', source_limits(landscape, r)[0]) for r in members)
    taken = min(amount, available)
    for r in members:
        r['supply'] = available - taken
    if kind == 'tree' and available - taken <= 1e-9:
        landscape['map_resources'].remove(source)
        _supply_cache = None
        stump = {k: source[k] for k in ('kind', 'x', 'y', 'species') if k in source}
        stump['remaining'] = 300 if source_limits(landscape, source)[0] == 8 else 900
        landscape.setdefault('tree_stumps', []).append(stump)
    return taken


def start_activity(town, landscape, action):
    player = town['player']
    if action == 'gather':
        chest = next((r for r in landscape['map_resources'] if r['kind'] == 'chest' and distance(player, r) <= 1.8), None)
        if chest:
            landscape['map_resources'].remove(chest)
            town['coins'] += 25
            found = find_item(player, chest)
            player.pop('activity', None)
            player['activity_status'] = f'Found 25 coins and {found.lower()}'
            return
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
    cap_room = production_room(town, key)
    if cap_room <= 0:
        player['activity_status'] = 'Production cap reached'; return
    room = min(storage_room(town, key), cap_room)
    if key in EDIBLE:
        room = min(room, (capacity - town['food']) / EDIBLE[key])
    if room <= 0:
        player['activity_status'] = 'Storage full'; return
    if source.get('supply', 0) <= 0:
        player['activity_status'] = 'Waiting for supply to regenerate'; return
    job = activity['job']
    boat_bonus = .25 * max(0, player_boat_level(player) - 1) if job == 'fishing' and any(
        r['kind'] == 'water' and (r['x'], r['y']) == (int(player['x'] + .5), int(player['y'] + .5))
        for r in landscape['map_resources']) else 0
    clothing_bonus = gathering_bonus(player, key) if job == 'gathering' else 0
    amount = min(room, (1 if key != 'wood' else .5) * (1 + player.get(job, 0) // 20)
                 * (1 + boat_bonus) * (1 + clothing_bonus))
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
    speed = (1.25 if 'saw' in upgrades or 'steel_pickaxe' in upgrades else 1) * efficiency(villager, villager['job'])
    if kind in RECIPES:
        reason = recipe_requirement(villager, building)
        if reason:
            return reason
        inputs, output, amount = production_recipe(building)
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
    elif kind in {'barn', 'stable'}:
        animals = building.get('animals', {})
        output_amounts = {}
        for animal in animals:
            adults = mature_animals(building, animal)
            if adults and animal not in {'pig', 'horse'}:
                key = ANIMALS[animal]['output']; output_amounts[key] = output_amounts.get(key, 0) + adults * .25 * speed
        if not sum(animals.values()):
            return 'Capture horses first' if kind == 'stable' else 'Buy animals first'
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
    practice(villager, villager['job'], production_seconds(building))
    return 'Working'


def public_catalog():
    return {'buildings': BUILDINGS, 'crops': CROPS, 'animals': ANIMALS,
            'animal_growth_seconds': ANIMAL_GROWTH_SECONDS, 'butcher_seconds': BUTCHER_SECONDS,
            'animal_meat': ANIMAL_MEAT, 'tradable_goods': TRADABLE_GOODS,
            'job_requirements': JOB_REQUIREMENTS, 'talent_roots': TALENT_ROOTS,
            'profession_recipes': PROFESSION_RECIPES, 'storage_keys': STORAGE_KEYS,
            'production_seconds': PRODUCTION_SECONDS, 'sale_prices': SALE_PRICES,
            'boats': PLAYER_BOATS, 'fish': FISH, 'upgrades': PROFESSION_UPGRADES, 'recipes': RECIPES, 'edible': EDIBLE,
            'storage_goods': STORAGE_GOODS, 'fixed_storage': FIXED_STORAGE}
