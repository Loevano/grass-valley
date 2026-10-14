import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


CAPACITIES = {
    'farm': (1, 2, 4), 'mill': (1,), 'bakery': (1,),
    'woodcutter': (1, 2, 3), 'sawpit': (1, 2), 'workshop': (1,),
    'mine': (1, 2, 4, 8), 'smelter': (1, 2, 3), 'steelworks': (1,),
}


class WorkerCapacity(unittest.TestCase):
    def test_upgrade_paths_cost_time_and_assignment_limits(self):
        for sandbox in (False, True):
            for kind, capacities in CAPACITIES.items():
                with self.subTest(kind=kind, sandbox=sandbox):
                    town = flat_town()
                    town.update(sandbox=sandbox, wood=500)
                    building = {'kind': kind, 'x': 7, 'y': 5}
                    town['buildings'].append(building)
                    for tier, capacity in enumerate(capacities, 1):
                        self.assertEqual(server.settler_capacity(building), capacity)
                        job = server.BUILDING_JOBS[kind]
                        town['villagers'] = [dict(name=str(i), job='idle', talents={},
                                                 **economy.JOB_REQUIREMENTS.get(job, {}))
                                            for i in range(capacity + 1)]
                        for i in range(capacity):
                            server.command(town, dict(action='assign_settler', index=i, building_index=1))
                        before = copy.deepcopy(town)
                        for action in (dict(action='assign_settler', index=capacity, building_index=1),
                                       dict(action='job', index=capacity, job=job)):
                            with self.assertRaises(ValueError):
                                server.command(town, action)
                            self.assertEqual(town, before)
                        if tier < len(capacities):
                            wood = town['wood']
                            storage = economy.storage_allocations(building)
                            server.command(town, dict(action='upgrade_capacity', index=1))
                            self.assertEqual(town['wood'], wood - (0 if sandbox else 20 * tier))
                            if not sandbox:
                                self.assertEqual(building['construction']['total'], 30 * tier)
                                self.assertEqual(server.settler_capacity(building), capacity)
                                for _ in range(100):
                                    server.construct(town, town, workers={0})
                                    if 'construction' not in building:
                                        break
                            self.assertNotIn('construction', building)
                            self.assertEqual(economy.storage_allocations(building), storage)
                    before = copy.deepcopy(town)
                    with self.assertRaises(ValueError):
                        server.command(town, dict(action='upgrade_capacity', index=1))
                    self.assertEqual(town, before)
                    self.assertEqual(server.settler_capacity(dict(building, settler_level=99)), capacities[-1])

    def test_legacy_saves_preserve_capacity_cargo_and_residents(self):
        for kind, capacities in CAPACITIES.items():
            with self.subTest(kind=kind):
                town = flat_town()
                town.pop('worker_capacity_version')
                building = dict(kind=kind, x=7, y=5, settler_level=3,
                                construction={'progress': 5, 'total': 60,
                                              'upgrade': {'settler_level': 4}})
                town['buildings'].append(building)
                town['villagers'] = [dict(name=str(i), job=server.BUILDING_JOBS[kind],
                                         building_index=1, worker_tool='fine_axe', farming=12)
                                    for i in range(10)]
                excess = town['villagers'][-1]
                logistics.record_output(town, building, 'wood', 2, excess)
                server.ensure_resources(town)
                self.assertEqual(server.settler_capacity(building), capacities[-1])
                self.assertEqual(building['settler_level'], len(capacities))
                self.assertEqual(building['construction']['upgrade']['settler_level'], len(capacities))
                self.assertEqual(building['construction']['progress'], 5)
                self.assertEqual(server.assigned_settlers(town, 1), list(range(capacities[-1])))
                self.assertEqual(len(town['villagers']), 10)
                self.assertEqual(excess['job'], 'idle')
                self.assertNotIn('building_index', excess)
                self.assertEqual(excess['worker_tool'], 'fine_axe')
                self.assertEqual(excess['farming'], 12)
                self.assertEqual(logistics.collection_inventory(building)['wood'], 2)
                restored = json.loads(json.dumps(town))
                before = copy.deepcopy(restored)
                server.ensure_resources(restored)
                self.assertEqual(restored, before)

    def test_legacy_base_slots_and_catalog(self):
        town = flat_town()
        town.pop('worker_capacity_version')
        for kind in CAPACITIES:
            town['buildings'].append({'kind': kind, 'x': 7, 'y': 5})
        server.ensure_resources(town)
        for building in town['buildings'][1:]:
            expected = {'farm': 4, 'mill': 1, 'mine': 2, 'smelter': 2}.get(building['kind'], 1)
            self.assertEqual(server.settler_capacity(building), expected)
        self.assertEqual(economy.public_catalog()['worker_capacities'], {**CAPACITIES, 'acre': (1,)})
        self.assertEqual(economy.public_catalog()['buildings']['smelter']['label'], 'Forgery')
