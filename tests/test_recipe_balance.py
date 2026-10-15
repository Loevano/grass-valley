import copy
import json
import unittest

import logistics
import progression as economy
import server


class CraftingSaleBalance(unittest.TestCase):
    def test_baking_unlocks_preserve_food_values_and_sale_prices(self):
        for output, food, coins in [('bread', 1.5, 1), ('bread_rolls', 1.75, 2),
                                    ('herb_bread', 2, 2), ('fruit_pies', 3.5, 4),
                                    ('cakes', 4.5, 5)]:
            with self.subTest(output=output):
                self.assertEqual(economy.EDIBLE[output], food)
                self.assertEqual(economy.sale_price(output), coins)

    def test_advanced_recipes_sell_for_more_than_all_inputs_including_fuel(self):
        recipes = [(r['inputs'], r['output'], r['amount'])
                   for rs in economy.PROFESSION_RECIPES.values() for r in rs
                   if r['level'] > 1]
        recipes.extend(economy.RECIPES[kind] for kind in
                       ('tannery', 'smelter', 'saddler', 'steelworks', 'tradehouse'))
        for inputs, output, amount in recipes:
            with self.subTest(output=output):
                ingredient_value = sum(count * economy.sale_price(key)
                                       for key, count in inputs.items())
                self.assertGreater(amount * economy.sale_price(output), ingredient_value)

    def test_higher_workshop_tiers_preserve_or_improve_profit_per_work_second(self):
        previous_profit = 0
        for recipe in economy.PROFESSION_RECIPES['workshop'][:5]:
            with self.subTest(output=recipe['output']):
                ingredient_value = sum(count * economy.sale_price(key)
                                       for key, count in recipe['inputs'].items())
                profit = (recipe['amount'] * economy.sale_price(recipe['output'])
                          - ingredient_value) / economy.PRODUCTION_SECONDS[recipe['output']]
                self.assertGreaterEqual(profit, previous_profit)
                previous_profit = profit


