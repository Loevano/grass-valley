import copy
import json
import unittest

import logistics
import progression as economy
import server


class ResourceLogistics(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Logistics')
        self.town.update(terrain=['g' * 120] * 120, npcs=[], map_resources=[], level=5,
                         forage_version=1, forage_halls=[[5, 5]])
        for v in self.town['villagers']:
            v['talents'] = {}
        self.world = {'towns': {'a': self.town}}

    def add(self, kind, x=3, y=2, **extra):
        b = {'kind': kind, 'x': x, 'y': y, **extra}
        self.town['buildings'].append(b)
        return b

    def assign(self, worker, b):
        job = server.BUILDING_JOBS.get(b['kind'], 'idle')
        self.town['villagers'][worker].update(economy.JOB_REQUIREMENTS.get(job, {}))
        server.assign_settler(self.town, worker, self.town['buildings'].index(b))
        return self.town['villagers'][worker]

    def ticks(self, n=1):
        for _ in range(n):
            server.tick(self.world)

    def test_outputs_are_local_and_full_buffer_stops_practice_without_room(self):
        b = self.add('woodcutter')
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        v = self.assign(0, b)
        self.ticks()
        self.assertEqual(b['output_buffer']['wood'], .5)
        self.assertEqual(self.town['wood'], 65.5)
        b['output_buffer']['wood'] = 50
        self.town['buildings'][0]['stored_goods']['wood'] = 100
        self.town['wood'] = 150
        skill = v['woodcutting']
        self.ticks()
        self.assertEqual(b['output_buffer']['wood'], 50)
        self.assertEqual(v['woodcutting'], skill)
        self.assertIn('buffer full', v['production_status'])

    def test_camp_wood_can_pay_for_construction_before_hauling(self):
        camp = self.add('woodcutter')
        self.town['wood'] = 0
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        self.assign(0, camp)
        self.ticks()
        logistics.record_output(self.town, camp, 'wood', 19.5)
        server.command(self.town, {'action': 'build', 'kind': 'home', 'x': 4, 'y': 4})
        self.assertEqual(self.town['wood'], 0)
        self.assertEqual(camp['output_buffer']['wood'], 0)

    def test_migration_and_hauling_preserve_inventory_totals(self):
        camp = self.add('woodcutter', output_buffer={'wood': 7})
        store = self.add('storehouse', storage_allocations={'wood': 100})
        economy.ensure_inventory(self.town)
        self.assertEqual(self.town['wood'], 72)
        economy.ensure_inventory(self.town)
        logistics.transfer(self.town, camp, store, 'collect', self.town['villagers'][0])
        self.assertEqual(self.town['wood'], 72)
        self.assertEqual(store['stored_goods']['wood'], 7)
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['wood'], 72)
        economy.spend(restored, {'wood': 70})
        self.assertEqual(restored['wood'], 2)
        self.assertEqual(sum(economy.inventory(b).get('wood', 0) for b in restored['buildings']), 2)

    def test_workplace_allocations_and_deliveries_only_accept_relevant_goods(self):
        camp = self.add('woodcutter', storage_allocations={'wood': 50, 'tools': 50})
        source = self.add('storehouse', stored_goods={'wood': 10, 'tools': 5})
        economy.ensure_inventory(self.town)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'relevant'):
            server.command(self.town, {'action': 'allocate_storage', 'index': 1,
                                      'allocations': {'tools': 10}})
        self.assertEqual(self.town, before)
        self.assertEqual(economy.local_room(camp, 'tools'), 0)
        self.assertEqual(economy.storage_limit(self.town, 'tools'), 0)
        logistics.transfer(self.town, source, camp, 'collect', self.town['villagers'][0])
        self.assertEqual(camp['stored_goods'], {'wood': 10})
        self.assertEqual(source['stored_goods']['tools'], 5)
        server.command(self.town, {'action': 'allocate_storage', 'index': 1, 'allocations': {'wood': 50}})
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'allocate_storage', 'index': 1, 'allocations': {'wood': 9}})

    def test_fishing_food_space_only_accepts_fish(self):
        hut = self.add('fishing_hut', storage_allocations={'food': 50})
        source = self.add('storehouse', stored_goods={'fish': 5, 'bread': 4, 'food': 3})
        self.assertEqual(economy.local_room(hut, 'bread'), 0)
        self.assertEqual(economy.local_room(hut, 'food'), 0)
        self.assertEqual(economy.storage_limit(self.town, 'fish'), 150)
        self.assertEqual(economy.storage_limit(self.town, 'bread'), 100)
        logistics.transfer(self.town, source, hut, 'collect', self.town['villagers'][0])
        self.assertEqual(hut['stored_goods'], {'fish': 5})
        self.assertEqual(source['stored_goods'], {'fish': 0, 'bread': 4, 'food': 3})

    def test_home_has_no_storage_and_food_is_consumed_from_workplaces(self):
        home = self.add('home', storage_allocations={'wood': 25, 'food': 25})
        hut = self.add('fishing_hut', stored_goods={'fish': 5})
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['food'] = 0
        self.town.update(food=5, goods={'fish': 5})
        self.assertEqual(economy.storage_size(home), 0)
        self.assertEqual(economy.storage_allocations(home), {})
        self.assertEqual(economy.local_room(home, 'fish'), 0)
        for key in ('wood', 'food'):
            with self.assertRaisesRegex(ValueError, 'relevant'):
                server.command(self.town, {'action': 'allocate_storage', 'index': 1, 'allocations': {key: 10}})
        economy.eat(self.town, 2)
        self.assertEqual(hut['stored_goods']['fish'], 3)
        self.assertEqual(self.town['food'], 3)

    def test_legacy_unrelated_goods_survive_reload_and_can_be_hauled_away(self):
        camp = self.add('woodcutter', stored_goods={'wool': 5}, storage_allocations={'wool': 50})
        store = self.add('storehouse', storage_allocations={'wool': 100})
        economy.ensure_inventory(self.town)
        server.command(self.town, {'action': 'allocate_storage', 'index': 1, 'allocations': {'wood': 50}})
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['buildings'][1]['stored_goods']['wool'], 5)
        logistics.transfer(self.town, camp, store, 'collect', self.town['villagers'][0])
        self.assertEqual(store['stored_goods']['wool'], 5)
        self.assertEqual(camp['stored_goods']['wool'], 0)

    def test_processor_and_farming_storage_accepts_inputs_and_outputs(self):
        for kind, (inputs, output, _) in economy.RECIPES.items():
            building = {'kind': kind, 'storage_allocations': {k: 10 for k in economy.STORAGE_KEYS}}
            with self.subTest(kind=kind):
                for key in (*inputs, output):
                    self.assertGreater(economy.local_room(building, key), 0)
                self.assertEqual(economy.local_room(building, 'herbs'), 0)
        for kind in ('farm', 'acre'):
            building = {'kind': kind, 'storage_allocations': {'food': 10, 'wheat': 10, 'fodder': 10, 'manure': 10}}
            for key in (*economy.CROPS, 'manure'):
                self.assertGreater(economy.local_room(building, key), 0)
        for kind in ('hall', 'storehouse', 'market'):
            building = {'kind': kind, 'storage_allocations': {'herbs': 10, 'food': 10}}
            self.assertGreater(economy.local_room(building, 'herbs'), 0)
            self.assertGreater(economy.local_room(building, 'bread'), 0)

    def test_supply_comes_from_the_building_visited(self):
        store = self.add('storehouse', storage_allocations={'wood': 100})
        saw = self.add('sawpit')
        worker = self.assign(0, saw)
        economy.ensure_inventory(self.town)
        logistics.transfer(self.town, saw, store, 'supply', worker)
        self.assertEqual(self.town['wood'], 65)
        self.assertNotIn('input_buffer', saw)
        logistics.transfer(self.town, saw, self.town['buildings'][0], 'supply', worker)
        self.assertEqual(saw['input_buffer']['wood'], 50)
        self.assertEqual(self.town['wood'], 15)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['wood'], 15)

    def test_spending_output_releases_a_full_processor(self):
        saw = self.add('sawpit', input_buffer={'wood': 10})
        worker = self.assign(0, saw)
        logistics.record_output(self.town, saw, 'planks', 49.5)
        self.assertEqual(logistics.produce(self.town, saw, worker), 'Workplace buffer full')
        economy.spend(self.town, {'planks': 5})
        self.assertNotIn('buffer_full', saw)
        self.assertEqual(logistics.produce(self.town, saw, worker), 'Working')
        self.assertEqual(saw['input_buffer']['wood'], 8)

    def test_producer_delivers_and_returns_without_counting_output_twice(self):
        b = self.add('woodcutter', output_buffer={'wood': 50})
        self.town['wood'] = 0
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        v = self.assign(0, b)
        self.town.update(harvested=50, produced={'wood': 50})
        self.ticks(3)
        self.assertEqual(self.town['wood'], 50 + logistics.buffered(b))
        self.assertLess(logistics.buffered(b), 50)
        self.assertEqual(self.town['harvested'], 50 + logistics.buffered(b))
        self.assertEqual(self.town['produced']['wood'], self.town['harvested'])
        self.assertNotIn('logistics_trip', v)

    def test_storage_worker_collects_small_batches_while_producer_works(self):
        store = self.add('storehouse', x=1, storage_allocations={'wood': 100})
        b = self.add('woodcutter')
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        producer = self.assign(0, b)
        hauler = self.assign(1, store)
        self.assertEqual(hauler['job'], 'hauling')
        self.ticks(6)
        self.assertEqual(producer['woodcutting'], .6)
        self.assertGreater(self.town['wood'], 65)
        self.assertAlmostEqual(self.town['wood'] - 65, 3)
        self.assertGreater(store['stored_goods']['wood'], 0)

    def test_farmer_delivers_through_surrounding_completed_acres(self):
        farm = self.add('farm', x=8, y=8, output_buffer={'vegetables': 50})
        self.town['map_resources'] = [{'kind': 'water', 'x': 8, 'y': 9}]
        for x, y in [(7, 8), (9, 8), (8, 7)]:
            self.add('acre', x=x, y=y, farm_index=self.town['buildings'].index(farm))
        farmer = self.assign(0, farm)
        self.town['food'] = 0
        self.ticks(10)
        self.assertGreater(self.town['food'], 0)
        self.assertLess(logistics.buffered(farm), 50)
        self.assertGreater(farmer['farming'], 0)

    def test_unfinished_acres_still_block_deliveries(self):
        farm = self.add('farm', x=8, y=8, output_buffer={'vegetables': 50})
        self.town['map_resources'] = [{'kind': 'water', 'x': 8, 'y': 9}]
        for x, y in [(7, 8), (9, 8), (8, 7)]:
            self.add('acre', x=x, y=y, construction={'total': 30, 'progress': 0})
        farmer = self.assign(0, farm)
        self.assertFalse(logistics.start_trip(self.town, self.town, farmer, farm,
                         self.town['buildings'][0], 'collect', server.construction_route))

    def test_hauler_supplies_sawyer_and_collects_planks(self):
        store = self.add('storehouse', x=1, storage_allocations={'wood': 50, 'planks': 50})
        saw = self.add('sawpit')
        v = self.assign(0, saw)
        self.assign(1, store)
        self.town['wood'] = 20
        self.ticks(8)
        self.assertGreater(v['woodworking'], 0)
        planks = self.town.get('goods', {}).get('planks', 0)
        remaining = self.town['wood'] + saw.get('input_buffer', {}).get('wood', 0)
        self.assertAlmostEqual(remaining + planks * 2, 20)
        self.assertGreater(self.town.get('goods', {}).get('planks', 0), 0)

    def test_food_and_wool_share_the_same_collection_rules(self):
        store = self.add('storehouse', x=1, storage_allocations={'food': 50, 'wool': 50})
        barn = self.add('barn', output_buffer={'wool': 3, 'eggs': 4})
        self.assign(0, store)
        self.ticks(8)
        self.assertEqual(self.town['goods']['wool'], 3)
        self.assertEqual(barn['output_buffer']['eggs'], 0)
        self.assertEqual(self.town.get('produced', {}).get('wool', 0), 0)
        self.assertAlmostEqual(self.town['food'], 44 - 8 * .78)

    def test_allocations_share_one_budget_and_cannot_discard_stored_goods(self):
        store = self.add('storehouse')
        index = self.town['buildings'].index(store)
        server.command(self.town, {'action': 'allocate_storage', 'index': index,
                                  'allocations': {'wood': 30, 'planks': 40, 'wool': 30}})
        self.assertEqual(economy.storage_limit(self.town, 'planks'), 40)
        for allocations in ({'wood': 101}, {'wool': -1}, {'wood': True}, {'wood': 1.5}, {'coins': 10}):
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                server.command(self.town, {'action': 'allocate_storage', 'index': index, 'allocations': allocations})
            self.assertEqual(self.town, before)
        self.town['goods'] = {'planks': 20}
        store['stored_goods'] = {'planks': 20}
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'allocate_storage', 'index': index, 'allocations': {'wood': 100}})

    def test_upgrade_reaches_warehouse_level_five_and_leaves_new_spaces_free(self):
        store = self.add('storehouse', storage_allocations={'wood': 100})
        self.town['wood'] = 500
        index = self.town['buildings'].index(store)
        for level in range(2, 6):
            server.command(self.town, {'action': 'upgrade_building', 'index': index})
            for _ in range(100):
                if 'construction' not in store:
                    break
                server.construct(self.town, self.town)
            self.assertEqual(store['level'], level)
        self.assertEqual(economy.storage_size(store), 500)
        self.assertEqual(store['storage_allocations'], {'wood': 100})
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'upgrade_building', 'index': index})

    def test_unreachable_storage_preserves_buffer_and_practice(self):
        b = self.add('woodcutter', output_buffer={'wood': 50})
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        hall = self.town['buildings'][0]
        self.town['map_resources'] += [{'kind': 'rock', 'x': hall['x'] + dx, 'y': hall['y'] + dy}
                                      for dx, dy in [(1, 0), (-1, 0), (0, 1), (0, -1)]]
        v = self.assign(0, b)
        self.ticks()
        self.assertEqual(self.town['wood'], 115)
        self.assertEqual(b['output_buffer']['wood'], 50)
        self.assertEqual(v['woodcutting'], 0)
        self.assertNotIn('logistics_trip', v)

    def test_trip_survives_save_and_cancels_safely_when_building_moves(self):
        b = self.add('woodcutter', x=20, y=20, output_buffer={'wood': 50})
        self.town['map_resources'] = [{'kind': 'tree', 'x': 20, 'y': 19}]
        v = self.assign(0, b)
        self.town['wood'] = 0
        self.ticks()
        self.assertIn('logistics_trip', v)
        saved = json.loads(json.dumps(self.world))
        self.assertEqual(saved['towns']['a']['villagers'][0]['logistics_trip'], v['logistics_trip'])
        for _ in range(18):
            server.tick(saved)
        self.assertEqual(saved['towns']['a']['wood'], 50 + logistics.buffered(saved['towns']['a']['buildings'][-1]))
        b['x'] = 21
        self.ticks()
        self.assertEqual(b['output_buffer']['wood'], 50)
        self.assertEqual(self.town['wood'], 50)
        self.assertNotIn('logistics_trip', v)

    def test_recipe_can_receive_ingredients_from_different_storage_allocations(self):
        a = self.add('storehouse', x=1, storage_allocations={'wood': 100})
        b = self.add('storehouse', x=1, y=3, storage_allocations={'planks': 100})
        workshop = self.add('workshop')
        self.town.update(wood=10, goods={'planks': 10})
        producer = self.assign(0, workshop)
        self.assign(1, a)
        self.assign(2, b)
        self.ticks(6)
        self.assertGreater(producer['crafting'], 0)
        self.assertLessEqual(sum(workshop['input_buffer'].values()), 50)
        self.assertLessEqual(logistics.buffered(workshop), 50)

    def test_two_haulers_cannot_collect_the_same_goods_twice(self):
        a = self.add('storehouse', x=1, storage_allocations={'wool': 100})
        b = self.add('storehouse', x=1, y=3, storage_allocations={'wool': 100})
        barn = self.add('barn', output_buffer={'wool': 10})
        self.assign(0, a)
        self.assign(1, b)
        self.ticks(4)
        self.assertEqual(self.town['goods']['wool'], 10)
        self.assertEqual(barn['output_buffer']['wool'], 0)

    def test_partial_recipe_batch_triggers_delivery_without_losing_inputs(self):
        store = self.add('storehouse', x=1, storage_allocations={'planks': 100})
        saw = self.add('sawpit', input_buffer={'wood': 10}, output_buffer={'planks': 49.5})
        v = self.assign(0, saw)
        self.ticks()
        self.assertTrue(saw['buffer_full'])
        self.assertEqual(saw['input_buffer']['wood'], 10)
        self.assertEqual(v.get('woodworking', 0), 0)
        self.ticks(3)
        self.assertEqual(store['stored_goods']['planks'], 49.5)
        self.assertEqual(self.town['goods']['planks'], 49.5 + logistics.buffered(saw))
        self.assertNotIn('buffer_full', saw)

    def test_food_uses_one_shared_allocation_and_preserves_undelivered_goods(self):
        self.town['food'] = 99
        barn = self.add('barn', output_buffer={'bread': 2, 'eggs': 4})
        logistics.transfer(self.town, barn, self.town['buildings'][0], 'collect', self.town['villagers'][0])
        self.assertEqual(self.town['food'], 106)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['food'], 99)
        self.assertAlmostEqual(barn['output_buffer']['bread'], 2 - 1 / 1.5)
        self.assertEqual(barn['output_buffer']['eggs'], 4)


if __name__ == '__main__':
    unittest.main()
