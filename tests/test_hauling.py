import copy
import json
import unittest

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
        self.store = {'kind': 'storehouse', 'x': 1, 'y': 2,
                      'storage_allocations': {'wood': 200, 'planks': 100}}
        self.town['buildings'].extend([self.source, self.store])

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
                self.assertAlmostEqual(self.worker['hauling'], round(practice + .04, 3))

    def test_supply_shares_capacity_and_trains_only_for_goods_moved(self):
        self.worker['talents'] = {'hauling': -1}
        self.store['stored_goods'] = {'wood': 100, 'planks': 100}
        workshop = {'kind': 'workshop', 'x': 4, 'y': 2}
        self.town['buildings'].append(workshop)
        producer = self.town['villagers'][1]
        producer.update(job='crafting', building_index=3, crafting=0, talents={})
        logistics.transfer(self.town, workshop, self.store, 'supply', self.worker)
        self.assertAlmostEqual(sum(workshop['input_buffer'].values()), 37.5)
        self.assertAlmostEqual(self.worker['hauling'], .03)
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
        self.assertEqual(self.worker['hauling'], .04)
        restored = json.loads(json.dumps(self.town))
        worker = restored['villagers'][0]
        logistics.advance_trip(restored, landscape, worker, 1)
        self.assertEqual(worker['hauling'], .04)
        self.assertNotIn('logistics_trip', worker)

    def test_cargo_preview_matches_delivery_without_changing_stock(self):
        economy.ensure_inventory(self.town)
        before = copy.deepcopy(self.town)
        cargo = logistics.planned_cargo(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(cargo, {'wood': 50})
        self.assertEqual(self.town, before)
        self.assertEqual(logistics.transfer(self.town, self.source, self.store, 'collect', self.worker), cargo)

    def test_mixed_cargo_shares_capacity_and_storage_food_space(self):
        self.source['output_buffer'] = {'bread': 2, 'eggs': 4, 'wood': 40}
        self.source['stored_goods'] = {}
        self.store['storage_enabled'] = {'food': True, 'wood': True}
        self.store['stored_goods'] = {'eggs': 97}
        cargo = logistics.planned_cargo(self.town, self.source, self.store, 'collect', self.worker)
        self.assertEqual(cargo, {'bread': 2, 'wood': 40})
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


if __name__ == '__main__':
    unittest.main()
