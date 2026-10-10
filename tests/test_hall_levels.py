import copy
import json
import unittest

from server import HALL_UPGRADES, command, new_town, tick


class HallLevels(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Hall levels')
        self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
        self.world = {'towns': {'a': self.town}}

    def test_all_levels_require_choice_and_finish_once(self):
        for upgrade in HALL_UPGRADES:
            with self.subTest(level=upgrade['level']):
                self.town['harvested'] = upgrade['harvested']
                self.town['wood'] = upgrade['wood']
                tick(self.world)
                self.assertEqual(self.town['level'], upgrade['level'] - 1)
                coins = self.town['coins']
                command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town['wood'], 0)
                self.assertEqual(self.town['upgrade_remaining'], upgrade['seconds'])
                pending = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'already underway'):
                    command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town, pending)
                # A saved upgrade resumes with its remaining time.
                self.world['towns']['a'] = self.town = json.loads(json.dumps(self.town))
                for _ in range(upgrade['seconds'] // 3 - 1):
                    tick(self.world)
                self.assertEqual(self.town['level'], upgrade['level'] - 1)
                self.assertEqual(self.town['coins'], coins)
                tick(self.world)
                self.assertEqual(self.town['level'], upgrade['level'])
                self.assertEqual(self.town['coins'], coins + upgrade['coins'])
                tick(self.world)
                self.assertEqual(self.town['coins'], coins + upgrade['coins'])
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'maximum level of 10'):
            command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town, before)

    def test_each_level_rejects_missing_milestone_or_wood_without_spending(self):
        for upgrade in HALL_UPGRADES:
            for missing in ['harvested', 'wood']:
                with self.subTest(level=upgrade['level'], missing=missing):
                    self.town.update(level=upgrade['level'] - 1,
                                     harvested=upgrade['harvested'], wood=upgrade['wood'])
                    self.town[missing] -= .5
                    self.town['achievements'] = ['Timber roots']
                    before = copy.deepcopy(self.town)
                    with self.assertRaises(ValueError):
                        command(self.town, {'action': 'upgrade'})
                    self.assertEqual(self.town, before)

    def test_old_level_two_town_can_upgrade_and_old_timer_finishes(self):
        self.town.update(level=2, harvested=90, wood=80)
        command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['upgrade_remaining'], 60)
        del self.town['buildings'][0]['construction']
        self.town.update(level=1, upgrade_remaining=3)
        coins = self.town['coins']
        tick(self.world)
        self.assertEqual(self.town['level'], 2)
        self.assertEqual(self.town['coins'], coins + 30)
