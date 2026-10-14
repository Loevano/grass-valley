"""Saved, seed-based terrain with clustered renewable resources."""
import math
import random
import secrets

TREE_BIOMES = frozenset('gfmb')


def tree_regrows(landscape, source):
    """Older worlds retain their original regrowth rules."""
    if landscape.get('terrain_version', 1) < 6:
        return True
    terrain = landscape.get('terrain')
    return bool(terrain and terrain[source['y']][source['x']] in 'fb')


def remove_invalid_trees(landscape):
    """Keep saved trees and regrowing stumps on green ground."""
    terrain = landscape.get('terrain')
    if not terrain:
        return
    for field in ('map_resources', 'tree_stumps'):
        allowed = TREE_BIOMES
        if landscape.get('terrain_version', 1) >= 6:
            allowed = 'fb' if field == 'tree_stumps' else 'gfb'
        resources = landscape.get(field, [])
        resources[:] = [r for r in resources if r['kind'] != 'tree'
                        or terrain[r['y']][r['x']] in allowed]


def ground_variant(x, y):
    """Match the visible ground shade used by the browser."""
    return ((x * 17 + y * 31 + x * y * 7) % 7) % 3


def generate_map(size, seed):
    rng = random.Random(seed)

    def field(scale):
        width = math.ceil(size / scale) + 1
        values = [[rng.random() for _ in range(width)] for _ in range(width)]

        def sample(x, y):
            ix, iy = x // scale, y // scale
            fx, fy = (x % scale) / scale, (y % scale) / scale
            fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
            top = values[iy][ix] * (1 - fx) + values[iy][ix + 1] * fx
            bottom = values[iy + 1][ix] * (1 - fx) + values[iy + 1][ix + 1] * fx
            return top * (1 - fy) + bottom * fy

        return sample

    moisture, ground = field(8), field(11)
    lakes, lake_detail, coast = field(23), field(6), field(17)
    center_x, center_y = rng.uniform(.45, .55), rng.uniform(.45, .55)
    radius_x, radius_y = rng.uniform(.49, .55), rng.uniform(.49, .55)
    swamp_x, swamp_y = rng.uniform(.3, .7) * size, rng.uniform(.3, .7) * size
    swamp_rx, swamp_ry = rng.uniform(.12, .18) * size, rng.uniform(.12, .18) * size
    terrain, resources = [], []
    for y in range(size):
        row = []
        for x in range(size):
            outline = (abs(x / size - center_x) / radius_x) ** 4 + (abs(y / size - center_y) / radius_y) ** 4
            if size > 12 and not (x < 12 and y < 12) and outline > .8 + coast(x, y) * .5:
                row.append('v')
                continue
            wet, soil = moisture(x, y), ground(x, y)
            biome = 'f' if wet > .58 else 's' if soil < .3 else 'r' if soil > .75 else 'm' if wet > .4 else 'g'
            marsh = ((x - swamp_x) / swamp_rx) ** 2 + ((y - swamp_y) / swamp_ry) ** 2
            swamp = size > 12 and not (x < 12 and y < 12) and marsh < .8 + wet * .5
            if swamp:
                biome = 'b'
            kind = None
            if lakes(x, y) * .75 + lake_detail(x, y) * .25 < (.48 if swamp else .28):
                kind, biome = 'water', 'b' if swamp else 's'
            elif swamp and rng.random() < .22:
                kind = 'tree'
            elif biome == 'f' and rng.random() < .7:
                kind = 'tree'
            elif biome == 'r' and rng.random() < .18:
                kind = 'rock'
            elif rng.random() < .025:
                kind = 'tree'
            # A familiar, accessible starter clearing; wilderness varies per town.
            if x < 12 and y < 12:
                kind = None
                if 1 <= x <= 8 and 1 <= y <= 9:
                    biome = 'g'
            row.append(biome)
            if kind:
                resource = {'kind': kind, 'x': x, 'y': y}
                if kind == 'tree' and not swamp and rng.random() < .06:
                    resource['species'] = 'ancient'
                # Keep the seed's random sequence stable when rejecting a tree.
                if kind != 'tree' or biome in TREE_BIOMES:
                    resources.append(resource)
        terrain.append(''.join(row))
    starter = {
        'tree': [(0, 2), (0, 3), (1, 4), (2, 0), (3, 0), (3, 1),
                 (0, 7), (1, 8), (2, 9), (3, 10), (7, 0), (8, 1),
                 (8, 2), (10, 6), (11, 7), (10, 9), (9, 10), (11, 11)],
        'water': [(x, y) for y in range(3) for x in range(9, 9 + rng.randint(1, 3))],
        'rock': [(10, 3), (11, 4), (0, 10)],
    }
    # Guaranteed starter wood needs green ground too.
    for x, y in starter['tree']:
        if x < size and y < size and terrain[y][x] not in TREE_BIOMES:
            terrain[y] = terrain[y][:x] + 'g' + terrain[y][x + 1:]
    resources.extend({'kind': kind, 'x': x, 'y': y}
                     for kind, tiles in starter.items() for x, y in tiles
                     if x < size and y < size)
    return terrain, resources


def ensure_terrain(town, new=False):
    if new:
        town.setdefault('map_size', 120)
    size = town.get('map_size', 12)
    if 'terrain' in town and len(town['terrain']) == size:
        return
    if 'terrain_seed' not in town:
        town['terrain_seed'] = secrets.randbits(64)
    town['terrain'], resources = generate_map(size, town['terrain_seed'])
    if not new:
        # Adding ground to old saves must keep their playable area intact.
        town['terrain'] = [row.replace('v', 'g') for row in town['terrain']]
    if new:
        occupied = {(b['x'], b['y']) for b in town['buildings']}
        town['map_resources'] = [r for r in resources if (r['x'], r['y']) not in occupied]


def ensure_industry(landscape, obstacles=(), origin=(0, 0)):
    """Add mineral veins and wild meadow horses without moving saved terrain."""
    if landscape.get('mineral_version') != 1:
        for resource in landscape['map_resources']:
            if resource['kind'] == 'rock':
                if (resource['x'] * 73 + resource['y'] * 137) % 7 == 0:
                    resource['ore'] = 'iron'
                else:
                    resource.pop('ore', None)
        landscape['mineral_version'] = 1
    version = landscape.get('industry_version', 0)
    if version == 1:
        # Thin existing herds once, without replacing captured horses.
        landscape['map_resources'][:] = [r for r in landscape['map_resources']
            if r['kind'] != 'wild_horse' or (r['x'] * 31 + r['y'] * 53) % 10 == 0]
    if version >= 1:
        landscape['industry_version'] = 2
        return
    occupied = {(r['x'], r['y']) for r in landscape['map_resources']}
    occupied.update((b['x'], b['y']) for b in obstacles)
    occupied.update((r['x'], r['y']) for r in landscape.get('tree_stumps', []))
    ox, oy = origin
    for y, row in enumerate(landscape.get('terrain', [])):
        for x, biome in enumerate(row):
            tx, ty = x + ox, y + oy
            if (biome in 'gm' and (tx * 73 + ty * 137 + tx * ty * 11) % 499 == 71
                    and (tx * 31 + ty * 53) % 10 == 0 and (tx, ty) not in occupied):
                landscape['map_resources'].append({'kind': 'wild_horse', 'x': tx, 'y': ty})
                occupied.add((tx, ty))
    landscape['industry_version'] = 2
