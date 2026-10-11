import copy
import json
import unittest
from unittest.mock import patch

import progression as economy
from server import command, construct, move_player, new_town, resource_reach, tick


class Boats(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Boats')
        self.town.update(level=5, coins=200, wood=200, goods={'planks': 100},
                         npcs=[], terrain=['g' * 120] * 120)
        self.town['player']['fishing'] = 40

    def reject(self, data, message):
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, message):
            command(self.town, data)
        self.assertEqual(self.town, before)

    def test_exploration_purchase_upgrade_costs_requirements_and_save(self):
        buy = {'action': 'buy_boat'}
        upgrade = {'action': 'upgrade_boat'}
        self.reject(upgrade, 'first')
        self.town['level'] = 2
        self.reject(buy, 'town hall')
        self.town['level'] = 5
        self.town['coins'] = 19
        self.reject(buy, 'materials')
        self.town['coins'] = 200
        command(self.town, buy)
        self.assertEqual(self.town['coins'], 180)
        self.reject(buy, 'already')
        self.town['player']['fishing'] = 29
        self.reject(upgrade, 'fishing')
        self.town['player']['fishing'] = 40
        self.town['level'] = 3
        self.reject(upgrade, 'town hall')
        self.town['level'] = 5
        for level in (2, 3):
            command(self.town, upgrade)
            self.assertEqual(economy.player_boat_level(self.town['player']), level)
            self.town = json.loads(json.dumps(self.town))
        self.assertEqual(self.town['coins'], 120)
        self.assertEqual(self.town['goods']['planks'], 65)
        self.reject(upgrade, 'fully upgraded')

    def test_legacy_boat_upgrades_and_water_speed_keeps_land_speed(self):
        self.town['player'].update(boat=True, x=20., y=20.)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': y}
                                      for x in range(19, 23) for y in range(19, 23)]
        self.assertEqual(economy.player_boat_level(self.town['player']), 1)
        command(self.town, {'action': 'upgrade_boat'})
        for afloat in (False, True):
            self.town['player'].update(x=20., y=20.)
            water = self.town['map_resources']
            if not afloat:
                self.town['map_resources'] = []
            move_player(self.town, {'dx': 1, 'dy': -1})
            self.assertAlmostEqual(self.town['player']['x'] - 20, .3 / 2**.5 * (1.25 if afloat else 1))
            self.town['map_resources'] = water

    def test_hut_tiers_require_previous_purchase_and_complete_before_effect(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2}
        self.town['buildings'].append(hut)
        upgrade = {'action': 'profession_upgrade', 'index': 1, 'upgrade': 'master_boat'}
        self.reject(upgrade, 'previous')
        for key, reach in [('boat', 6), ('improved_boat', 8), ('master_boat', 10)]:
            before_reach = resource_reach(hut)
            command(self.town, dict(upgrade, upgrade=key))
            self.assertEqual(resource_reach(hut), before_reach)
            self.reject(dict(upgrade, upgrade=key), 'construction')
            for _ in range(100):
                construct(self.town, self.town)
                if 'construction' not in hut:
                    break
            self.assertNotIn('construction', hut)
            self.assertEqual(resource_reach(hut), reach)
        self.assertEqual(self.town['coins'], 140)
        self.assertEqual(self.town['goods']['planks'], 55)
        saved = json.loads(json.dumps(self.town))
        self.assertEqual(resource_reach(saved['buildings'][1]), 10)
        self.reject(upgrade, 'Already unlocked')

    def test_hut_upgrades_increase_catches_and_stack_with_nets(self):
        self.town['buildings'].append({'kind': 'fishing_hut', 'x': 6, 'y': 2,
                                      'profession_upgrades': ['boat', 'improved_boat', 'master_boat', 'nets']})
        self.town['villagers'][0].update(job='fishing', talents={})
        with patch('server.secrets.randbelow', return_value=1):
            tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['buildings'][1]['pending_output']['fish'], 1.75)

    def test_character_catch_bonus_only_applies_afloat_and_respects_supply(self):
        for afloat, supply, expected in [(True, 12, 1.5), (False, 12, 1), (True, .5, .5)]:
            with self.subTest(afloat=afloat, supply=supply):
                self.setUp()
                self.town['player'].update(boat=True, boat_level=3, fishing=0,
                                           x=20., y=20. if afloat else 21.)
                source = {'kind': 'water', 'x': 20, 'y': 20, 'supply': supply}
                self.town['map_resources'] = [source]
                self.town['player']['activity'] = {'job': 'fishing', 'kind': 'water', 'x': 20, 'y': 20}
                food = self.town['food']
                with patch('progression.secrets.randbelow', return_value=1):
                    economy.player_tick(self.town, self.town, 200)
                self.assertAlmostEqual(self.town['food'] - food, expected)


if __name__ == '__main__':
    unittest.main()
