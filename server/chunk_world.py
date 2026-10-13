"""Bounded working sets and incremental storage for explorable landscapes."""
import hashlib
import json
import math
import random
from copy import deepcopy
from collections import OrderedDict
from functools import lru_cache
from pathlib import Path

from terrain import remove_invalid_trees, ensure_industry
from villages import apply_villages, ensure_villages

CHUNK_SIZE = 40
WORLD_SIZE = 480
START_OFFSET = 160
LEGACY_WORLD_SIZE = 1920
LEGACY_START_OFFSET = 880
MAX_VIEW_CHUNKS = 49


def chunk_key(x, y):
    return f'{math.floor(x / CHUNK_SIZE)},{math.floor(y / CHUNK_SIZE)}'


def parse_keys(value):
    # Accept legacy cache keys; snapshots enforce the active world boundary.
    keys = value.split(';') if value else []
    if len(keys) > MAX_VIEW_CHUNKS:
        raise ValueError('Too many map chunks.')
    result = set()
    for key in keys:
        parts = key.split(',')
        if len(parts) != 2 or not all(p.isdecimal() and len(p) <= 2 for p in parts):
            raise ValueError('Invalid map chunk.')
        x, y = map(int, parts)
        if not (0 <= x < LEGACY_WORLD_SIZE // CHUNK_SIZE and 0 <= y < LEGACY_WORLD_SIZE // CHUNK_SIZE):
            raise ValueError('Map chunk outside the world.')
        result.add(f'{x},{y}')
    return result


def parse_versions(value):
    items = value.split(';') if value else []
    if len(items) > 128:
        raise ValueError('Too many cached resource chunks.')
    versions = {}
    for item in items:
        key, separator, version = item.partition(':')
        if not separator or len(version) != 16 or any(c not in '0123456789abcdef' for c in version):
            raise ValueError('Invalid resource chunk version.')
        keys = parse_keys(key)
        if len(keys) != 1:
            raise ValueError('Invalid resource chunk key.')
        key = next(iter(keys))
        versions[key] = version
    return versions


class TerrainRow:
    def __init__(self, terrain, y):
        self.terrain, self.y = terrain, y

    def __len__(self):
        return self.terrain.size

    def __getitem__(self, x):
        record = self.terrain.records.get(chunk_key(x, self.y))
        return record['terrain'][self.y % CHUNK_SIZE][x % CHUNK_SIZE] if record else 'v'


class ChunkTerrain:
    """Existing tile lookups work; unloaded tiles are impassable."""
    def __init__(self, records, size):
        self.records = records
        self.size = size

    def __len__(self):
        return self.size

    def __getitem__(self, y):
        if not 0 <= y < self.size:
            raise IndexError(y)
        return TerrainRow(self, y)

    def tiles(self):
        for key in self.records:
            cx, cy = map(int, key.split(','))
            for y in range(cy * CHUNK_SIZE, (cy + 1) * CHUNK_SIZE):
                for x in range(cx * CHUNK_SIZE, (cx + 1) * CHUNK_SIZE):
                    yield x, y


@lru_cache(maxsize=8192)
def _value(seed, label, x, y):
    digest = hashlib.blake2b(f'{seed}:{label}:{x}:{y}'.encode(), digest_size=8).digest()
    return int.from_bytes(digest, 'big') / 2**64


def _field(seed, label, x, y, scale):
    ix, iy = x // scale, y // scale
    fx, fy = (x % scale) / scale, (y % scale) / scale
    fx, fy = fx * fx * (3 - 2 * fx), fy * fy * (3 - 2 * fy)
    a = _value(seed, label, ix, iy) * (1 - fx) + _value(seed, label, ix + 1, iy) * fx
    b = _value(seed, label, ix, iy + 1) * (1 - fx) + _value(seed, label, ix + 1, iy + 1) * fx
    return a * (1 - fy) + b * fy


def _legacy_chunk(seed, key):
    """Coordinate noise keeps terrain continuous and independent of visit order."""
    cx, cy = map(int, key.split(','))
    rng = random.Random(f'{seed}:chunk-v1:{key}')
    rows, resources = [], []
    for y in range(cy * CHUNK_SIZE, (cy + 1) * CHUNK_SIZE):
        row = []
        for x in range(cx * CHUNK_SIZE, (cx + 1) * CHUNK_SIZE):
            wet = _field(seed, 'moisture', x, y, 18)
            ground = _field(seed, 'ground', x, y, 25)
            lake = _field(seed, 'lake', x, y, 45)
            temperature = _field(seed, 'temperature', x, y, 90)
            biome = 'i' if temperature < .22 else 'd' if temperature > .65 and wet < .45 else 'b' if wet > .78 else 'f' if wet > .58 else 's' if ground < .3 else 'r' if ground > .75 else 'm' if wet > .4 else 'g'
            kind = 'water' if lake < .25 else 'tree' if biome in ('f', 'b') and rng.random() < .45 else 'rock' if biome == 'r' and rng.random() < .15 else None
            if kind == 'water' and biome not in ('b', 'd', 'i'):
                biome = 's'
            row.append(biome)
            if kind:
                resources.append({'kind': kind, 'x': x, 'y': y})
            elif biome in ('g', 'f', 'm', 'b') and rng.random() < .012:
                resources.append({'kind': rng.choice(['berry_bush', 'herb_patch']), 'x': x, 'y': y})
        rows.append(''.join(row))
    return {'terrain': rows, 'map_resources': resources, 'tree_stumps': [], 'clock': 0}


@lru_cache(maxsize=64)
def _region_layout(seed):
    """Keep landmarks near their villages, with variation between worlds."""
    rng = random.Random(f'{seed}:regions-v2')
    return {
        'swamp': (.17 + rng.uniform(-.025, .025), .42, .22, .26),
        'desert': (.78 + rng.uniform(-.025, .025), .43, .25, .30),
        'ice': (.28, .04, .36, .25),
        'lake': (.57 + rng.uniform(-.025, .025), .79, .16, .13),
        'phase': rng.uniform(0, math.tau),
    }


def regional_tile(seed, x, y, size=WORLD_SIZE):
    """Sample world coordinates, never chunk-local noise or visit order."""
    layout = _region_layout(seed)
    u, v = x / size, y / size
    detail = _field(seed, 'region-edge', x, y, size / 20)

    def distance(name):
        cx, cy, rx, ry = layout[name]
        return ((u - cx) / rx) ** 2 + ((v - cy) / ry) ** 2

    wet = _field(seed, 'regional-moisture', x, y, size / 5)
    soil = _field(seed, 'regional-ground', x, y, size / 8)
    biome = 'f' if wet > .56 else 'r' if soil > .76 else 'm' if wet > .38 else 'g'
    edge = .88 + detail * .24
    marsh = distance('swamp')
    if distance('ice') < edge:
        biome = 'i'
    elif distance('desert') < edge:
        biome = 'd'
    elif marsh < edge or (marsh < 1.6 and detail > .67):
        biome = 'b'
    elif soil < .23 and detail < .30:
        biome = 's'

    # Two uninterrupted channels meet the major lake. Smooth centerlines and
    # widths keep water connected even where chunks are generated separately.
    lx, ly, _, _ = layout['lake']
    phase = layout['phase']
    # Keep the northern channel east of the starter valley and west of
    # Sunwell, so authored clearings/paths cannot cut the river into pieces.
    approach = min(1, max(0, (v - .60) / (ly - .60)))
    river_x = (.65 * (1 - approach) + lx * approach
               + .012 * (math.sin((v - ly) * math.tau + phase) - math.sin(phase)))
    tributary_y = ly + .035 * math.sin((u - lx) * math.tau)
    water = (distance('lake') < .92 + detail * .16
             or (v <= ly and abs(u - river_x) < .012)
             or (u <= lx and abs(v - tributary_y) < .010))
    if biome == 'b' and _field(seed, 'marsh-pools', x, y, size / 24) < .27:
        water = True
    if water and biome not in ('b', 'd', 'i'):
        biome = 's'
    return biome, water


def generate_chunk(seed, key, version=1, size=WORLD_SIZE):
    """Version saved worlds so untouched legacy chunks never change generator."""
    if version == 1:
        return _legacy_chunk(seed, key)
    if version not in (2, 3, 4, 5):
        raise ValueError(f'Unsupported terrain generator version: {version}')
    if version >= 3:
        from terrain_regions import sample_tile
    cx, cy = map(int, key.split(','))
    rng = random.Random(f'{seed}:chunk-v{min(version, 3)}:{key}')
    rows, resources = [], []
    for y in range(cy * CHUNK_SIZE, (cy + 1) * CHUNK_SIZE):
        row = []
        for x in range(cx * CHUNK_SIZE, (cx + 1) * CHUNK_SIZE):
            if version >= 3:
                biome, water, trees, rocks, species = sample_tile(
                    seed, x, y, size, _field, sparse_grass=version >= 4,
                    open_farmland=version >= 5)
            else:
                biome, water = regional_tile(seed, x, y, size)
                trees, rocks, species = (.45 if biome in ('f', 'b') else 0), (.15 if biome == 'r' else 0), None
            kind = ('water' if water else
                    'tree' if biome in 'gfmb' and trees and rng.random() < trees else
                    'rock' if rocks and rng.random() < rocks else None)
            row.append(biome)
            if kind:
                resource = {'kind': kind, 'x': x, 'y': y}
                if kind == 'tree' and species:
                    resource['species'] = species
                resources.append(resource)
            elif biome in ('g', 'f', 'm', 'b') and rng.random() < .012:
                resources.append({'kind': rng.choice(['berry_bush', 'herb_patch']), 'x': x, 'y': y})
        rows.append(''.join(row))
    return {'terrain': rows, 'map_resources': resources, 'tree_stumps': [], 'clock': 0}


def regional_starter(seed, version=2):
    """The inhabited valley uses the same underlying terrain as its neighbors."""
    chunks = {(cx, cy): generate_chunk(seed, f'{cx},{cy}', version=version)
              for cy in range(4, 7) for cx in range(4, 7)}
    rows = [''.join(chunks[cx, y // CHUNK_SIZE]['terrain'][y % CHUNK_SIZE]
                    for cx in range(4, 7)) for y in range(START_OFFSET, START_OFFSET + 120)]
    resources = [dict(r, x=r['x'] - START_OFFSET, y=r['y'] - START_OFFSET)
                 for record in chunks.values() for r in record['map_resources']]
    return rows, resources


class ChunkMap:
    def __init__(self, world, directory=None, base=None):
        self.world = world
        self.landscape = world['shared_map']
        self.directory = Path(directory) / str(self.landscape['terrain_seed']) if directory else None
        self.records, self.saved, self.memory = {}, {}, {}
        self.views = OrderedDict()
        self.snapshots = {}
        self.membership = None
        self.resource_index = None
        self.base = base
        self.size = self.landscape['map_size']
        self.start_offset = self.landscape.get('starter_offset', LEGACY_START_OFFSET)
        self.terrain = ChunkTerrain(self.records, self.size)

    def _starter(self):
        if self.base is None:
            from shared_world import new_world
            self.base = new_world(self.landscape['terrain_seed'],
                                  terrain_version=self.landscape.get('terrain_version', 1))['shared_map']
        return self.base

    def read(self, key):
        if key in self.records:
            return self.records[key]
        if key in self.views:
            self.views.move_to_end(key)
            if self.views[key].get('clock') != self.landscape.get('chunk_clock', 0):
                self.catch_up(key, self.views[key])
            return self.views[key]
        record = apply_villages(self._read(key), key, self.landscape.get('villages', []), CHUNK_SIZE)
        self.catch_up(key, record)
        self.views[key] = record
        while len(self.views) > MAX_VIEW_CHUNKS:
            self.views.popitem(last=False)
        return record

    def catch_up(self, key, record):
        from progression import SUPPLIES, TREE_REGROWTH_RATE, source_limits
        clock = self.landscape.get('chunk_clock', 0)
        elapsed = max(0, clock - record.get('clock', 0))
        cx, cy = map(int, key.split(','))
        obstacles = [b for t in self.world['towns'].values() for b in t['buildings'] + t.get('roads', [])]
        obstacles += self.landscape.get('npcs', [])
        obstacles += [dict(x=round(t['player']['x']), y=round(t['player']['y']))
                      for t in self.world['towns'].values() if 'player' in t]
        ensure_industry(record, obstacles, (cx * CHUNK_SIZE, cy * CHUNK_SIZE))
        landscape = dict(record, terrain=ChunkTerrain({key: record}, self.size))
        remove_invalid_trees(landscape)
        for resource in record['map_resources']:
            if resource['kind'] not in SUPPLIES:
                continue
            maximum, regeneration = source_limits(landscape, resource)
            resource.update(supply=min(maximum, resource.get('supply', maximum) + elapsed / 3 * regeneration),
                            supply_max=maximum, regeneration=regeneration)
        for stump in record['tree_stumps']:
            stump['remaining'] = max(3 * TREE_REGROWTH_RATE,
                                     round(stump['remaining'] - elapsed * TREE_REGROWTH_RATE, 9))
        record['clock'] = clock

    def _read(self, key):
        path = self.directory / f'{key}.json' if self.directory else None
        if path and path.exists():
            return json.loads(path.read_text())
        if key in self.memory:
            return json.loads(self.memory[key])
        cx, cy = map(int, key.split(','))
        x, y = cx * CHUNK_SIZE - self.start_offset, cy * CHUNK_SIZE - self.start_offset
        if 0 <= x < 120 and 0 <= y < 120:
            base = self._starter()
            resources = [dict(r, x=r['x'] + self.start_offset, y=r['y'] + self.start_offset)
                         for r in base['map_resources'] if x <= r['x'] < x + CHUNK_SIZE and y <= r['y'] < y + CHUNK_SIZE]
            return {'terrain': [row[x:x + CHUNK_SIZE].replace('v', 'g') for row in base['terrain'][y:y + CHUNK_SIZE]],
                    'map_resources': resources, 'tree_stumps': [], 'clock': 0}
        return generate_chunk(self.landscape['terrain_seed'], key,
                              version=self.landscape.get('terrain_version', 1), size=self.size)

    def desired(self):
        towns = self.world['towns'].values()
        points = [t['town_origin'] for t in towns] + [t['player'] for t in towns]
        if not points:
            points = self.world['town_sites']
        result = set()
        for point in points:
            cx, cy = map(int, chunk_key(point['x'], point['y']).split(','))
            for x in range(max(0, cx - 1), min(self.size // CHUNK_SIZE, cx + 2)):
                for y in range(max(0, cy - 1), min(self.size // CHUNK_SIZE, cy + 2)):
                    result.add(f'{x},{y}')
        return result

    def sync(self):
        # Supply changes are visible through shared resource references. Regroup
        # only when a resource is added, removed, replaced, or moved.
        membership = tuple(tuple((id(item), item['x'], item['y'])
                                 for item in self.landscape.get(field, []))
                           for field in ('map_resources', 'tree_stumps'))
        if membership == self.membership:
            return
        self.resource_index = None
        for record in self.records.values():
            record['map_resources'] = []
            record['tree_stumps'] = []
        for field in ('map_resources', 'tree_stumps'):
            for item in self.landscape.get(field, []):
                self.records[chunk_key(item['x'], item['y'])][field].append(item)
        self.membership = membership

    def flush(self):
        if not self.records:
            return
        self.sync()
        clock = self.landscape.get('chunk_clock', 0)
        for key, record in self.records.items():
            # Full, untouched supplies do not need a new timestamp every tick.
            depleted = any(r.get('supply', r.get('supply_max', 0)) < r.get('supply_max', 0)
                           and r.get('regeneration', 0) for r in record['map_resources'])
            if depleted or record['tree_stumps']:
                record['clock'] = clock
            body = json.dumps(record, separators=(',', ':'))
            if body == self.saved.get(key):
                continue
            if self.directory:
                self.directory.mkdir(parents=True, exist_ok=True)
                path = self.directory / f'{key}.json'
                temporary = path.with_suffix('.json.tmp')
                temporary.write_text(body)
                temporary.replace(path)
            else:
                self.memory[key] = body
            self.saved[key] = body

    def activate(self):
        desired = self.desired()
        if desired == self.records.keys():
            return
        self.flush()
        self.membership = None
        for key in self.records.keys() - desired:
            del self.records[key]
            self.saved.pop(key, None)
            self.views.pop(key, None)
        for key in sorted(desired - self.records.keys()):
            record = self.read(key)
            self.views.pop(key, None)
            self.saved[key] = json.dumps(record, separators=(',', ':'))
            self.records[key] = record
        self.landscape.update(terrain=self.terrain,
                              map_resources=[r for key in sorted(self.records) for r in self.records[key]['map_resources']],
                              tree_stumps=[r for key in sorted(self.records) for r in self.records[key]['tree_stumps']])

    def movement(self, town):
        """Movement checks inspect nearby tiles instead of every active resource."""
        resources = self.landscape['map_resources']
        if self.resource_index is None or self.resource_index[:2] != (id(resources), len(resources)):
            index = {}
            for resource in resources:
                index.setdefault((resource['x'], resource['y']), []).append(resource)
            self.resource_index = (id(resources), len(resources), index)
        index = self.resource_index[2]
        x, y = math.floor(town['player']['x'] + .5), math.floor(town['player']['y'] + .5)
        nearby = [r for ty in range(y - 2, y + 3) for tx in range(x - 2, x + 3)
                  for r in index.get((tx, ty), [])]
        return dict(town, map_resources=nearby)

    def snapshot(self, keys=(), known=(), versions=None):
        for key in keys:
            x, y = map(int, key.split(','))
            if not (0 <= x < self.size // CHUNK_SIZE and 0 <= y < self.size // CHUNK_SIZE):
                raise ValueError('Map chunk outside the world.')
        self.sync()
        keys = self.records.keys() | set(keys)
        chunks = {key: self.read(key) for key in sorted(keys)}
        updates = {}
        # Retain only the active area and this request's bounded camera view.
        self.snapshots = {key: value for key, value in self.snapshots.items() if key in keys}
        for key, record in chunks.items():
            payload = {field: record[field] for field in ('map_resources', 'tree_stumps')}
            cached = self.snapshots.get(key)
            if cached is None or cached[0] != payload:
                body = json.dumps(payload, separators=(',', ':'))
                version = hashlib.blake2b(body.encode(), digest_size=8).hexdigest()
                # Resources mutate in place; cached data must be independent.
                cached = self.snapshots[key] = (deepcopy(payload), version)
            payload, version = cached
            if (versions or {}).get(key) != version:
                updates[key] = dict(payload, version=version)
        result = {k: v for k, v in self.landscape.items()
                  if k not in ('terrain', 'map_resources', 'tree_stumps')}
        return dict(result, terrain_chunks={key: r['terrain'] for key, r in chunks.items() if key not in known},
                    resource_chunks=updates, visible_chunks=sorted(keys))


def attach(world, directory=None):
    if not world.get('shared_map', {}).get('chunked'):
        return None
    ensure_villages(world)
    manager = world.get('_chunks')
    if manager is None:
        manager = world['_chunks'] = ChunkMap(world, directory)
    elif directory:
        manager.directory = Path(directory) / str(manager.landscape['terrain_seed'])
    manager.activate()
    return manager


def expand_new_world(world, directory=None):
    base = world['shared_map']
    for site in world['town_sites']:
        site['x'] += START_OFFSET
        site['y'] += START_OFFSET
    world['shared_map'] = {'map_size': WORLD_SIZE, 'terrain_seed': base['terrain_seed'],
                           'chunked': 1, 'chunk_size': CHUNK_SIZE, 'chunk_clock': 0, 'starter_offset': START_OFFSET,
                           'npc_placement_version': base.get('npc_placement_version', 0),
                           'npcs': [dict(n, x=n['x'] + START_OFFSET, y=n['y'] + START_OFFSET) for n in base['npcs']]}
    if 'terrain_version' in base:
        world['shared_map']['terrain_version'] = base['terrain_version']
    ensure_villages(world)
    world['_chunks'] = ChunkMap(world, directory, base)
    world['_chunks'].activate()
    return world
