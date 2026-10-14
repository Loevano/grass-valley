import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class GrainChain(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Grain chain', size=32)
        self.town.update(level=4, sandbox=True)
        for worker in self.town['villagers']:
            worker.update(talents={}, farming=0)

    def add(self, kind, x=7, y=5, **extra):
        building = dict(kind=kind, x=x, y=y, **extra)
        self.town['buildings'].append(building)
        return building, len(self.town['buildings']) - 1

    def farms(self):
        self.add('well', x=5, y=8)
        for worker, x in zip(self.town['villagers'], (3, 8)):
            _, index = self.add('farm', x=x, y=8, crop='wheat')
            for field_x in (x, x + 1):
                self.add('acre', x=field_x, y=9, farm_index=index, crop='wheat')
            worker.update(job='farming', building_index=index,
                          construction_position={'x': x, 'y': 9})

    def advance(self, ticks):
        for _ in range(ticks):
            server.tick({'towns': {'grain': self.town}})

    def test_two_farms_with_two_acres_each_supply_one_base_miller(self):
        self.farms()
        self.advance(20)  # Allow the first field round to start delivering.
        before = self.town.get('produced', {}).get('wheat', 0)
        self.advance(100)
        self.assertGreaterEqual(self.town['produced']['wheat'] - before, 5 * 40)
        for worker in self.town['villagers'][:2]:
            self.assertLess(worker['farming'], 20)

    def test_one_miller_matches_two_bread_bakers_per_active_minute(self):
        mill, _ = self.add('mill', input_buffer={'wheat': 40})
        miller = dict(job='milling', farming=10, talents={})
        bakeries = [self.add('bakery', x=x, input_buffer={'flour': 20, 'wood': 5})[0]
                    for x in (9, 11)]
        bakers = [dict(job='baking', milling=2, talents={}) for _ in bakeries]
        for _ in range(20):
            self.assertEqual(logistics.produce(self.town, mill, miller), 'Working')
            for bakery, baker in zip(bakeries, bakers):
                logistics.produce(self.town, bakery, baker)
        self.assertEqual(mill['input_buffer']['wheat'], 0)
        self.assertEqual(miller['pending_output'], {'flour': 40})
        for bakery, baker in zip(bakeries, bakers):
            self.assertEqual(bakery['input_buffer'], {'flour': 0, 'wood': 0})
            self.assertEqual(baker['pending_output'], {'bread': 20})
            self.assertAlmostEqual(baker['baking'], .8)
        self.assertAlmostEqual(miller['milling'], .8)

    def test_chain_delivers_bread_from_fields_with_normal_worker_hauling(self):
        self.farms()
        # Fuel and output storage isolate the grain chain from forestry and sales.
        self.town['wood'] = 200
        self.add('storehouse', x=6, y=14, level=8, storage_enabled={'bread': True})
        for worker, kind, x, y in zip(self.town['villagers'][2:],
                                     ('mill', 'bakery', 'bakery'), (5, 4, 7), (11, 13, 13)):
            _, index = self.add(kind, x=x, y=y)
            job = economy.BUILDINGS[kind]['job']
            worker.update(job=job, building_index=index, **{job: 0},
                          **economy.JOB_REQUIREMENTS[job])
        self.advance(20)
        before = dict(self.town.get('produced', {}))
        practice_before = [baker.get('baking', 0) for baker in self.town['villagers'][3:]]
        self.advance(100)
        # Travel reduces throughput; both bakers should sustain at least 80%.
        produced = self.town['produced']
        self.assertGreaterEqual(produced['wheat'] - before.get('wheat', 0), 200)
        self.assertGreaterEqual(produced['flour'] - before.get('flour', 0), 160)
        self.assertGreaterEqual(produced['bread'] - before.get('bread', 0), 160)
        for baker, practice in zip(self.town['villagers'][3:], practice_before):
            self.assertGreaterEqual(baker['baking'] - practice, 5 * .8 * .8)
        self.assertGreater(self.town['goods']['bread'], 0)

    def test_storage_upgrades_grow_buffers_without_changing_workers_or_batches(self):
        for kind, base, allocations in (
            ('mill', 160, {'wheat': 80, 'flour': 80}),
            ('bakery', 100, {'flour': 40, 'wood': 10, 'bread': 40}),
        ):
            building, index = self.add(kind)
            for level in (1, 2, 3):
                with self.subTest(kind=kind, level=level):
                    scale = 1 + .25 * (level - 1)
                    self.assertEqual(economy.storage_size(building), base * scale)
                    self.assertEqual(logistics.buffer_size(building), base * scale)
                    for key, amount in allocations.items():
                        self.assertEqual(economy.storage_allocations(building)[key], amount * scale)
                    self.assertEqual(server.settler_capacity(building), 1)
                    self.assertEqual(server.production_bonus(building), 0)
                    if level < 3:
                        server.command(self.town, dict(action='upgrade_building', index=index))
            with self.assertRaisesRegex(ValueError, 'fully upgraded'):
                server.command(self.town, dict(action='upgrade_building', index=index))
            if kind == 'mill':
                building['input_buffer'] = {'wheat': 2}
                worker = dict(job='milling', farming=10, talents={})
            else:
                building['input_buffer'] = {'flour': 2, 'wood': .5}
                worker = dict(job='baking', milling=2, talents={})
            self.assertEqual(logistics.produce(self.town, building, worker,
                                               economy.production_seconds(building)), 'Working')
            self.assertEqual(sum(worker['pending_output'].values()), 2)

    def test_pending_output_reserves_the_new_allowance_and_upgrade_resumes_work(self):
        mill, index = self.add('mill', input_buffer={'wheat': 8},
                               stored_goods={'wheat': 80}, output_buffer={'flour': 78})
        worker = dict(job='milling', farming=10, talents={})
        self.assertEqual(logistics.produce(self.town, mill, worker), 'Working')
        before = copy.deepcopy(worker)
        self.assertEqual(logistics.produce(self.town, mill, worker), 'Workplace buffer full')
        self.assertEqual(worker, before)
        self.assertEqual(mill['input_buffer'], {'wheat': 6})
        server.command(self.town, dict(action='upgrade_building', index=index))
        self.assertEqual(logistics.produce(self.town, mill, worker), 'Working')
        self.assertEqual(worker['pending_output'], {'flour': 4})
        self.assertEqual(mill['input_buffer'], {'wheat': 4})

    def test_saved_stock_assignments_and_partial_batches_survive_new_storage_rules(self):
        bakery, index = self.add('bakery', level=2, input_buffer={'flour': 50, 'wood': 5},
                                 output_buffer={'bread': 60})
        worker = self.town['villagers'][0]
        worker.update(job='baking', building_index=index, milling=2)
        economy.ensure_inventory(self.town)
        worker['production_work'] = {'task': ['baking', 'bakery', 7, 5, 'bread'], 'elapsed': 3}
        restored = json.loads(json.dumps(self.town))
        before = copy.deepcopy(restored)
        economy.ensure_inventory(restored)
        self.assertEqual(restored, before)
        building = restored['buildings'][index]
        baker = restored['villagers'][0]
        self.assertEqual(logistics.produce(restored, building, baker), 'Workplace buffer full')
        economy.spend(restored, {'bread': 14})
        self.assertEqual(logistics.produce(restored, building, baker), 'Working')
        self.assertEqual(building['input_buffer'], {'flour': 48, 'wood': 4.5})
        self.assertEqual(baker['pending_output'], {'bread': 2})


if __name__ == '__main__':
    unittest.main()
