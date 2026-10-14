import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class LivestockBalance(unittest.TestCase):
    def test_automatic_pig_cycle_supplies_meat_recipes_without_feed(self):
        town = flat_town('Luxury meat')
        economy.ensure_inventory(town)
        town.update(level=4, food=0, goods={})
        town['buildings'][0]['stored_goods'] = {}
        barn = dict(kind='barn', x=3, y=7, animals={'pig': 1}, animal_growth={'pig': [0]})
        shop = dict(kind='butcher', x=8, y=7)
        town['buildings'].extend([barn, shop])
        logistics.advance_barn(town, barn, 120)
        self.assertEqual(barn['animal_growth']['pig'], [0])
        self.assertEqual(economy.quantity(town, 'hides'), 2)
        self.assertEqual(barn['output_buffer']['meat'], 8)
        butcher = town['villagers'][0]
        butcher.update(job='butchery', building_index=2, husbandry=2, talents={})
        economy.spend(town, {'meat': 8})
        shop['input_buffer'] = {'meat': 8}
        for _ in range(4):
            self.assertEqual(logistics.produce(town, shop, butcher, 6), 'Working')
            logistics.drop_off(town, shop, butcher)
        self.assertEqual(economy.quantity(town, 'prepared_meat'), 8)
        self.assertEqual(town['food'], 32)
        self.assertAlmostEqual(butcher['butchery'], .4)

    def test_every_meat_recipe_adds_food_and_sale_value_over_its_inputs(self):
        for recipe in economy.PROFESSION_RECIPES['butcher']:
            with self.subTest(output=recipe['output']):
                alternative_food = sum(amount * (economy.EDIBLE['prepared_meat']
                                       if key == 'meat' and recipe['level'] > 1 else
                                       economy.EDIBLE['bread'] if key == 'flour' else
                                       economy.EDIBLE.get(key, 0))
                                       for key, amount in recipe['inputs'].items())
                self.assertGreater(recipe['amount'] * economy.EDIBLE[recipe['output']], alternative_food)
                ingredient_price = sum(amount * economy.sale_price(key)
                                       for key, amount in recipe['inputs'].items())
                self.assertGreater(recipe['amount'] * economy.sale_price(recipe['output']), ingredient_price)

    def test_saved_meat_is_revalued_once_preserving_untyped_food_and_buffers(self):
        town = flat_town('Saved meat')
        economy.ensure_inventory(town)
        old = {'prepared_meat': 1.5, 'herbed_meat': 1.75, 'smoked_meat': 2,
               'sausages': 2.25, 'meat_pies': 4}
        town.update(food_balance_version=1, goods={key: 2 for key in old},
                    food=10 + 2 * sum(old.values()))
        stock = copy.deepcopy(town['goods'])
        town['buildings'][0]['stored_goods'] = dict(stock, food=10)
        shop = dict(kind='butcher', input_buffer={'meat_pies': 20},
                    pending_output={'prepared_meat': 10},
                    output_buffer={'meat_pies': 2})
        town['buildings'].append(shop)
        buffers = copy.deepcopy(shop)
        economy.ensure_inventory(town)
        expected = 10 + 2 * sum(economy.EDIBLE[key] for key in old)
        self.assertEqual(town['food'], expected)
        self.assertEqual(town['goods'], stock)
        self.assertEqual(shop, buffers)
        self.assertEqual(town['buildings'][0]['stored_goods']['food'], 10)
        restored = json.loads(json.dumps(town))
        economy.ensure_inventory(restored)
        self.assertEqual(restored['food'], expected)
        economy.eat(restored, 4)
        self.assertEqual(restored['food'], expected - 4)


if __name__ == '__main__':
    unittest.main()
