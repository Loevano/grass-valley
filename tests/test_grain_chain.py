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

    def test_chain_delivers_bread_with_producers_fetching_inputs_and_hauler_collecting_outputs(self):
        self.farms()
        # Fuel and output storage isolate the grain chain from forestry and sales.
        self.town['wood'] = 200
        _, store_index = self.add('storehouse', x=6, y=14, level=8, storage_enabled={'bread': True})
        for worker, kind, x, y in zip(self.town['villagers'][2:],
                                     ('mill', 'bakery', 'bakery'), (5, 4, 7), (11, 13, 13)):
            _, index = self.add(kind, x=x, y=y)
            job = economy.BUILDINGS[kind]['job']
            worker.update(job=job, building_index=index, **{job: 0},
                          **economy.JOB_REQUIREMENTS[job])
        bakers = self.town['villagers'][3:5]
        self.town['villagers'].append(dict(name='Hauler', job='hauling', building_index=store_index,
                                          talents={}, hauling=0))
        self.advance(20)
        before = dict(self.town.get('produced', {}))
        practice_before = [baker.get('baking', 0) for baker in bakers]
        self.advance(100)
        # Travel reduces throughput; both bakers should sustain at least 80%.
        produced = self.town['produced']
        self.assertGreaterEqual(produced['wheat'] - before.get('wheat', 0), 200)
        self.assertGreaterEqual(produced['flour'] - before.get('flour', 0), 160)
        self.assertGreaterEqual(produced['bread'] - before.get('bread', 0), 160)
        for baker, practice in zip(bakers, practice_before):
            self.assertGreaterEqual(baker['baking'] - practice, 5 * .8 * .8)
        self.assertGreater(self.town['goods']['bread'], 0)
        self.assertGreater(self.town['buildings'][store_index]['stored_goods']['bread'], 0)
        for producer in self.town['villagers'][:5]:
            self.assertNotEqual(producer.get('logistics_trip', {}).get('operation'), 'collect')

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

    def test_equipment_scales_inputs_storage_and_capacity_without_extra_workers(self):
        for kind in ('mill', 'bakery'):
            building, index = self.add(kind)
            job = economy.BUILDINGS[kind]['job']
            self.town['level'] = 6
            worker = dict(job=job, talents={}, **economy.JOB_REQUIREMENTS[job])
            for key, spec in economy.PROFESSION_UPGRADES.items():
                if spec['building'] != kind or 'throughput' not in spec:
                    continue
                server.command(self.town, dict(action='profession_upgrade', index=index, upgrade=key))
                self.assertEqual(server.settler_capacity(building), 1)
                self.assertEqual(economy.grain_equipment_multiplier(building), spec['throughput'])
                self.assertEqual(logistics.buffer_size(building), economy.WORKPLACE_STORAGE_SIZES[kind] * spec['throughput'])
            inputs, output, amount = economy.production_recipe(building)
            multiplier = economy.grain_equipment_multiplier(building)
            building['input_buffer'] = {k: n * multiplier for k, n in inputs.items()}
            self.assertEqual(logistics.produce(self.town, building, worker, economy.production_seconds(building)), 'Working')
            self.assertEqual(worker['pending_output'], {output: amount * multiplier})
            self.assertTrue(all(n == 0 for n in building['input_buffer'].values()))
            self.assertEqual(logistics.carry_capacity(worker, building), 50 * multiplier)
            with self.assertRaises(ValueError):
                server.command(self.town, dict(action='upgrade_capacity', index=index))

    def test_old_mill_migration_preserves_stock_cargo_and_partial_work_once(self):
        mill, index = self.add('mill', settler_level=2, input_buffer={'wheat': 80},
                               construction={'progress': 12, 'total': 30,
                                             'upgrade': {'settler_level': 2}})
        self.town.pop('grain_chain_version')
        for worker in self.town['villagers'][:2]:
            worker.update(job='milling', building_index=index, farming=10, milling=3)
        first, second = self.town['villagers'][:2]
        first['production_work'] = {'task': ['milling', 'mill', 7, 5, 'flour'], 'elapsed': 1}
        second['production_work'] = {'task': ['milling', 'mill', 7, 5, 'flour'], 'elapsed': 2}
        logistics.record_output(self.town, mill, 'flour', 2, second)
        server.ensure_resources(self.town)
        self.assertEqual(first['production_work']['elapsed'], 1)
        self.assertEqual(second['job'], 'idle')
        self.assertEqual(second['milling'], 3)
        self.assertEqual(mill['grain_work_queue'][0]['elapsed'], 2)
        self.assertEqual(mill['orphaned_output'], {'flour': 2})
        self.assertEqual(mill['input_buffer'], {'wheat': 80})
        self.assertEqual(mill['construction']['progress'], 12)
        self.assertEqual(mill['construction']['upgrade']['settler_level'], 1)
        self.assertIn('improved_millstones', mill['profession_upgrades'])
        restored = json.loads(json.dumps(self.town))
        before = copy.deepcopy(restored)
        server.ensure_resources(restored)
        self.assertEqual(restored, before)
        mill.pop('construction')
        self.assertEqual(logistics.produce(self.town, mill, first, 2), 'Working')
        self.assertEqual(logistics.produce(self.town, mill, first, 1), 'Working')
        self.assertEqual(mill['grain_work_queue'], [])

    def test_dropoffs_make_bread_available_once_without_flow_summaries(self):
        bakery, _ = self.add('bakery')
        worker = dict(job='baking')
        logistics.record_output(self.town, bakery, 'bread', 4, worker)
        before = economy.quantity(self.town, 'bread')
        logistics.drop_off(self.town, bakery, worker)
        self.assertEqual(economy.quantity(self.town, 'bread'), before + 4)
        logistics.drop_off(self.town, bakery, worker)
        self.assertEqual(economy.quantity(self.town, 'bread'), before + 4)
        self.assertNotIn('grain_flow', bakery)
        self.assertNotIn('grain_delivery_pending', bakery)

    def test_old_flow_summaries_are_removed_without_changing_stock(self):
        bakery, _ = self.add('bakery', grain_flow={'seconds': 120, 'rates': {'bread': 80}},
                             grain_delivery_pending={'bread': 4}, stored_goods={'bread': 10})
        server.ensure_resources(self.town)
        self.assertNotIn('grain_flow', bakery)
        self.assertNotIn('grain_delivery_pending', bakery)
        self.assertEqual(bakery['stored_goods'], {'bread': 10})
        before = copy.deepcopy(self.town)
        server.ensure_resources(self.town)
        self.assertEqual(self.town, before)

    def test_200_working_residents_eat_300_food_per_minute(self):
        self.town['villagers'] = [dict(name=str(i), job='building') for i in range(200)]
        self.town['food'] = 1000
        for _ in range(20):
            server.feed_settlers(self.town)
        self.assertAlmostEqual(self.town['food'], 700)
        self.assertAlmostEqual(economy.food_demand(self.town), 300)

    def test_idle_residents_and_construction_helpers_keep_the_meal_ratio(self):
        self.town['villagers'] = [dict(job='idle'), dict(job='idle', construction_target={'x': 7, 'y': 5})]
        self.town['food'] = 10
        self.assertAlmostEqual(economy.food_demand(self.town), 2.1)
        server.feed_settlers(self.town)
        self.assertAlmostEqual(self.town['food'], 10 - .03 - .075)

    def test_equipment_requires_previous_tier_and_finishes_after_builder_work(self):
        self.town.update(level=6, sandbox=False, wood=500, goods={'planks': 100, 'tools': 50})
        bakery, index = self.add('bakery')
        self.town['villagers'][0].update(job='baking', building_index=index, milling=2, baking=5)
        with self.assertRaisesRegex(ValueError, 'previous equipment tier'):
            server.command(self.town, dict(action='profession_upgrade', index=index, upgrade='double_hearth_oven'))
        wood = self.town['wood']
        server.command(self.town, dict(action='profession_upgrade', index=index, upgrade='larger_oven'))
        self.assertEqual(self.town['wood'], wood - 30)
        self.assertEqual(economy.grain_equipment_multiplier(bakery), 1)
        self.assertEqual(bakery['construction']['total'], 30)
        for _ in range(100):
            server.construct(self.town, self.town, workers={0})
            if 'construction' not in bakery:
                break
        self.assertNotIn('construction', bakery)
        self.assertEqual(economy.grain_equipment_multiplier(bakery), 4)
        with self.assertRaisesRegex(ValueError, 'Already unlocked'):
            server.command(self.town, dict(action='profession_upgrade', index=index, upgrade='larger_oven'))

    def test_variety_bakeries_share_flour_and_require_their_own_extra_ingredients(self):
        bakeries = [self.add('bakery', x=x, recipe=recipe, profession_upgrades=['larger_oven'])[0]
                    for x, recipe in ((9, 'bread'), (11, 'herb_bread'))]
        workers = [dict(job='baking', milling=2, baking=10, talents={}) for _ in bakeries]
        for building, worker in zip(bakeries, workers):
            building['input_buffer'] = {'flour': 8, 'wood': 2}
        self.assertEqual(logistics.produce(self.town, bakeries[0], workers[0], 6), 'Working')
        before = copy.deepcopy(bakeries[1]['input_buffer'])
        self.assertIn('herbs', logistics.produce(self.town, bakeries[1], workers[1], 6))
        self.assertEqual(bakeries[1]['input_buffer'], before)
        bakeries[1]['input_buffer']['herbs'] = 4
        self.assertEqual(logistics.produce(self.town, bakeries[1], workers[1], 6), 'Working')
        self.assertEqual(workers[0]['pending_output'], {'bread': 8})
        self.assertEqual(workers[1]['pending_output'], {'herb_bread': 8})
        self.assertEqual(sum(b['input_buffer']['flour'] for b in bakeries), 0)

    def test_one_upgraded_mill_and_bakery_deliver_food_for_200_residents(self):
        self.town = flat_town('Large grain chain', size=64)
        self.town.update(level=6, sandbox=True, wood=5000, food=0)
        self.town['villagers'] = [dict(name=f'Worker {i}', job='building', talents={}) for i in range(200)]
        # Six compact farm groups: real growth, water trips, and harvest hauling.
        for group in range(6):
            y = 8 + group * 4
            self.add('well', x=5, y=y)
            for offset, x in enumerate((3, 8)):
                _, index = self.add('farm', x=x, y=y, crop='wheat')
                for field_x in (x, x + 1):
                    self.add('acre', x=field_x, y=y + 1, farm_index=index, crop='wheat')
                self.town['villagers'][group * 2 + offset].update(
                    job='farming', building_index=index, farming=0,
                    construction_position={'x': x, 'y': y + 1})
        for kind, x, y, worker_index, upgrade in (
            ('mill', 6, 15, 12, 'masterwork_millstones'),
            ('bakery', 6, 17, 13, 'masterwork_oven'),
        ):
            _, index = self.add(kind, x=x, y=y, profession_upgrades=[upgrade])
            job = economy.BUILDINGS[kind]['job']
            self.town['villagers'][worker_index].update(job=job, building_index=index,
                                                      **economy.JOB_REQUIREMENTS[job])
        # Fuel and overflow space isolate this check from forestry and sales.
        _, store = self.add('storehouse', x=6, y=14, level=8,
                            storage_enabled={'wheat': True, 'flour': True})
        for worker in self.town['villagers'][14:18]:
            worker.update(job='hauling', building_index=store, hauling=0)
        self.add('storehouse', x=7, y=17, level=8, storage_enabled={'bread': True})
        for i in range(33):
            self.add('home', x=30 + i % 11, y=8 + i // 11, level=3)
        self.advance(100)  # Startup: seed fields and fill the processors.
        food = self.town['food']
        self.advance(100)
        self.assertGreaterEqual(self.town['food'], food)
        self.assertEqual(self.town['food_shortage_seconds'], 0)
        bakeries = [b for b in self.town['buildings'] if b['kind'] == 'bakery']
        self.assertEqual(len(bakeries), 1)


if __name__ == '__main__':
    unittest.main()
