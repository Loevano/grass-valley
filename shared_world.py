"""One saved landscape and separated town sites for new worlds."""
import random
import secrets

from npcs import ensure_npcs
from terrain import generate_map

MAP_FIELDS = ('map_size', 'terrain_seed', 'terrain', 'map_resources', 'npcs', 'forage_version', 'forage_halls')
TOWN_RADIUS = 15
HALL_SEPARATION = 40


def building_range(town):
    return 10 + 5 * (town['level'] - 1)


def new_world(seed=None):
    seed = secrets.randbits(64) if seed is None else seed
    rng = random.Random(seed)
    sites = [{'x': rng.randint(lo_x, lo_x + 16), 'y': rng.randint(lo_y, lo_y + 16)}
             for lo_x, lo_y in [(22, 22), (82, 22), (22, 82), (82, 82)]]
    rng.shuffle(sites)
    terrain, resources = generate_map(120, seed)
    rows = [list(row) for row in terrain]
    cleared = set()
    # Clear starter spaces and wide paths so every town can be reached on foot.
    for site in sites:
        for y in range(site['y'] - 5, site['y'] + 6):
            for x in range(site['x'] - 5, site['x'] + 6):
                cleared.add((x, y))
    for site in sites:
        x, y = site['x'], site['y']
        target = sites[0]
        while (x, y) != (target['x'], target['y']):
            for dx in (-1, 0, 1):
                for dy in (-1, 0, 1):
                    cleared.add((x + dx, y + dy))
            if x != target['x']:
                x += 1 if x < target['x'] else -1
            else:
                y += 1 if y < target['y'] else -1
    for x, y in cleared:
        rows[y][x] = 'g'
    resources = [r for r in resources if (r['x'], r['y']) not in cleared]
    # Each clearing has wood and water within its construction area.
    starters = [{'kind': kind, 'x': site['x'] + dx, 'y': site['y'] + dy}
                for site in sites for kind, dx, dy in
                [('tree', -4, -3), ('tree', -4, -2), ('water', 4, -3), ('water', 4, -2)]]
    # Dense timber patches give woodcutters a forest beside each starting town.
    occupied = {(r['x'], r['y']) for r in resources + starters}
    for site in sites:
        for dx in range(-7, -3):
            for dy in range(-5, -1):
                x, y = site['x'] + dx, site['y'] + dy
                if (x, y) not in occupied and rows[y][x] != 'v':
                    resources.append({'kind': 'tree', 'x': x, 'y': y})
                    rows[y][x] = 'f'
    resources.extend(starters)
    landscape = {'map_size': 120, 'terrain_seed': seed,
                 'terrain': [''.join(row) for row in rows], 'map_resources': resources}
    npc_map = dict(landscape, buildings=[])
    ensure_npcs(npc_map)
    landscape['npcs'] = [n for n in npc_map['npcs']
                         if all(abs(n['x'] - s['x']) + abs(n['y'] - s['y']) > 6 for s in sites)]
    return {'towns': {}, 'shared_map': landscape, 'town_sites': sites}


def bind_map(world):
    if 'shared_map' in world:
        for town in world['towns'].values():
            town.update(world['shared_map'])


def export_world(world):
    if 'shared_map' not in world:
        return world
    return dict(world, towns={key: {k: v for k, v in town.items() if k not in MAP_FIELDS}
                              for key, town in world['towns'].items()})


def walking_map(world, town):
    return dict(town, buildings=[b for t in world['towns'].values() for b in t['buildings']],
                roads=[r for t in world['towns'].values() for r in t.get('roads', [])])


def validate_placement(world, town, data):
    x, y = data.get('x'), data.get('y')
    if type(x) is not int or type(y) is not int:
        raise ValueError('Choose a tile inside your town.')
    origin = town['town_origin']
    if abs(x - origin['x']) + abs(y - origin['y']) > max(TOWN_RADIUS, building_range(town)):
        raise ValueError('Build inside your town territory.')
    for other in world['towns'].values():
        if other is not town and any(b['x'] == x and b['y'] == y for b in other['buildings']):
            raise ValueError('Another town occupies this tile.')
        player = other['player']
        if abs(player['x'] - x) < .68 and abs(player['y'] - y) < .68:
            raise ValueError('A player is standing here. Choose another tile.')
    index = data.get('index')
    if (data.get('action') == 'move' and type(index) is int
            and 0 <= index < len(town['buildings']) and town['buildings'][index]['kind'] == 'hall'):
        if any((b['x'] - x) ** 2 + (b['y'] - y) ** 2 < HALL_SEPARATION ** 2
               for other in world['towns'].values() if other is not town
               for b in other['buildings'] if b['kind'] == 'hall'):
            raise ValueError('Keep town halls at least 40 tiles apart.')
