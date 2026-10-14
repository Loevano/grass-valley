"""Small wilderness settlements, persisted independently of player towns."""
from spatial import occupied_tiles

VILLAGE_RADIUS = 24
THEMES = (
    ('Reedhaven', 'b', -80, 40, ('Fen', 'Lily'),
     'Our raised cottages keep the marsh at bay. I trade swamp boots: wear them to reach dark swamp ground and gather 20% more herbs.'),
    ('Sunwell', 'd', 200, 40, ('Zara', 'Amir'),
     'Welcome to Sunwell. The dunes stretch beyond our sandstone homes. I trade desert clothing: wear it to gather 20% more berries.'),
    ('Frostbrook', 'i', 40, -80, ('Eira', 'Boreal'),
     'Our snowy village is a warm stop on the ice fields. Stay awhile and rest before your next journey.'),
)


# Shared story threads are introductions for future quests, not active objectives.
RESIDENTS = {
    'Reedhaven': (
        ('Sedge', 'marsh herbalist', -6, -1, 'The remedies at Willow Still are running low. A herb shipment vanished along the lantern causeway; Fen last saw its boat.'),
        ('Bracken', 'reed boatwright', 6, -1, 'I mend flat-bottomed boats at Reedworks. The missing herb boat had a split pole; someone may have taken shelter at the old sluice.'),
        ('Nessa', 'lantern keeper', -6, 4, 'I keep the causeway lanterns lit. One went dark the night our herb shipment disappeared. Sedge worries about the travelers waiting for medicine.'),
        ('Osier', 'marsh fisher', 6, 4, 'A loose reed basket drifted past the old sluice. Bracken recognized its weave from the missing shipment.'),
        ('Tansy', 'remedy apprentice', -1, -6, 'Sedge taught me to dry willow leaves at Willow Still. I saved a little medicine for the boat crew, if they return.'),
        ('Mirel', 'causeway innkeeper', 1, 7, 'The Lantern Rest keeps a warm bed for every lost traveler. Nessa and I are listening for word of the missing herb boat.'),
    ),
    'Sunwell': (
        ('Safiya', 'caravan keeper', -6, -1, 'Our spice caravan is late. Its last message mentioned the buried mile markers beyond Sunwell. Amir knows the old route.'),
        ('Idris', 'sandstone mason', 6, -1, 'I repair the well and caravan stones. A sandstorm covered the route markers; Safiya fears the caravan followed the wrong ridge.'),
        ('Noor', 'well tender', -6, 4, 'I keep water ready at the Sunwell cistern. The overdue caravan carried spare well ropes; Zara is saving provisions for its return.'),
        ('Rafi', 'dune scout', 6, 4, 'I found a strip of caravan cloth near a buried marker. Idris remembers where the older road crossed the dunes.'),
        ('Samira', 'cloth dyer', -1, -6, 'At Dune Loom I dye cloth bright enough to see through blowing sand. Rafi brought back a torn strip in our caravan colors.'),
        ('Farid', 'caravan innkeeper', 1, 7, 'The Amber Rest has empty beds waiting for Safiya’s caravan. Their last letter spoke of a sheltered ruin beside the old road.'),
    ),
    'Frostbrook': (
        ('Siv', 'hearth keeper', -6, -1, 'The northern watchpost has stopped lighting its beacon. Eira is keeping our hearth warm for the wardens when they return.'),
        ('Hakon', 'winter woodworker', 6, -1, 'I split timber at Pineworks. The watchpost asked for a new beacon frame before the blizzard; Boreal still has their message.'),
        ('Yrsa', 'ice fisher', -6, 4, 'I saw tracks leading away from the silent watchpost toward the frozen brook. Siv hopes the wardens found shelter.'),
        ('Leif', 'trail warden', 6, 4, 'The watchpost beacon guides travelers across the ice. Hakon and I are checking the old trail for fallen branches.'),
        ('Astrid', 'wool weaver', -1, -6, 'I keep blankets ready at Snowthread. Yrsa found a warden’s scarf by the brook; its owner may be waiting out the storm.'),
        ('Inga', 'winter innkeeper', 1, 7, 'The Hearth Rest has hot broth for anyone coming off the ice. Leif promised to bring news from the silent watchpost.'),
    ),
}
BASE_BUILDINGS = (('village_hut', -3, -2), ('village_hut', 3, -2),
                  ('village_hut', -3, 3), ('village_market', 3, 3))
