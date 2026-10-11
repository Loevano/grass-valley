"""Small wilderness settlements, persisted independently of player towns."""

VILLAGE_RADIUS = 24
THEMES = (
    ('Reedhaven', 'b', -80, 40, ('Fen', 'Lily'),
     'Our raised cottages keep the marsh at bay. Come into the clearing to trade for swamp boots.'),
    ('Sunwell', 'd', 200, 40, ('Zara', 'Amir'),
     'Welcome to Sunwell. The dunes stretch beyond our sandstone homes. I sell light desert clothing for the journey.'),
    ('Frostbrook', 'i', 40, -80, ('Eira', 'Boreal'),
     'Our snowy village is a warm stop on the ice fields. Stay awhile and browse our travel supplies.'),
)


def ensure_villages(world):
    landscape = world['shared_map']
    if 'villages' in landscape:
        return
    offset = landscape.get('starter_offset', 880)
    size = landscape['map_size']
    obstacles = [item for town in world['towns'].values()
                 for field in ('buildings', 'roads') for item in town.get(field, [])]
    obstacles += landscape.get('npcs', [])
    villages = []
    for name, biome, dx, dy, names, dialogue in THEMES:
        # Prefer the wilderness beside the starter valley. In old worlds, move
        # the whole settlement away from anything players have already built.
        preferred = offset + dx, offset + dy
        candidates = [preferred] + [(x, y) for y in range(40, size - 40, 40)
                                    for x in range(40, size - 40, 40)]
        for x, y in candidates:
            if not (VILLAGE_RADIUS < x < size - VILLAGE_RADIUS
                    and VILLAGE_RADIUS < y < size - VILLAGE_RADIUS):
                continue
            if (offset - VILLAGE_RADIUS <= x < offset + 120 + VILLAGE_RADIUS
                    and offset - VILLAGE_RADIUS <= y < offset + 120 + VILLAGE_RADIUS):
                continue
            if any(abs(x - o['x']) <= VILLAGE_RADIUS + 2
                   and abs(y - o['y']) <= VILLAGE_RADIUS + 2 for o in obstacles):
                continue
            if any(abs(x - v['x']) < 2 * VILLAGE_RADIUS + 4
                   and abs(y - v['y']) < 2 * VILLAGE_RADIUS + 4 for v in villages):
                continue
            villages.append({'name': name, 'biome': biome, 'x': x, 'y': y})
            for resident, rx in zip(names, (-1, 1)):
                landscape.setdefault('npcs', []).append({
                    'name': resident, 'role': f'{name} village trader',
                    'dialogue': dialogue, 'biome': biome, 'village': name,
                    'x': x + rx, 'y': y + 1})
            break
    landscape['villages'] = villages


def apply_villages(record, key, villages, chunk_size):
    """Overlay once per saved chunk, preserving subsequent resource changes."""
    if record.get('village_version') == 1:
        return record
    cx, cy = map(int, key.split(','))
    left, top = cx * chunk_size, cy * chunk_size
    rows = None
    for village in villages:
        vx, vy = village['x'], village['y']
        if (left > vx + VILLAGE_RADIUS or left + chunk_size <= vx - VILLAGE_RADIUS
                or top > vy + VILLAGE_RADIUS or top + chunk_size <= vy - VILLAGE_RADIUS):
            continue
        if rows is None:
            rows = [list(row) for row in record['terrain']]
        cleared = set()
        for y in range(max(top, vy - VILLAGE_RADIUS), min(top + chunk_size, vy + VILLAGE_RADIUS + 1)):
            for x in range(max(left, vx - VILLAGE_RADIUS), min(left + chunk_size, vx + VILLAGE_RADIUS + 1)):
                dx, dy = x - vx, y - vy
                if dx * dx + dy * dy > VILLAGE_RADIUS ** 2:
                    continue
                rows[y - top][x - left] = village['biome']
                # Open village square and wide approaches through marsh water.
                if max(abs(dx), abs(dy)) <= 5 or abs(dx) <= 1 or abs(dy) <= 1:
                    cleared.add((x, y))
                    if village['biome'] == 'b':
                        rows[y - top][x - left] = 'g'
        for field in ('map_resources', 'tree_stumps'):
            record[field] = [r for r in record[field]
                             if (r['x'], r['y']) not in cleared
                             and not (village['biome'] in ('d', 'i')
                                      and (r['x'] - vx) ** 2 + (r['y'] - vy) ** 2 <= VILLAGE_RADIUS ** 2)]
        for kind, dx, dy in [('village_hut', -3, -2), ('village_hut', 3, -2),
                             ('village_hut', -3, 3), ('village_market', 3, 3)]:
            x, y = vx + dx, vy + dy
            if left <= x < left + chunk_size and top <= y < top + chunk_size:
                record['map_resources'].append(dict(kind=kind, x=x, y=y,
                                                    biome=village['biome'], village=village['name']))
    if rows is not None:
        record['terrain'] = [''.join(row) for row in rows]
    record['village_version'] = 1
    return record
