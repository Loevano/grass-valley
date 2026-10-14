import json
import unittest
from unittest.mock import patch

import server

from server import (BUILD_SECONDS, COSTS, capacity, command, construct,
                    construction_route, new_town, tick, work_source)
from spatial import SpatialIndex


class Construction(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Builders')
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[], wood=500)
        self.world = {'towns': {'a': self.town}}

    def build(self, kind='home', x=8, y=6, **extra):
        command(self.town, dict(action='build', kind=kind, x=x, y=y, **extra))
        return self.town['buildings'][-1]

    def test_route_survives_ticks_save_and_unrelated_geometry(self):
        site = {'kind': 'home', 'x': 30, 'y': 30,
                'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        self.town['buildings'].append(site)
        with patch.object(server, 'construction_route', wraps=construction_route) as route:
            construct(self.town, self.town, {0}, elapsed=1)
            path = self.town['villagers'][0]['construction_walk']['path']
            self.town['map_resources'].append({'kind': 'rock', 'x': 100, 'y': 100})
            self.town['roads'] = [{'x': 3, 'y': 8}]
            construct(self.town, self.town, {0}, elapsed=1)
            self.assertIs(self.town['villagers'][0]['construction_walk']['path'], path)
            self.town = json.loads(json.dumps(self.town))
            construct(self.town, self.town, {0}, elapsed=1)
            self.assertEqual(route.call_count, 1)
            self.assertEqual(self.town['villagers'][0]['construction_walk']['path'], path)

    def test_blocked_step_replans_and_moved_target_changes_route(self):
        site = {'kind': 'home', 'x': 20, 'y': 20,
                'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        self.town['buildings'].append(site)
        with patch.object(server, 'construction_route', wraps=construction_route) as route:
            construct(self.town, self.town, {0}, elapsed=1)
            worker = self.town['villagers'][0]
            walk = worker['construction_walk']
            x, y = walk['path'][walk['step'] + 1]
            self.town['map_resources'].append({'kind': 'rock', 'x': x, 'y': y})
            construct(self.town, self.town, {0}, elapsed=1)
            self.assertEqual(route.call_count, 2)
            self.assertNotIn([x, y], worker['construction_walk']['path'])
            site['x'] = 21
            construct(self.town, self.town, {0}, elapsed=1)
            self.assertEqual(route.call_count, 3)
            self.assertEqual(worker['construction_target']['x'], 21)

    def test_new_site_does_not_replace_an_active_task(self):
        site = {'kind': 'home', 'x': 20, 'y': 20,
                'construction': {'progress': 0, 'total': BUILD_SECONDS}}
        self.town['buildings'].append(site)
        construct(self.town, self.town, {0}, elapsed=1)
        self.town['buildings'].insert(1, {'kind': 'home', 'x': 10, 'y': 10,
                                       'construction': {'progress': 0, 'total': BUILD_SECONDS}})
        construct(self.town, self.town, {0}, elapsed=1)
        self.assertEqual(self.town['villagers'][0]['construction_target'],
                         {'kind': 'home', 'x': 20, 'y': 20})

    def finish(self, building):
        for _ in range(100):
            if 'construction' not in building:
                return
            construct(self.town, self.town, {0})
        self.fail('Builder did not finish reachable construction')

    def test_every_placeable_kind_starts_as_a_site_and_costs_wood(self):
        self.town['buildings'].append({'kind': 'farm', 'x': 7, 'y': 6})
        self.town.update(level=10, wood=1000, goods={'planks': 200, 'tools': 10, 'iron': 20, 'steel': 20})
        for index, kind in enumerate(kind for kind, cost in COSTS.items() if cost):
            with self.subTest(kind=kind):
                wood = self.town['wood']
                site = self.build(kind, x=8, y=6, **({'farm_index': 1} if kind == 'acre' else {}))
                self.assertEqual(site['construction'], {'progress': 0, 'total': BUILD_SECONDS})
                self.assertEqual(self.town['wood'], wood - COSTS[kind])
                self.town['buildings'].pop()

    def test_builder_walks_before_starting_and_finishes_in_stages(self):
        site = self.build(x=5, y=1)
        construct(self.town, self.town, {0})
        self.assertEqual(site['construction']['progress'], 0)
        self.assertNotIn('construction_arrived', self.town['villagers'][0])
        for _ in range(100):
            construct(self.town, self.town, {0})
            if site['construction']['progress']:
                break
        self.assertEqual(site['construction']['progress'], 3)
        position = self.town['villagers'][0]['construction_position']
        self.assertEqual(abs(position['x'] - site['x']) + abs(position['y'] - site['y']), 1)
        while site['construction']['progress'] < 12:
            construct(self.town, self.town, {0})
        self.assertLess(site['construction']['progress'], BUILD_SECONDS)
        self.finish(site)
        self.assertNotIn('construction', site)
        self.assertNotIn('construction_target', self.town['villagers'][0])

    def test_no_workers_and_unreachable_sites_do_not_progress(self):
        site = self.build()
        construct(self.town, self.town, set())
        self.assertEqual(site['construction']['progress'], 0)
        self.town['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                      for x, y in [(7, 6), (9, 6), (8, 5), (8, 7)]]
        construct(self.town, self.town, {0})
        self.assertEqual(site['construction']['progress'], 0)
        self.assertNotIn('construction_target', self.town['villagers'][0])

    def test_farm_surrounded_by_completed_acres_can_upgrade(self):
        farm = {'kind': 'farm', 'x': 8, 'y': 8}
        self.town['buildings'].append(farm)
        self.town['buildings'].extend({'kind': 'acre', 'x': x, 'y': y, 'farm_index': 1}
                                      for x, y in [(7, 8), (9, 8), (8, 7), (8, 9)])
        for villager in self.town['villagers']:
            villager.update(job='farming', building_index=1)
        command(self.town, dict(action='upgrade_capacity', index=1))
        spatial = SpatialIndex(self.town['map_resources'])
        landscape = dict(self.town, _spatial_index=spatial)
        obstacles = spatial.blocked(landscape).copy()
        self.assertIsNotNone(construction_route(self.town, (6, 8), farm))
        self.assertIsNotNone(construction_route(landscape, (6, 8), farm))
        self.assertEqual(spatial.blocked(landscape), obstacles)
        for _ in range(30):
            construct(self.town, landscape)
        self.assertNotIn('construction', farm)
        self.assertEqual(farm['settler_level'], 2)
        self.assertTrue(all(v['job'] == 'farming' for v in self.town['villagers']))

    def test_unfinished_acres_still_block_construction_routes(self):
        farm = {'kind': 'farm', 'x': 8, 'y': 8}
        self.town['buildings'].append(farm)
        self.town['buildings'].extend({'kind': 'acre', 'x': x, 'y': y,
                                      'construction': {'progress': 0, 'total': BUILD_SECONDS}}
                                     for x, y in [(7, 8), (9, 8), (8, 7), (8, 9)])
        self.assertIsNone(construction_route(self.town, (6, 8), farm))

    def test_incomplete_buildings_have_no_housing_storage_or_production(self):
        home = self.build()
        store = self.build('storehouse', 4, 6)
        workplace = self.build('woodcutter', 2, 2)
        self.town['map_resources'] = [{'kind': 'tree', 'x': 1, 'y': 2}]
        self.assertEqual(capacity(self.town), 200)
        self.assertIsNone(work_source(self.town, 'woodcutting', 0))
        with self.assertRaises(ValueError):
            command(self.town, dict(action='job', index=0, job='woodcutting'))
        self.finish(home)
        self.finish(store)
        self.finish(workplace)
        self.assertEqual(capacity(self.town), 300)
        self.assertIsNotNone(work_source(self.town, 'woodcutting', 0))

    def test_progress_survives_save_and_relocation(self):
        site = self.build()
        for _ in range(3):
            construct(self.town, self.town, {0})
        progress = site['construction']['progress']
        self.assertGreater(progress, 0)
        self.town = json.loads(json.dumps(self.town))
        command(self.town, dict(action='move', index=1, x=9, y=6))
        self.assertEqual(self.town['buildings'][1]['construction']['progress'], progress)
        self.finish(self.town['buildings'][1])

    def test_multiple_workers_claim_different_sites(self):
        first = self.build()
        second = self.build('storehouse', 4, 6)
        construct(self.town, self.town, {0, 1})
        targets = [v.get('construction_target') for v in self.town['villagers'][:2]]
        self.assertTrue(all(targets))
        self.assertNotEqual(targets[0], targets[1])
        self.assertLessEqual(first['construction']['progress'], 3)
        self.assertLessEqual(second['construction']['progress'], 3)

    def test_legacy_buildings_remain_complete(self):
        self.town['buildings'].append({'kind': 'storehouse', 'x': 4, 'y': 6})
        self.assertEqual(capacity(self.town), 250)
        construct(self.town, self.town, {0})
        self.assertTrue(all('construction' not in b for b in self.town['buildings']))

    def test_idle_helpers_build_then_return_to_resting(self):
        site = self.build()
        self.assertIsNotNone(self.town['villagers'][0].get('construction_target'))
        for _ in range(20):
            tick(self.world)
        self.assertNotIn('construction', site)
        self.assertTrue(all(v['job'] == 'idle' and not v.get('construction_target')
                            for v in self.town['villagers']))

    def test_explicit_builder_has_priority_and_stays_assigned(self):
        command(self.town, dict(action='job', index=4, job='building'))
        site = self.build()
        self.assertIsNotNone(self.town['villagers'][4].get('construction_target'))
        self.assertNotIn('construction_target', self.town['villagers'][0])
        for _ in range(20):
            tick(self.world)
        self.assertNotIn('construction', site)
        self.assertEqual(self.town['villagers'][4]['job'], 'building')
        self.assertNotIn('construction_target', self.town['villagers'][4])

    def test_busy_workers_do_not_build_and_reassignment_pauses_progress(self):
        for v in self.town['villagers']:
            v['job'] = 'farming'
        site = self.build()
        tick(self.world)
        self.assertEqual(site['construction']['progress'], 0)
        self.assertEqual(len(self.town['villagers']), 5, 'Unfinished homes provide no housing')
        command(self.town, dict(action='job', index=0, job='building'))
        for _ in range(3):
            tick(self.world)
        progress = site['construction']['progress']
        self.assertGreater(progress, 0)
        command(self.town, dict(action='job', index=0, job='idle'))
        tick(self.world)
        self.assertGreater(site['construction']['progress'], progress, 'Idle villagers keep helping')
        self.town['villagers'][0]['job'] = 'farming'
        progress = site['construction']['progress']
        tick(self.world)
        self.assertEqual(site['construction']['progress'], progress)

    def test_destroying_site_clears_targets_and_releases_worker(self):
        self.build()
        self.build('storehouse', 4, 6)
        command(self.town, dict(action='destroy', index=1))
        targets = [v.get('construction_target') for v in self.town['villagers']]
        self.assertFalse(any(t and t['kind'] == 'home' for t in targets))
        self.assertTrue(any(t and t['kind'] == 'storehouse' for t in targets))

    def test_hall_upgrade_waits_for_worker_and_rewards_once(self):
        self.town.update(harvested=30, food=100)
        self.town['buildings'].extend([{'kind':'home','x':3,'y':5},{'kind':'farm','x':6,'y':4}])
        self.town['map_resources'].append({'kind':'water','x':7,'y':4})
        for v in self.town['villagers']:
            v['job'] = 'farming'
        command(self.town, dict(action='upgrade'))
        coins = self.town['coins']
        tick(self.world)
        self.assertEqual(self.town['upgrade_remaining'], 30)
        command(self.town, dict(action='job', index=0, job='building'))
        for _ in range(20):
            tick(self.world)
        self.assertEqual(self.town['level'], 2)
        self.assertEqual(self.town['coins'], coins + 30)
        self.assertEqual(self.town['upgrade_remaining'], 0)

    def test_unfinished_farms_cannot_add_acres(self):
        self.build('farm')
        wood = self.town['wood']
        with self.assertRaises(ValueError):
            self.build('acre', 6, 7, farm_index=1)
        self.assertEqual(self.town['wood'], wood)


if __name__ == '__main__':
    unittest.main()
