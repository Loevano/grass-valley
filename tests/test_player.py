import copy
import math
import unittest

from server import command, ensure_player, ensure_resources, new_town, walkable


class PlayerMovement(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Home')

    def walk(self, dx, dy):
        return command(self.town, {'action': 'walk', 'dx': dx, 'dy': dy})['player']

    def test_player_is_separate_from_workers_and_moves_without_producing(self):
        before = copy.deepcopy(self.town)
        player = self.walk(1, -1)
        self.assertGreater(player['x'], 5)
        self.assertLess(player['y'], 7)
        self.assertAlmostEqual(math.hypot(player['x'] - 5, player['y'] - 7), .3)
        before.pop('player')
        after = copy.deepcopy(self.town)
        after.pop('player')
        self.assertEqual(before, after)

    def test_diagonal_and_straight_moves_have_equal_speed(self):
        diagonal = self.walk(1, -1)
        self.town['player'] = {'x': 5., 'y': 7., 'facing': 1}
        straight = self.walk(1, 0)
        self.assertAlmostEqual(math.hypot(diagonal['x'] - 5, diagonal['y'] - 7),
                               straight['x'] - 5)

    def test_buildings_water_and_rocks_block_walking(self):
        for kind in ['hall', 'water', 'rock']:
            with self.subTest(kind=kind):
                self.town['buildings'] = [{'kind': 'hall', 'x': 5, 'y': 5}] if kind == 'hall' else []
                self.town['map_resources'] = [] if kind == 'hall' else [{'kind': kind, 'x': 5, 'y': 5}]
                self.town['player'] = {'x': 5., 'y': 7., 'facing': 1}
                for _ in range(20):
                    player = self.walk(0, -1)
                self.assertGreaterEqual(player['y'], 5.68)
                self.assertTrue(walkable(self.town, player['x'], player['y']))

    def test_player_can_walk_through_trees(self):
        self.town['buildings'] = []
        self.town['map_resources'] = [{'kind': 'tree', 'x': 5, 'y': 5}]
        self.town['player'] = {'x': 5., 'y': 7., 'facing': 1}
        self.assertTrue(walkable(self.town, 5, 5))
        for _ in range(10):
            player = self.walk(0, -1)
        self.assertAlmostEqual(player['y'], 4.)

    def test_player_in_tree_is_not_relocated(self):
        self.town['buildings'] = []
        self.town['map_resources'] = [{'kind': 'tree', 'x': 5, 'y': 7}]
        player = copy.deepcopy(self.town['player'])
        self.assertEqual(ensure_player(self.town), player)

    def test_world_bounds_stop_character(self):
        self.town['player'] = {'x': 0., 'y': 0., 'facing': 1}
        for _ in range(20):
            player = self.walk(-1, -1)
        self.assertGreaterEqual(player['x'], -.32)
        self.assertGreaterEqual(player['y'], -.32)

    def test_bad_directions_cannot_teleport(self):
        before = copy.deepcopy(self.town)
        for dx, dy in [(100, 0), (True, 0), (0, None), (.5, 0)]:
            with self.assertRaises(ValueError):
                self.walk(dx, dy)
            self.assertEqual(before, self.town)

    def test_old_saves_get_character_on_open_grass(self):
        del self.town['player']
        self.town['buildings'].append({'kind': 'home', 'x': 5, 'y': 7})
        ensure_resources(self.town)
        player = ensure_player(self.town)
        self.assertTrue(walkable(self.town, player['x'], player['y']))
        self.assertEqual(player, ensure_player(self.town))

    def test_building_and_relocation_cannot_trap_player(self):
        for action in ['build', 'move']:
            before = copy.deepcopy(self.town)
            with self.assertRaisesRegex(ValueError, 'Walk away'):
                command(self.town, {'action': action, 'kind': 'home', 'index': 0, 'x': 5, 'y': 7})
            self.assertEqual(before, self.town)


if __name__ == '__main__':
    unittest.main()
