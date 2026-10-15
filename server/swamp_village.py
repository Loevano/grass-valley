"""Reedhaven's dense, raised marsh quarter and safe saved-world expansion."""
from spatial import occupied_tiles

# Twice the former 19-tile side: four times the settled ground area.
MIN_OFFSET, MAX_OFFSET = -19, 18
SITES = tuple((x, y) for y in (-16, -12, 12, 16)
              for x in (-16, -12, -8, -4, 4, 8, 12, 16)) + tuple(
                  (x, y) for x in (-12, 12) for y in (-8, -4, 4, 8)) + ((-16, -4), (16, -4))
BUILDINGS = tuple(dict(kind='village_market' if i % 7 == 0 else 'village_hut',
                      x=x, y=y, variant='tavern' if (x, y) == (12, 4) else
                      'stall' if i % 7 == 0 else 'workshop' if i % 5 == 0 else
                      ('reed_house', 'net_house', 'moss_house')[i % 3],
                      label='The Crooked Candle' if (x, y) == (12, 4) else
                      'Marsh market stall' if i % 7 == 0 else
                      'Reedworker’s shed' if i % 5 == 0 else 'Stilt cottage')
                  for i, (x, y) in enumerate(SITES))
NAMES = ('Aster', 'Burdock', 'Cress', 'Dill', 'Elva', 'Fennel', 'Greta', 'Hollis',
         'Ivo', 'Juniper', 'Kel', 'Lark', 'Mallow', 'Nettle', 'Odo', 'Perrin',
         'Quill', 'Rowan', 'Sorrel', 'Thistle')
ROLES = ('basket weaver', 'reed cutter', 'marsh herbalist', 'marsh fisher', 'candle maker')
SLEEPERS = (('Bramble', 13, 5), ('Dewey', 11, 6), ('Gus', 13, 9), ('Hob', -11, 9))


def expanded(village):
    return village.get('swamp_version') == 1 and village.get('layout_version') == 2


def additions():
    for index, building in enumerate(BUILDINGS):
        yield building
        x, y = building['x'], building['y']
        for variant, dx, dy in (('bench', 1, 0), ('candle_lantern', -1, 1),
                                 (('ferns', 'cattails', 'mushrooms', 'reeds')[index % 4], 0, -1)):
            yield dict(kind='village_decor', x=x+dx, y=y+dy, variant=variant,
                       label=variant.replace('_', ' ').capitalize())
    for x, y in ((10, 5), (14, 4), (14, 8), (-10, 8)):
        yield dict(kind='village_decor', x=x, y=y, variant='barrels', label='Ale barrels')


def ensure(world):
    landscape = world['shared_map']
    for village in landscape['villages']:
        if (village['name'] != 'Reedhaven' or village.get('swamp_version') == 1
                or village.get('layout_version') != 2 or village.get('life_version') != 1):
            continue
        vx, vy = village['x'], village['y']
        obstacles = [item for town in world['towns'].values()
                     for field in ('buildings', 'roads') for item in town.get(field, [])]
        obstacles += [n for n in landscape.get('npcs', []) if n.get('village') != village['name']]
        if any(MIN_OFFSET <= x-vx <= MAX_OFFSET and MIN_OFFSET <= y-vy <= MAX_OFFSET
               for x, y in occupied_tiles(obstacles)):
            continue
        village['swamp_version'] = 1
        for i, name in enumerate(NAMES):
            x, y = SITES[i]
            landscape['npcs'].append(dict(name=name, role=ROLES[i % len(ROLES)],
                x=vx+x, y=vy+y+1, biome='b', village='Reedhaven',
                ambient_stop=[x+1, y+1],
                dialogue=('Crowded at the Crooked Candle!'
                          if i % 2 else 'Reed roofs, willow boats, warm ale.')))
        for name, x, y in SLEEPERS:
            landscape['npcs'].append(dict(name=name, role='sleeping tavern patron',
                x=vx+x, y=vy+y, biome='b', village='Reedhaven', pose='passed_out',
                dialogue='Zzz… too much marsh ale…'))


def apply(record, key, villages, chunk_size):
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
        for item in additions():
            x, y = vx+item['x'], vy+item['y']
            if left <= x < left+chunk_size and top <= y < top+chunk_size:
                record['map_resources'].append(dict(item, x=x, y=y, biome='b', village=village['name']))
    if rows is not None:
        record['terrain'] = [''.join(row) for row in rows]
        record['swamp_version'] = 1
    return record
