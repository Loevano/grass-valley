import copy
import math
import unittest

from server import command, construct, new_town
from shared_world import export_world, walking_map


class Roads(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Road town')
        self.town['map_resources'] = []
        self.town['npcs'] = []
        self.town['terrain'] = ['g' * 120] * 120

    def test_free_instant_and_removable(self):
        self.town['wood'] = 0
        before = copy.deepcopy(self.town['buildings'])
        command(self.town, {'action': 'build_road', 'x': 5, 'y': 8})
        command(self.town, {'action': 'build_road', 'x': 5, 'y': 8})
        self.assertEqual(self.town['roads'], [{'x': 5, 'y': 8}])
        self.assertEqual(self.town['wood'], 0)
        self.assertEqual(self.town['buildings'], before)
        command(self.town, {'action': 'remove_road', 'x': 5, 'y': 8})
        self.assertEqual(self.town['roads'], [])

    def test_placement_rejects_obstacles_bounds_and_range(self):
        self.town['map_resources'] = [{'kind': 'water', 'x': 6, 'y': 8}]
        for x, y in [(5, 5), (6, 8), (40, 40), (-1, 5), (True, 5), (120, 5)]:
            with self.subTest(x=x, y=y), self.assertRaises(ValueError):
                command(self.town, {'action': 'build_road', 'x': x, 'y': y})
        self.assertNotIn('roads', self.town)

    def test_speed_on_roads_and_normal_speed_off_roads(self):
        command(self.town, {'action': 'build_road', 'x': 5, 'y': 7})
        player = command(self.town, {'action': 'walk', 'dx': 1, 'dy': -1})['player']
        self.assertAlmostEqual(math.hypot(player['x'] - 5, player['y'] - 7), .45)
        self.town['player'].update(x=8., y=8.)
        player = command(self.town, {'action': 'walk', 'dx': 1, 'dy': -1})['player']
        self.assertAlmostEqual(math.hypot(player['x'] - 8, player['y'] - 8), .3)

    def test_shared_roads_are_saved_and_used_by_visitors(self):
        friend = copy.deepcopy(self.town)
        friend['roads'] = [{'x': 5, 'y': 7}]
        world = {'shared_map': {}, 'towns': {'own': self.town, 'friend': friend}}
        self.assertEqual(export_world(world)['towns']['friend']['roads'], friend['roads'])
        self.assertEqual(walking_map(world, self.town)['roads'], friend['roads'])
        player = command(self.town, {'action': 'walk', 'dx': 1, 'dy': -1}, world)['player']
        self.assertAlmostEqual(math.hypot(player['x'] - 5, player['y'] - 7), .45)

    def test_builders_reach_sites_faster_on_roads(self):
        self.town['buildings'].append({'kind': 'home', 'x': 12, 'y': 8,
                                      'construction': {'progress': 0, 'total': 30}})
        normal = copy.deepcopy(self.town)
        construct(normal, normal, workers={0})
        self.town['roads'] = [{'x': x, 'y': 8} for x in range(3, 12)]
        construct(self.town, self.town, workers={0})
        self.assertEqual(normal['villagers'][0]['construction_position']['x'], 8)
        self.assertEqual(self.town['villagers'][0]['construction_position']['x'], 11)

    def test_speed_boost_preserves_collision(self):
        self.town['roads'] = [{'x': 5, 'y': 7}]
        self.town['buildings'].append({'kind': 'home', 'x': 6, 'y': 7})
        for _ in range(10):
            player = command(self.town, {'action': 'walk', 'dx': 1, 'dy': 0})['player']
        self.assertLessEqual(player['x'], 5.32)
