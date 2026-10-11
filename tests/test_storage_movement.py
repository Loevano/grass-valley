import copy
import unittest

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

    def test_full_central_storage_stops_movement_and_production(self):
        server.tick(self.world)
        self.assertEqual(self.worker['production_status'], 'Waiting for storage space')
        self.assertNotIn('work_animation', self.worker)
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.worker['woodcutting'], 0)

    def test_freeing_hall_space_resumes_worker(self):
        server.tick(self.world)
        economy.spend(self.town, {'wood': 1})
        server.tick(self.world)
        self.assertEqual(self.worker['production_status'], 'Working')
        self.assertGreater(self.worker['woodcutting'], 0)
        self.assertEqual(self.town['produced']['wood'], .5)

    def test_existing_work_cycle_pauses_without_losing_pending_goods(self):
        economy.spend(self.town, {'wood': 1})
        server.tick(self.world)
        animation = copy.deepcopy(self.worker.get('work_animation'))
        pending = copy.deepcopy(self.worker.get('pending_output'))
        practice = self.worker['woodcutting']
        self.hall['stored_goods']['wood'] = 100
        self.town['wood'] = 100
        server.tick(self.world)
        self.assertEqual(self.worker.get('work_animation'), animation)
        self.assertEqual(self.worker.get('pending_output'), pending)
        economy.spend(self.town, {'wood': 1})
        server.tick(self.world)
        self.assertGreater(self.worker['woodcutting'], practice)

    def test_only_completed_central_storage_with_matching_allocation_counts(self):
        store = {'kind': 'storehouse', 'x': 8, 'y': 3,
                 'storage_allocations': {'food': 100}}
        self.town['buildings'].append(store)
        available = lambda: logistics.output_storage_available(self.town, self.camp, self.tree, self.worker)
        self.assertFalse(available())
        store['storage_allocations'] = {'wood': 100}
        self.assertTrue(available())
        store['construction'] = {'progress': 0, 'total': 30}
        self.assertFalse(available())
        del store['construction']
        store['stored_goods'] = {'wood': 100}
        self.assertFalse(available())

    def test_fish_requires_food_space_and_processors_require_output_space(self):
        self.worker['job'] = 'fishing'
        self.hall['stored_goods'] = {'wood': 0, 'food': 100}
        hut = {'kind': 'fishing_hut', 'x': 3, 'y': 3}
        self.assertFalse(logistics.output_storage_available(self.town, hut, self.tree, self.worker))
        self.hall['stored_goods']['food'] = 99
        self.assertTrue(logistics.output_storage_available(self.town, hut, self.tree, self.worker))
        saw = {'kind': 'sawpit', 'x': 3, 'y': 3}
        self.assertFalse(logistics.output_storage_available(self.town, saw, saw, self.worker))
        self.town['buildings'].append({'kind': 'storehouse', 'x': 8, 'y': 3,
                                       'storage_allocations': {'planks': 10}})
        self.assertTrue(logistics.output_storage_available(self.town, saw, saw, self.worker))

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


if __name__ == '__main__':
    unittest.main()
