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

    def test_full_central_storage_allows_production_into_local_buffer(self):
        self.camp['output_buffer'] = {'wood': 30}
        self.town['wood'] += 30
        server.tick(self.world)
        self.assertEqual(self.worker['production_status'], 'Working')
        self.assertIn('work_animation', self.worker)
        self.assertEqual(self.worker['pending_output'], {'wood': .5})
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


if __name__ == '__main__':
    unittest.main()
