import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class LivestockBalance(unittest.TestCase):
    def test_pig_grows_on_ten_feed_and_returns_thirty_two_food(self):
        for feed in ('fodder', 'vegetables'):
            with self.subTest(feed=feed):
                town = flat_town('Luxury meat')
                economy.ensure_inventory(town)
                town.update(level=4, food=0, goods={})
                town['buildings'][0]['stored_goods'] = {}
                barn = dict(kind='barn', x=3, y=7, animals={'pig': 1},
                            animal_growth={'pig': [0]}, input_buffer={feed: 10})
                shop = dict(kind='butcher', x=8, y=7)
                town['buildings'].extend([barn, shop])
                keeper = dict(job='husbandry', talents={})
                for _ in range(100):
                    self.assertEqual(logistics.produce(town, barn, keeper), 'Working')
                    economy.grow_animals(barn, 3)
                    # Hauling manure does not change feeding or growth.
                    barn['pending_output'] = {}
                    keeper.pop('pending_output', None)
                self.assertAlmostEqual(barn['input_buffer'][feed], 0)
                self.assertEqual(economy.mature_animals(barn, 'pig'), 1)
                butcher = town['villagers'][0]
                butcher.update(job='butchery', building_index=2, husbandry=2,
                               talents={}, work_animation={'phase': 'working'})
                server.command(town, dict(action='butcher_animal', index=2,
                                          barn_index=1, animal='pig'))
                self.assertEqual(logistics.butcher_animal(town, shop, butcher,
                                                         economy.BUTCHER_SECONDS), 'Working')
                self.assertEqual(economy.quantity(town, 'hides'), 2)
                economy.spend(town, {'meat': 8})
                shop['input_buffer'] = {'meat': 8}
                for _ in range(4):
                    self.assertEqual(logistics.produce(town, shop, butcher, 6), 'Working')
                    logistics.drop_off(town, shop, butcher)
                self.assertEqual(economy.quantity(town, 'prepared_meat'), 8)
                self.assertEqual(town['food'], 32)
                self.assertAlmostEqual(butcher['butchery'], 1.4)

    def test_bread_has_a_modest_full_chain_efficiency_advantage_over_pigs(self):
        # Base active work; no travel, water trips, equipment or byproduct credit.
        bread = economy.PROFESSION_RECIPES['bakery'][0]
        flour = bread['inputs']['flour']
        grain_work = (flour * 8 / 10 + flour / 2 * economy.PRODUCTION_SECONDS['flour']
                      + bread['inputs']['wood'] * 3 / economy.WOOD_PER_CYCLE
                      + economy.PRODUCTION_SECONDS['bread'])
        bread_rate = bread['amount'] * economy.EDIBLE['bread'] / grain_work
        meat = economy.ANIMAL_MEAT['pig']
        recipe = economy.PROFESSION_RECIPES['butcher'][0]
        feed = economy.ANIMALS['pig']['feed'] * economy.ANIMAL_GROWTH_SECONDS / 3
        # Four pigs share one keeper's growth work, but each needs butchery.
        pig_work = (feed * 8 / 10 + economy.ANIMAL_GROWTH_SECONDS / 4
                    + economy.BUTCHER_SECONDS + meat / recipe['inputs']['meat']
                    * economy.PRODUCTION_SECONDS['prepared_meat'])
        pig_rate = meat / recipe['inputs']['meat'] * recipe['amount'] * economy.EDIBLE['prepared_meat'] / pig_work
        self.assertGreater(bread_rate, pig_rate)
        self.assertLess(bread_rate / pig_rate, 1.35)
        self.assertGreater(meat * economy.EDIBLE['prepared_meat'], feed)

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
