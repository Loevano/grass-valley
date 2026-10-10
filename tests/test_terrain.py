import copy
import json
import unittest

from server import ensure_resources, new_town, walkable
from terrain import generate_map


class RandomTerrain(unittest.TestCase):
    def test_seed_reproduces_terrain_and_resources(self):
        first = generate_map(120, 1234)
        self.assertEqual(first, generate_map(120, 1234))
        second = generate_map(120, 5678)
        self.assertNotEqual(first[0], second[0])
        self.assertNotEqual(first[1], second[1])

    def test_maps_have_varied_ground_and_unique_in_bounds_resources(self):
        for seed in range(10):
            with self.subTest(seed=seed):
                terrain, resources = generate_map(120, seed)
                self.assertEqual(len(terrain), 120)
                self.assertTrue(all(len(row) == 120 for row in terrain))
                self.assertEqual(set(''.join(terrain)), set('gfmsr'))
                positions = {(r['x'], r['y']) for r in resources}
                self.assertEqual(len(positions), len(resources))
                self.assertTrue(all(0 <= x < 120 and 0 <= y < 120 for x, y in positions))
                for kind in ('tree', 'water', 'rock'):
                    self.assertTrue(any(r['kind'] == kind and r['x'] > 12 for r in resources))
                self.assertNotIn((5, 5), positions)
                town = {'map_size': 120, 'buildings': [], 'map_resources': resources}
                self.assertTrue(walkable(town, 5, 7))
                self.assertTrue(walkable(town, 5.3, 6.7))

    def test_new_towns_are_different_and_survive_save_round_trip(self):
        town = new_town('First')
        other = new_town('Second')
        self.assertNotEqual(town['terrain_seed'], other['terrain_seed'])
        self.assertNotEqual(town['map_resources'], other['map_resources'])
        saved = json.loads(json.dumps(town))
        ensure_resources(saved)
        self.assertEqual(saved, town)

    def test_old_maps_keep_resources_buildings_and_player(self):
        town = new_town('Existing')
        del town['terrain']
        del town['terrain_seed']
        before = copy.deepcopy(town)
        ensure_resources(town)
        for key in before:
            self.assertEqual(town[key], before[key])
        migrated = copy.deepcopy(town)
        ensure_resources(town)
        self.assertEqual(town, migrated)


if __name__ == '__main__':
    unittest.main()
