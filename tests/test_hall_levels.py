import copy
import json
import unittest

from server import HALL_UPGRADES, building_range, command, new_town, tick


class HallLevels(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Hall levels')
        self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
        self.world = {'towns': {'a': self.town}}

    def qualify(self, level):
        self.town.update(food=100, wood=500, goods={'planks': 50, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})
        if level == 2:
            self.town['buildings'].extend([{'kind': 'home', 'x': 3, 'y': 5}, {'kind': 'farm', 'x': 6, 'y': 2}])
        elif level == 3:
            self.town['villagers'][0].update(woodcutting=10, farming=10)
        elif level == 4:
            self.town['produced'] = {'planks': 10}
        elif level == 5:
            self.town['produced'] = {'flour': 1}
        elif level in {6, 7, 8}:
            self.town['produced'] = {6: {'leather': 5}, 7: {'iron_ore': 10}, 8: {'iron': 10}}[level]
        elif level == 9:
            self.town['horse_ridden'] = True
        elif level == 10:
            self.town.update(produced={'steel': 15}, completed_trades=1)

    def test_levels_two_through_ten_require_choice_and_finish_once(self):
        for upgrade in HALL_UPGRADES:
            with self.subTest(level=upgrade['level']):
                self.qualify(upgrade['level'])
                self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
                coins, wood = self.town['coins'], self.town['wood']
                command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town['wood'], wood - upgrade['wood'])
                pending = copy.deepcopy(self.town)
                with self.assertRaisesRegex(ValueError, 'already underway'):
                    command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town, pending)
                self.world['towns']['a'] = self.town = json.loads(json.dumps(self.town))
                for _ in range(upgrade['seconds'] // 3 - 1):
                    tick(self.world)
                self.assertEqual(self.town['level'], upgrade['level'] - 1)
                self.assertEqual(building_range(self.town), 30 + 5 * (upgrade['level'] - 2))
                tick(self.world)
                self.assertEqual(self.town['level'], upgrade['level'])
                self.assertEqual(building_range(self.town), 30 + 5 * (upgrade['level'] - 1))
                self.assertEqual(self.town['coins'], coins + upgrade['coins'])
                tick(self.world)
                self.assertEqual(self.town['coins'], coins + upgrade['coins'])
        with self.assertRaisesRegex(ValueError, 'fully upgraded'):
            command(self.town, {'action': 'upgrade'})

    def test_missing_milestones_do_not_spend_and_wood_alone_is_insufficient(self):
        for level in range(2, 11):
            self.setUp()
            self.town.update(level=level-1, wood=500, harvested=10000, goods={'planks': 50, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                command(self.town, {'action': 'upgrade'})
            self.assertEqual(self.town, before)

    def test_missing_materials_do_not_spend(self):
        for upgrade in HALL_UPGRADES:
            self.town['level'] = upgrade['level'] - 1
            self.qualify(upgrade['level'])
            for key in [k for k in ('wood', 'planks', 'leather', 'iron_ore', 'iron', 'steel') if k in upgrade]:
                if key == 'wood':
                    self.town['wood'] = upgrade[key] - .5
                else:
                    self.town['goods'][key] = upgrade[key] - .5
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town, before)
                self.town.update(wood=500, goods={'planks': 50, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})

    def test_legacy_high_level_and_pending_upgrade_are_preserved(self):
        self.town.update(level=7, upgrade_remaining=3)
        tick(self.world)
        self.assertEqual(self.town['level'], 8)
        self.assertEqual(self.town['upgrade_remaining'], 0)
        with self.assertRaisesRegex(ValueError, 'Saddle a settled horse'):
            command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['level'], 8)
