import unittest
from unittest.mock import patch

import logistics
import progression as economy
import server


class StorageMovement(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Storage')
        self.town.update(terrain=['g' * 16] * 16, map_size=16, map_resources=[],
                         npcs=[], forage_version=1, forage_halls=[[5, 5]])
        economy.ensure_inventory(self.town)
        self.hall = self.town['buildings'][0]
        self.hall['stored_goods'] = {'wood': 100, 'food': 100}
        self.town.update(wood=100, food=100)
        self.camp = {'kind': 'woodcutter', 'x': 3, 'y': 3}
        self.tree = {'kind': 'tree', 'x': 3, 'y': 1}
        self.town['buildings'].append(self.camp)
        self.town['map_resources'].append(self.tree)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='woodcutting', building_index=1, talents={})
        self.world = {'towns': {'own': self.town}}

    def test_full_central_storage_allows_production_into_local_buffer(self):
        self.camp['output_buffer'] = {'wood': 30}
        self.town['wood'] += 30
        server.tick(self.world)
        self.assertEqual(self.worker['production_status'], 'Working')
        self.assertIn('work_animation', self.worker)
        self.assertEqual(self.worker['pending_output'], {'wood': .75})
        self.assertGreater(self.worker['woodcutting'], 0)

    def test_full_local_buffer_stops_production_until_space_is_freed(self):
        self.camp['output_buffer'] = {'wood': 50}
        self.town['wood'] += 50
        server.tick(self.world)
        self.assertIn('Workplace buffer full', self.worker['production_status'])
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.worker['woodcutting'], 0)
        economy.withdraw(self.town, 'wood', 1, source=self.camp)
        economy.add_stock(self.town, 'wood', -1)
        server.tick(self.world)
        self.assertEqual(self.worker['production_status'], 'Working')
        self.assertGreater(self.worker['woodcutting'], 0)

    def test_freeing_central_storage_allows_full_buffer_to_be_hauled(self):
        self.camp['output_buffer'] = {'wood': 50}
        self.town['wood'] += 50
        server.tick(self.world)
        self.assertNotIn('logistics_trip', self.worker)
        economy.withdraw(self.town, 'wood', 10, source=self.hall)
        economy.add_stock(self.town, 'wood', -10)
        server.tick(self.world)
        self.assertIn('logistics_trip', self.worker)
        for _ in range(30):
            server.tick(self.world)
            if self.hall['stored_goods']['wood'] > 90:
                break
        self.assertEqual(self.hall['stored_goods']['wood'], 100)
        self.assertLess(self.camp['output_buffer']['wood'], 50)

    def test_pending_goods_finish_delivery_with_central_storage_full(self):
        self.camp['output_buffer'] = {'wood': 49.5}
        self.town['wood'] += 49.5
        server.tick(self.world)
        self.assertEqual(self.worker['pending_output'], {'wood': .5})
        for _ in range(30):
            server.tick(self.world)
            if not self.worker.get('pending_output'):
                break
        self.assertFalse(self.worker.get('pending_output'))
        self.assertEqual(self.camp['output_buffer']['wood'], 50)
        self.assertIn('Workplace buffer full', self.worker['production_status'])

    def test_hauler_keeps_round_trip_moving_while_producer_waits(self):
        store = {'kind': 'storehouse', 'x': 8, 'y': 3,
                 'storage_allocations': {'wood': 100}}
        self.town['buildings'].append(store)
        self.camp['output_buffer'] = {'wood': 5}
        hauler = self.town['villagers'][1]
        hauler.update(job='hauling', building_index=2, talents={})
        server.tick(self.world)
        self.assertIn('logistics_trip', hauler)
        for _ in range(10):
            server.tick(self.world)
            if store.get('stored_goods', {}).get('wood', 0):
                break
        self.assertGreater(store['stored_goods']['wood'], 0)
        self.assertGreater(hauler['hauling'], 0)