EXTRA_BUILDINGS = (('village_hut', -7, -4), ('village_hut', 7, -4),
                   ('village_hut', -7, 6), ('village_hut', 7, 6),
                   ('village_hut', -3, -7), ('village_hut', 3, 8))
LIFE_BUILDINGS = (('village_hut', -3, 8, 'hall', 'Village hall'),
                  ('village_hut', -7, -7, 'barn', 'Animal shelter'),
                  ('village_market', 3, 5, 'stall', 'Produce stall'),
                  ('village_market', 5, 3, 'stall', 'Travelers’ stall'))
PLANTINGS = (('tree', -5, -4), ('tree', 5, -4),
             ('herb_patch', -5, 6), ('berry_bush', 5, 6),
             ('herb_patch', 3, -6), ('berry_bush', -4, 7), ('water', 7, -7))
VILLAGE_ANIMALS = {'b': {'chicken': 3, 'pig': 1},
                   'd': {'horse': 2, 'chicken': 2},
                   'i': {'sheep': 3, 'chicken': 1}}
LANDMARKS = {
    'Reedhaven': ('Willow Still', 'Reedworks', 'Lantern Rest'),
    'Sunwell': ('Sunwell cistern', 'Dune Loom', 'Amber Rest'),
    'Frostbrook': ('Pineworks', 'Snowthread', 'Hearth Rest'),
}


def expand_villages(world):
    landscape = world['shared_map']
    for village in landscape['villages']:
        if village.get('layout_version') in (1, 2):
            continue
        vx, vy = village['x'], village['y']
        obstacles = [item for town in world['towns'].values()
                     for field in ('buildings', 'roads') for item in town.get(field, [])]
        obstacles += [n for n in landscape.get('npcs', []) if n.get('village') != village['name']]
        # Existing player work takes priority over expanding an old settlement.
        if any(max(abs(x - vx), abs(y - vy)) <= 10 for x, y in occupied_tiles(obstacles)):
            village['layout_version'] = 1
            continue
        for name, role, dx, dy, dialogue in RESIDENTS[village['name']]:
            landscape.setdefault('npcs', []).append(dict(
                name=name, role=role, x=vx + dx, y=vy + dy, dialogue=dialogue,
                biome=village['biome'], village=village['name']))
        village['layout_version'] = 2


def ensure_villages(world):
    landscape = world['shared_map']
    dialogue_by_village = {name: dialogue for name, _, _, _, _, dialogue in THEMES}
    for npc in landscape.get('npcs', []):
        if npc.get('village') in dialogue_by_village and npc['name'] in next(
                theme[4] for theme in THEMES if theme[0] == npc['village']):
            npc['dialogue'] = dialogue_by_village[npc['village']]
    if 'villages' in landscape:
        expand_villages(world)
        ensure_village_life(world)
        return
    offset = landscape.get('starter_offset', 880)
    size = landscape['map_size']
    obstacles = [item for town in world['towns'].values()
                 for field in ('buildings', 'roads') for item in town.get(field, [])]
    obstacles += landscape.get('npcs', [])
    occupied = occupied_tiles(obstacles)
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
            if any(abs(x - ox) <= VILLAGE_RADIUS + 2
                   and abs(y - oy) <= VILLAGE_RADIUS + 2 for ox, oy in occupied):
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
    expand_villages(world)
    ensure_village_life(world)


def ensure_village_life(world):
    obstacles = [item for town in world['towns'].values()
                 for field in ('buildings', 'roads') for item in town.get(field, [])]
    for village in world['shared_map']['villages']:
        if 'life_version' in village or village.get('layout_version') != 2:
            continue
        footprints = {(village['x'] + dx, village['y'] + dy)
                      for _, dx, dy in PLANTINGS}
        footprints.update((village['x'] + dx, village['y'] + dy)
                          for _, dx, dy, _, _ in LIFE_BUILDINGS)
        village['life_version'] = 0 if occupied_tiles(obstacles).intersection(footprints) else 1


