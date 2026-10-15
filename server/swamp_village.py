"""Reedhaven's dense, raised marsh quarter and safe saved-world expansion."""
from spatial import occupied_tiles
from collections import deque
from functools import lru_cache

# Bounds of the previous square quarter, retained for safe saved-world upgrades.
MIN_OFFSET, MAX_OFFSET = -19, 18
LEGACY_SITES = tuple((x, y) for y in (-16, -12, 12, 16)
              for x in (-16, -12, -8, -4, 4, 8, 12, 16)) + tuple(
                  (x, y) for x in (-12, 12) for y in (-8, -4, 4, 8)) + ((-16, -4), (16, -4))
# A crooked ribbon of neighborhoods, inspired by Giethoorn's Binnenpad.
# The old central landmarks keep their positions and story/trade identities.
SITES = ((-23, -8), (-20, -9), (-18, -7), (-22, -4), (-18, -1),
         (-15, -5), (-14, 3), (-17, 3), (-15, 6), (-11, 10), (-8, 10), (-8, 14),
         (-11, -19), (-7, -18), (-12, -15), (-6, -13), (-3, -17), (-2, -13),
         (2, -11), (5, -12), (9, -12), (11, -9), (5, -7),
         (11, -3), (14, -1), (19, -2), (12, 4), (20, 3), (22, 5),
         (24, 8), (27, 8), (23, 12), (19, 12),
         (-3, 11), (3, 10), (4, 14), (8, 13), (9, 18), (13, 19),
         (6, 21), (12, 22), (0, 15))
STREETS = (
    ((-25, -6), (-19, -6), (-19, -3), (-13, -3), (-13, 0), (0, 0)),
    ((0, 0), (0, -5), (-1, -5), (-1, -10), (-5, -10), (-5, -15), (-9, -15), (-9, -21)),
    ((0, 0), (8, 0), (8, 2), (15, 2), (15, 7), (20, 7), (20, 10), (29, 10)),
    ((0, 0), (0, 6), (1, 6), (1, 12), (6, 12), (6, 16), (10, 16), (10, 24)),
    ((-13, 0), (-13, 7), (-10, 7), (-10, 12), (-5, 12), (-5, 16)),
    ((-5, -10), (8, -10), (8, -4), (20, -4)),
)
RADIUS = 31


def buildings(sites):
    for i, (x, y) in enumerate(sites):
        tavern = (x, y) == (12, 4)
        yield dict(kind='village_market' if i % 7 == 0 and not tavern else 'village_hut',
                   x=x, y=y, variant='tavern' if tavern else
                   'stall' if i % 7 == 0 else 'workshop' if i % 5 == 0 else
                   ('reed_house', 'net_house', 'moss_house')[i % 3],
                   label='The Crooked Candle' if tavern else
                   'Marsh market stall' if i % 7 == 0 else
                   'Reedworker’s shed' if i % 5 == 0 else 'Stilt cottage')


BUILDINGS = tuple(buildings(SITES))
NAMES = ('Aster', 'Burdock', 'Cress', 'Dill', 'Elva', 'Fennel', 'Greta', 'Hollis',
         'Ivo', 'Juniper', 'Kel', 'Lark', 'Mallow', 'Nettle', 'Odo', 'Perrin',
         'Quill', 'Rowan', 'Sorrel', 'Thistle')
ROLES = ('basket weaver', 'reed cutter', 'marsh herbalist', 'marsh fisher', 'candle maker')
SLEEPERS = (('Bramble', 13, 5), ('Dewey', 11, 6), ('Gus', 13, 9), ('Hob', -11, 9))


def expanded(village):
    return village.get('swamp_version') in (1, 2) and village.get('layout_version') == 2


def legacy_additions():
    for index, building in enumerate(buildings(LEGACY_SITES)):
        yield building
        x, y = building['x'], building['y']
        for variant, dx, dy in (('bench', 1, 0), ('candle_lantern', -1, 1),
                                 (('ferns', 'cattails', 'mushrooms', 'reeds')[index % 4], 0, -1)):
            yield dict(kind='village_decor', x=x+dx, y=y+dy, variant=variant,
                       label=variant.replace('_', ' ').capitalize())
    for x, y in ((10, 5), (14, 4), (14, 8), (-10, 8)):
        yield dict(kind='village_decor', x=x, y=y, variant='barrels', label='Ale barrels')