class AllWorkplaceStorage(unittest.TestCase):
    cases = [('woodcutter', 'wood'), ('fishing_hut', 'fish'), ('orchard', 'fruit')]
    cases += [(kind, recipe[1]) for kind, recipe in economy.RECIPES.items()]
    cases += [(kind, crop) for kind in ('farm', 'acre') for crop in economy.CROPS]
    cases += [('barn', spec['output']) for animal, spec in economy.ANIMALS.items() if animal not in {'pig', 'horse'}]

    def workplace(self, kind, output, stored):
        town = server.new_town('All storage')
        town.update(terrain=['g' * 16] * 16, map_size=16, map_resources=[],
                    npcs=[], forage_version=1, forage_halls=[[5, 5]])
        economy.ensure_inventory(town)
        town['buildings'][0]['stored_goods'] = {'wood': 100, 'food': 100}
        town.update(wood=100, food=100)
        store = {'kind': 'storehouse', 'x': 8, 'y': 3,
                 'storage_enabled': {key: True for key in economy.STORAGE_KEYS},
                 'stored_goods': {key: 50 for key in economy.STORAGE_KEYS}}
        town['buildings'].append(store)
        for key, amount in store['stored_goods'].items():
            economy.add_stock(town, key, amount)
        building = {'kind': 'farm' if kind == 'acre' else kind, 'x': 3, 'y': 3,
                    'output_buffer': {output: stored}}
        town['buildings'].append(building)
        economy.add_stock(town, output, stored)
        worker = town['villagers'][0]
        job = server.BUILDING_JOBS[building['kind']]
        worker.update(job=job, building_index=2, talents={})
        worker.update(economy.JOB_REQUIREMENTS.get(job, {}))
        if kind == 'woodcutter':
            town['map_resources'].append({'kind': 'tree', 'x': 3, 'y': 1})
        elif kind == 'fishing_hut':
            town['map_resources'].append({'kind': 'water', 'x': 3, 'y': 1})
        elif kind in ('farm', 'acre'):
            building['crop'] = output
            if kind == 'acre':
                town['buildings'].append({'kind': 'acre', 'x': 4, 'y': 3,
                                          'farm_index': 2, 'crop': output,
                                          'stored_goods': {'vegetables': 20, 'wheat': 10,
                                                           'fodder': 10, 'manure': 10}})
                for key, amount in town['buildings'][-1]['stored_goods'].items():
                    economy.add_stock(town, key, amount)
                worker['building_index'] = 3
        elif kind in economy.RECIPES:
            if kind == 'mine':
                town['mineral_version'] = 1
                town['map_resources'].append({'kind': 'rock', 'ore': 'iron', 'x': 3, 'y': 1})
            building['input_buffer'] = {key: amount * 10
                                        for key, amount in economy.RECIPES[kind][0].items()}
        elif kind == 'barn':
            animal = next(key for key, spec in economy.ANIMALS.items() if spec['output'] == output)
            building.update(animals={animal: 1}, input_buffer={'fodder': 5})
        return {'towns': {'own': town}}, town, building, worker

    @patch('server.secrets.randbelow', return_value=1)
    def test_every_workplace_produces_with_central_storage_full(self, _random):
        for kind, output in self.cases:
            with self.subTest(kind=kind, output=output):
                limit = economy.local_room({'kind': 'farm' if kind == 'acre' else kind}, output)
                world, town, building, worker = self.workplace(kind, output, limit / 2)
                for storage in town['buildings'][:2]:
                    self.assertEqual(economy.local_room(storage, output), 0)
                for _ in range(economy.production_seconds(building) // 3):
                    server.tick(world)
                self.assertEqual(worker['production_status'], 'Working')
                self.assertGreater(worker.get('pending_output', {}).get(output, 0), 0)
                self.assertGreater(worker.get(worker['job'], 0), 0)
                self.assertLessEqual(logistics.buffered(building), 50)
                self.assertLessEqual(logistics.production_stock(building, output)
                                     + building.get('pending_output', {}).get(output, 0), limit + 1e-9)

    @patch('server.secrets.randbelow', return_value=1)
    def test_every_workplace_stops_at_full_buffer_and_resumes_after_spending(self, _random):
        for kind, output in self.cases:
            with self.subTest(kind=kind, output=output):
                limit = economy.local_room({'kind': 'farm' if kind == 'acre' else kind}, output)
                world, town, building, worker = self.workplace(kind, output, limit)
                practice = worker.get(worker['job'], 0)
                inputs = dict(building.get('input_buffer', {}))
                server.tick(world)
                self.assertIn('Workplace buffer full', worker['production_status'])
                self.assertNotIn('pending_output', worker)
                self.assertEqual(worker.get(worker['job'], 0), practice)
                self.assertEqual(building.get('input_buffer', {}), inputs)
                self.assertLessEqual(logistics.buffered(building), 50)
                economy.withdraw(town, output, limit / 2, source=building)
                economy.add_stock(town, output, -limit / 2)
                for _ in range(economy.production_seconds(building) // 3):
                    server.tick(world)
                self.assertEqual(worker['production_status'], 'Working')
                self.assertGreater(worker.get('pending_output', {}).get(output, 0), 0)
                self.assertGreater(worker.get(worker['job'], 0), practice)

    def test_gathering_uses_only_remaining_resource_allowance(self):
        world, town, building, worker = self.workplace('farm', 'wheat', 9.75)
        server.tick(world)
        self.assertEqual(worker['pending_output'], {'wheat': .25})
        practice = worker['farming']
        server.tick(world)
        self.assertIn('Workplace buffer full', worker['production_status'])
        self.assertEqual(worker['farming'], practice)
        self.assertEqual(logistics.production_stock(building, 'wheat')
                         + building.get('pending_output', {}).get('wheat', 0), 10)


if __name__ == '__main__':
    unittest.main()
