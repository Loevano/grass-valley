import json
import unittest

from server import (BUILD_SECONDS, COSTS, capacity, command, construct,
                    new_town, tick, work_source)


class Construction(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Builders')
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[], wood=500)
        self.world = {'towns': {'a': self.town}}

    def build(self, kind='home', x=6, y=6, **extra):
        command(self.town, dict(action='build', kind=kind, x=x, y=y, **extra))
        return self.town['buildings'][-1]

    def finish(self, building):
        for _ in range(100):
            if 'construction' not in building:
                return
            construct(self.town, self.town, {0})
        self.fail('Builder did not finish reachable construction')

    def test_every_placeable_kind_starts_as_a_site_and_costs_wood(self):
        self.town['buildings'].append({'kind': 'farm', 'x': 7, 'y': 6})
        self.town.update(level=5, goods={'planks': 100})
        for index, kind in enumerate(COSTS):
            with self.subTest(kind=kind):
                wood = self.town['wood']
                site = self.build(kind, x=6, y=6, **({'farm_index': 1} if kind == 'acre' else {}))
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
                                      for x, y in [(5, 6), (7, 6), (6, 5), (6, 7)]]
        construct(self.town, self.town, {0})
        self.assertEqual(site['construction']['progress'], 0)
        self.assertNotIn('construction_target', self.town['villagers'][0])

    def test_incomplete_buildings_have_no_housing_storage_or_production(self):
        home = self.build()
        store = self.build('storehouse', 4, 6)
        workplace = self.build('woodcutter', 2, 2)
        self.town['map_resources'] = [{'kind': 'tree', 'x': 1, 'y': 2}]
        self.assertEqual(capacity(self.town), 100)
        self.assertIsNone(work_source(self.town, 'woodcutting', 0))
        with self.assertRaises(ValueError):
            command(self.town, dict(action='job', index=0, job='woodcutting'))
        self.finish(home)
        self.finish(store)
        self.finish(workplace)
        self.assertEqual(capacity(self.town), 200)
        self.assertIsNotNone(work_source(self.town, 'woodcutting', 0))

    def test_progress_survives_save_and_relocation(self):
        site = self.build()
        construct(self.town, self.town, {0})
        construct(self.town, self.town, {0})
        progress = site['construction']['progress']
        self.assertGreater(progress, 0)
        self.town = json.loads(json.dumps(self.town))
        command(self.town, dict(action='move', index=1, x=8, y=6))
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
        self.assertEqual(capacity(self.town), 200)
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
        tick(self.world)
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
