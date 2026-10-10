import copy
import unittest

from server import construct, capacity, command, ensure_resources, new_town, tick


class VillageRules(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Willowbrook')

    def finish_construction(self):
        for _ in range(100):
            if not any('construction' in b for b in self.town['buildings']):
                return
            construct(self.town, self.town)
        self.fail('Construction did not finish')

    def test_expanded_map_and_legacy_migration(self):
        self.assertEqual(self.town['map_size'] ** 2, 100 * 12 ** 2)
        self.assertTrue(any(r['x'] > 100 for r in self.town['map_resources']))
        legacy = copy.deepcopy(self.town)
        del legacy['map_size']
        legacy['map_resources'] = [r for r in legacy['map_resources']
                                   if r['x'] < 12 and r['y'] < 12]
        original = copy.deepcopy(legacy)
        ensure_resources(legacy)
        self.assertEqual(legacy['buildings'], original['buildings'])
        self.assertEqual([r for r in legacy['map_resources'] if r['x'] < 12 and r['y'] < 12],
                         original['map_resources'])
        migrated = copy.deepcopy(legacy)
        ensure_resources(legacy)
        self.assertEqual(legacy, migrated)

    def test_build_and_move_preserve_building(self):
        command(self.town, {'action': 'build', 'kind': 'farm', 'x': 2, 'y': 3})
        self.finish_construction()
        self.assertEqual(self.town['wood'], 50)
        command(self.town, {'action': 'move', 'index': 1, 'x': 4, 'y': 4})
        self.assertEqual(self.town['buildings'][1], {'kind': 'farm', 'x': 4, 'y': 4})
        self.assertEqual(self.town['coins'], 19)

    def test_rejected_build_does_not_spend_resources(self):
        for x, y in [(5, 5), (-1, 3), (120, 2)]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                command(self.town, {'action': 'build', 'kind': 'home', 'x': x, 'y': y})
            self.assertEqual(before, self.town)

    def test_moving_remains_available_without_coins(self):
        self.town['coins'] = 0
        command(self.town, {'action': 'move', 'index': 0, 'x': 1, 'y': 1})
        self.assertEqual(self.town['coins'], 0)
        self.assertEqual(self.town['buildings'][0]['x'], 1)

    def test_job_requires_matching_building(self):
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 2, 'y': 2})
        self.finish_construction()
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        wood, skill = self.town['wood'], self.town['villagers'][0]['woodcutting']
        tick({'towns': {'absent-owner': self.town}})
        self.assertGreater(self.town['wood'], wood)
        self.assertEqual(self.town['villagers'][0]['woodcutting'], skill + 1)

    def test_destroy_removes_each_building_kind_without_refund(self):
        for kind in ['home', 'woodcutter', 'farm', 'storehouse']:
            with self.subTest(kind=kind):
                town = new_town('Test')
                command(town, {'action': 'build', 'kind': kind, 'x': 2, 'y': 3})
                resources = [town[key] for key in ['wood', 'food', 'coins']]
                command(town, {'action': 'destroy', 'index': 1})
                self.assertEqual(town['buildings'], [{'kind': 'hall', 'x': 5, 'y': 5}])
                self.assertEqual([town[key] for key in ['wood', 'food', 'coins']], resources)
                self.assertEqual(capacity(town), 100)
                command(town, {'action': 'build', 'kind': kind, 'x': 2, 'y': 3})

    def test_destroy_rejects_town_hall_and_invalid_indices(self):
        for index in [0, -1, 1, None, True, 0.0, '0']:
            with self.subTest(index=index):
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    command(self.town, {'action': 'destroy', 'index': index})
                self.assertEqual(self.town, before)

    def test_destroy_preserves_acre_links_when_building_indices_shift(self):
        self.town['buildings'].extend([
            {'kind': 'home', 'x': 1, 'y': 1},
            {'kind': 'farm', 'x': 2, 'y': 2},
            {'kind': 'acre', 'x': 2, 'y': 3, 'farm_index': 2},
        ])
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual(self.town['buildings'][2]['farm_index'], 1)
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertIsNone(self.town['buildings'][1]['farm_index'])

    def test_destroy_workplace_clears_targets_and_pauses_production(self):
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 2, 'y': 2})
        self.finish_construction()
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        self.assertIsNotNone(self.town['villagers'][0]['work_target'])
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertIsNone(self.town['villagers'][0]['work_target'])
        wood = self.town['wood']
        skill = self.town['villagers'][0]['woodcutting']
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['wood'], wood)
        self.assertEqual(self.town['villagers'][0]['woodcutting'], skill)

    def test_full_storage_pauses_production_and_skill(self):
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 1, 'y': 1})
        self.finish_construction()
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        self.town['wood'] = capacity(self.town)
        skill = self.town['villagers'][0]['woodcutting']
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['wood'], capacity(self.town))
        self.assertEqual(self.town['villagers'][0]['woodcutting'], skill)

    def test_food_shortage_pauses_growth_without_losing_villagers(self):
        command(self.town, {'action': 'build', 'kind': 'home', 'x': 1, 'y': 1})
        self.finish_construction()
        self.town['food'] = 0
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 5)
        self.town['food'] = 40
        tick({'towns': {'a': self.town}})
        self.assertEqual(len(self.town['villagers']), 6)

    def test_upgrade_requires_milestone_and_explicit_choice(self):
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade'})
        self.town['harvested'] = 30
        self.town['buildings'].extend([{'kind':'home','x':3,'y':5},{'kind':'farm','x':6,'y':2}])
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['level'], 1)
        self.town['villagers'][0]['construction_position'] = {'x': 5, 'y': 6}
        command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['level'], 1)
        self.assertEqual(self.town['upgrade_remaining'], 30)
        wood, coins = self.town['wood'], self.town['coins']
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade'})
        self.assertEqual(self.town['wood'], wood)
        for _ in range(9):
            tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['level'], 1)
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['level'], 2)
        self.assertEqual(self.town['coins'], coins + 30)
        tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['coins'], coins + 30)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'upgrade'})

    def test_resources_reserve_tiles_without_spending_on_rejection(self):
        for kind in ['tree', 'water', 'rock']:
            tile = next(r for r in self.town['map_resources'] if r['kind'] == kind)
            for action in [{'action': 'build', 'kind': 'home'}, {'action': 'move', 'index': 0}]:
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    command(self.town, dict(action, x=tile['x'], y=tile['y']))
                self.assertEqual(self.town, before)

    def test_woodcutting_pauses_outside_forest_reach_and_resumes_after_move(self):
        command(self.town, {'action': 'build', 'kind': 'woodcutter', 'x': 6, 'y': 6})
        self.finish_construction()
        command(self.town, {'action': 'job', 'index': 0, 'job': 'woodcutting'})
        before = self.town['wood'], self.town['villagers'][0]['woodcutting']
        tick({'towns': {'a': self.town}})
        self.assertEqual(before, (self.town['wood'], self.town['villagers'][0]['woodcutting']))
        self.assertIsNone(self.town['villagers'][0]['work_target'])
        command(self.town, {'action': 'move', 'index': 1, 'x': 2, 'y': 2})
        tick({'towns': {'a': self.town}})
        self.assertGreater(self.town['wood'], before[0])
        self.assertEqual(self.town['villagers'][0]['work_target']['kind'], 'tree')

    def test_farming_uses_water_at_reach_boundary(self):
        command(self.town, {'action': 'build', 'kind': 'farm', 'x': 6, 'y': 2})
        self.finish_construction()
        command(self.town, {'action': 'job', 'index': 0, 'job': 'farming'})
        food = self.town['food']
        tick({'towns': {'a': self.town}})
        self.assertGreater(self.town['food'], food)
        self.assertEqual(self.town['villagers'][0]['work_target']['kind'], 'water')
        command(self.town, {'action': 'move', 'index': 1, 'x': 5, 'y': 2})
        food, skill = self.town['food'], self.town['villagers'][0]['farming']
        tick({'towns': {'a': self.town}})
        self.assertLess(self.town['food'], food)
        self.assertEqual(self.town['villagers'][0]['farming'], skill)
        self.assertIsNone(self.town['villagers'][0]['work_target'])

    def test_legacy_towns_keep_existing_buildings_when_resources_are_added(self):
        del self.town['map_resources']
        self.town['buildings'].extend([{'kind': 'farm', 'x': 9, 'y': 1},
                                       {'kind': 'woodcutter', 'x': 0, 'y': 2}])
        buildings = copy.deepcopy(self.town['buildings'])
        ensure_resources(self.town)
        self.assertEqual(self.town['buildings'], buildings)
        occupied = {(b['x'], b['y']) for b in buildings}
        self.assertFalse(any((r['x'], r['y']) in occupied for r in self.town['map_resources']))
        resources = copy.deepcopy(self.town['map_resources'])
        ensure_resources(self.town)
        self.assertEqual(resources, self.town['map_resources'])


if __name__ == '__main__':
    unittest.main()
