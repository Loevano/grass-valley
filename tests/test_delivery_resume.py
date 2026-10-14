import unittest

import progression as economy
import server


class ProducerDeliveryResume(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Fields')
        self.town.update(terrain=['g' * 120] * 120, npcs=[], map_resources=[],
                         forage_version=1, forage_halls=[[5, 5]])
        economy.ensure_inventory(self.town)
        hall = self.town['buildings'][0]
        hall['stored_goods'] = {'wood': 65, 'vegetables': 98}
        farm = {'kind': 'farm', 'x': 8, 'y': 8, 'output_buffer': {'vegetables': 50}}
        self.town['buildings'].append(farm)
        for x, y in [(9, 8), (9, 7), (9, 9)]:
            self.town['buildings'].append({'kind': 'acre', 'x': x, 'y': y, 'farm_index': 1})
        self.town.update(food=148, goods={'vegetables': 148})
        self.world = {'towns': {'fields': self.town}}
        for villager in self.town['villagers']:
            villager['talents'] = {}

    def test_shared_farm_delivery_returns_to_work_before_buffer_refills(self):
        self.town['buildings'][1]['settler_level'] = 2
        rowan, bram = self.town['villagers'][1], self.town['villagers'][3]
        server.assign_settler(self.town, 1, 1)
        server.assign_settler(self.town, 3, 1)
        rowan.update(farming=20, talents={'farming': 1})
        bram.update(farming=15, talents={'farming': -1})
        self.town['buildings'][1]['output_buffer']['vegetables'] = 46.88
        bram['logistics_trip'] = {
            'operation': 'collect', 'workplace': {'kind': 'farm', 'x': 8, 'y': 8},
            'storage': {'kind': 'hall', 'x': 5, 'y': 5},
            'path': [[5, 6], [6, 6], [7, 6], [8, 6], [8, 7]],
            'delivery_step': 0, 'delivered': True,
        }
        bram['logistics_position'] = {'x': 5, 'y': 6}
        server.tick(self.world)
        self.assertNotIn('logistics_trip', bram)
        self.assertNotEqual(bram['production_status'], 'Working')
        self.assertGreaterEqual(bram['farming'], 15)
        self.assertFalse(self.town.get('produced'))
        self.assertAlmostEqual(sum(b.get(field, {}).get('vegetables', 0) for b in self.town['buildings']
                                   for field in ('output_buffer', 'pending_output', 'stored_goods')), 46.88 + 98 - .24)
        server.tick(self.world)
        self.assertIn('work_animation', bram)

    def test_saved_output_trip_is_cancelled_and_full_farm_waits(self):
        self.town['buildings'][0].update(x=2, y=2)
        bram = self.town['villagers'][3]
        server.assign_settler(self.town, 3, 1)
        # Fill the farm and field stores so this delivery must travel to the hall.
        farm = self.town['buildings'][1]
        extra = economy.storage_allocations(farm)['vegetables'] - farm['output_buffer']['vegetables']
        farm['output_buffer']['vegetables'] += extra
        economy.add_stock(self.town, 'vegetables', extra)
        for acre in self.town['buildings'][2:]:
            acre['stored_goods'] = {'vegetables': 20}
        self.town['food'] += 60
        self.town['goods']['vegetables'] += 60
        bram['logistics_trip'] = {
            'operation': 'collect', 'workplace': {'kind': 'farm', 'x': 8, 'y': 8},
            'storage': {'kind': 'hall', 'x': 2, 'y': 2},
            'path': [[8, 7], [7, 7]], 'delivery_step': 1, 'delivered': False,
        }
        practice = bram['farming']
        stock = farm['output_buffer']['vegetables']
        self.town['buildings'][0]['x'] += 1
        server.tick(self.world)
        self.assertNotIn('logistics_trip', bram)
        self.assertIn('Workplace buffer full', bram['production_status'])
        self.assertEqual(bram['farming'], practice)
        self.assertEqual(farm['output_buffer']['vegetables'], stock)


if __name__ == '__main__':
    unittest.main()
