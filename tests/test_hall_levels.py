from fixtures import flat_town
import copy
import json
import unittest

import progression as economy
from server import HALL_UPGRADES, building_range, command, tick


class HallLevels(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Hall levels')
        self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
        self.world = {'towns': {'a': self.town}}

    def qualify(self, level):
        self.town.update(food=100, wood=500, goods={'planks': 50, 'tools': 20, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})
        if level == 2:
            self.town['buildings'].extend([{'kind': 'home', 'x': 3, 'y': 5}, {'kind': 'farm', 'x': 6, 'y': 2}])
            self.town['map_resources'].append({'kind': 'water', 'x': 6, 'y': 3})
        elif level == 3:
            self.town['produced'] = {'wheat': 20, 'eggs': 6}
        elif level == 4:
            self.town['produced'] = {'planks': 20}
        elif level == 5:
            self.town['produced'] = {'tools': 5, 'bread': 10}
        elif level in {6, 7, 8}:
            self.town['produced'] = {6: {'leather': 6}, 7: {'iron_ore': 20}, 8: {'iron': 10}}[level]
        elif level == 9:
            self.town['produced'] = {'saddles': 2}
        elif level == 10:
            self.town.update(produced={'steel': 15}, completed_trades=1)

    def test_levels_two_through_ten_require_choice_and_finish_once(self):
        for upgrade in HALL_UPGRADES:
            with self.subTest(level=upgrade['level']):
                self.qualify(upgrade['level'])
                self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
                coins, wood = self.town['coins'], self.town['wood']
                costs = economy.costs_of(upgrade)
                costs.pop('coins')
                stock = {key: economy.quantity(self.town, key) for key in costs}
                command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town['wood'], wood - upgrade['wood'])
                for key, amount in costs.items():
                    self.assertEqual(economy.quantity(self.town, key), stock[key] - amount)
                self.assertEqual(self.town['coins'], coins)
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
            self.town.update(level=level-1, wood=500, harvested=10000, goods={'planks': 50, 'tools': 20, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                command(self.town, {'action': 'upgrade'})
            self.assertEqual(self.town, before)

    def test_missing_materials_do_not_spend(self):
        for upgrade in HALL_UPGRADES:
            self.town['level'] = upgrade['level'] - 1
            self.qualify(upgrade['level'])
            for key in [k for k in ('wood', 'planks', 'tools', 'leather', 'iron_ore', 'iron', 'steel') if k in upgrade]:
                if key == 'wood':
                    self.town['wood'] = upgrade[key] - .5
                else:
                    self.town['goods'][key] = upgrade[key] - .5
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    command(self.town, {'action': 'upgrade'})
                self.assertEqual(self.town, before)
                self.town.update(wood=500, goods={'planks': 50, 'tools': 20, 'leather': 20, 'iron_ore': 20, 'iron': 30, 'steel': 20})

    def test_production_thresholds_require_local_output_of_every_good(self):
        requirements = {
            3: {'wheat': 20, 'eggs': 6}, 4: {'planks': 20},
            5: {'tools': 5, 'bread': 10}, 6: {'leather': 6},
            7: {'iron_ore': 20}, 8: {'iron': 10}, 9: {'saddles': 2}, 10: {'steel': 15},
        }
        for level, goods in requirements.items():
            for key, count in goods.items():
                with self.subTest(level=level, missing=key):
                    self.setUp()
                    self.town['level'] = level - 1
                    self.qualify(level)
                    self.town['produced'] = dict(goods)
                    self.assertTrue(economy.milestone(self.town, level))
                    self.town['produced'][key] = count - .01
                    self.town['goods'][key] = 100  # Imports cannot qualify.
                    self.assertFalse(economy.milestone(self.town, level))
                    before = copy.deepcopy(self.town)
                    with self.assertRaisesRegex(ValueError, 'required to upgrade'):
                        command(self.town, {'action': 'upgrade'})
                    self.assertEqual(self.town, before)

    def test_level_five_accepts_processed_food_variety_but_not_raw_food_or_flour(self):
        self.town['produced'] = {'tools': 5, 'flour': 100, 'fish': 100, 'meat': 100}
        self.assertFalse(economy.milestone(self.town, 5))
        self.town['produced'].update(bread=3, smoked_trout=3, herbed_meat=4)
        self.assertTrue(economy.milestone(self.town, 5))
        self.town['produced']['tools'] = 4.99
        self.assertFalse(economy.milestone(self.town, 5))

    def test_saddles_are_kept_for_optional_riding_and_ore_for_smelting(self):
        for level, good in ((7, 'iron_ore'), (9, 'saddles')):
            with self.subTest(level=level):
                self.setUp()
                self.town['level'] = level - 1
                self.qualify(level)
                self.town['goods'][good] = 20
                self.assertFalse(self.town.get('horse_ridden', False))
                command(self.town, {'action': 'upgrade'})
                self.assertEqual(economy.quantity(self.town, good), 20)

    def test_every_production_gate_and_material_is_available_before_its_unlock(self):
        sources = {'wheat': economy.CROPS['wheat'], 'eggs': economy.ANIMALS['chicken']['level']}
        sources.update({output: economy.BUILDINGS[kind]['level']
                        for kind, (_, output, _) in economy.RECIPES.items()})
        for upgrade in HALL_UPGRADES[1:]:
            level = upgrade['level']
            for key in upgrade['production']:
                self.assertEqual(sources[key], level - 1, (level, key))
            for key in economy.costs_of(upgrade):
                if key not in {'wood', 'coins'}:
                    self.assertLess(sources[key], level, (level, key))

    def test_completed_production_survives_consumption_sales_and_reload(self):
        self.qualify(5)
        self.town.update(goods={}, food=0, wood=0)
        self.town = json.loads(json.dumps(self.town))
        self.assertTrue(economy.milestone(self.town, 5))

    def test_level_ten_requires_steel_and_either_trade_route(self):
        self.town['produced'] = {'steel': 15}
        self.assertFalse(economy.milestone(self.town, 10))
        self.town['market_sales'] = 9
        self.assertFalse(economy.milestone(self.town, 10))
        self.town['market_sales'] = 10
        self.assertTrue(economy.milestone(self.town, 10))
        self.town['market_sales'] = 0
        self.town['completed_trades'] = 1
        self.assertTrue(economy.milestone(self.town, 10))
        self.town['produced']['steel'] = 14.99
        self.assertFalse(economy.milestone(self.town, 10))

    def test_legacy_high_level_and_pending_upgrade_are_preserved(self):
        self.town.update(level=7, upgrade_remaining=3)
        tick(self.world)
        self.assertEqual(self.town['level'], 8)
        self.assertEqual(self.town['upgrade_remaining'], 0)
        with self.assertRaisesRegex(ValueError, 'Craft 2 saddles'):
            command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['level'], 8)
