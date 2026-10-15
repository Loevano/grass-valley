"""Cooperative host routing, fairness, invalidation and saved waiting state."""
import copy
import json
import unittest
from unittest.mock import patch

from fixtures import flat_town
from navigation import RouteCache
import logistics
import server
import work_cycles


class HostSearchQueue(unittest.TestCase):
    def setUp(self):
        self.cache = RouteCache(budget=8, quantum=2, pending=4)
        self.world = {'towns': {}}
        self.workers = [{'job': 'building'} for _ in range(8)]
        self.terrain = ['g' * 20] * 20

    def request(self, worker, target=None, blocked=(), priority=None):
        result = 'pending'
        with self.cache.worker(worker, priority=priority):
            result = self.cache.route((0, 0), target or {'x': 19, 'y': 19},
                                      blocked, 20, self.terrain, False, diagonal=True)
        return result

    def test_search_resumes_under_one_global_budget_and_matches_sync_result(self):
        expected = self.cache.route((0, 0), {'x': 19, 'y': 19}, (), 20, self.terrain,
                                    False, diagonal=True)
        self.cache.graphs.clear()
        for update in range(30):
            with self.cache.update(self.world):
                results = [self.request(worker) for worker in self.workers[:3]]
            self.assertLessEqual(self.cache.work, 8)
            if all(result != 'pending' for result in results):
                break
        else:
            self.fail('Ordinary searches did not complete')
        self.assertGreater(update, 0)
        self.assertTrue(all(result == expected for result in results))
        results[0][0]['x'] = 999
        self.assertNotEqual(results[1][0]['x'], 999)

    def test_priority_and_round_robin_prevent_ordinary_starvation(self):
        counts = [0, 0, 0]

        def search(start, target, *args):
            while True:
                counts[target['x']] += 1
                yield

        self.cache.budget, self.cache.quantum = 7, 1
        with patch.object(self.cache, 'search_steps', search):
            for _ in range(10):
                with self.cache.update(self.world):
                    for priority, worker in enumerate(self.workers[:3]):
                        self.request(worker, {'x': priority, 'y': 19}, priority=priority)
        self.assertEqual(counts, [40, 20, 10])

    def test_early_blocked_workers_cannot_spend_late_ordinary_service(self):
        self.cache = RouteCache(budget=8, quantum=1)
        workers = [{'job': 'building'} for _ in range(25)]
        ordinary_work = []

        def search(start, target, *args):
            while True:
                if target['y'] == 24:
                    ordinary_work.append(1)
                yield

        with patch.object(self.cache, 'search_steps', search):
            for _ in range(10):
                with self.cache.update(self.world):
                    for index, worker in enumerate(workers):
                        self.request(worker, {'x': 19, 'y': index}, priority=2 if index == 24 else 0)
        self.assertTrue(ordinary_work, 'Late ordinary actors must receive their reserved share')

    def test_pending_limit_backpressure_and_removed_workers_release_slots(self):
        with self.cache.update(self.world):
            for worker in self.workers:
                self.assertEqual(self.request(worker), 'pending')
                self.assertLessEqual(len(self.cache.pending), 4)
        with self.cache.update(self.world):
            pass
        self.assertEqual(self.cache.pending, {})
        with self.cache.update(self.world):
            self.request(self.workers[-1])
        self.assertEqual(len(self.cache.pending), 1)

    def test_urgency_distinguishes_blocked_routes_from_new_pending_work(self):
        cases = [({'work_animation': {'phase': 'waiting'}}, 0),
                 ({'work_animation': {'phase': 'waiting', 'route_pending': True}}, 2),
                 ({'pending_output': {'wood': 2}}, 1),
                 ({'logistics_trip': {'picked_up': True}}, 1)]
        for worker, priority in cases:
            with self.cache.worker(worker):
                self.assertEqual(self.cache.priority, priority)

    def test_saturated_admission_does_not_starve_ordinary_retries(self):
        self.cache = RouteCache(budget=2, quantum=1, pending=1)
        completed = [0, 0, 0]

        def search(*args):
            yield
            yield
            return ((1, 0),)

        with patch.object(self.cache, 'search_steps', search):
            for update in range(50):
                with self.cache.update(self.world):
                    for index, worker in enumerate(self.workers[:3]):
                        target = {'x': 5 + completed[index], 'y': index}
                        if self.request(worker, target, priority=0 if index < 2 else 2) != 'pending':
                            completed[index] += 1
                self.assertLessEqual(len(self.cache.pending), 1)
                if completed[2]:
                    break
        self.assertGreater(completed[2], 0)
        self.assertLess(update, 20)

    def test_pending_chunk_search_restarts_from_changed_immutable_terrain(self):
        from chunk_world import ChunkTerrain
        records = {'0,0': {'terrain': ['g' * 40] * 40}}
        ground = ChunkTerrain(records, 40)
        worker = self.workers[0]
        result = 'pending'
        for update in range(30):
            with self.cache.update(self.world), self.cache.worker(worker):
                result = self.cache.route((0, 0), {'x': 10, 'y': 0}, (), 40, ground, False,
                                          bounds=(0, 39, 0, 0))
            if update == 0:
                records['0,0']['terrain'][0] = 'gv' + 'g' * 38
            if result != 'pending':
                break
        self.assertIsNone(result)

    def test_navigation_task_and_world_changes_discard_obsolete_searches(self):
        worker = self.workers[0]
        with self.cache.update(self.world):
            self.request(worker)
        old = next(iter(self.cache.pending.values()))
        self.terrain[1] = 'v' * 20
        with self.cache.update(self.world):
            self.request(worker, {'x': 18, 'y': 18})
        self.assertIsNot(next(iter(self.cache.pending.values())), old)
        with self.cache.update({'towns': {}}):
            pass
        self.assertFalse(self.cache.pending)

    def test_multileg_prefix_survives_shared_graph_eviction(self):
        self.cache.graph_limit = 1
        worker = self.workers[0]
        first = None
        for _ in range(40):
            result = 'pending'
            with self.cache.update(self.world):
                with self.cache.worker(worker):
                    first = self.cache.route((0, 0), {'x': 3, 'y': 0}, (), 20, self.terrain, False)
                    result = self.cache.route((3, 0), {'x': 19, 'y': 19}, {(5, 5)}, 20,
                                              self.terrain, False, diagonal=True)
            if result != 'pending':
                break
        else:
            self.fail('A later leg was restarted by eviction of the first graph')
        self.assertEqual(first[-1], {'x': 3, 'y': 0})
        self.assertEqual(result[-1], {'x': 19, 'y': 19})

    def test_generator_preserves_bfs_astar_unreachable_and_fence_contracts(self):
        for diagonal, roads in ((False, ()), (False, {(x, 1) for x in range(20)}), (True, ())):
            for target, blocked in (({'x': 8, 'y': 0}, {(2, 0)}), ({(3, 3), (1, 4)}, ()),
                                    ({'x': 8, 'y': 0}, {(1, 0), (0, 1)})):
                args = ( (0, 0), target, blocked, 20, self.terrain, False, None, roads, 'smooth', diagonal)
                expected = RouteCache.search(*args)
                search = RouteCache.search_steps(*args)
                while True:
                    try:
                        next(search)
                    except StopIteration as done:
                        self.assertEqual(done.value, expected)
                        break


