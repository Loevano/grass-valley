from fixtures import flat_town
import copy
import json
import unittest

import logistics
import progression as economy
import server


class ResourceLogistics(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Logistics', size=120)
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
        self.assertEqual(b['pending_output']['wood'], .75)
        self.assertEqual(self.town['wood'], 65)
        b.setdefault('output_buffer', {})['wood'] = 50
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
        # 19.25 available plus the new 0.75 gathered per tick pays 20 only
        # after the pending delivery is credited.
        logistics.record_output(self.town, camp, 'wood', 19.25)
        with self.assertRaisesRegex(ValueError, 'Missing building materials'):
            server.command(self.town, {'action': 'build', 'kind': 'home', 'x': 4, 'y': 4})
        logistics.drop_off(self.town, camp, self.town['villagers'][0])
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

    def test_workplaces_reject_allocations_and_deliveries_only_accept_relevant_goods(self):
        camp = self.add('woodcutter', storage_allocations={'wood': 50, 'tools': 50})
        source = self.add('storehouse', stored_goods={'wood': 10, 'tools': 5})
        economy.ensure_inventory(self.town)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'Only storehouses'):
            server.command(self.town, {'action': 'configure_storage', 'index': 1,
                                      'enabled': {'tools': True}})
        self.assertEqual(self.town, before)
        self.assertEqual(economy.local_room(camp, 'tools'), 0)
        self.assertEqual(economy.storage_limit(self.town, 'tools'), 0)
        logistics.transfer(self.town, source, camp, 'collect', self.town['villagers'][0])
        self.assertEqual(camp['stored_goods'], {'wood': 10})
        self.assertEqual(source['stored_goods']['tools'], 5)
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'configure_storage', 'index': 1, 'enabled': {'wood': True}})
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'configure_storage', 'index': 1, 'enabled': {'wood': False}})

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
            with self.assertRaisesRegex(ValueError, 'Only storehouses'):
                server.command(self.town, {'action': 'configure_storage', 'index': 1, 'enabled': {key: True}})
        economy.eat(self.town, 2)
        self.assertEqual(hut['stored_goods']['fish'], 3)
        self.assertEqual(self.town['food'], 3)

    def test_legacy_unrelated_goods_survive_reload_and_can_be_hauled_away(self):
        camp = self.add('woodcutter', stored_goods={'wool': 5}, storage_allocations={'wool': 50})
        store = self.add('storehouse', storage_allocations={'wool': 100})
        economy.ensure_inventory(self.town)
        self.assertEqual(economy.storage_allocations(camp), {'wood': 50})
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['buildings'][1]['stored_goods']['wool'], 5)
        logistics.transfer(self.town, camp, store, 'collect', self.town['villagers'][0])
        self.assertEqual(store['stored_goods']['wool'], 5)
        self.assertEqual(camp['stored_goods']['wool'], 0)

    def test_fixed_capacities_ignore_legacy_allocations_and_match_building_goods(self):
        for kind in economy.STORAGE_GOODS:
            if kind == 'storehouse':
                continue
            building = {'kind': kind, 'storage_allocations': {'herbs': 999}}
            limits = economy.storage_allocations(building)
            with self.subTest(kind=kind):
                if kind == 'workshop':
                    # Additional crafted tools have individual allowances,
                    # sharing the workshop's 50-space total inventory limit.
                    self.assertEqual(sum(limits.values()), 60)
                    building['stored_goods'] = {'planks': 15, 'wood': 5, 'wool': 5,
                                                'tools': 5, 'wooden_bowls': 5,
                                                'crates': 5, 'furniture': 5, 'carved_furniture': 5}
                    self.assertEqual(economy.local_room(building, 'crafted_axe'), 0)
                    self.assertEqual(economy.local_room(building, 'crafted_saw'), 0)
                elif not any(key in economy.EDIBLE for key in limits):
                    self.assertEqual(sum(limits.values()), economy.storage_size(building))
                self.assertLessEqual(set(limits), set(economy.building_storage_keys(building)))
                if kind != 'hall':
                    self.assertEqual(set(limits), set(economy.building_storage_keys(building)))
                before = copy.deepcopy(self.town)
                index = len(self.town['buildings'])
                self.town['buildings'].append(building)
                with self.assertRaisesRegex(ValueError, 'Only storehouses'):
                    server.command(self.town, {'action': 'configure_storage', 'index': index, 'enabled': {}})
                self.town['buildings'].pop()
                self.assertEqual(self.town, before)
        self.assertEqual(economy.storage_allocations({'kind': 'mill'}), {'wheat': 80, 'flour': 80})
        self.assertEqual(economy.public_catalog()['fixed_storage'], economy.FIXED_STORAGE)

    def test_processor_and_farming_storage_accepts_inputs_and_outputs(self):
        for kind, (inputs, output, _) in economy.RECIPES.items():
            building = {'kind': kind, 'storage_allocations': {k: 10 for k in economy.STORAGE_KEYS}}
            with self.subTest(kind=kind):
                for key in (*inputs, output):
                    self.assertGreater(economy.local_room(building, key), 0)
                if 'herbs' in economy.STORAGE_GOODS[kind]:
                    self.assertGreater(economy.local_room(building, 'herbs'), 0)
                else:
                    self.assertEqual(economy.local_room(building, 'herbs'), 0)
        for kind in ('farm', 'acre'):
            building = {'kind': kind, 'storage_allocations': {'food': 10, 'wheat': 10, 'fodder': 10, 'manure': 10}}
            for key in (*economy.CROPS, 'manure'):
                self.assertGreater(economy.local_room(building, key), 0)
        building = {'kind': 'storehouse', 'storage_allocations': {'herbs': 10, 'food': 10}}
        self.assertGreater(economy.local_room(building, 'herbs'), 0)
        self.assertGreater(economy.local_room(building, 'bread'), 0)
        market = {'kind': 'market', 'storage_allocations': {'herbs': 10, 'food': 10}}
        self.assertEqual(economy.storage_size(market), 0)
        self.assertEqual(economy.storage_allocations(market), {})
        self.assertEqual(economy.local_room(market, 'herbs'), 0)
        self.assertEqual(economy.local_room(market, 'bread'), 0)

    def test_old_market_stock_moves_to_hall_once_without_changing_totals(self):
        economy.ensure_inventory(self.town)
        market = self.add('market', stored_goods={'planks': 4, 'fish': 3}, output_buffer={'planks': 2})
        economy.add_stock(self.town, 'planks', 6)
        economy.add_stock(self.town, 'fish', 3)
        self.town.pop('market_storage_version', None)
        totals = {key: economy.quantity(self.town, key) for key in ('planks', 'fish', 'food')}
        economy.ensure_inventory(self.town)
        self.assertNotIn('stored_goods', market)
        self.assertNotIn('output_buffer', market)
        hall = self.town['buildings'][0]
        self.assertEqual(hall['stored_goods']['planks'], 6)
        self.assertEqual(hall['stored_goods']['fish'], 3)
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['buildings'][0]['stored_goods'], hall['stored_goods'])
        self.assertEqual({key: economy.quantity(restored, key) for key in totals}, totals)

    def test_disabling_market_product_preserves_cap_and_cancels_planned_sale(self):
        market = self.add('market')
        worker = self.assign(0, market)
        economy.ensure_inventory(self.town)
        hall = self.town['buildings'][0]
        hall['stored_goods']['planks'] = 20
        economy.add_stock(self.town, 'planks', 20)
        server.command(self.town, {'action': 'configure_market_sale', 'resource': 'planks',
                                  'enabled': True, 'cap': 20})
        cargo = logistics.sale_cargo(self.town, hall, worker)
        self.assertEqual(cargo, {'planks': 10})
        server.command(self.town, {'action': 'configure_market_sale', 'resource': 'planks',
                                  'enabled': False, 'cap': 20})
        restored = json.loads(json.dumps(self.town))
        self.assertEqual(restored['rules']['market_sales']['planks']['cap'], 20)
        self.assertEqual(logistics.sale_cargo(restored, restored['buildings'][0], restored['villagers'][0]), {})
        coins = self.town['coins']
        self.assertEqual(logistics.transfer(self.town, hall, market, 'sell', worker, cargo), {})
        self.assertEqual(self.town['coins'], coins)
        self.assertEqual(economy.quantity(self.town, 'planks'), 20)

    def start_plank_sale(self, adjacent=False):
        economy.ensure_inventory(self.town)
        source = self.add('storehouse', x=3, y=2, stored_goods={'planks': 20},
                          storage_enabled={'planks': True})
        market = self.add('market', x=7, y=2)
        worker = self.assign(0, market)
        economy.add_stock(self.town, 'planks', 20)
        server.command(self.town, {'action': 'configure_market_sale', 'resource': 'planks',
                                  'enabled': True, 'cap': 20})
        outward = [(3, 1)] if adjacent else [(x, 1) for x in range(7, 2, -1)]
        paths = iter([outward, [(x, 1) for x in range(3, 8)]])
        landscape = {'buildings': [], 'map_resources': []}
        logistics.start_trip(self.town, landscape, worker, source, market, 'sell',
                             lambda *_: next(paths), hauler=True)
        return source, market, worker, landscape

    def test_merchant_removes_planks_at_pickup_and_sells_only_at_market(self):
        source, _, worker, landscape = self.start_plank_sale()
        coins = self.town['coins']
        logistics.advance_trip(self.town, landscape, worker, .6)
        self.assertEqual(source['stored_goods']['planks'], 20)
        self.assertEqual(economy.quantity(self.town, 'planks'), 20)
        logistics.advance_trip(self.town, landscape, worker, 1.7)
        self.assertTrue(worker['logistics_trip']['picked_up'])
        self.assertEqual(worker['logistics_trip']['cargo'], {'planks': 10})
        self.assertEqual(source['stored_goods']['planks'], 10)
        self.assertEqual(economy.quantity(self.town, 'planks'), 10)
        self.assertEqual(self.town['coins'], coins)
        economy.spend(self.town, {'planks': 10})
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(source['stored_goods']['planks'], 0)
        self.assertEqual(economy.quantity(self.town, 'planks'), 0)
        self.assertEqual(self.town['coins'], coins + 10 * economy.sale_price('planks'))
        self.assertEqual(self.town['market_sales'], 10)
        self.assertNotIn('logistics_trip', worker)

    def test_collected_sale_cargo_survives_reload_without_another_withdrawal(self):
        _, _, worker, landscape = self.start_plank_sale()
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        worker = restored['villagers'][0]
        coins = restored['coins']
        logistics.advance_trip(restored, landscape, worker, 3)
        self.assertEqual(economy.quantity(restored, 'planks'), 10)
        self.assertEqual(restored['buildings'][1]['stored_goods']['planks'], 10)
        self.assertEqual(restored['coins'], coins + 10 * economy.sale_price('planks'))

    def test_adjacent_merchant_collects_once_before_walking(self):
        source, _, worker, landscape = self.start_plank_sale(adjacent=True)
        logistics.advance_trip(self.town, landscape, worker, 0)
        self.assertEqual(source['stored_goods']['planks'], 10)
        logistics.advance_trip(self.town, landscape, worker, 0)
        self.assertEqual(source['stored_goods']['planks'], 10)
        self.assertEqual(worker['logistics_trip']['cargo'], {'planks': 10})

    def test_old_returning_sale_trip_collects_stock_once_when_resumed(self):
        source, _, worker, landscape = self.start_plank_sale()
        worker['logistics_trip']['step'] = worker['logistics_trip']['pickup_step']
        logistics.advance_trip(self.town, landscape, worker, 0)
        self.assertEqual(source['stored_goods']['planks'], 10)
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(source['stored_goods']['planks'], 10)
        self.assertEqual(economy.quantity(self.town, 'planks'), 10)
        self.assertEqual(self.town['market_sales'], 10)

    def test_unavailable_sale_source_salvages_carried_goods_at_hall(self):
        source, _, worker, landscape = self.start_plank_sale()
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        source['x'] = 8
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(source['stored_goods']['planks'], 10)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['planks'], 10)
        self.assertEqual(economy.quantity(self.town, 'planks'), 20)
        self.assertNotIn('logistics_trip', worker)

    def test_second_merchant_cannot_collect_stock_below_trigger(self):
        source, _, worker, landscape = self.start_plank_sale()
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        other = self.town['villagers'][1]
        self.assertEqual(logistics.pickup_sale(self.town, source, other), {})
        self.assertEqual(source['stored_goods']['planks'], 10)
        self.assertEqual(economy.quantity(self.town, 'planks'), 10)

    def test_disabling_collected_sale_restores_goods_without_payment(self):
        source, _, worker, landscape = self.start_plank_sale()
        coins = self.town['coins']
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        server.command(self.town, {'action': 'configure_market_sale', 'resource': 'planks',
                                  'enabled': False, 'cap': 20})
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(source['stored_goods']['planks'], 20)
        self.assertEqual(economy.quantity(self.town, 'planks'), 20)
        self.assertEqual(self.town['coins'], coins)
        self.assertEqual(self.town.get('market_sales', 0), 0)

    def test_smaller_sale_batch_returns_excess_collected_goods(self):
        source, _, worker, landscape = self.start_plank_sale()
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        server.command(self.town, {'action': 'configure_market_sale', 'resource': 'planks',
                                  'enabled': True, 'cap': 20, 'amount': 4})
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(source['stored_goods']['planks'], 16)
        self.assertEqual(economy.quantity(self.town, 'planks'), 16)
        self.assertEqual(self.town['market_sales'], 4)

    def test_cancelled_merchant_trip_restores_cargo_once(self):
        for cancellation in ('blocked', 'remove_settler', 'destroy_source', 'destroy_market'):
            with self.subTest(cancellation=cancellation):
                self.setUp()
                source, market, worker, landscape = self.start_plank_sale()
                coins = self.town['coins']
                logistics.advance_trip(self.town, landscape, worker, 2.3)
                if cancellation == 'blocked':
                    landscape['map_resources'] = [{'kind': 'rock', 'x': 4, 'y': 1}]
                    logistics.advance_trip(self.town, landscape, worker, 3)
                elif cancellation == 'remove_settler':
                    server.command(self.town, {'action': 'remove_settler', 'index': 0})
                else:
                    target = source if cancellation == 'destroy_source' else market
                    server.command(self.town, {'action': 'destroy', 'index': self.town['buildings'].index(target)})
                logistics.cancel_trip(self.town, worker)
                self.assertEqual(economy.quantity(self.town, 'planks'), 20)
                self.assertEqual(sum(economy.inventory(b).get('planks', 0) for b in self.town['buildings']), 20)
                self.assertEqual(self.town['coins'], coins)
                self.assertNotIn('logistics_trip', worker)

    def test_collected_sale_goods_count_toward_production_cap_until_sold(self):
        _, _, worker, landscape = self.start_plank_sale()
        self.town['rules']['production_caps'] = {'planks': 20}
        logistics.advance_trip(self.town, landscape, worker, 2.3)
        self.assertEqual(economy.production_room(self.town, 'planks'), 0)
        logistics.advance_trip(self.town, landscape, worker, 3)
        self.assertEqual(economy.production_room(self.town, 'planks'), 10)

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

    def test_smokehouse_delivers_and_consumes_each_raw_fish_type(self):
        for key in economy.FISH:
            with self.subTest(fish=key):
                self.setUp()
                smoke = self.add('smokehouse', input_buffer={'wood': 10})
                source = self.add('fishing_hut', x=8, output_buffer={key: 50})
                worker = self.assign(0, smoke)
                economy.ensure_inventory(self.town)
                before = economy.quantity(self.town, key)
                self.assertEqual(logistics.planned_cargo(self.town, smoke, source, 'supply', worker), {key: 40})
                self.assertEqual(logistics.transfer(self.town, smoke, source, 'supply', worker), {key: 40})
                self.assertEqual(source['output_buffer'][key], 10)
                self.assertEqual(economy.quantity(self.town, key), before - 40)
                self.assertEqual(logistics.produce(self.town, smoke, worker, 6), 'Working')
                self.assertEqual(smoke['input_buffer'], {'wood': 9.5, key: 38})
                self.assertEqual(worker['pending_output'], {'smoked_fish': 2})
                self.assertEqual(economy.quantity(self.town, 'smoked_fish'), 0)
                logistics.drop_off(self.town, smoke, worker)
                self.assertEqual(economy.quantity(self.town, 'smoked_fish'), 2)
                self.assertGreater(economy.local_room({'kind': 'smokehouse'}, key), 0)

    def test_smokehouse_mixed_fish_reserve_wood_space_and_scale_with_talent(self):
        smoke = self.add('smokehouse', input_buffer={'fish': 1})
        source = self.add('storehouse', x=8, stored_goods={'trout': 1, 'salmon': 1, 'sturgeon': 50, 'wood': 20})
        worker = self.assign(0, smoke)
        worker['talents'] = {'fishing': 1}
        economy.ensure_inventory(self.town)
        self.assertEqual(logistics.transfer(self.town, smoke, source, 'supply', worker),
                         {'trout': 1, 'salmon': 1, 'sturgeon': 37, 'wood': 10})
        before = copy.deepcopy(smoke['input_buffer'])
        self.assertIn('3s remaining', logistics.produce(self.town, smoke, worker, 3))
        self.assertEqual(smoke['input_buffer'], before)
        self.assertEqual(logistics.produce(self.town, smoke, worker, 3), 'Working')
        self.assertEqual(smoke['input_buffer'], {'fish': 0, 'trout': 0, 'salmon': .5, 'sturgeon': 37, 'wood': 9.375})
        self.assertEqual(worker['pending_output'], {'smoked_fish': 2.5})
        # A full output buffer must preserve the next batch's raw ingredients.
        smoke['output_buffer'] = {'smoked_fish': 50}
        before = copy.deepcopy(smoke['input_buffer'])
        self.assertEqual(logistics.produce(self.town, smoke, worker, 6), 'Workplace buffer full')
        self.assertEqual(smoke['input_buffer'], before)

    def test_disabling_keeps_stock_available_for_processors_and_building(self):
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['wood'] = 0
        self.town['wood'] = 40
        store = self.add('storehouse', stored_goods={'wood': 40}, storage_allocations={'wood': 100},
                         wood_reserve=40, wood_reserve_enabled=True)
        saw = self.add('sawpit')
        worker = self.assign(0, saw)
        index = self.town['buildings'].index(store)
        server.command(self.town, {'action': 'configure_storage', 'index': index, 'enabled': {'wood': False}})
        self.assertEqual(economy.local_room(store, 'wood'), 0)
        self.assertEqual(store['stored_goods']['wood'], 40)
        self.assertNotIn('wood_reserve', store)
        restored = json.loads(json.dumps(self.town))
        self.assertFalse(economy.storage_enabled(restored['buildings'][index])['wood'])
        economy.spend(self.town, {'wood': 10})
        self.assertEqual(store['stored_goods']['wood'], 30)
        logistics.transfer(self.town, saw, store, 'supply', worker)
        self.assertEqual(saw['input_buffer']['wood'], 30)
        self.assertEqual(store['stored_goods']['wood'], 0)

    def test_storage_switches_reject_invalid_commands_without_mutating_stock(self):
        store = self.add('storehouse', stored_goods={'wood': 20})
        for enabled in (None, [], {'wood': 1}, {'wood': 'yes'}, {'wood': None}, {'coins': True}):
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                server.command(self.town, {'action': 'configure_storage', 'index': 1, 'enabled': enabled})
            self.assertEqual(self.town, before)
        for index in (None, True, -1, 100, 0):
            with self.assertRaises(ValueError):
                server.command(self.town, {'action': 'configure_storage', 'index': index, 'enabled': {'wood': True}})
        store['construction'] = {'total': 30, 'progress': 0}
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'configure_storage', 'index': 1, 'enabled': {'wood': True}})

    def test_spending_output_releases_a_full_processor(self):
        saw = self.add('sawpit', input_buffer={'wood': 10})
        worker = self.assign(0, saw)
        logistics.record_output(self.town, saw, 'planks', 19.5)
        self.assertEqual(logistics.produce(self.town, saw, worker, elapsed=economy.production_seconds(saw)), 'Workplace buffer full')
        economy.spend(self.town, {'planks': 5})
        self.assertNotIn('buffer_full', saw)
        self.assertEqual(logistics.produce(self.town, saw, worker, elapsed=economy.production_seconds(saw)), 'Working')
        self.assertEqual(saw['input_buffer']['wood'], 8)

    def test_sawpit_reserves_returning_planks_against_its_output_limit(self):
        saw = self.add('sawpit', stored_goods={'planks': 18}, input_buffer={'wood': 10})
        worker = self.assign(0, saw)
        economy.ensure_inventory(self.town)
        economy.add_stock(self.town, 'planks', 18)
        duration = economy.production_seconds(saw)
        self.assertEqual(logistics.produce(self.town, saw, worker, duration), 'Working')
        self.assertEqual(logistics.produce(self.town, saw, worker, duration), 'Working')
        inputs, practice = dict(saw['input_buffer']), worker['woodworking']
        self.assertEqual(logistics.produce(self.town, saw, worker, duration), 'Workplace buffer full')
        self.assertEqual(saw['input_buffer'], inputs)
        self.assertEqual(worker['woodworking'], practice)
        self.assertEqual(worker['pending_output'], {'planks': 2})
        logistics.drop_off(self.town, saw, worker)
        self.assertEqual(economy.inventory(saw)['planks'], 20)
        self.assertEqual(economy.quantity(self.town, 'planks'), 20)
        self.assertEqual(economy.local_room(saw, 'wood'), 30)

    def test_legacy_excess_planks_are_preserved_until_used(self):
        saw = self.add('sawpit', output_buffer={'planks': 49}, input_buffer={'wood': 17})
        worker = self.assign(0, saw)
        economy.ensure_inventory(self.town)
        duration = economy.production_seconds(saw)
        self.assertEqual(logistics.produce(self.town, saw, worker, duration), 'Workplace buffer full')
        self.assertEqual(saw['output_buffer'], {'planks': 49})
        self.assertEqual(saw['input_buffer'], {'wood': 17})
        economy.spend(self.town, {'planks': 30})
        self.assertEqual(logistics.produce(self.town, saw, worker, duration), 'Working')
        self.assertEqual(saw['pending_output'], {'planks': 1})

    def test_producer_hauls_at_resource_limit_before_total_buffer_is_full(self):
        saw = self.add('sawpit', output_buffer={'planks': 20})
        store = self.add('storehouse', x=8, storage_enabled={'planks': True})
        worker = self.assign(0, saw)
        economy.ensure_inventory(self.town)
        self.assertTrue(logistics.worker(self.town, self.town, worker, saw,
                                         server.construction_route, 0))
        trip = worker['logistics_trip']
        self.assertEqual(trip['operation'], 'collect')
        self.assertEqual(trip['storage'], {k: store[k] for k in ('kind', 'x', 'y')})
        self.assertEqual(trip['cargo'], {'planks': 20})

    def test_full_processor_ingredient_storage_does_not_block_production(self):
        for kind, ingredient, stored, inputs, output, amount in [
            ('mill', 'wheat', 80, {'wheat': 30}, 'flour', 2),
            ('bakery', 'flour', 40, {'flour': 10, 'wood': 5}, 'bread', 2),
            ('sawpit', 'wood', 30, {'wood': 10}, 'planks', 1),
        ]:
            with self.subTest(kind=kind):
                self.setUp()
                building = self.add(kind, stored_goods={ingredient: stored}, input_buffer=inputs)
                worker = self.assign(0, building)
                economy.ensure_inventory(self.town)
                self.assertEqual(economy.local_room(building, ingredient), 0)
                before = copy.deepcopy(self.town)
                self.assertFalse(logistics.worker(self.town, self.town, worker, building,
                                                  server.construction_route, 0))
                self.assertEqual(self.town, before)
                self.assertEqual(logistics.produce(self.town, building, worker,
                                                   economy.production_seconds(building)), 'Working')
                self.assertEqual(worker['pending_output'], {output: amount})
                self.assertEqual(building['stored_goods'][ingredient], stored)
                for key, cost in economy.production_recipe(building)[0].items():
                    self.assertEqual(building['input_buffer'][key], inputs[key] - cost)

    def test_full_wheat_storage_still_supplies_flour_to_bakery(self):
        store = self.add('storehouse', x=1, storage_enabled={'wheat': True},
                         stored_goods={'wheat': 100})
        mill = self.add('mill', stored_goods={'wheat': 80}, input_buffer={'wheat': 30})
        bakery = self.add('bakery', x=5, input_buffer={'wood': 5})
        self.assign(0, mill)
        self.assign(1, bakery)
        economy.ensure_inventory(self.town)
        self.assertEqual(economy.local_room(store, 'wheat'), 0)
        self.ticks(20)
        self.assertGreater(self.town.get('produced', {}).get('flour', 0), 0)
        self.assertGreater(self.town.get('produced', {}).get('bread', 0), 0)

    def test_output_batches_use_separate_food_limits_and_preserve_stock(self):
        cases = [
            ({'kind': 'barn', 'output_buffer': {'milk': 19.5}}, {'milk': .75, 'eggs': .75}),
            ({'kind': 'smokehouse', 'stored_goods': {'smoked_fish': 43}}, {'smoked_fish': 3}),
            ({'kind': 'sawpit', 'stored_goods': {'wood': 30, 'planks': 19},
              'pending_output': {'planks': .5}}, {'planks': 1}),
        ]
        for building, outputs in cases:
            with self.subTest(kind=building['kind']):
                before = copy.deepcopy(building)
                self.assertFalse(logistics.output_fits(building, outputs))
                self.assertEqual(building, before)
        self.assertTrue(logistics.output_fits({'kind': 'sawpit', 'stored_goods': {'planks': 19}},
                                             {'planks': 1}))
        self.assertTrue(logistics.output_fits({'kind': 'barn', 'output_buffer': {'milk': 19}},
                                             {'milk': .75, 'eggs': .75}))
        self.assertTrue(logistics.output_fits({'kind': 'smokehouse', 'stored_goods': {'sturgeon': 45}},
                                             {'smoked_fish': 3}))

    def test_food_storage_limits_are_independent_item_counts_in_every_building(self):
        for kind in economy.STORAGE_GOODS:
            foods = [key for key in economy.STORAGE_GOODS[kind] if key in economy.EDIBLE]
            if not foods:
                continue
            building = {'kind': kind, 'storage_enabled': {key: True for key in foods}}
            limits = economy.storage_allocations(building)
            building['stored_goods'] = {key: limits[key] for key in foods[:-1]}
            last = foods[-1]
            with self.subTest(kind=kind):
                self.assertEqual(economy.local_room(building, last), limits[last])
                building['pending_output'] = {last: 2}
                self.assertEqual(economy.local_room(building, last), limits[last] - 2)
                building['stored_goods'][last] = limits[last]
                self.assertEqual(economy.local_room(building, last), 0)

    def test_old_food_filters_expand_and_explicit_food_switches_survive_reload(self):
        store = self.add('storehouse', storage_allocations={'food': 50},
                         stored_goods={'bread': 80, 'fish': 50, 'food': 3})
        index = self.town['buildings'].index(store)
        original = copy.deepcopy(store['stored_goods'])
        self.assertTrue(all(economy.storage_enabled(store)[key] for key in economy.EDIBLE))
        self.assertEqual(economy.local_room(store, 'bread'), 0)
        self.assertEqual(economy.local_room(store, 'smoked_fish'), 50)
        enabled = economy.storage_enabled(store)
        enabled['bread'] = False
        server.command(self.town, {'action': 'configure_storage', 'index': index, 'enabled': enabled})
        restored = json.loads(json.dumps(self.town))
        economy.ensure_inventory(restored)
        saved = restored['buildings'][index]
        self.assertFalse(economy.storage_enabled(saved)['bread'])
        self.assertTrue(economy.storage_enabled(saved)['fish'])
        self.assertEqual(saved['stored_goods'], original)
        self.assertEqual(economy.local_room(saved, 'smoked_fish'), 50)

    def test_producer_delivers_and_returns_without_counting_output_twice(self):
        b = self.add('woodcutter', output_buffer={'wood': 50})
        self.town['wood'] = 0
        self.town['map_resources'] = [{'kind': 'tree', 'x': 3, 'y': 1}]
        v = self.assign(0, b)
        self.town.update(harvested=50, produced={'wood': 50})
        self.ticks(3)
        self.assertEqual(self.town['wood'], 50 + sum(economy.inventory(b).values()))
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
        self.assertEqual(producer['woodcutting'], .24)
        self.assertGreater(self.town['wood'], 65)
        self.assertAlmostEqual(self.town['wood'] - 65 + sum(b.get('pending_output', {}).values()), 4.5)
        self.assertGreater(store['stored_goods']['wood'], 0)

    def test_farmer_delivers_through_surrounding_completed_acres(self):
        farm = self.add('farm', x=8, y=8, output_buffer={'vegetables': economy.storage_allocations({'kind': 'farm'})['vegetables']})
        self.town['map_resources'] = [{'kind': 'water', 'x': 8, 'y': 9}]
        for x, y in [(7, 8), (9, 8), (8, 7)]:
            self.add('acre', x=x, y=y, farm_index=self.town['buildings'].index(farm))
        farmer = self.assign(0, farm)
        self.town['food'] = 0
        self.ticks(10)
        self.assertGreater(self.town['food'], 0)
        self.assertLess(logistics.buffered(farm), economy.storage_allocations(farm)['vegetables'])
        self.assertGreater(farmer['farming'], 0)

    def test_unfinished_acres_still_block_deliveries(self):
        farm = self.add('farm', x=8, y=8, output_buffer={'vegetables': economy.storage_allocations({'kind': 'farm'})['vegetables']})
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
        self.assertAlmostEqual(remaining + (planks + saw.get('pending_output', {}).get('planks', 0)) * 2, 20)
        self.assertGreater(self.town.get('goods', {}).get('planks', 0), 0)

    def test_two_filtered_storehouses_route_wood_through_sawpit_to_planks(self):
        camp = self.add('woodcutter', x=2, y=2, output_buffer={'wood': 20})
        woods = self.add('storehouse', x=1, y=2, storage_enabled={'wood': True})
        saw = self.add('sawpit', x=4, y=2)
        city = self.add('storehouse', x=7, y=2, storage_enabled={'planks': True})
        sawyer = self.assign(0, saw)
        first = self.assign(1, woods)
        second = self.assign(2, city)
        economy.ensure_inventory(self.town)
        self.town['buildings'][0]['stored_goods']['wood'] = 0
        self.town['wood'] = 20
        for _ in range(100):
            logistics.haul(self.town, self.town, first, woods, server.construction_route, 3)
            if woods.get('stored_goods', {}).get('wood', 0) > 0:
                break
        self.assertEqual(woods['stored_goods']['wood'], 20)
        self.assertEqual(camp['output_buffer']['wood'], 0)
        for _ in range(100):
            logistics.haul(self.town, self.town, first, woods, server.construction_route, 3)
            if saw.get('input_buffer', {}).get('wood', 0) > 0:
                break
        self.assertEqual(saw['input_buffer']['wood'], 20)
        self.assertEqual(logistics.produce(self.town, saw, sawyer, elapsed=economy.production_seconds(saw)), 'Working')
        logistics.drop_off(self.town, saw, sawyer)
        produced = saw['output_buffer']['planks']
        for _ in range(100):
            logistics.haul(self.town, self.town, second, city, server.construction_route, 3)
            if city.get('stored_goods', {}).get('planks', 0) > 0:
                break
        self.assertEqual(city['stored_goods']['planks'], produced)
        self.assertEqual(city['stored_goods'].get('wood', 0), 0)
        self.assertEqual(woods['stored_goods'].get('planks', 0), 0)
        economy.spend(self.town, {'planks': produced})
        self.assertEqual(city['stored_goods']['planks'], 0)

    def test_switching_off_in_transit_rechecks_delivery_and_preserves_goods(self):
        camp = self.add('woodcutter', x=8, y=2, output_buffer={'wood': 20})
        store = self.add('storehouse', x=1, y=2, storage_enabled={'wood': True})
        hauler = self.assign(0, store)
        economy.ensure_inventory(self.town)
        self.town['wood'] = 20
        self.town['buildings'][0]['stored_goods']['wood'] = 0
        total = self.town['wood']
        self.assertTrue(logistics.start_trip(self.town, self.town, hauler, camp, store,
                                            'collect', server.construction_route, hauler=True))
        server.command(self.town, {'action': 'configure_storage', 'index': 2, 'enabled': {'wood': False}})
        logistics.advance_trip(self.town, self.town, hauler, 100)
        self.assertEqual(camp['output_buffer']['wood'], 20)
        self.assertEqual(store.get('stored_goods', {}).get('wood', 0), 0)
        self.assertEqual(self.town['wood'], total)
        server.command(self.town, {'action': 'configure_storage', 'index': 2, 'enabled': {'wood': True}})
        logistics.haul(self.town, self.town, hauler, store, server.construction_route, 100)
        self.assertEqual(store['stored_goods']['wood'], 20)
        self.assertEqual(self.town['wood'], total)

    def test_food_and_wool_share_the_same_collection_rules(self):
        store = self.add('storehouse', x=1, storage_allocations={'food': 50, 'wool': 50})
        barn = self.add('barn', output_buffer={'wool': 3, 'eggs': 4})
        self.assign(0, store)
        self.ticks(8)
        self.assertEqual(self.town['goods']['wool'], 3)
        self.assertEqual(barn['output_buffer']['eggs'], 0)
        self.assertEqual(self.town.get('produced', {}).get('wool', 0), 0)
        self.assertAlmostEqual(self.town['food'], 44 - 8 * .78)

    def test_enabled_resources_have_independent_capacity_and_preserve_old_saves(self):
        store = self.add('storehouse', storage_allocations={'wood': 30, 'planks': 40, 'wool': 0})
        self.assertEqual(economy.storage_allocations(store)['wood'], 50)
        self.assertEqual(economy.storage_allocations(store)['planks'], 50)
        self.assertEqual(economy.storage_allocations(store)['wool'], 0)
        store['stored_goods'] = {'wood': 100, 'planks': 20}
        self.assertEqual(economy.local_room(store, 'wood'), 0)
        self.assertEqual(economy.local_room(store, 'planks'), 30)
        server.command(self.town, {'action': 'configure_storage', 'index': 1,
                                  'enabled': {'wood': True, 'planks': False, 'wool': True}})
        self.assertEqual(store['stored_goods'], {'wood': 100, 'planks': 20})
        self.assertEqual(economy.local_room(store, 'planks'), 0)
        self.assertEqual(economy.local_room(store, 'wool'), 50)
        self.assertEqual(economy.storage_limit(self.town, 'wool'), 50)
        store['level'] = 2
        self.assertEqual(economy.local_room(store, 'wood'), 0)
        self.assertEqual(economy.local_room(store, 'wool'), 75)
        self.assertEqual(economy.local_room(store, 'planks'), 0)

    def test_upgrade_reaches_warehouse_level_eight_and_expands_enabled_resources(self):
        store = self.add('storehouse', storage_allocations={'wood': 50, 'planks': 50})
        self.town['wood'] = 1000
        index = self.town['buildings'].index(store)
        capacities = (50, 75, 100, 150, 200, 250, 300, 350)
        for level in range(2, 9):
            wood = self.town['wood']
            server.command(self.town, {'action': 'upgrade_building', 'index': index})
            self.assertEqual(self.town['wood'], wood - 25 * (level - 1))
            self.assertEqual(store['construction']['total'], server.BUILD_SECONDS * (level - 1))
            for _ in range(100):
                if 'construction' not in store:
                    break
                server.construct(self.town, self.town)
            self.assertEqual(store['level'], level)
            self.assertEqual(economy.storage_allocations(store)['wood'], capacities[level - 1])
            self.assertEqual(economy.storage_allocations(store)['planks'], capacities[level - 1])
            self.assertEqual(economy.storage_allocations(store)['wheat'], 0)
        self.assertEqual(economy.storage_size(store), 350)
        self.assertEqual(economy.storage_allocations(store)['wood'], 350)
        self.assertEqual(economy.storage_allocations(store)['planks'], 350)
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
        self.assertEqual(saved['towns']['a']['wood'], 50 + sum(economy.inventory(saved['towns']['a']['buildings'][-1]).values()))
        b['x'] = 21
        self.ticks()
        self.assertEqual(b['output_buffer']['wood'], 50)
        self.assertEqual(self.town['wood'], 50)
        self.assertNotIn('logistics_trip', v)

    def test_recipe_can_receive_ingredients_from_different_storage_allocations(self):
        a = self.add('storehouse', x=1, storage_allocations={'wood': 100})
        b = self.add('storehouse', x=1, y=3, storage_allocations={'planks': 100})
        self.add('storehouse', x=10, y=10, storage_allocations={'tools': 100})
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
        saw = self.add('sawpit', input_buffer={'wood': 10}, output_buffer={'planks': 19.5})
        v = self.assign(0, saw)
        self.ticks()
        self.assertTrue(saw['buffer_full'])
        self.assertEqual(saw['input_buffer']['wood'], 10)
        self.assertEqual(v.get('woodworking', 0), 0)
        self.ticks(3)
        self.assertEqual(store['stored_goods']['planks'], 19.5)
        self.assertEqual(self.town['goods']['planks'], 19.5 + sum(economy.inventory(saw).values()))
        self.assertNotIn('buffer_full', saw)

    def test_hall_food_types_have_separate_capacity_and_preserve_undelivered_goods(self):
        self.town['food'] = 99
        barn = self.add('barn', output_buffer={'bread': 2, 'eggs': 4})
        logistics.transfer(self.town, barn, self.town['buildings'][0], 'collect', self.town['villagers'][0])
        self.assertEqual(self.town['food'], 106)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['food'], 99)
        self.assertEqual(barn['output_buffer']['bread'], 0)
        self.assertEqual(barn['output_buffer']['eggs'], 0)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['bread'], 2)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['eggs'], 4)
        self.town['buildings'][0]['stored_goods']['bread'] = 100
        barn['output_buffer']['bread'] = 3
        logistics.transfer(self.town, barn, self.town['buildings'][0], 'collect', self.town['villagers'][0])
        self.assertEqual(barn['output_buffer']['bread'], 3)


if __name__ == '__main__':
    unittest.main()