def apply_villages(record, key, villages, chunk_size):
    """Overlay once per saved chunk, preserving subsequent resource changes."""
    previous_version = record.get('village_version', 0)
    if previous_version == 3:
        return record
    cx, cy = map(int, key.split(','))
    left, top = cx * chunk_size, cy * chunk_size
    rows = None
    for village in villages:
        vx, vy = village['x'], village['y']
        expanded = village.get('layout_version') == 2
        if previous_version >= 2 or (previous_version == 1 and not expanded):
            if village.get('life_version') == 1:
                add_village_life(record, village, left, top, chunk_size)
            continue
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
                if previous_version == 0:
                    rows[y - top][x - left] = village['biome']
                # Open village square and wide approaches through marsh water.
                if (max(abs(dx), abs(dy)) <= (9 if expanded else 5)
                        or (previous_version == 0 and (abs(dx) <= 1 or abs(dy) <= 1))):
                    cleared.add((x, y))
                    if village['biome'] == 'b':
                        rows[y - top][x - left] = 'g'
        for field in ('map_resources', 'tree_stumps'):
            record[field] = [r for r in record[field]
                             if (r['x'], r['y']) not in cleared
                             and not (previous_version == 0 and village['biome'] in ('d', 'i')
                                      and (r['x'] - vx) ** 2 + (r['y'] - vy) ** 2 <= VILLAGE_RADIUS ** 2)]
        buildings = BASE_BUILDINGS + (EXTRA_BUILDINGS if expanded else ())
        for index, (kind, dx, dy) in enumerate(buildings):
            x, y = vx + dx, vy + dy
            if left <= x < left + chunk_size and top <= y < top + chunk_size:
                structure = dict(kind=kind, x=x, y=y, biome=village['biome'], village=village['name'])
                if index in (4, 8, 9):
                    structure.update(variant={4: 'workshop', 8: 'workshop', 9: 'inn'}[index],
                                     label=LANDMARKS[village['name']][{4: 0, 8: 1, 9: 2}[index]])
                record['map_resources'].append(structure)
        if village.get('life_version') == 1:
            # Commit the terrain first so planted green pockets survive below.
            record['terrain'] = [''.join(row) for row in rows]
            add_village_life(record, village, left, top, chunk_size)
            rows = [list(row) for row in record['terrain']]
    if rows is not None:
        record['terrain'] = [''.join(row) for row in rows]
    record['village_version'] = 3
    return record


def add_village_life(record, village, left, top, chunk_size):
    rows = [list(row) for row in record['terrain']]
    additions = []
    vx, vy = village['x'], village['y']
    for kind, dx, dy, variant, label in LIFE_BUILDINGS:
        x, y = vx + dx, vy + dy
        if left <= x < left + chunk_size and top <= y < top + chunk_size:
            structure = dict(kind=kind, x=x, y=y, biome=village['biome'],
                             village=village['name'], variant=variant, label=label)
            if variant == 'barn':
                structure['animals'] = VILLAGE_ANIMALS[village['biome']]
            additions.append(structure)
    for kind, dx, dy in PLANTINGS:
        x, y = vx + dx, vy + dy
        if left <= x < left + chunk_size and top <= y < top + chunk_size:
            rows[y-top][x-left] = village['biome'] if kind == 'water' or (kind == 'tree' and village['biome'] == 'b') else 'g'
            plant = dict(kind=kind, x=x, y=y, village=village['name'])
            if kind == 'tree' and village['biome'] == 'd':
                plant['species'] = 'palm'
            additions.append(plant)
    occupied = {(r['x'], r['y']) for r in additions}
    for field in ('map_resources', 'tree_stumps'):
        record[field] = [r for r in record[field] if (r['x'], r['y']) not in occupied]
    record['map_resources'].extend(additions)
    record['terrain'] = [''.join(row) for row in rows]