def organic(village):
    return expanded(village) and village['swamp_version'] == 2


@lru_cache(maxsize=1)
def layout():
    # Resolve the shared core after villages has finished importing this module.
    from villages import BASE_BUILDINGS, EXTRA_BUILDINGS, LIFE_BUILDINGS, PLANTINGS
    core = {(x, y) for _, x, y in BASE_BUILDINGS + EXTRA_BUILDINGS + PLANTINGS}
    core.update((x, y) for _, x, y, _, _ in LIFE_BUILDINGS)
    blocked = core | set(SITES)
    canal = {(x, y) for x in (16, 17) for y in range(-8, 15)}
    roads = {}

    def connect(start, goals):
        queue, parents = deque([start]), {start: None}
        while queue:
            point = queue.popleft()
            if point in goals:
                while point is not None:
                    roads[point] = 'w' if point in canal else 'p'
                    point = parents[point]
                return
            x, y = point
            for next_ in ((x+1, y), (x, y+1), (x-1, y), (x, y-1)):
                if next_ in parents or next_ in blocked or max(map(abs, next_)) > RADIUS-1:
                    continue
                parents[next_] = point
                queue.append(next_)
        raise ValueError(f'Unreachable Reedhaven lane: {start}')

    for street in STREETS:
        for start, goal in zip(street, street[1:]):
            connect(start, {goal})
    # The main causeway is two tiles wide, with narrower residential branches.
    for x, y in tuple(roads):
        if (x, y+1) not in blocked and abs(y) < 10:
            roads[x, y+1] = 'w' if (x, y+1) in canal else 'p'
    homes = []
    for x, y in SITES:
        home = min(((x, y+1), (x+1, y), (x-1, y), (x, y-1)),
                   key=lambda p: min(abs(p[0]-rx)+abs(p[1]-ry) for rx, ry in roads))
        connect(home, roads)
        homes.append(home)
    # Join the saved traders, workplaces, village hall, and tavern patrons.
    stops = ((-1, 1), (1, 1), (-6, -1), (6, -1), (-6, 4), (6, 4), (-1, -6), (1, 7),
             (-4, -4), (6, -7), (-4, 6), (-6, -7), (2, 8), (-2, -7), (-2, 8), (2, 3),
             (10, 6), (11, 6), (10, 7), (11, 7), (10, 8), (11, 8))
    for stop in stops + tuple((x, y) for _, x, y in SLEEPERS):
        connect(stop, roads)
    # An uneven market green and a smaller courtyard in front of the tavern.
    for cx, cy, rx, ry in ((2, 3, 4, 3), (12, 6, 3, 2)):
        for y in range(cy-ry, cy+ry+1):
            for x in range(cx-rx, cx+rx+1):
                if ((x-cx)/rx)**2 + ((y-cy)/ry)**2 <= 1 and (x, y) not in blocked:
                    roads[x, y] = 'c'
    decor, reserved = [], blocked | set(roads) | canal | set(homes)
    offsets = ((-1, 0), (1, 0), (0, -1), (0, 1), (-1, 1), (1, 1), (-1, -1), (1, -1),
               (-2, 0), (2, 0), (0, -2), (0, 2))
    for index, (x, y) in enumerate(SITES):
        for variant in ('bench', 'candle_lantern', ('ferns', 'cattails', 'mushrooms', 'reeds')[index % 4]):
            for dx, dy in offsets:
                point = x+dx, y+dy
                if point not in reserved:
                    reserved.add(point)
                    decor.append(dict(kind='village_decor', x=point[0], y=point[1],
                                      variant=variant, label=variant.replace('_', ' ').capitalize()))
                    break
            else:
                raise ValueError(f'No decoration space beside {x}, {y}')
    for x, y in ((10, 5), (14, 4), (14, 8), (-10, 8)):
        if (x, y) in reserved:
            candidates = ((x+dx, y+dy) for dx in range(-4, 5) for dy in range(-4, 5)
                          if (x+dx, y+dy) not in reserved)
            x, y = min(candidates,
                       key=lambda p: abs(p[0]-x)+abs(p[1]-y))
        reserved.add((x, y))
        decor.append(dict(kind='village_decor', x=x, y=y, variant='barrels', label='Ale barrels'))
    footprint = {(x, y) for y in range(-9, 10) for x in range(-9, 10)}
    for x, y in roads.keys() | reserved:
        radius = 2 if (x, y) in SITES else 1
        footprint.update((x+dx, y+dy) for dx in range(-radius, radius+1)
                         for dy in range(-radius, radius+1) if dx*dx+dy*dy <= radius*radius+1)
    return dict(roads=roads, homes=homes, decor=decor, footprint=footprint,
                water=canal - set(roads) - blocked)