class RecipeBalance(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Balance')
        economy.ensure_inventory(self.town)
        self.building = {'kind': 'bakery', 'x': 4, 'y': 6,
                         'input_buffer': {'flour': 10, 'wood': 5}}
        self.town['buildings'].append(self.building)
        self.worker = self.town['villagers'][0]
        self.worker.update(job='baking', milling=2, baking=5, talents={},
                           building_index=len(self.town['buildings']) - 1)

    def test_bread_only_consumes_inputs_and_awards_output_and_xp_at_completion(self):
        inputs = copy.deepcopy(self.building['input_buffer'])
        self.assertIn('3s remaining', logistics.produce(self.town, self.building, self.worker))
        self.assertEqual(self.building['input_buffer'], inputs)
        self.assertNotIn('pending_output', self.worker)
        self.assertEqual(self.worker['baking'], 5)
        self.assertEqual(logistics.produce(self.town, self.building, self.worker), 'Working')
        self.assertEqual(self.building['input_buffer'], {'flour': 8, 'wood': 4.5})
        self.assertEqual(self.worker['pending_output'], {'bread': 2})
        self.assertEqual(self.worker['baking'], 5.16)
        self.assertNotIn('production_work', self.worker)

    def test_blocked_time_does_not_advance_timer_or_grant_practice(self):
        logistics.produce(self.town, self.building, self.worker)
        for blockage in ('inputs', 'buffer', 'cap'):
            with self.subTest(blockage=blockage):
                building = copy.deepcopy(self.building)
                worker = copy.deepcopy(self.worker)
                if blockage == 'inputs':
                    building['input_buffer']['flour'] = 0
                elif blockage == 'buffer':
                    building['output_buffer'] = {'bread': 50}
                else:
                    logistics.configure_rules(self.town, {'resource': 'bread', 'cap': 0})
                before = copy.deepcopy(worker)
                self.assertNotEqual(logistics.produce(self.town, building, worker), 'Working')
                self.assertEqual(worker, before)
                logistics.configure_rules(self.town, {'resource': 'bread', 'cap': None})
        self.assertEqual(logistics.produce(self.town, self.building, self.worker), 'Working')
        self.assertEqual(self.worker['baking'], 5.16)

    def test_partial_batch_survives_save_and_two_workers_have_independent_timers(self):
        other = copy.deepcopy(self.worker)
        logistics.produce(self.town, self.building, self.worker)
        self.assertIn('3s remaining', logistics.produce(self.town, self.building, other))
        restored = json.loads(json.dumps(self.town))
        building, worker = restored['buildings'][-1], restored['villagers'][0]
        self.assertEqual(logistics.produce(restored, building, worker), 'Working')
        self.assertEqual(worker['baking'], 5.16)
        self.assertEqual(other['baking'], 5)

    def test_recipe_switch_discards_partial_batch_without_consuming_inputs(self):
        logistics.produce(self.town, self.building, self.worker)
        self.town['level'] = 5
        self.worker['baking'] = 20
        server.command(self.town, {'action': 'set_recipe',
                                  'index': self.worker['building_index'], 'recipe': 'cakes'})
        self.building['input_buffer'].update(eggs=4, milk=2)
        for remaining in (9, 6, 3):
            self.assertIn(f'{remaining}s remaining', logistics.produce(self.town, self.building, self.worker))
        self.assertEqual(logistics.produce(self.town, self.building, self.worker), 'Working')
        self.assertEqual(self.worker['baking'], 20.32)
        self.assertEqual(self.worker['pending_output'], {'cakes': 4})

    def test_complex_foods_reward_processing_over_eating_ingredients_separately(self):
        for key in ('fruit_pies', 'cakes', 'meat_pies'):
            recipes = [r for rs in economy.PROFESSION_RECIPES.values() for r in rs]
            recipe = next(r for r in recipes if r['output'] == key)
            food = recipe['amount'] * economy.EDIBLE[key]
            # Compare with processing flour into bread and meat into prepared meat.
            alternative = sum(amount * (economy.EDIBLE['bread'] if ingredient == 'flour'
                                        else economy.EDIBLE['prepared_meat'] if ingredient == 'meat'
                                        else economy.EDIBLE.get(ingredient, 0))
                              for ingredient, amount in recipe['inputs'].items())
            self.assertGreater(food, alternative)
            self.assertGreater(economy.SALE_PRICES[key], economy.SALE_PRICES['bread'])

    def test_talent_scales_batch_inputs_output_and_time_based_practice_together(self):
        self.worker['talents'] = {'farming': 1}
        self.assertIn('3s remaining', logistics.produce(self.town, self.building, self.worker))
        self.assertEqual(logistics.produce(self.town, self.building, self.worker), 'Working')
        self.assertEqual(self.worker['pending_output'], {'bread': 2.5})
        self.assertEqual(self.building['input_buffer'], {'flour': 7.5, 'wood': 4.375})
        self.assertEqual(self.worker['baking'], 5.2)

    def test_reassignment_cancels_unfinished_work(self):
        logistics.produce(self.town, self.building, self.worker)
        server.assign_settler(self.town, 0, self.worker['building_index'])
        self.assertNotIn('production_work', self.worker)
        self.assertIn('3s remaining', logistics.produce(self.town, self.building, self.worker))

    def test_existing_pantry_is_revalued_once_without_counting_reserved_goods(self):
        self.town.pop('food_balance_version')
        self.town['goods'] = {'fruit_pies': 2, 'cakes': 3, 'meat_pies': 1}
        self.town['food'] = 24.5  # Includes ten legacy untyped food.
        self.building['input_buffer']['cakes'] = 20
        self.building['pending_output'] = {'cakes': 10}
        economy.ensure_inventory(self.town)
        self.assertEqual(self.town['food'], 37.5)
        economy.ensure_inventory(self.town)
        self.assertEqual(self.town['food'], 37.5)

    def test_fractional_meal_does_not_withdraw_negative_food(self):
        hall = self.town['buildings'][0]
        hall['stored_goods'] = {'bread': 1, 'bread_rolls': 1}
        self.town['goods'] = dict(hall['stored_goods'])
        self.town['food'] = sum(amount * economy.EDIBLE[key]
                                for key, amount in self.town['goods'].items())
        food = self.town['food']

        # Converting this serving back from bread overshoots by floating-point rounding.
        economy.eat(self.town, .23)

        self.assertAlmostEqual(self.town['food'], food - .23)
        self.assertAlmostEqual(self.town['goods']['bread'], 1 - .23 / 1.5)
        self.assertAlmostEqual(hall['stored_goods']['bread'], self.town['goods']['bread'])
        self.assertEqual(self.town['goods']['bread_rolls'], 1)
        self.assertEqual(hall['stored_goods']['bread_rolls'], 1)
        self.assertEqual(set(hall['stored_goods']), {'bread', 'bread_rolls'})


if __name__ == '__main__':
    unittest.main()
