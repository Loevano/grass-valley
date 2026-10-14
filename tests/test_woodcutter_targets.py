import copy
import json
import unittest

from fixtures import flat_town
import logistics
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