def additions(version=2):
    if version == 1:
        yield from legacy_additions()
    else:
        yield from BUILDINGS
        yield from layout()['decor']


def ensure(world):
    landscape = world['shared_map']
    for village in landscape['villages']:
        if (village['name'] != 'Reedhaven' or organic(village)
                or village.get('layout_version') != 2 or village.get('life_version') != 1):
            continue
        vx, vy = village['x'], village['y']
        obstacles = [item for town in world['towns'].values()
                     for field in ('buildings', 'roads') for item in town.get(field, [])]
        obstacles += [n for n in landscape.get('npcs', []) if n.get('village') != village['name']]
        touched = layout()['footprint'] | layout()['water']
        if village.get('swamp_version') == 1:
            touched = touched | {(x, y) for y in range(MIN_OFFSET, MAX_OFFSET+1)
                                 for x in range(MIN_OFFSET, MAX_OFFSET+1)}
        if (any((x-vx, y-vy) in touched for x, y in occupied_tiles(obstacles))
                or any(not (0 <= vx+x < landscape['map_size'] and 0 <= vy+y < landscape['map_size'])
                       for x, y in touched)):
            continue
        # Only the new quarter moves. Transfer its visit keys using the same
        # migration helper as other relocated locals; original traders stay put.
        from npcs import local_key, move_local_visits
        residents = {n['name']: n for n in landscape['npcs'] if n.get('village') == village['name']}
        moved = {}
        for i, name in enumerate(NAMES):
            x, y = layout()['homes'][i]
            npc = residents.get(name)
            if npc is None:
                npc = dict(name=name, role=ROLES[i % len(ROLES)], biome='b', village='Reedhaven',
                           dialogue='Crowded at the Crooked Candle!' if i % 2 else 'Reed roofs, willow boats, warm ale.')
                landscape['npcs'].append(npc)
            else:
                old_key = local_key(npc)
            npc.update(x=vx+x, y=vy+y, ambient_stop=[x, y])
            npc.pop('routine', None)
            if name in residents:
                moved[old_key] = local_key(npc)
        for name, x, y in SLEEPERS:
            if name not in residents:
                landscape['npcs'].append(dict(name=name, role='sleeping tavern patron',
                    x=vx+x, y=vy+y, biome='b', village='Reedhaven', pose='passed_out',
                    dialogue='Zzz… too much marsh ale…'))
        for town in world['towns'].values():
            move_local_visits(town, moved)
        village['swamp_version'] = 2
        for npc in residents.values():
            state = npc.get('routine')
            if state:
                # Discard old routes through the newly restored marsh pockets.
                point = round(state['x'])-vx, round(state['y'])-vy
                if point not in layout()['roads']:
                    x, y = min(layout()['roads'], key=lambda p: abs(p[0]-point[0])+abs(p[1]-point[1]))
                    state.update(x=vx+x, y=vy+y)
                state.update(path=[], phase='idle', wait=0, origin=[state['x'], state['y']])
        manager = world.get('_chunks')
        if manager:
            # Upgrade active and camera-cached chunks together. Rebind their
            # resources before the next collision query or snapshot can use them.
            for key, record in list(manager.records.items()) + list(manager.views.items()):
                apply(record, key, [village], manager.landscape.get('chunk_size', 40))
            for field in ('map_resources', 'tree_stumps'):
                landscape[field] = [r for record in manager.records.values() for r in record[field]]
            manager.membership = None
            manager.resource_index = None


