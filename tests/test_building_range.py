import copy
import unittest

from fixtures import start_woodcutting
from server import building_range, command, new_town, tick


class BuildingRange(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Range')
        self.town['terrain'] = ['g' * 120 for _ in range(120)]
        self.town['map_resources'] = []
        self.town['npcs'] = []

    def test_boundary_counts_both_axes(self):
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 20, 'y': 20})
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'within 30 tile steps'):
            command(self.town, {'action': 'build', 'kind': 'home', 'x': 21, 'y': 20})
        self.assertEqual(self.town, before)

    def test_upgrade_expands_range_only_when_finished(self):
        self.town['buildings'][0].update(x=1, y=1)
        self.town['villagers'][0]['construction_position'] = {'x': 1, 'y': 2}
        self.town['harvested'] = 30
        self.town['buildings'].extend([{'kind':'home','x':3,'y':5},{'kind':'farm','x':6,'y':2}])
        self.town['map_resources'].append({'kind':'water','x':7,'y':2})
        tick({'towns': {'owner': self.town}})
        command(self.town, {'action': 'upgrade'})
        if self.town.get('upgrade_remaining'):
            self.assertEqual(building_range(self.town), 30)
            with self.assertRaisesRegex(ValueError, 'within 30 tile steps'):
                command(self.town, {'action': 'build', 'kind': 'home', 'x': 31, 'y': 6})
        for _ in range(10):
            tick({'towns': {'owner': self.town}})
        self.assertEqual(building_range(self.town), 35)
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 31, 'y': 6})
        with self.assertRaisesRegex(ValueError, 'within 35 tile steps'):
            command(self.town, {'action': 'build', 'kind': 'acre', 'x': 31, 'y': 7})

    def test_moves_enforce_range_without_spending_coins(self):
        self.town['buildings'].append({'kind': 'home', 'x': 40, 'y': 40})
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'within 30 tile steps'):
            command(self.town, {'action': 'move', 'index': 1, 'x': 21, 'y': 20})
        self.assertEqual(self.town, before)
        command(self.town, {'action': 'move', 'index': 1, 'x': 20, 'y': 20})
        self.assertEqual(self.town['buildings'][1]['x'], 20)

    def test_each_hall_level_expands_build_move_and_road_boundaries(self):
        self.town['buildings'][0].update(x=30, y=30)
        for level, reach in enumerate((30, 35, 40, 45, 50), start=1):
            with self.subTest(level=level):
                self.town.update(level=level, wood=100)
                self.assertEqual(building_range(self.town), reach)
                command(self.town, {'action': 'build', 'kind': 'home', 'x': 30 + reach, 'y': 30})
                command(self.town, {'action': 'build_road', 'x': 30, 'y': 30 + reach})
                before = copy.deepcopy(self.town)
                for action in ('build', 'move', 'build_road'):
                    with self.assertRaisesRegex(ValueError, f'within {reach} tile steps'):
                        command(self.town, {'action': action, 'kind': 'home', 'index': 1,
                                            'x': 30 + reach, 'y': 31})
                    self.assertEqual(self.town, before)
                command(self.town, {'action': 'move', 'index': 1, 'x': 30, 'y': 30 + reach})

    def test_hall_move_shifts_range_and_keeps_existing_buildings(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 40, 'y': 40})
        self.town['map_resources'].append({'kind': 'tree', 'x': 40, 'y': 41})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        start_woodcutting(self.town)
        wood = self.town['wood']
        tick({'towns': {'owner': self.town}})
        self.assertEqual(self.town['wood'], wood)
        self.assertGreater(self.town['buildings'][1]['pending_output']['wood'], 0)
        with self.assertRaisesRegex(ValueError, 'within 30 tile steps'):
            command(self.town, {'action': 'move', 'index': 0, 'x': 21, 'y': 20})
        command(self.town, {'action': 'move', 'index': 0, 'x': 1, 'y': 1})
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 1, 'y': 31})
        self.assertEqual((self.town['buildings'][1]['x'], self.town['buildings'][1]['y']), (40, 40))


if __name__ == '__main__':
    unittest.main()
