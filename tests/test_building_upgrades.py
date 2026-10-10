import copy
import json
import unittest
from unittest.mock import patch

from server import capacity, command, new_town, tick


class BuildingUpgrades(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Upgrades')
        self.town['wood'] = 100

    def add(self, kind, **extra):
        self.town['buildings'].append(dict(kind=kind, x=6, y=2, **extra))
        return len(self.town['buildings']) - 1

    def upgrade(self, index):
        command(self.town, {'action': 'upgrade_building', 'index': index})

    def test_cost_levels_save_and_move(self):
        index = self.add('home')
        self.upgrade(index)
        self.assertEqual(self.town['wood'], 80)
        self.upgrade(index)
        self.assertEqual(self.town['wood'], 40)
        self.town = json.loads(json.dumps(self.town))
        command(self.town, {'action': 'move', 'index': index, 'x': 6, 'y': 3})
        self.assertEqual(self.town['buildings'][index]['level'], 3)
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.upgrade(index)
        self.assertEqual(self.town, before)

    def test_rejected_upgrades_do_not_change_town(self):
        index = self.add('home', construction={'progress': 0, 'total': 30})
        for invalid in [None, True, -1, 100, 0, index]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.upgrade(invalid)
            self.assertEqual(self.town, before)
        del self.town['buildings'][index]['construction']
        self.town['wood'] = 19
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            self.upgrade(index)
        self.assertEqual(self.town, before)

    def test_housing_and_storage_benefits(self):
        index = self.add('home')
        self.upgrade(index)
        for _ in range(8):
            self.town['food'] = 100
            tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 9)
        storage = self.add('storehouse')
        self.assertEqual(capacity(self.town), 200)
        self.upgrade(storage)
        self.assertEqual(capacity(self.town), 300)
        self.upgrade(storage)
        self.assertEqual(capacity(self.town), 400)

    @patch('server.secrets.randbelow', return_value=1)
    def test_each_workplace_boosts_only_its_workers(self, _roll):
        for kind, job, resource, base in [('woodcutter', 'woodcutting', 'wood', .5),
                                          ('farm', 'farming', 'food', 1),
                                          ('fishing_hut', 'fishing', 'food', 1)]:
            with self.subTest(kind=kind):
                self.setUp()
                self.town['map_resources'] = [{'kind': 'tree' if resource == 'wood' else 'water', 'x': 7, 'y': 2}]
                first = self.add(kind)
                self.upgrade(first)
                second = self.add(kind)
                self.town['buildings'][second]['y'] = 3
                for i, workplace in enumerate([first, second]):
                    self.town['villagers'][i].update(job=job, building_index=workplace)
                self.town['wood'] = self.town['food'] = 30
                tick({'towns': {'a': self.town}})
                upkeep = 5 * .12 + 2 * .18 if resource == 'food' else 0
                self.assertAlmostEqual(self.town[resource], 30 + base * 2.25 - upkeep)

    @patch('server.secrets.randbelow', return_value=0)
    def test_failed_catches_and_full_storage_do_not_gain_bonus_food(self, _roll):
        index = self.add('fishing_hut')
        self.upgrade(index)
        self.town['map_resources'] = [{'kind': 'water', 'x': 7, 'y': 2}]
        command(self.town, {'action': 'job', 'index': 0, 'job': 'fishing'})
        for food in [30, 100]:
            self.town['food'] = food
            tick({'towns': {'a': self.town}})
            self.assertAlmostEqual(self.town['food'], food - .78)

    def test_farm_and_acre_bonuses_add_and_respect_storage(self):
        farm = self.add('farm')
        self.upgrade(farm)
        acre = self.add('acre', farm_index=farm)
        self.town['buildings'][acre]['y'] = 3
        self.upgrade(acre)
        self.town['map_resources'] = [{'kind': 'water', 'x': 7, 'y': 2}]
        command(self.town, {'action': 'job', 'index': 0, 'job': 'farming'})
        self.town['food'] = 30
        tick({'towns': {'a': self.town}})
        self.assertAlmostEqual(self.town['food'], 30 + 1.5 - .78)
        self.town['food'] = 99.9
        tick({'towns': {'a': self.town}})
        self.assertAlmostEqual(self.town['food'], 100 - .78)
        self.town['map_resources'] = []
        tick({'towns': {'a': self.town}})
        self.assertIsNone(self.town['villagers'][0]['work_target'])


if __name__ == '__main__':
    unittest.main()
