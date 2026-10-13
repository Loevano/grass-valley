"""One saved landscape and separated town sites for new worlds."""
import random
import secrets

from npcs import ensure_npcs
from terrain import generate_map

MAP_FIELDS = ('map_size', 'terrain_seed', 'terrain', 'map_resources', 'tree_stumps', 'npcs', 'npc_roster_version', 'npc_placement_version', 'forage_version', 'forage_halls',
              'chunked', 'chunk_size', 'chunk_clock', 'starter_offset', 'villages', 'industry_version', 'mineral_version', 'terrain_version')
TOWN_RADIUS = 15
HALL_SEPARATION = 40


def building_range(town):
    return 30 + 5 * (town['level'] - 1)


def new_world(seed=None, explorable=False, chunk_directory=None, terrain_version=None, sandbox=False):
    seed = secrets.randbits(64) if seed is None else seed
    rng = random.Random(seed)
    sites = [{'x': rng.randint(lo_x, lo_x + 16), 'y': rng.randint(lo_y, lo_y + 16)}
             for lo_x, lo_y in [(22, 22), (82, 22), (22, 82), (82, 82)]]
    rng.shuffle(sites)
    terrain_version = (6 if explorable else 1) if terrain_version is None else terrain_version
    if terrain_version in (2, 3, 4, 5, 6):
        from chunk_world import regional_starter
        terrain, resources = regional_starter(seed, version=terrain_version)
    elif terrain_version == 1:
        terrain, resources = generate_map(120, seed)
    else:
        raise ValueError(f'Unsupported terrain generator version: {terrain_version}')
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
        rows[y][x] = 'm' if terrain_version >= 6 else 'g'
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
    if terrain_version >= 6:
        for resource in starters:
            if resource['kind'] == 'tree':
                rows[resource['y']][resource['x']] = 'f'
    if terrain_version >= 2:
        # Small fishing lakes sit outside the hall clearings and connecting
        # paths, within each town's initial construction range.
        ponds = {(site['x'] + 9 + dx, site['y'] - 8 + dy)
                 for site in sites for dy in range(-3, 4) for dx in range(-5, 6)
                 if (dx / 5) ** 2 + (dy / 3) ** 2 <= 1}
        resources = [r for r in resources if (r['x'], r['y']) not in ponds]
        for x, y in sorted(ponds):
            rows[y][x] = 's'
            resources.append({'kind': 'water', 'x': x, 'y': y})
    # A broad inland lake with water-only access to its treasure islands.
    lake_x, lake_y = rng.randint(54, 56), rng.randint(57, 61)
    lake = {(x, y) for y in range(lake_y - 14, lake_y + 15)
            for x in range(lake_x - 14, lake_x + 15)
            if ((x - lake_x) / 14) ** 2 + ((y - lake_y) / 14) ** 2 <= 1}
    islands = [(lake_x - 7, lake_y - 3), (lake_x + 4, lake_y + 4), (lake_x + 9, lake_y - 5)]
    island_tiles = {(cx + dx, cy + dy) for cx, cy in islands
                    for dx in range(-2, 3) for dy in range(-2, 3) if dx * dx + dy * dy <= 5}
    resources = [r for r in resources if (r['x'], r['y']) not in lake]
    for x, y in sorted(lake):
        rows[y][x] = 'g' if (x, y) in island_tiles else 's'
        if (x, y) not in island_tiles:
            resources.append({'kind': 'water', 'x': x, 'y': y})
    resources.extend({'kind': 'chest', 'x': x, 'y': y} for x, y in islands)
    landscape = {'map_size': 120, 'terrain_seed': seed,
                 'terrain': [''.join(row) for row in rows], 'map_resources': resources, 'tree_stumps': []}
    if terrain_version >= 2:
        landscape['terrain_version'] = terrain_version
    npc_map = dict(landscape, buildings=[])
    ensure_npcs(npc_map)
    landscape['npc_roster_version'] = npc_map['npc_roster_version']
    landscape['npc_placement_version'] = npc_map['npc_placement_version']
    landscape['npcs'] = [n for n in npc_map['npcs']
                         if all(abs(n['x'] - s['x']) + abs(n['y'] - s['y']) > 6 for s in sites)]
    world = {'towns': {}, 'shared_map': landscape, 'town_sites': sites}
    if sandbox:
        world['sandbox'] = True
    if explorable:
        from chunk_world import expand_new_world
        return expand_new_world(world, chunk_directory)
    return world


def bind_map(world):
    if 'shared_map' in world:
        for town in world['towns'].values():
            town.update(world['shared_map'])


def export_world(world):
    if 'shared_map' not in world:
        return world
    result = {k: v for k, v in world.items() if k != '_chunks'}
    result['towns'] = {key: {k: v for k, v in town.items() if k not in MAP_FIELDS}
                       for key, town in world['towns'].items()}
    if world['shared_map'].get('chunked'):
        result['shared_map'] = {k: v for k, v in world['shared_map'].items()
                               if k not in ('terrain', 'map_resources', 'tree_stumps')}
    return result


def walking_map(world, town, local=False):
    if local and world.get('_chunks'):
        town = world['_chunks'].movement(town)
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
