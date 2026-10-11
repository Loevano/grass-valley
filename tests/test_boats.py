import copy
import json
import unittest
from unittest.mock import patch

import work_cycles
import progression as economy
from server import (command, construct, move_player, new_town, resource_reach,
                    tick, work_assignment)
from spatial import SpatialIndex


class Boats(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Boats')
        self.town.update(level=5, coins=200, wood=200, goods={'planks': 100},
                         npcs=[], terrain=['g' * 120] * 120)
        self.town['player']['fishing'] = 40

    def test_fishers_sail_offshore_and_return_to_unload(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        water = [{'kind': 'water', 'x': x, 'y': 2} for x in range(7, 13)]
        self.town['map_resources'] = water + [{'kind': 'water', 'x': 6, 'y': 6}]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', building_index=1, pending_output={'fish': 1},
                      logistics_position={'x': 6, 'y': 3})
        for spatial in (None, SpatialIndex(self.town['map_resources'])):
            self.assertEqual(work_assignment(self.town, 'fishing', 0, spatial), (hut, water[-1]))
        phases, positions = set(), set()
        for _ in range(500):
            unloaded = work_cycles.advance(self.town, self.town, fisher, hut, water[-1], 0, .1)
            state = fisher['work_animation']
            phases.add(state['phase'])
            positions.add((round(state['x']), round(state['y'])))
            if unloaded:
                break
        self.assertTrue(unloaded)
        self.assertTrue({'working', 'returning', 'depositing'} <= phases)
        self.assertIn((12, 2), positions)
        self.assertLessEqual(abs(state['x'] - hut['x']) + abs(state['y'] - hut['y']), 1)
        self.assertEqual(state['phase'], 'walking')

    def test_boat_tiers_reach_farther_and_prefer_available_fish(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2}
        self.town['buildings'].append(hut)
        self.town['villagers'][0].update(job='fishing', building_index=1)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2} for x in range(7, 18)]
        for tier, upgrades in enumerate((['boat'], ['boat', 'improved_boat'], ['boat', 'improved_boat', 'master_boat']), 1):
            hut['profession_upgrades'] = upgrades
            source = work_assignment(self.town, 'fishing', 0)[1]
            self.assertEqual(source['x'], 6 + 4 + 2 * tier)
            source['supply'] = 0
            self.assertEqual(work_assignment(self.town, 'fishing', 0)[1]['x'], source['x'] - 1)
            source.pop('supply')

    def test_sailing_follows_curved_water_and_resumes_after_save(self):
        hut = {'kind': 'fishing_hut', 'x': 2, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        tiles = [(3, 2), (3, 3), (3, 4), (4, 4), (5, 4), (5, 3), (5, 2)]
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': y} for x, y in tiles]
        source = self.town['map_resources'][-1]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', pending_output={'fish': 1}, logistics_position={'x': 2, 'y': 3})
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
        path = [(p['x'], p['y']) for p in fisher['work_animation']['path']]
        self.assertEqual(path[path.index(tiles[0]):], tiles)
        for step in range(500):
            if step == 50:
                self.town = json.loads(json.dumps(self.town))
                fisher = self.town['villagers'][0]
                hut = self.town['buildings'][1]
                source = self.town['map_resources'][-1]
            unloaded = work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
            if fisher['work_animation']['phase'] == 'returning':
                self.assertTrue(all((p['x'], p['y']) in tiles for p in fisher['work_animation']['path']))
            if unloaded:
                break
        self.assertTrue(unloaded)
        self.assertEqual((fisher['work_animation']['x'], fisher['work_animation']['y']), tiles[0])

    def reject(self, data, message):
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, message):
            command(self.town, data)
        self.assertEqual(self.town, before)

    def test_retargeting_offshore_preserves_position_after_delivery_and_save(self):
        hut = {'kind': 'fishing_hut', 'x': 2, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': y}
                                      for x, y in [(3, 2), (3, 3), (3, 4), (4, 4), (5, 4), (5, 3), (5, 2)]]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', logistics_position={'x': 2, 'y': 3})
        source = self.town['map_resources'][-1]
        for _ in range(31):
            work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
        self.town = json.loads(json.dumps(self.town))
        fisher, hut = self.town['villagers'][0], self.town['buildings'][1]
        before = dict(fisher['work_animation'])
        self.assertGreater(before['x'], 3)
        source = self.town['map_resources'][-2]
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, 0)
        state = fisher['work_animation']
        self.assertEqual((state['x'], state['y']), (before['x'], before['y']))
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
        self.assertLessEqual(((state['x'] - before['x']) ** 2 + (state['y'] - before['y']) ** 2) ** .5, .180001)
        water = {(r['x'], r['y']) for r in self.town['map_resources']}
        self.assertTrue(all((p['x'], p['y']) in water for p in state['path']))

    def test_blocked_route_keeps_position_for_retry(self):
        hut = {'kind': 'fishing_hut', 'x': 2, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2} for x in range(3, 9)]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', logistics_position={'x': 2, 'y': 3})
        source = self.town['map_resources'][-1]
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, 2)
        before = dict(fisher['work_animation'])
        obstacle = {'kind': 'home', **before['path'][0]}
        self.town['buildings'].append(obstacle)
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
        self.assertEqual(fisher['work_animation']['phase'], 'waiting')
        self.town['buildings'].remove(obstacle)
        work_cycles.advance(self.town, self.town, fisher, hut, source, 0, 0)
        state = fisher['work_animation']
        self.assertEqual((state['x'], state['y']), (before['x'], before['y']))
        self.assertEqual(state['phase'], 'walking')

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
            self.assertEqual(resource_reach(hut), reach if key == 'boat' else before_reach)
            self.reject(dict(upgrade, upgrade=key), 'Already unlocked' if key == 'boat' else 'construction')
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

    def test_each_hut_buys_one_independent_boat_immediately(self):
        self.town['buildings'].extend([{'kind': 'fishing_hut', 'x': 6, 'y': 2},
                                      {'kind': 'fishing_hut', 'x': 7, 'y': 2}])
        buy = {'action': 'profession_upgrade', 'index': 1, 'upgrade': 'boat'}
        command(self.town, buy)
        self.assertEqual(economy.fishing_boat_level(self.town['buildings'][1]), 1)
        self.assertEqual(economy.fishing_boat_level(self.town['buildings'][2]), 0)
        self.assertNotIn('construction', self.town['buildings'][1])
        self.reject(buy, 'Already unlocked')
        command(self.town, dict(buy, index=2))
        saved = json.loads(json.dumps(self.town))
        self.assertEqual([economy.fishing_boat_level(b) for b in saved['buildings'][1:]], [1, 1])
        self.assertEqual(self.town['goods']['planks'], 80)
        self.assertFalse(self.town['player'].get('boat'))

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
