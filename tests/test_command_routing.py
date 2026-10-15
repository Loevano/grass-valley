"""Player commands share bounded routing work and preserve other towns' jobs."""
import json
import unittest
from unittest.mock import patch

from fixtures import flat_town
from navigation import RouteCache
import logistics
import server
import work_cycles


class CommandRouting(unittest.TestCase):
    def setUp(self):
        self.cache = RouteCache(budget=2048)
        for module in (server, work_cycles, logistics):
            replacement = patch.object(module, 'routes', self.cache)
            replacement.start()
            self.addCleanup(replacement.stop)
        self.town = flat_town(size=40)
        self.town.update(level=5, wood=1000)
        self.barn = {'kind': 'barn', 'x': 22, 'y': 22,
                     'animals': {'pig': 2}, 'animal_growth': {'pig': [240, 240]}}
        self.shop = {'kind': 'butcher', 'x': 30, 'y': 30}
        self.town['buildings'].extend([self.barn, self.shop])
        self.town['villagers'] = [
            {'name': 'Builder', 'job': 'building',
             'construction_position': {'x': 2, 'y': 2}},
            {'name': 'Butcher', 'job': 'butchery', 'building_index': 2,
             'husbandry': 2, 'talents': {}}]
        self.world = {'towns': {'own': self.town}}

    def run_command(self, action):
        data = {'action': action, 'index': 2}
        if action == 'butcher_animal':
            data.update(barn_index=1, animal='pig')
            # Existing construction must not delay unrelated livestock orders.
            self.town['buildings'].append({'kind': 'home', 'x': 35, 'y': 2,
                'construction': {'progress': 0, 'total': 30}})
        with patch.object(self.cache, 'search', side_effect=AssertionError('Unbounded search')):
            server.command(self.town, data, self.world)
        self.assertLessEqual(self.cache.work, server.COMMAND_ROUTE_BUDGET)
        self.assertEqual(self.cache.budget, 2048, 'Commands retain the full simulation allowance')
        self.assertTrue(self.cache.pending, 'Long routes remain available for the next tick')
        self.assertEqual(self.town['villagers'][0]['construction_position'], {'x': 2, 'y': 2})
        self.assertNotIn('_spatial_index', json.dumps(self.world))

    def test_upgrade_returns_before_long_builder_search_finishes_and_resumes(self):
        self.run_command('upgrade_capacity')
        self.assertEqual(self.shop['construction']['upgrade'], {'settler_level': 2})
        for _ in range(30):
            with self.cache.update(self.world):
                server.construct(self.town, self.town)
            if self.town['villagers'][0]['construction_position'] != {'x': 2, 'y': 2}:
                break
        else:
            self.fail('Queued builder never resumed')

    def test_butchery_reserves_one_animal_before_unrelated_builder_search_finishes(self):
        self.run_command('butcher_animal')
        self.assertEqual(self.barn['animals']['pig'], 1)
        self.assertEqual(self.shop['butchering']['stage'], 'fetching')
        self.assertEqual(self.shop['butchering']['remaining'], 60)

    def test_command_preserves_another_towns_pending_route_and_admission_reservation(self):
        neighbor = flat_town(size=40)
        worker = neighbor['villagers'][0]
        self.world['towns']['neighbor'] = neighbor
        with self.cache.update(self.world, budget=0), self.cache.worker(worker):
            self.cache.route((1, 1), {'x': 38, 'y': 38}, (), 40, neighbor['terrain'])
        owner = ('work', id(worker))
        pending = self.cache.pending[owner]
        self.cache.reservation = ('work', id(neighbor['villagers'][1]))
        reservation = self.cache.reservation
        self.run_command('butcher_animal')
        self.assertIs(self.cache.pending[owner], pending)
        self.assertEqual(self.cache.reservation, reservation)

    def test_work_target_refresh_uses_bounded_routes_without_moving_worker(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 35, 'y': 35})
        self.town['map_resources'] = [{'kind': 'tree', 'x': 35, 'y': 36}]
        worker = {'name': 'Cutter', 'job': 'woodcutting', 'building_index': 3,
                  'construction_position': {'x': 1, 'y': 1}, 'woodcutting': 0}
        self.town['villagers'].append(worker)
        self.run_command('butcher_animal')
        self.assertEqual(worker['work_target'], self.town['map_resources'][0])
        self.assertEqual(worker['construction_position'], {'x': 1, 'y': 1})


if __name__ == '__main__':
    unittest.main()
