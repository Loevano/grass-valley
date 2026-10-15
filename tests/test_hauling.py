import copy
import json
import unittest

from fixtures import flat_town

import logistics
import progression as economy
import server


class HaulingSkill(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Hauling')
        self.worker = self.town['villagers'][0]
        self.worker['talents'] = {}
        self.source = {'kind': 'woodcutter', 'x': 3, 'y': 2,
                       'output_buffer': {'wood': 40}, 'stored_goods': {'wood': 40}}
        self.store = {'kind': 'storehouse', 'level': 3, 'x': 1, 'y': 2,
                      'storage_allocations': {'wood': 200, 'planks': 100}}
        self.town['buildings'].extend([self.source, self.store])

    def test_diagonal_delivery_steps_take_longer_than_cardinal_steps(self):
        self.worker['logistics_trip'] = {
            'workplace': {k: self.source[k] for k in ('kind', 'x', 'y')},
            'storage': {k: self.store[k] for k in ('kind', 'x', 'y')},
            'operation': 'collect', 'path': [[0, 0], [1, 1], [2, 2]],
            'pickup_step': 2, 'delivery_step': 2, 'cargo': {}, 'delivered': False}
        landscape = {'buildings': [], 'map_resources': []}
        logistics.advance_trip(self.town, landscape, self.worker, .6)
        self.assertEqual(self.worker['logistics_trip']['step'], 0)
        logistics.advance_trip(self.town, landscape, self.worker, .2)
        self.assertEqual(self.worker['logistics_trip']['step'], 1)

    def test_diagonal_delivery_replans_when_a_corner_becomes_blocked(self):
        self.worker['logistics_trip'] = {
            'workplace': {k: self.source[k] for k in ('kind', 'x', 'y')},
            'storage': {k: self.store[k] for k in ('kind', 'x', 'y')},
            'operation': 'collect', 'path': [[0, 0], [1, 1]],
            'pickup_step': 1, 'delivery_step': 1, 'cargo': {}, 'delivered': False}
        landscape = {'buildings': [], 'map_resources': [{'kind': 'rock', 'x': 1, 'y': 0}]}
        logistics.advance_trip(self.town, landscape, self.worker, .8)
        trip = self.worker['logistics_trip']
        self.assertEqual(self.worker['logistics_position'], {'x': 0, 'y': 0})
        self.assertEqual(trip['step'], 0)
        self.assertEqual(trip['path'][0], [0, 0])
        self.assertNotEqual(trip['path'][1], [1, 1])
        self.assertNotIn([1, 0], trip['path'])
        self.assertFalse(trip['delivered'])
        self.assertEqual(trip['cargo'], {})
        self.assertEqual(self.worker['production_status'], 'Delivery blocked; finding another route')

    def test_capacity_is_shared_across_buffers_and_improves_with_practice(self):
        for practice, moved in [(0, 50), (19.99, 50), (20, 80)]:
            with self.subTest(practice=practice):
                self.setUp()
                self.worker['hauling'] = practice
                economy.ensure_inventory(self.town)
                total = self.town['wood']
                logistics.transfer(self.town, self.source, self.store, 'collect', self.worker)
                self.assertEqual(self.store['stored_goods']['wood'], moved)
                self.assertEqual(logistics.buffered(self.source), 80 - moved)
                self.assertEqual(self.town['wood'], total)
                self.assertAlmostEqual(self.worker['hauling'], round(practice + .08, 3))

    def test_supply_shares_capacity_and_trains_only_for_goods_moved(self):
        self.worker['talents'] = {'hauling': -1}
        self.store['stored_goods'] = {'wood': 100, 'planks': 100}
        workshop = {'kind': 'workshop', 'x': 4, 'y': 2}
        self.town['buildings'].append(workshop)
        producer = self.town['villagers'][1]
        producer.update(job='crafting', building_index=3, crafting=0, talents={})
        logistics.transfer(self.town, workshop, self.store, 'supply', self.worker)
        self.assertAlmostEqual(sum(workshop['input_buffer'].values()), 37.5)
        self.assertAlmostEqual(self.worker['hauling'], .06)
        self.assertAlmostEqual(sum(self.store['stored_goods'].values()), 162.5)
        self.assertEqual(producer['crafting'], 0)

    def test_empty_or_full_delivery_does_not_train(self):
        self.source['output_buffer'] = {}
        self.source['stored_goods'] = {}
        logistics.transfer(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(self.worker.get('hauling', 0), 0)
        self.source['output_buffer'] = {'wood': 5}
        self.store['storage_allocations'] = {'wood': 0}
        logistics.transfer(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(self.worker.get('hauling', 0), 0)

    def test_experience_and_roads_increase_delivery_speed(self):
        trip = {'workplace': {k: self.source[k] for k in ('kind', 'x', 'y')},
                'storage': {k: self.store[k] for k in ('kind', 'x', 'y')},
                'operation': 'collect', 'path': [[x, 0] for x in range(10)],
                'delivery_step': 9, 'delivered': False}
        landscape = {'buildings': [], 'map_resources': []}
        for practice, roads, expected in [(0, [], 1), (20, [], 2),
                (20, [{'x': x, 'y': 0} for x in range(10)], 3)]:
            worker = {'hauling': practice, 'logistics_trip': copy.deepcopy(trip)}
            logistics.advance_trip(self.town, dict(landscape, roads=roads), worker, 1)
            self.assertEqual(worker['logistics_trip']['step'], expected)
            self.assertEqual(worker['hauling'], practice)

    def test_successful_trip_trains_once_and_survives_reload(self):
        self.worker['logistics_trip'] = {
            'workplace': {k: self.source[k] for k in ('kind', 'x', 'y')},
            'storage': {k: self.store[k] for k in ('kind', 'x', 'y')},
            'operation': 'collect', 'path': [[0, 0], [1, 0], [2, 0]],
            'delivery_step': 1, 'delivered': False}
        landscape = {'buildings': [], 'map_resources': []}
        logistics.advance_trip(self.town, landscape, self.worker, .6)
        self.assertEqual(self.worker['hauling'], .08)
        restored = json.loads(json.dumps(self.town))
        worker = restored['villagers'][0]
        logistics.advance_trip(restored, landscape, worker, 1)
        self.assertEqual(worker['hauling'], .08)
        self.assertNotIn('logistics_trip', worker)

    def test_cargo_preview_matches_delivery_without_changing_stock(self):
        economy.ensure_inventory(self.town)
        before = copy.deepcopy(self.town)
        cargo = logistics.planned_cargo(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(cargo, {'wood': 50})
        self.assertEqual(self.town, before)
        self.assertEqual(logistics.transfer(self.town, self.source, self.store, 'collect', self.worker), cargo)

    def test_mixed_cargo_shares_carry_capacity_with_separate_food_storage(self):
        self.source['output_buffer'] = {'bread': 2, 'eggs': 4, 'wood': 40}
        self.source['stored_goods'] = {}
        self.store['storage_enabled'] = {'food': True, 'wood': True}
        self.store['stored_goods'] = {'eggs': 97}
        cargo = logistics.planned_cargo(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(cargo, {'bread': 2, 'eggs': 3, 'wood': 40})
        self.assertEqual(logistics.transfer(self.town, self.source, self.store, 'collect', self.worker), cargo)

    def test_supply_manifest_contains_each_recipe_ingredient(self):
        self.store['stored_goods'] = {'wood': 100, 'planks': 100}
        workshop = {'kind': 'workshop', 'x': 4, 'y': 2}
        self.town['buildings'].append(workshop)
        self.town['villagers'][1].update(job='crafting', building_index=3, crafting=0, talents={})
        cargo = logistics.planned_cargo(self.town, workshop, self.store, 'supply', self.worker)
        self.assertEqual(set(cargo), {'wood', 'planks'})
        self.assertAlmostEqual(sum(cargo.values()), 50)
        self.assertEqual(logistics.transfer(self.town, workshop, self.store, 'supply', self.worker), cargo)

    def test_trip_records_pickup_and_refreshes_cargo_before_delivery(self):
        self.worker['job'] = 'hauling'
        paths = iter([[(0, 0), (1, 0), (2, 0)], [(2, 0), (3, 0), (4, 0)]])
        route = lambda *_: next(paths)
        landscape = {'buildings': [], 'map_resources': []}
        self.assertTrue(logistics.start_trip(self.town, landscape, self.worker, self.source, self.store,
                                            'collect', route, hauler=True))
        trip = self.worker['logistics_trip']
        self.assertEqual(trip['pickup_step'], 2)
        self.assertEqual(trip['delivery_step'], 4)
        self.assertEqual(trip['cargo'], {'wood': 50})
        self.source['output_buffer']['wood'] = 0
        self.source['stored_goods']['wood'] = 7
        logistics.advance_trip(self.town, landscape, self.worker, 1.2)
        self.assertFalse(trip['delivered'])
        self.assertEqual(trip['cargo'], {'wood': 7})
        restored = json.loads(json.dumps(self.town))
        self.assertEqual(restored['villagers'][0]['logistics_trip']['cargo'], {'wood': 7})

    def test_delivery_manifest_reflects_goods_that_actually_fit_on_arrival(self):
        self.worker['logistics_trip'] = {
            'workplace': {k: self.source[k] for k in ('kind', 'x', 'y')},
            'storage': {k: self.store[k] for k in ('kind', 'x', 'y')},
            'operation': 'collect', 'path': [[0, 0], [1, 0], [2, 0]],
            'pickup_step': 0, 'delivery_step': 1, 'delivered': False, 'cargo': {'wood': 50}}
        self.store['stored_goods'] = {'wood': 97}
        logistics.advance_trip(self.town, {'buildings': [], 'map_resources': []}, self.worker, .6)
        self.assertEqual(self.worker['logistics_trip']['cargo'], {'wood': 3})
        self.assertTrue(self.worker['logistics_trip']['delivered'])


class CarriedInventory(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Carrying')
        economy.ensure_inventory(self.town)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='hauling', talents={})
        self.source = {'kind': 'woodcutter', 'x': 3, 'y': 2, 'output_buffer': {'wood': 20}}
        self.store = {'kind': 'storehouse', 'x': 7, 'y': 2, 'storage_enabled': {'wood': True}}
        self.town['buildings'].extend([self.source, self.store])
        economy.add_stock(self.town, 'wood', 20)
        self.landscape = {'buildings': [], 'map_resources': []}
        self.total = self.town['wood']

    def start(self, operation='collect', hauler=True, worker=None):
        if operation == 'supply' and hauler:
            (worker or self.worker)['logistics_position'] = {'x': self.store['x'], 'y': self.store['y'] - 1}
        paths = iter([[(0, 0), (1, 0), (2, 0)], [(2, 0), (3, 0), (4, 0)]])
        self.assertTrue(logistics.start_trip(self.town, self.landscape, worker or self.worker,
                                            self.source, self.store, operation, lambda *_: next(paths),
                                            hauler=hauler))

    def advance(self, seconds):
        logistics.advance_trip(self.town, self.landscape, self.worker, seconds)

    def test_pickup_removes_source_and_delivery_credits_only_the_held_load(self):
        self.start()
        self.advance(.6)
        self.assertEqual(self.source['output_buffer']['wood'], 20)
        self.advance(.6)
        trip = self.worker['logistics_trip']
        self.assertTrue(trip['picked_up'])
        self.assertFalse(trip['delivered'])
        self.assertEqual(trip['cargo'], {'wood': 20})
        self.assertEqual(self.source['output_buffer']['wood'], 0)
        self.assertEqual(economy.quantity(self.town, 'wood'), self.total - 20)
        self.assertEqual(self.store.get('stored_goods', {}), {})
        logistics.record_output(self.town, self.source, 'wood', 5)
        self.advance(1.2)
        self.assertEqual(self.store['stored_goods']['wood'], 20)
        self.assertEqual(self.source['output_buffer']['wood'], 5)
        self.assertEqual(self.town['wood'], self.total + 5)
        self.assertEqual(self.worker['hauling'], .08)

    def test_reload_and_second_worker_cannot_pick_up_the_same_goods(self):
        self.start()
        self.advance(1.2)
        other = self.town['villagers'][1]
        other.update(job='hauling', talents={})
        self.start(worker=other)
        logistics.advance_trip(self.town, self.landscape, other, 1.2)
        self.assertEqual(other['logistics_trip']['cargo'], {})
        self.town = json.loads(json.dumps(self.town))
        self.worker = self.town['villagers'][0]
        economy.ensure_inventory(self.town)
        self.advance(0)
        self.assertEqual(self.town['wood'], self.total - 20)
        self.advance(2)
        self.assertEqual(self.town['buildings'][-1]['stored_goods']['wood'], 20)
        self.assertEqual(self.town['wood'], self.total)

    def test_partial_delivery_keeps_remainder_until_room_is_available(self):
        self.start()
        self.advance(1.2)
        self.store['stored_goods'] = {'wood': 45}
        economy.add_stock(self.town, 'wood', 45)
        self.advance(2)
        trip = self.worker['logistics_trip']
        self.assertFalse(trip['delivered'])
        self.assertEqual(trip['cargo'], {'wood': 15})
        self.assertEqual(self.store['stored_goods']['wood'], 50)
        self.advance(100)
        self.assertEqual(trip['cargo'], {'wood': 15})
        self.assertEqual(self.worker['hauling'], .08)
        economy.withdraw(self.town, 'wood', 20, source=self.store)
        economy.add_stock(self.town, 'wood', -20)
        self.advance(0)
        self.assertNotIn('logistics_trip', self.worker)
        self.assertEqual(self.store['stored_goods']['wood'], 45)
        self.assertEqual(self.worker['hauling'], .08)
        self.assertEqual(self.town['wood'], self.total + 25)

    def test_disabled_destination_preserves_carried_goods_until_reenabled(self):
        self.start()
        self.advance(1.2)
        self.store['storage_enabled']['wood'] = False
        self.advance(20)
        self.assertEqual(self.worker['logistics_trip']['cargo'], {'wood': 20})
        self.assertEqual(self.store.get('stored_goods', {}), {})
        self.assertEqual(self.source['output_buffer']['wood'], 0)
        self.store['storage_enabled']['wood'] = True
        self.advance(0)
        self.assertEqual(self.store['stored_goods']['wood'], 20)
        self.assertEqual(self.town['wood'], self.total)

    def test_supply_is_unavailable_at_pickup_and_reserved_until_delivery(self):
        self.source.update(kind='sawpit', output_buffer={})
        self.store['stored_goods'] = {'wood': 20}
        self.worker.update(job='woodworking', building_index=1)
        self.start('supply', hauler=False)
        self.advance(1.2)
        self.assertEqual(self.store['stored_goods']['wood'], 0)
        self.assertEqual(self.source.get('input_buffer', {}), {})
        self.assertEqual(self.town['wood'], self.total - 20)
        self.assertEqual(logistics.supply_request(self.town, self.source, self.worker), {})
        self.advance(2)
        self.assertEqual(self.source['input_buffer']['wood'], 20)
        self.assertEqual(self.town['wood'], self.total - 20)

    def test_hauler_takes_supplies_before_leaving_storage(self):
        self.source.update(kind='sawpit', output_buffer={})
        self.store['stored_goods'] = {'wood': 20}
        self.town['villagers'][1].update(job='woodworking', building_index=1, talents={})
        self.start('supply')
        self.advance(0)
        self.assertTrue(self.worker['logistics_trip']['picked_up'])
        self.assertEqual(self.store['stored_goods']['wood'], 0)
        self.assertEqual(self.source.get('input_buffer', {}), {})
        self.advance(1.2)
        self.assertEqual(self.source['input_buffer']['wood'], 20)
        self.assertTrue(self.worker['logistics_trip']['delivered'])
        self.advance(2)
        self.assertNotIn('logistics_trip', self.worker)

    def test_carried_collection_counts_toward_production_caps(self):
        self.town['rules'] = {'production_caps': {'wood': self.total}}
        self.start()
        self.advance(1.2)
        self.assertEqual(economy.production_room(self.town, 'wood'), 0)

    def test_reassigned_hauler_reaches_supply_source_before_pickup_even_after_replan(self):
        self.source.update(kind='sawpit', output_buffer={})
        self.store['stored_goods'] = {'wood': 20}
        self.town['villagers'][1].update(job='woodworking', building_index=1, talents={})
        self.worker['logistics_position'] = {'x': 0, 'y': 0}
        logistics.start_trip(self.town, self.landscape, self.worker, self.source, self.store,
                             'supply', server.logistics_route, hauler=True)
        trip = self.worker['logistics_trip']
        self.assertGreater(trip['pickup_step'], 0)
        self.advance(.6)
        self.assertEqual(self.store['stored_goods']['wood'], 20)
        self.assertFalse(trip.get('picked_up', False))
        logistics.replan_trip(self.landscape, self.worker, self.source, self.store, set(), set(), ())
        pickup = trip['path'][trip['pickup_step']]
        self.assertEqual(abs(pickup[0] - self.store['x']) + abs(pickup[1] - self.store['y']), 1)
        self.advance(100)
        self.assertEqual(self.source['input_buffer']['wood'], 20)
        self.assertEqual(self.store['stored_goods']['wood'], 0)

    def test_cancellation_returns_goods_once_to_the_correct_source(self):
        for operation in ('collect', 'supply'):
            with self.subTest(operation=operation):
                self.setUp()
                if operation == 'supply':
                    self.source.update(kind='sawpit', output_buffer={})
                    self.store['stored_goods'] = {'wood': 20}
                    self.town['villagers'][1].update(job='woodworking', building_index=1, talents={})
                self.start(operation)
                self.advance(1.2 if operation == 'collect' else 0)
                logistics.cancel_trip(self.town, self.worker)
                logistics.cancel_trip(self.town, self.worker)
                origin = self.source if operation == 'collect' else self.store
                self.assertEqual(economy.inventory(origin).get('wood'), 20)
                self.assertEqual(self.town['wood'], self.total)

    def test_orphaned_output_moves_from_pending_to_carrying_once(self):
        self.source.update(output_buffer={}, pending_output={'wood': 20}, orphaned_output={'wood': 20})
        economy.add_stock(self.town, 'wood', -20)
        self.start()
        self.advance(1.2)
        self.assertEqual(self.source['pending_output']['wood'], 0)
        self.assertEqual(self.source['orphaned_output']['wood'], 0)
        self.assertEqual(self.worker['logistics_trip']['cargo'], {'wood': 20})
        self.assertEqual(self.town['wood'], self.total - 20)
        self.advance(2)
        self.assertEqual(self.town['wood'], self.total)

    def test_demolition_salvages_a_carried_load_once(self):
        for index in (1, 2):
            with self.subTest(destroyed=index):
                self.setUp()
                self.start()
                self.advance(1.2)
                server.command(self.town, {'action': 'destroy', 'index': index})
                self.assertNotIn('logistics_trip', self.worker)
                self.assertEqual(self.town['wood'], self.total)
                self.assertEqual(sum(economy.inventory(b).get('wood', 0) for b in self.town['buildings']), self.total)


if __name__ == '__main__':
    unittest.main()
