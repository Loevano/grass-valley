import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy


class ResourcePrecision(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Precision')
        economy.ensure_inventory(self.town)
        self.bakery = {'kind': 'bakery', 'x': 8, 'y': 5,
                       'recipe': 'bread', 'profession_upgrades': ['larger_oven'],
                       'input_buffer': {'flour': 203.5, 'wood': 4.999999999999972}}
        self.town['buildings'].append(self.bakery)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='baking', milling=3, baking=33.1,
                           talents={'farming': 1}, building_index=1)

    def test_saved_bakery_resumes_when_fuel_only_differs_by_roundoff(self):
        # Jenstown's saved batch requires 20 flour and 5 wood. Delivery already
        # treats this fuel as sufficient; production must make the same decision.
        town = json.loads(json.dumps(self.town))
        bakery, worker = town['buildings'][-1], town['villagers'][0]
        self.assertEqual(logistics.supply_request(town, bakery, worker), {})
        inputs = copy.deepcopy(bakery['input_buffer'])
        self.assertIn('3s remaining', logistics.produce(town, bakery, worker))
        self.assertEqual(bakery['input_buffer'], inputs)
        self.assertEqual(logistics.produce(town, bakery, worker), 'Working')
        self.assertEqual(bakery['input_buffer'], {'flour': 183.5, 'wood': 0})
        self.assertEqual(worker['pending_output'], {'bread': 20})
        logistics.drop_off(town, bakery, worker)
        self.assertEqual(bakery['output_buffer']['bread'], 20)

    def test_real_ingredient_shortage_still_blocks_without_consuming_or_training(self):
        for key, amount in (('flour', 19.999999), ('wood', 4.999999)):
            with self.subTest(ingredient=key):
                bakery, worker = copy.deepcopy((self.bakery, self.worker))
                bakery['input_buffer'][key] = amount
                before = copy.deepcopy((bakery, worker))
                self.assertEqual(logistics.produce(self.town, bakery, worker, elapsed=6),
                                 'Waiting for delivery: ' + key)
                self.assertEqual((bakery, worker), before)

    def test_spending_roundoff_shortfall_leaves_zero_stock(self):
        for key in ('wood', 'flour', 'bread', 'coins'):
            with self.subTest(resource=key):
                town = copy.deepcopy(self.town)
                amount = 4.999999999999972
                if key in ('wood', 'coins'):
                    town[key] = amount
                else:
                    town.setdefault('goods', {})[key] = amount
                town['buildings'][0]['stored_goods'][key] = amount
                self.assertTrue(economy.afford(town, {key: 5}))
                economy.spend(town, {key: 5})
                self.assertEqual(economy.quantity(town, key), 0)
                self.assertEqual(town['buildings'][0]['stored_goods'][key], 0)
                self.assertFalse(economy.afford(town, {key: .000001}))

    def test_spending_preserves_intentional_fractional_stock(self):
        self.town['wood'] = 4.625
        self.town['buildings'][0]['stored_goods']['wood'] = 4.625
        economy.spend(self.town, {'wood': .5})
        self.assertEqual(self.town['wood'], 4.125)
        self.assertEqual(self.town['buildings'][0]['stored_goods']['wood'], 4.125)


if __name__ == '__main__':
    unittest.main()
