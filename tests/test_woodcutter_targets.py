import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server
import work_cycles
from spatial import SpatialIndex


class WoodcutterTargets(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Separate trees')
        self.camp = {'kind': 'woodcutter', 'x': 5, 'y': 3, 'settler_level': 2}
        self.town['buildings'].append(self.camp)
        self.trees = [{'kind': 'tree', 'x': 5, 'y': 2},
                      {'kind': 'tree', 'x': 7, 'y': 2},
                      {'kind': 'tree', 'x': 8, 'y': 2}]
        self.town['map_resources'] = self.trees.copy()
        for worker in self.town['villagers'][:2]:
            worker.update(job='woodcutting', building_index=1, talents={})

    def claim(self, index, tree, phase='walking'):
        camp = self.town['buildings'][self.town['villagers'][index]['building_index']]
        worker = self.town['villagers'][index]
        work_cycles.advance(self.town, self.town, worker, camp, tree, index, 0)
        worker['work_animation']['phase'] = phase

    def source(self, index):
        return server.work_source(self.town, 'woodcutting', index)

    def block_tree(self, tree):
        for dx, dy in ((1, 0), (0, 1), (-1, 0), (0, -1)):
            x, y = tree['x'] + dx, tree['y'] + dy
            if not any(b['x'] == x and b['y'] == y for b in self.town['buildings']):
                self.town['map_resources'].append({'kind': 'rock', 'x': x, 'y': y})

    def test_skips_boxed_in_nearest_tree(self):
        self.block_tree(self.trees[0])
        before = copy.deepcopy(self.town)
        self.assertEqual(self.source(0), self.trees[1])
        self.assertEqual(self.town, before)

    def test_skips_tree_on_other_side_of_impassable_terrain(self):
        self.town['terrain'] = ['g' * 16 if y != 3 else 'v' * 16 for y in range(16)]
        self.trees.append({'kind': 'tree', 'x': 8, 'y': 6})
        self.town['map_resources'] = self.trees.copy()
        self.assertEqual(self.source(0), self.trees[-1])

    def test_waiting_worker_replaces_newly_unreachable_tree(self):
        self.claim(0, self.trees[0], 'waiting')
        self.block_tree(self.trees[0])
        self.assertEqual(self.source(0), self.trees[1])

    def test_tree_selection_honors_other_towns_buildings(self):
        other = flat_town('Neighbor')
        other['buildings'] = [{'kind': 'home', 'x': x, 'y': y}
                              for x, y in ((4, 2), (5, 1), (6, 2))]
        shared = {key: self.town[key] for key in ('terrain', 'map_size', 'map_resources', 'npcs')}
        world = {'towns': {'owner': self.town, 'neighbor': other}, 'shared_map': shared}
        server.bind_map(world)
        walking = server.walking_map(world, self.town)
        assignment = server.work_assignment(self.town, 'woodcutting', 0, walking=walking)
        self.assertEqual(assignment[1], self.trees[1])

    def test_inaccessible_trees_do_not_produce_or_train_and_resume_when_opened(self):
        self.town['villagers'][1]['job'] = 'idle'
        self.town['map_resources'] = self.trees[:1]
        self.block_tree(self.trees[0])
        worker = self.town['villagers'][0]
        worker['construction_position'] = {'x': 8, 'y': 8}
        world = {'towns': {'owner': self.town}}
        skill, harvested = worker['woodcutting'], self.town['harvested']
        for _ in range(3):
            server.tick(world)
        self.assertIsNone(worker['work_target'])
        self.assertEqual(worker['production_status'], 'Waiting for an available tree')
        self.assertNotIn('pending_output', worker)
        self.assertEqual((worker['woodcutting'], self.town['harvested']), (skill, harvested))
        self.town['map_resources'] = [r for r in self.town['map_resources']
                                      if (r['x'], r['y']) != (6, 2)]
        for _ in range(5):
            server.tick(world)
            if worker.get('pending_output'):
                break
        self.assertEqual(worker['work_target'], self.trees[0])
        self.assertGreater(worker['pending_output']['wood'], 0)

    def test_travel_does_not_produce_or_train(self):
        self.town['villagers'][1]['job'] = 'idle'
        tree = {'kind': 'tree', 'x': 5, 'y': 12}
        self.town['map_resources'] = [tree]
        worker = self.town['villagers'][0]
        worker['construction_position'] = {'x': 5, 'y': 0}
        skill, harvested = worker['woodcutting'], self.town['harvested']
        server.tick({'towns': {'owner': self.town}})
        self.assertEqual(worker['work_animation']['phase'], 'walking')
        self.assertNotIn('pending_output', worker)
        self.assertEqual((worker['woodcutting'], self.town['harvested']), (skill, harvested))

    def test_partial_productive_tick_scales_wood_and_practice(self):
        self.town['villagers'][1]['job'] = 'idle'
        worker = self.town['villagers'][0]
        worker.update(woodcutting=0, construction_position={'x': 5, 'y': 0})
        server.tick({'towns': {'owner': self.town}})
        seconds = server.TICK_SECONDS - 1 / 1.8
        self.assertAlmostEqual(worker['pending_output']['wood'], economy.WOOD_PER_CYCLE * seconds / 3)
        self.assertEqual(worker['woodcutting'], round(.08 * seconds / 3, 3))

    def test_return_and_unloading_time_do_not_earn_wood_or_practice(self):
        self.town['villagers'][1]['job'] = 'idle'
        self.claim(0, self.trees[0], 'working')
        worker = self.town['villagers'][0]
        worker.update(woodcutting=0)
        worker['work_animation'].update(x=5, y=1, elapsed=3, path=[])
        server.tick({'towns': {'owner': self.town}})
        # Chopping finishes after .2s; delivery and the next approach fill the rest.
        phases = {sample['phase'] for sample in worker['work_animation']['playback']}
        self.assertTrue({'returning', 'depositing', 'walking'} <= phases)
        self.assertAlmostEqual(worker['pending_output']['wood'], economy.WOOD_PER_CYCLE * .2 / 3)
        self.assertEqual(worker['woodcutting'], round(.08 * .2 / 3, 3))

    def test_saved_waiting_cargo_returns_before_retrying_unreachable_tree(self):
        for removed in (False, True):
            with self.subTest(tree_removed=removed):
                self.setUp()
                self.town['villagers'][1]['job'] = 'idle'
                self.claim(0, self.trees[0], 'waiting')
                worker = self.town['villagers'][0]
                worker['work_animation'].update(x=8, y=8, path=[])
                logistics.record_output(self.town, self.camp, 'wood', 29, worker)
                if removed:
                    self.town['map_resources'].remove(self.trees[0])
                else:
                    self.block_tree(self.trees[0])
                self.town = json.loads(json.dumps(self.town))
                worker = self.town['villagers'][0]
                before, harvested = self.town['wood'], self.town['harvested']
                skill = worker['woodcutting']
                world = {'towns': {'owner': self.town}}
                server.tick(world)
                self.assertEqual(worker['work_animation']['phase'], 'returning')
                self.assertEqual(worker['pending_output'], {'wood': 29})
                self.assertEqual((worker['woodcutting'], self.town['harvested']), (skill, harvested))
                for _ in range(10):
                    if not worker.get('pending_output'):
                        break
                    server.tick(world)
                self.assertNotIn('pending_output', worker)
                self.assertEqual(self.town['wood'], before + 29)
                self.assertEqual(self.town['buildings'][1]['output_buffer']['wood'], 29)
                server.tick(world)
                self.assertEqual((worker['work_target']['x'], worker['work_target']['y']), (7, 2))

    def test_tick_assigns_separate_trees_with_shared_camp(self):
        server.tick({'towns': {'owner': self.town}})
        targets = [worker['work_target'] for worker in self.town['villagers'][:2]]
        self.assertEqual([(t['x'], t['y']) for t in targets], [(5, 2), (7, 2)])
        self.assertTrue(all(worker.get('work_animation') for worker in self.town['villagers'][:2]))

    def test_overlapping_camps_cannot_claim_the_same_tree(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 5, 'y': 1})
        self.town['villagers'][1]['building_index'] = 2
        self.claim(0, self.trees[0])
        self.assertEqual(self.source(1), self.trees[1])

    def test_target_survives_reload_and_nearer_tree_becoming_available(self):
        for phase in ('walking', 'working', 'returning', 'depositing'):
            with self.subTest(phase=phase):
                self.claim(1, self.trees[1], phase)
                self.town = json.loads(json.dumps(self.town))
                self.assertEqual(self.source(0), self.trees[0])
                self.assertEqual(self.source(1), self.trees[1])
                spatial = SpatialIndex(self.town['map_resources'])
                self.assertEqual(server.work_assignment(self.town, 'woodcutting', 1, spatial)[1], self.trees[1])

    def test_duplicate_saved_targets_resolve_in_worker_order(self):
        for index in (0, 1):
            self.claim(index, self.trees[0])
        self.assertEqual(self.source(0), self.trees[0])
        self.assertEqual(self.source(1), self.trees[1])
        server.tick({'towns': {'owner': self.town}})
        self.assertNotEqual(self.town['villagers'][0]['work_target'], self.town['villagers'][1]['work_target'])

    def test_waiting_worker_resumes_when_tree_is_released(self):
        self.town['map_resources'] = self.trees[:1]
        server.tick({'towns': {'owner': self.town}})
        waiting = self.town['villagers'][1]
        self.assertIsNone(waiting['work_target'])
        self.assertEqual(waiting['production_status'], 'Waiting for an available tree')
        server.command(self.town, {'action': 'job', 'index': 0, 'job': 'idle'})
        server.tick({'towns': {'owner': self.town}})
        self.assertEqual(waiting['work_target']['kind'], 'tree')

    def test_inactive_and_reassigned_workers_release_their_claim(self):
        for change in ({'job': 'idle'}, {'construction_target': {'x': 3, 'y': 3}},
                       {'logistics_trip': {'phase': 'walking'}}, {'building_index': 0}):
            with self.subTest(change=change):
                self.setUp()
                self.claim(0, self.trees[0])
                self.town['villagers'][0].update(change)
                self.assertEqual(self.source(1), self.trees[0])

    def test_felled_tree_is_replaced_after_carried_wood_is_delivered(self):
        self.claim(0, self.trees[0])
        self.town['map_resources'].remove(self.trees[0])
        self.assertEqual(self.source(0), self.trees[1])
        self.town['villagers'][0]['pending_output'] = {'wood': 2}
        self.assertIsNone(self.source(0), 'Finish delivery through the existing fallback')
        self.town['villagers'][0].pop('pending_output')
        self.assertEqual(self.source(0), self.trees[1])

    def test_tick_delivers_carried_wood_before_replacing_a_felled_tree(self):
        self.town['villagers'][1]['job'] = 'idle'
        self.claim(0, self.trees[0])
        worker = self.town['villagers'][0]
        logistics.record_output(self.town, self.camp, 'wood', 2, worker)
        self.town['map_resources'].remove(self.trees[0])
        before = self.town['wood']
        world = {'towns': {'owner': self.town}}
        server.tick(world)
        self.assertEqual((worker['work_target']['x'], worker['work_target']['y']), (5, 2))
        self.assertEqual(worker['production_status'], 'Returning gathered goods')
        for _ in range(20):
            if self.town['wood'] > before:
                break
            server.tick(world)
        self.assertEqual(self.town['wood'], before + 2)
        server.tick(world)
        self.assertEqual((worker['work_target']['x'], worker['work_target']['y']), (7, 2))

    def test_assignment_does_not_mutate_snapshots_or_reserve_fishing_water(self):
        self.claim(0, self.trees[0])
        before = copy.deepcopy(self.town)
        self.assertEqual(self.source(1), self.trees[1])
        self.assertEqual(self.town, before)
        self.camp['kind'] = 'fishing_hut'
        self.town['map_resources'] = [{'kind': 'water', 'x': 5, 'y': 2}]
        for worker in self.town['villagers'][:2]:
            worker['job'] = 'fishing'
        self.assertEqual(server.work_source(self.town, 'fishing', 0),
                         server.work_source(self.town, 'fishing', 1))


if __name__ == '__main__':
    unittest.main()
