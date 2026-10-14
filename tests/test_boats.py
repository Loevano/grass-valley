from fixtures import flat_town
import copy
import json
import unittest
from unittest.mock import patch

import logistics
import work_cycles
import progression as economy
from server import (command, construct, move_player, new_town, resource_reach,
                    tick, work_assignment)
from spatial import SpatialIndex


class Boats(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Boats', size=120)
        self.town.update(level=5, coins=200, wood=200, goods={'planks': 100},
                         npcs=[], terrain=['g' * 120] * 120)
        self.town['player']['fishing'] = 40

    def test_pier_and_route_are_retained_during_unchanged_work(self):
        hut = {'kind': 'fishing_hut', 'x': 2, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2} for x in range(3, 9)]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', logistics_position={'x': 2, 'y': 3})
        source = self.town['map_resources'][-1]
        with patch.object(work_cycles, 'route', wraps=work_cycles.route) as route:
            work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
            queries = route.call_count
            path = fisher['work_animation']['path']
            self.town['buildings'].append({'kind': 'home', 'x': 50, 'y': 50})
            work_cycles.advance(self.town, self.town, fisher, hut, source, 0, .1)
            self.assertEqual(route.call_count, queries)
            self.assertIs(fisher['work_animation']['path'], path)

    def test_boat_crew_capacity_leaves_other_fishers_on_the_dock(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'settler_level': 4}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2} for x in range(7, 18)]
        self.town['villagers'] = [{'job': 'fishing', 'building_index': 1,
                                  'logistics_position': {'x': 6, 'y': 3}} for _ in range(4)]
        for tier in range(1, 4):
            hut['profession_upgrades'] = ['boat', 'improved_boat', 'master_boat'][:tier]
            for index, fisher in enumerate(self.town['villagers']):
                with self.subTest(tier=tier, worker=index):
                    for spatial in (None, SpatialIndex(self.town['map_resources'])):
                        source = work_assignment(self.town, 'fishing', index, spatial)[1]
                        self.assertEqual(source['x'], 6 + 4 + 2 * tier if index < tier else 7)
                    self.assertEqual(work_cycles.fishing_boating(self.town, hut, index), index < tier)
                    work_cycles.advance(self.town, self.town, fisher, hut, source, index, 0)
                    if index >= tier:
                        self.assertTrue(all(p['x'] <= 7 for p in fisher['work_animation']['path']))
                        work_cycles.advance(self.town, self.town, fisher, hut, source, index, 3)
                        self.assertEqual((fisher['work_animation']['x'], fisher['work_animation']['y']), (7, 2))

    def test_fishers_sail_offshore_and_return_to_unload(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        water = [{'kind': 'water', 'x': x, 'y': 2} for x in range(7, 13)]
        self.town['map_resources'] = water + [{'kind': 'water', 'x': 6, 'y': 6}]
        fisher = self.town['villagers'][0]
        fisher.update(job='fishing', building_index=1, pending_output={'fish': 5},
                      logistics_position={'x': 6, 'y': 3})
        for spatial in (None, SpatialIndex(self.town['map_resources'])):
            self.assertEqual(work_assignment(self.town, 'fishing', 0, spatial), (hut, water[-1]))
        work_cycles.advance(self.town, self.town, fisher, hut, water[-1], 0, 0)
        path = fisher['work_animation']['path']
        self.assertEqual([(p['x'], p['y']) for p in path if p['y'] == 2][:2], [(7, 2), (8, 2)])
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
        departure = next(p for p in state['playback'] if p['phase'] == 'walking')
        self.assertEqual((departure['x'], departure['y']), (8, 2), 'Unloading finishes aboard one tile beyond the dock')
        self.assertGreater(state['x'], departure['x'], 'Remaining tick time starts the next voyage')
        self.assertEqual(state['phase'], 'walking')

    def test_partial_cargo_waits_for_full_load_or_empty_source_and_survives_save(self):
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'profession_upgrades': ['boat']}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2, 'supply': 10} for x in range(7, 13)]
        source = self.town['map_resources'][-1]
        worker = self.town['villagers'][0]
        worker.update(job='fishing', building_index=1, logistics_position={'x': 12, 'y': 2})
        logistics.record_output(self.town, hut, 'fish', 2, worker)
        work_cycles.advance(self.town, self.town, worker, hut, source, 0, 7)
        self.assertEqual(worker['work_animation']['phase'], 'working')
        self.town = json.loads(json.dumps(self.town))
        worker, hut = self.town['villagers'][0], self.town['buildings'][1]
        source = self.town['map_resources'][-1]
        source['supply'] = 0
        for _ in range(100):
            if work_cycles.advance(self.town, self.town, worker, hut, source, 0, .1):
                break
        departure = next(p for p in worker['work_animation']['playback'] if p['phase'] == 'walking')
        self.assertEqual((departure['x'], departure['y']), (8, 2))
        logistics.drop_off(self.town, hut, worker)
        self.assertEqual(hut['output_buffer']['fish'], 2)
        self.assertEqual(work_cycles.boat_load(self.town, hut), 0)

    def test_boat_capacity_is_shared_by_crew_and_survives_unloading(self):
        for tier, capacity in enumerate((5, 10, 20), 1):
            with self.subTest(tier=tier):
                self.setUp()
                hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'settler_level': 3,
                       'profession_upgrades': ['boat', 'improved_boat', 'master_boat'][:tier]}
                self.town['buildings'].append(hut)
                self.town['map_resources'] = [{'kind': 'water', 'x': x, 'y': 2} for x in range(7, 13)]
                self.town['villagers'] = [{'job': 'fishing', 'building_index': 1, 'talents': {},
                                           'logistics_position': {'x': 12, 'y': 2}} for _ in range(tier)]
                for worker in self.town['villagers']:
                    logistics.record_output(self.town, hut, 'fish', (capacity - .25) / tier, worker)
                self.assertEqual(economy.fishing_boat_capacity(hut), capacity)
                with patch('server.secrets.randbelow', return_value=1):
                    tick({'towns': {'a': self.town}})
                self.assertAlmostEqual(work_cycles.boat_load(self.town, hut), capacity)
                leader = self.town['villagers'][0]['work_animation']
                # Tick may also add an idle resident; only the original crew
                # shares this voyage.
                for worker in self.town['villagers'][1:tier]:
                    self.assertEqual(worker['work_animation'], leader, 'Crew shares one confirmed voyage')
                for _ in range(20):
                    tick({'towns': {'a': self.town}})
                    if hut.get('output_buffer', {}).get('fish', 0) >= capacity:
                        break
                self.assertEqual(hut['output_buffer']['fish'], capacity)

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
        fisher.update(job='fishing', pending_output={'fish': 5}, logistics_position={'x': 2, 'y': 3})
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
        departure = next(p for p in fisher['work_animation']['playback'] if p['phase'] == 'walking')
        self.assertEqual((departure['x'], departure['y']), tiles[1])

    def test_upgrades_choose_deeper_water_and_catch_better_species(self):
        upgrades = []
        for tier, (upgrade, species) in enumerate(zip(('boat', 'improved_boat', 'master_boat'), ('trout', 'salmon', 'sturgeon')), 1):
            upgrades.append(upgrade)
            with self.subTest(tier=tier):
                town = new_town('Deep fishing')
                town.update(npcs=[], terrain=['g' * 120] * 120,
                            map_resources=[{'kind': 'water', 'x': x, 'y': y} for x in range(3, 18) for y in range(1, 16)])
                hut = {'kind': 'fishing_hut', 'x': 2, 'y': 8, 'profession_upgrades': list(upgrades)}
                town['buildings'].append(hut)
                worker = town['villagers'][0]
                worker.update(job='fishing', building_index=1, talents={})
                for spatial in (None, SpatialIndex(town['map_resources'])):
                    source = work_assignment(town, 'fishing', 0, spatial)[1]
                    depth = SpatialIndex(town['map_resources']).water_depths()[(source['x'], source['y'])]
                    self.assertEqual(depth, tier + 1)
                    self.assertEqual(abs(source['x'] - hut['x']) + abs(source['y'] - hut['y']), resource_reach(hut))
                worker['logistics_position'] = {'x': source['x'], 'y': source['y']}
                with patch('server.secrets.randbelow', return_value=1):
                    tick({'towns': {'a': town}})
                self.assertEqual(hut['pending_output'], {species: 1 + .25 * (tier - 1)})
                self.assertTrue(economy.accepts_storage(hut, species))
                depths = SpatialIndex(town['map_resources']).water_depths()
                allowed = work_cycles.fishing_water(hut, town['map_resources'], set(), resource_reach(hut), depths=depths)
                self.assertTrue(all(depths[(r['x'], r['y'])] <= tier + 1 for r in allowed))

    def test_depth_accounts_for_islands_and_refreshes_with_water_layout(self):
        resources = [{'kind': 'water', 'x': x, 'y': y} for x in range(7) for y in range(7)]
        spatial = SpatialIndex(resources)
        self.assertEqual(spatial.water_depths()[(3, 3)], 4)
        self.assertIs(spatial.water_depths(), spatial.water_depths())
        resources.remove(next(r for r in resources if (r['x'], r['y']) == (3, 3)))
        self.assertEqual(spatial.water_depths()[(3, 2)], 1)

    def test_premium_fish_food_storage_and_market_values(self):
        economy.ensure_inventory(self.town)
        self.town['buildings'].append({'kind': 'market', 'x': 2, 'y': 2})
        hut = {'kind': 'fishing_hut', 'x': 6, 'y': 2, 'stored_goods': {'sturgeon': 16}}
        self.assertAlmostEqual(economy.local_room(hut, 'sturgeon'), 34)
        for key, spec in economy.FISH.items():
            with self.subTest(species=key):
                economy.add_stock(self.town, key, 1)
                self.town['buildings'][0].setdefault('stored_goods', {})[key] = 1
                food, coins = self.town['food'], self.town['coins']
                command(self.town, {'action': 'sell_goods', 'resource': key, 'amount': 1})
                self.assertEqual(self.town['coins'] - coins, spec['coins'])
                self.assertAlmostEqual(food - self.town['food'], spec['food'])
        for tier, upgrades in enumerate(([], ['boat'], ['boat', 'improved_boat'], ['boat', 'improved_boat', 'master_boat'])):
            self.assertEqual(economy.fishing_catch({'profession_upgrades': upgrades}, 1), 'fish')
            self.assertEqual(economy.fishing_catch({'profession_upgrades': upgrades}, 10), list(economy.FISH)[tier])

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
        self.town['map_resources'] = [{'kind': 'water', 'x': 7, 'y': 2}]
        self.town['buildings'].append({'kind': 'fishing_hut', 'x': 6, 'y': 2,
                                      'profession_upgrades': ['boat', 'improved_boat', 'master_boat', 'nets']})
        self.town['villagers'][0].update(job='fishing', talents={}, logistics_position={'x': 7, 'y': 2})
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
