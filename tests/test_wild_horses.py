import json
import unittest
from unittest.mock import patch

import horses
from terrain import ensure_industry


class WildHorses(unittest.TestCase):
    def test_sparse_population_and_partial_ore_survive_reload(self):
        rocks = [{'kind': 'rock', 'x': x, 'y': 0, 'ore': 'iron'} for x in range(120)]
        landscape = {'terrain': ['g' * 120] * 120, 'map_resources': rocks}
        ensure_industry(landscape)
        herd = [r for r in landscape['map_resources'] if r['kind'] == 'wild_horse']
        self.assertGreater(len(herd), 0)
        self.assertLessEqual(len(herd), 5)
        self.assertGreater(sum('ore' in r for r in rocks), 0)
        self.assertLess(sum('ore' in r for r in rocks), 25)
        landscape['map_resources'].remove(herd[0])
        saved = json.loads(json.dumps(landscape))
        ensure_industry(saved)
        self.assertEqual(saved, landscape)

    def test_old_population_is_thinned_without_respawning(self):
        herd = [{'kind': 'wild_horse', 'x': x, 'y': 0} for x in range(100)]
        landscape = {'terrain': ['g' * 100], 'map_resources': herd, 'industry_version': 1}
        ensure_industry(landscape)
        self.assertEqual(len(landscape['map_resources']), 10)
        landscape['map_resources'].clear()
        ensure_industry(landscape)
        self.assertEqual(landscape['map_resources'], [])

    def test_walk_avoids_obstacles_and_stays_near_home(self):
        horse = {'kind': 'wild_horse', 'x': 5, 'y': 5, 'wander_remaining': 0}
        landscape = {'map_size': 12, 'terrain': ['g' * 12] * 12, 'map_resources': [horse,
            {'kind': 'tree', 'x': 4, 'y': 5}, {'kind': 'water', 'x': 5, 'y': 6}]}
        town = {'buildings': [{'kind': 'home', 'x': 6, 'y': 5}], 'player': {'x': 0, 'y': 0}}
        with patch('horses.random.choice', side_effect=lambda options: options[0]):
            horses.wander(landscape, [town], 3)
            self.assertEqual((horse['x'], horse['y']), (5, 4))
            self.assertEqual(horse['wander_from'], [5, 5])
            for _ in range(100):
                horse['wander_remaining'] = 0
                horses.wander(landscape, [town], 3)
                self.assertLessEqual(abs(horse['x'] - 5) + abs(horse['y'] - 5), 6)
                self.assertNotIn((horse['x'], horse['y']), ((4, 5), (5, 6), (6, 5)))

    def test_horse_pauses_near_player_and_on_unsuitable_ground(self):
        for player in ({'x': 5, 'y': 4}, {'x': 0, 'y': 0}):
            horse = {'kind': 'wild_horse', 'x': 5, 'y': 5, 'wander_remaining': 0}
            landscape = {'terrain': ['s' * 12] * 12, 'map_resources': [horse]}
            if player['x'] == 5:
                landscape['terrain'] = ['g' * 12] * 12
            horses.wander(landscape, [{'buildings': [], 'player': player}], 3)
            self.assertEqual((horse['x'], horse['y']), (5, 5))
            self.assertGreater(horse['wander_remaining'], 0)

    def test_chunk_edge_move_is_saved_in_destination_chunk(self):
        # Use the normal chunk manager through a fresh world.
        from shared_world import new_world
        world = new_world(42, explorable=True)
        manager = world['_chunks']
        for key in ('0,0', '1,0'):
            record = manager.read(key)
            record.update(terrain=['g' * 40] * 40, map_resources=[], tree_stumps=[])
            manager.records[key] = record
        horse = {'kind': 'wild_horse', 'x': 39, 'y': 20, 'wander_remaining': 0}
        manager.records['0,0']['map_resources'].append(horse)
        manager.landscape.update(terrain=manager.terrain, map_resources=[horse], tree_stumps=[])
        with patch('horses.random.choice', side_effect=lambda options: options[0]):
            horses.wander(manager.landscape, [], 3)
        manager.sync()
        self.assertNotIn(horse, manager.records['0,0']['map_resources'])
        self.assertIn(horse, manager.records['1,0']['map_resources'])
        manager.flush()
        self.assertEqual(json.loads(manager.memory['1,0'])['map_resources'][0]['x'], 40)