def apply(record, key, villages, chunk_size):
    previous = record.get('swamp_version', 0)
    left, top = (int(n)*chunk_size for n in key.split(','))
    for village in villages:
        if not expanded(village) or previous >= village['swamp_version']:
            continue
        if village['swamp_version'] == 1:
            # A saved quarter protected by player construction stays unchanged.
            return apply_legacy(record, key, [village], chunk_size)
        vx, vy = village['x'], village['y']
        inside = lambda x, y: left <= vx+x < left+chunk_size and top <= vy+y < top+chunk_size
        data = layout()
        touched = data['footprint'] | data['water']
        if not any(inside(x, y) for x, y in touched):
            continue
        rows = [list(row) for row in record['terrain']]
        core = lambda x, y: max(abs(x-vx), abs(y-vy)) <= 9
        for field in ('map_resources', 'tree_stumps'):
            record[field] = [r for r in record[field] if core(r['x'], r['y']) or
                not ((r['x']-vx, r['y']-vy) in touched or r.get('village') == village['name'])]
        if previous == 1:
            # Re-wet the unused corners of the old square without disturbing
            # the harvested plants in the established central neighborhood.
            for y in range(MIN_OFFSET, MAX_OFFSET+1):
                for x in range(MIN_OFFSET, MAX_OFFSET+1):
                    if (x, y) not in touched and inside(x, y):
                        rows[vy+y-top][vx+x-left] = 'b'
                        if ((x*11+y*7) % 9 == 0
                                and not any(r['x'] == vx+x and r['y'] == vy+y for r in record['map_resources'])):
                            record['map_resources'].append(dict(kind='tree', x=vx+x, y=vy+y))
        for x, y in touched:
            if inside(x, y) and not core(vx+x, vy+y):
                rows[vy+y-top][vx+x-left] = 'b' if (x, y) in data['water'] else 'g'
        for (x, y), material in data['roads'].items():
            if inside(x, y):
                rows[vy+y-top][vx+x-left] = material
        for item in additions():
            x, y = item['x'], item['y']
            if inside(x, y):
                record['map_resources'].append(dict(item, x=vx+x, y=vy+y, biome='b', village=village['name']))
        for x, y in sorted(data['water']):
            if inside(x, y):
                record['map_resources'].append(dict(kind='water', x=vx+x, y=vy+y))
        record['terrain'] = [''.join(row) for row in rows]
        record['swamp_version'] = 2
    return record


def apply_legacy(record, key, villages, chunk_size):
    if record.get('swamp_version') == 1:
        return record
    left, top = (int(n)*chunk_size for n in key.split(','))
    rows = None
    for village in villages:
        if not expanded(village):
            continue
        vx, vy = village['x'], village['y']
        if (left > vx+MAX_OFFSET or left+chunk_size <= vx+MIN_OFFSET
                or top > vy+MAX_OFFSET or top+chunk_size <= vy+MIN_OFFSET):
            continue
        if rows is None:
            rows = [list(row) for row in record['terrain']]
        # Keep the old center's harvested plants and resource progress intact.
        cleared = set()
        for y in range(max(top, vy+MIN_OFFSET), min(top+chunk_size, vy+MAX_OFFSET+1)):
            for x in range(max(left, vx+MIN_OFFSET), min(left+chunk_size, vx+MAX_OFFSET+1)):
                if max(abs(x-vx), abs(y-vy)) > 9:
                    rows[y-top][x-left] = 'g'
                    cleared.add((x, y))
        for field in ('map_resources', 'tree_stumps'):
            record[field] = [r for r in record[field] if (r['x'], r['y']) not in cleared
                             or r.get('village') == village['name']]
        for item in legacy_additions():
            x, y = vx+item['x'], vy+item['y']
            if left <= x < left+chunk_size and top <= y < top+chunk_size:
                record['map_resources'].append(dict(item, x=x, y=y, biome='b', village=village['name']))
    if rows is not None:
        record['terrain'] = [''.join(row) for row in rows]
        record['swamp_version'] = 1
    return record
