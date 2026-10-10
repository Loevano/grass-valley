import copy
import unittest

from server import building_range, command, new_town, tick


class BuildingRange(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Range')
        self.town['terrain'] = ['g' * 120 for _ in range(120)]
        self.town['map_resources'] = []
        self.town['npcs'] = []

    def test_boundary_counts_both_axes(self):
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 10, 'y': 10})
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'within 10 tile steps'):
            command(self.town, {'action': 'build', 'kind': 'home', 'x': 11, 'y': 10})
        self.assertEqual(self.town, before)

    def test_upgrade_expands_range_only_when_finished(self):
        self.town['buildings'][0].update(x=1, y=1)
        self.town['villagers'][0]['construction_position'] = {'x': 1, 'y': 2}
        self.town['harvested'] = 30
        tick({'towns': {'owner': self.town}})
        command(self.town, {'action': 'upgrade'})
        if self.town.get('upgrade_remaining'):
            self.assertEqual(building_range(self.town), 10)
            with self.assertRaisesRegex(ValueError, 'within 10 tile steps'):
                command(self.town, {'action': 'build', 'kind': 'home', 'x': 11, 'y': 6})
        for _ in range(10):
            tick({'towns': {'owner': self.town}})
        self.assertEqual(building_range(self.town), 15)
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 11, 'y': 6})
        with self.assertRaisesRegex(ValueError, 'within 15 tile steps'):
            command(self.town, {'action': 'build', 'kind': 'acre', 'x': 11, 'y': 7})

    def test_moves_enforce_range_without_spending_coins(self):
        self.town['buildings'].append({'kind': 'home', 'x': 40, 'y': 40})
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'within 10 tile steps'):
            command(self.town, {'action': 'move', 'index': 1, 'x': 11, 'y': 10})
        self.assertEqual(self.town, before)
        command(self.town, {'action': 'move', 'index': 1, 'x': 10, 'y': 10})
        self.assertEqual(self.town['buildings'][1]['x'], 10)

    def test_hall_move_shifts_range_and_keeps_existing_buildings(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 40, 'y': 40})
        self.town['map_resources'].append({'kind': 'tree', 'x': 40, 'y': 41})
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        wood = self.town['wood']
        tick({'towns': {'owner': self.town}})
        self.assertGreater(self.town['wood'], wood)
        with self.assertRaisesRegex(ValueError, 'within 10 tile steps'):
            command(self.town, {'action': 'move', 'index': 0, 'x': 11, 'y': 10})
        command(self.town, {'action': 'move', 'index': 0, 'x': 1, 'y': 1})
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 1, 'y': 11})
        self.assertEqual(self.town['buildings'][1], {'kind': 'woodcutter', 'x': 40, 'y': 40})


if __name__ == '__main__':
    unittest.main()
