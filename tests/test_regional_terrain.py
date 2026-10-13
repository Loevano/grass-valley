"""Saved generator versions, connected water, and reproducible starter terrain."""
from collections import deque
import hashlib
import json
import unittest

from chunk_world import CHUNK_SIZE, WORLD_SIZE, ChunkMap, attach, generate_chunk
from shared_world import export_world, new_world


class RegionalTerrain(unittest.TestCase):
    def test_legacy_generator_and_unversioned_saves_keep_their_tiles(self):
        record = generate_chunk(42, '3,4')
        digest = hashlib.sha256(json.dumps(record, sort_keys=True).encode()).hexdigest()
        self.assertEqual(digest, '3b82ec1f2b33a643495c6ddd76af88b19f1e5f343232999fc4846ccfad5b1a4a')
        # Lack of a version is intentional for saves made before regions-v2.
        legacy = {'towns': {}, 'shared_map': {
            'map_size': WORLD_SIZE, 'terrain_seed': 42, 'starter_offset': 160}}
        self.assertEqual(ChunkMap(legacy)._read('3,4'), record)

    def test_new_world_version_and_starter_survive_reconstruction(self):
        world = new_world(42, explorable=True)
        self.assertEqual(world['shared_map']['terrain_version'], 2)
        manager = world['_chunks']
        expected = {key: manager._read(key) for key in ('4,4', '5,5', '6,6', '8,9')}
        restored = json.loads(json.dumps(export_world(world)))
        restored_manager = attach(restored)
        self.assertEqual(restored['shared_map']['terrain_version'], 2)
        self.assertEqual({key: restored_manager._read(key) for key in expected}, expected)
        self.assertNotIn('terrain_version', new_world(42)['shared_map'])

    def test_major_water_network_and_biomes_span_chunk_boundaries(self):
        for seed in (0, 42, 99):
            with self.subTest(seed=seed):
                water, regions = set(), {kind: set() for kind in 'bdi'}
                manager = new_world(seed, explorable=True)['_chunks']
                for cy in range(WORLD_SIZE // CHUNK_SIZE):
                    for cx in range(WORLD_SIZE // CHUNK_SIZE):
                        record = generate_chunk(seed, f'{cx},{cy}', version=2)
                        # Include authored starter paths and village clearings:
                        # they must not interrupt the main water network.
                        inhabited = manager.read(f'{cx},{cy}')
                        water.update((r['x'], r['y']) for r in inhabited['map_resources']
                                     if r['kind'] == 'water')
                        for y, row in enumerate(record['terrain']):
                            for x, biome in enumerate(row):
                                if biome in regions:
                                    regions[biome].add((cx * CHUNK_SIZE + x, cy * CHUNK_SIZE + y))
                # Follow the north river, its lake, and the western tributary.
                start = next(point for point in water if point[1] == 0)
                queue, connected = deque([start]), {start}
                while queue:
                    x, y = queue.popleft()
                    for point in ((x-1, y), (x+1, y), (x, y-1), (x, y+1)):
                        if point in water and point not in connected:
                            connected.add(point)
                            queue.append(point)
                self.assertTrue(any(x == 0 for x, y in connected))
                self.assertTrue(any(y > WORLD_SIZE * .88 for x, y in connected))
                self.assertGreater(len(connected), 15000)
                for biome, center in [('b', (80, 200)), ('d', (360, 200)), ('i', (200, 80))]:
                    # A whole village-scale interior belongs to its region.
                    self.assertTrue(all((x, y) in regions[biome]
                                        for y in range(center[1]-15, center[1]+16)
                                        for x in range(center[0]-15, center[0]+16)))


if __name__ == '__main__':
    unittest.main()