class QueuedWorkers(unittest.TestCase):
    def setUp(self):
        self.cache = RouteCache(budget=4, quantum=2)
        self.town = flat_town(size=32)
        self.world = {'towns': {'a': self.town}}
        self.patches = [patch.object(module, 'routes', self.cache)
                        for module in (server, work_cycles, logistics)]
        for replacement in self.patches:
            replacement.start()
            self.addCleanup(replacement.stop)

    def test_builder_claim_position_and_cargo_survive_wait_save_and_removal(self):
        worker = self.town['villagers'][0]
        worker.update(job='building', construction_position={'x': 2, 'y': 2},
                      pending_output={'wood': 2})
        site = {'kind': 'home', 'x': 27, 'y': 27, 'construction': {'progress': 0, 'total': 30}}
        self.town['buildings'].append(site)
        for _ in range(3):
            with self.cache.update(self.world):
                server.construct(self.town, self.town, {0})
            self.assertEqual(worker['construction_position'], {'x': 2, 'y': 2})
            self.assertEqual(worker['construction_target'], {'kind': 'home', 'x': 27, 'y': 27})
            self.assertEqual(worker['pending_output'], {'wood': 2})
            self.assertEqual(site['construction']['progress'], 0)
        saved = json.loads(json.dumps(self.world))
        self.assertEqual(saved, self.world)
        self.town['buildings'].remove(site)
        with self.cache.update(self.world):
            server.construct(self.town, self.town, {0})
        self.assertFalse(self.cache.pending)
        self.assertNotIn('construction_target', worker)

    def test_returning_worker_keeps_cargo_and_resumes_correct_leg(self):
        building = {'kind': 'woodcutter', 'x': 25, 'y': 25}
        tree = {'kind': 'tree', 'x': 2, 'y': 1}
        self.town['buildings'].append(building)
        self.town['map_resources'] = [tree]
        worker = self.town['villagers'][0]
        worker.update(job='woodcutting', pending_output={'wood': 2}, work_animation={
            'task': ['woodcutting', 'woodcutter', 25, 25, 'tree', 2, 1],
            'x': 2.2, 'y': 2, 'phase': 'waiting', 'resume_phase': 'returning',
            'path': [], 'elapsed': 0, 'version': 1, 'facing': 1})
        before = copy.deepcopy(worker)
        for _ in range(3):
            with self.cache.update(self.world), self.cache.worker(worker):
                self.assertFalse(work_cycles.advance(self.town, self.town, worker, building, tree, 0, 3))
            self.assertEqual(worker['work_animation']['x'], before['work_animation']['x'])
            self.assertEqual(worker['work_animation']['y'], 2)
            self.assertEqual(worker['pending_output'], {'wood': 2})
            self.assertEqual(worker['work_animation']['resume_phase'], 'returning')
        self.world = json.loads(json.dumps(self.world))
        self.town = self.world['towns']['a']
        worker = self.town['villagers'][0]
        building = self.town['buildings'][-1]
        tree = self.town['map_resources'][0]
        for _ in range(80):
            with self.cache.update(self.world), self.cache.worker(worker):
                work_cycles.advance(self.town, self.town, worker, building, tree, 0, 3)
            if worker['work_animation']['phase'] != 'waiting':
                break
        self.assertEqual(worker['work_animation']['phase'], 'returning')
        self.assertGreater(worker['work_animation']['x'], 2.2)

    def test_blocked_sale_preserves_actual_cargo_trip_and_position_while_queued(self):
        source = {'kind': 'storehouse', 'x': 2, 'y': 1}
        market = {'kind': 'market', 'x': 26, 'y': 1}
        self.town['buildings'] += [source, market]
        worker = self.town['villagers'][0]
        trip = {'operation': 'sell', 'workplace': dict(source), 'storage': dict(market),
                'path': [[x, 2] for x in range(2, 27)], 'step': 1,
                'pickup_step': 0, 'turn_step': 0, 'delivery_step': 24,
                'delivered': False, 'picked_up': True, 'cargo': {'planks': 4}}
        worker.update(job='trading', logistics_trip=trip, logistics_position={'x': 3, 'y': 2})
        self.town['map_resources'] = [{'kind': 'rock', 'x': 4, 'y': 2}]
        original = copy.deepcopy(trip['cargo'])
        for _ in range(3):
            with self.cache.update(self.world), self.cache.worker(worker):
                logistics.advance_trip(self.town, self.town, worker, 3)
            self.assertIs(worker['logistics_trip'], trip)
            self.assertEqual(trip['cargo'], original)
            self.assertEqual(worker['logistics_position'], {'x': 3, 'y': 2})
            self.assertTrue(trip['picked_up'])
        for _ in range(100):
            with self.cache.update(self.world), self.cache.worker(worker):
                logistics.advance_trip(self.town, self.town, worker, 0)
            if [4, 2] not in trip['path']:
                break
        self.assertNotIn([4, 2], trip['path'])
        self.assertEqual(trip['cargo'], original)
        self.assertEqual(trip['step'], 0)

    def test_queued_processor_approach_resumes_work_instead_of_unloading(self):
        building = {'kind': 'sawpit', 'x': 25, 'y': 25}
        self.town['buildings'].append(building)
        worker = self.town['villagers'][0]
        worker.update(job='sawing', construction_position={'x': 2, 'y': 2})
        for _ in range(100):
            with self.cache.update(self.world), self.cache.worker(worker):
                work_cycles.advance(self.town, self.town, worker, building, building, 0, 0)
            state = worker['work_animation']
            if state['phase'] != 'waiting':
                break
        self.assertEqual(state['phase'], 'walking')
        self.assertNotIn('resume_phase', state)

    def test_water_return_keeps_filled_can_while_route_waits(self):
        farm = {'kind': 'farm', 'x': 25, 'y': 25}
        acre = {'kind': 'acre', 'x': 26, 'y': 25, 'farm_index': 1,
                'field': {'stage': 'thirsty', 'cycle': 0, 'progress': 0, 'crop': 'vegetables'}}
        self.town['buildings'] += [farm, acre]
        worker = self.town['villagers'][0]
        worker.update(job='farming', building_index=1, work_animation={
            'task': ['farming', 'farm', 25, 25, 'acre', 26, 25, 'field_cycle', 0],
            'x': 2, 'y': 2, 'phase': 'collecting_water', 'waterSource': {'kind': 'water', 'x': 1, 'y': 2},
            'path': [], 'elapsed': .65, 'version': 1, 'facing': 1})
        for _ in range(3):
            with self.cache.update(self.world), self.cache.worker(worker):
                work_cycles.advance_farming(self.town, self.town, worker, farm, acre, 0, 3)
            state = worker['work_animation']
            self.assertEqual((state['x'], state['y']), (2, 2))
            self.assertEqual(state['phase'], 'waiting')
            self.assertEqual(state['resume_phase'], 'returning_water')
            self.assertTrue(state['waterReady'])
        for _ in range(100):
            with self.cache.update(self.world), self.cache.worker(worker):
                work_cycles.advance_farming(self.town, self.town, worker, farm, acre, 0, 0)
            if worker['work_animation']['phase'] != 'waiting':
                break
        self.assertEqual(worker['work_animation']['phase'], 'returning_water')
        self.assertTrue(worker['work_animation']['waterReady'])

    def test_work_completed_before_pending_return_still_earns_output_once(self):
        self.cache.budget = 0
        building = {'kind': 'woodcutter', 'x': 25, 'y': 25}
        tree = {'kind': 'tree', 'x': 24, 'y': 25, 'supply': 30}
        self.town['buildings'].append(building)
        self.town['map_resources'] = [tree]
        server.assign_settler(self.town, 0, 1)
        worker = self.town['villagers'][0]
        worker.update(talents={}, work_animation={
            'task': ['woodcutting', 'woodcutter', 25, 25, 'tree', 24, 25],
            'x': 23, 'y': 25, 'phase': 'working', 'elapsed': 3.1,
            'path': [], 'version': 1, 'facing': 1})
        server.tick(self.world)
        self.assertAlmostEqual(worker['pending_output']['wood'], .025)
        practice = worker['woodcutting']
        server.tick(self.world)
        self.assertAlmostEqual(worker['pending_output']['wood'], .025)
        self.assertEqual(worker['woodcutting'], practice)
        self.assertEqual(worker['work_animation']['phase'], 'waiting')

    def test_tick_budget_shared_by_towns_and_waiting_tree_claims_stay_separate(self):
        self.cache.budget = 0
        for n in range(2):
            town = self.town if n == 0 else flat_town(size=32)
            town['buildings'].append({'kind': 'woodcutter', 'x': 25, 'y': 25, 'settler_level': 2})
            town['map_resources'] = [{'kind': 'tree', 'x': 26, 'y': y, 'supply': 30} for y in (24, 25)]
            server.assign_settler(town, 0, 1)
            server.assign_settler(town, 1, 1)
            self.world['towns'][str(n)] = town
        del self.world['towns']['a']
        before = copy.deepcopy(self.world)
        with patch.object(self.cache, 'search', side_effect=AssertionError('Unbudgeted search')):
            server.tick(self.world)
        targets = []
        for name, town in self.world['towns'].items():
            for index in (0, 1):
                worker = town['villagers'][index]
                self.assertEqual(worker['production_status'], 'Waiting for a route')
                self.assertEqual(worker['woodcutting'], before['towns'][name]['villagers'][index]['woodcutting'])
                targets.append(worker['work_target']['y'])
            self.assertNotEqual(*targets[-2:])
        self.assertEqual(self.cache.work, 0)
        self.assertEqual(len(self.cache.pending), 4)


if __name__ == '__main__':
    unittest.main()
