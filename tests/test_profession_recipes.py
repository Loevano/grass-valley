from fixtures import flat_town
import copy
import json
import unittest

import logistics
import progression as economy
import server


class ProfessionRecipes(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Recipes')
        self.town['level'] = 5
        self.worker = self.town['villagers'][0]
        self.worker['talents'] = {}

    def workplace(self, kind):
        index = len(self.town['buildings'])
        self.town['buildings'].append({'kind': kind, 'x': 4, 'y': 6})
        job = economy.BUILDINGS[kind]['job']
        self.worker.update(economy.JOB_REQUIREMENTS[job])
        server.command(self.town, {'action': 'assign_settler', 'index': 0, 'building_index': index})
        return index, self.town['buildings'][index], job

    def select(self, index, output):
        building = self.town['buildings'][index]
        if building['kind'] == 'workshop':
            for order in list(building.get('craft_queue', [])):
                server.command(self.town, dict(action='cancel_workshop', index=index, request_id=order['id']))
            server.command(self.town, dict(action='queue_workshop', index=index, recipe=output, amount=100))
            return
        server.command(self.town, {'action': 'set_recipe', 'index': index, 'recipe': output})

    def test_third_tier_prerequisites_are_two_personal_practice(self):
        for job, previous in [('crafting', 'woodworking'), ('baking', 'milling'), ('butchery', 'husbandry')]:
            self.assertEqual(economy.JOB_REQUIREMENTS[job], {previous: 2})
            self.assertIsNotNone(economy.qualification({previous: 1.999}, job))
            self.assertIsNone(economy.qualification({previous: 2}, job))
        self.assertEqual(economy.JOB_REQUIREMENTS['woodworking'], {'woodcutting': 10})

    def test_every_recipe_uses_local_inputs_and_records_pending_outputs(self):
        for kind, recipes in economy.PROFESSION_RECIPES.items():
            for spec in recipes:
                with self.subTest(kind=kind, output=spec['output']):
                    self.setUp()
                    index, building, job = self.workplace(kind)
                    self.worker[job] = 0 if spec['level'] == 1 else spec['level']
                    self.select(index, spec['output'])
                    speed = economy.processing_efficiency(self.worker, building)
                    costs = {k: n * speed for k, n in spec['inputs'].items()}
                    amount = spec['amount'] * speed
                    building['input_buffer'] = {k: n * 2 for k, n in costs.items()}
                    self.assertEqual(logistics.requirements(building, self.worker), costs)
                    self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Working')
                    self.assertEqual(building['input_buffer'], costs)
                    self.assertEqual(self.worker['pending_output'], {spec['output']: amount})
                    rate = .1 if job == 'butchery' else .08
                    self.assertAlmostEqual(self.worker[job], (0 if spec['level'] == 1 else spec['level']) + rate * economy.production_seconds(building) / 3)
                    logistics.drop_off(self.town, building, self.worker)
                    self.assertEqual(economy.quantity(self.town, spec['output']), amount)

    def test_bakery_milestones_gate_selection_and_existing_saved_recipes(self):
        for output, level in [('bread_rolls', 5), ('herb_bread', 10),
                              ('fruit_pies', 15), ('cakes', 20)]:
            with self.subTest(output=output):
                self.setUp()
                index, building, job = self.workplace('bakery')
                self.worker[job] = level - .001
                with self.assertRaisesRegex(ValueError, f'level {level} baking'):
                    self.select(index, output)
                building['recipe'] = output  # A previously selected recipe in a save.
                building['input_buffer'] = {'flour': 20, 'wood': 5, 'milk': 10,
                                            'eggs': 10, 'fruit': 10, 'herbs': 10}
                before = copy.deepcopy(self.town)
                self.assertIn(f'Requires level {level}', logistics.produce(
                    self.town, building, self.worker, elapsed=12))
                self.assertEqual(self.town, before)
                self.worker[job] = level
                self.select(index, output)
                self.assertEqual(logistics.produce(
                    self.town, building, self.worker,
                    elapsed=economy.production_seconds(building)), 'Working')

    def test_unlock_uses_assigned_worker_and_production_rechecks_each_worker(self):
        index, building, job = self.workplace('bakery')
        self.worker[job] = 4.999
        self.town['player'][job] = 100
        self.town['villagers'][1][job] = 100
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'assigned worker'):
            self.select(index, 'bread_rolls')
        self.assertEqual(self.town, before)
        self.worker[job] = 5
        self.select(index, 'bread_rolls')
        building['input_buffer'] = {'flour': 4, 'milk': 2, 'wood': 1}
        self.worker[job] = 4.999
        before = copy.deepcopy(self.town)
        self.assertIn('Requires level 5', logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)))
        self.assertEqual(logistics.supply_request(self.town, building, self.worker), {})
        self.assertEqual(self.town, before)
        self.select(index, 'bread')
        self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Working')

    def test_existing_workplaces_default_to_original_recipe(self):
        for kind in economy.PROFESSION_RECIPES:
            self.assertEqual(economy.production_recipe({'kind': kind}), economy.RECIPES[kind])

    def test_selection_rejects_wrong_building_recipe_and_invalid_index(self):
        index, building, _ = self.workplace('workshop')
        for data in [dict(index=index, recipe='bread'), dict(index=-1, recipe='tools'),
                     dict(index=True, recipe='tools'), dict(index=index, recipe=['tools'])]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                server.command(self.town, dict(action='set_recipe', **data))
            self.assertEqual(self.town, before)
        building['construction'] = {'progress': 0, 'total': 30}
        with self.assertRaisesRegex(ValueError, 'construction'):
            self.select(index, 'tools')

    def test_switching_releases_unused_inputs_without_creating_production(self):
        index, building, job = self.workplace('workshop')
        self.worker[job] = 4
        economy.ensure_inventory(self.town)
        building['input_buffer'] = {'planks': 10, 'wood': 5}
        before = {k: self.town.get(k, 0) for k in ('wood', 'harvested')}
        self.select(index, 'furniture')
        self.assertEqual(building['input_buffer'], {'planks': 10})
        self.assertEqual(building['output_buffer'], {'wood': 5})
        self.assertEqual(self.town['wood'], before['wood'] + 5)
        self.assertEqual(self.town['harvested'], before['harvested'])
        self.assertNotIn('wood', self.town.get('produced', {}))
        self.select(index, 'furniture')
        self.assertEqual(building['output_buffer'], {'wood': 5, 'planks': 10})

    def test_new_products_obey_caps_storage_food_and_market_sales(self):
        for kind, recipes in economy.PROFESSION_RECIPES.items():
            for spec in recipes[1:]:
                with self.subTest(output=spec['output']):
                    self.setUp()
                    index, building, job = self.workplace(kind)
                    self.worker[job] = spec['level']
                    self.select(index, spec['output'])
                    speed = economy.processing_efficiency(self.worker, building)
                    amount = spec['amount'] * speed
                    building['input_buffer'] = {k: n * speed for k, n in spec['inputs'].items()}
                    logistics.configure_rules(self.town, {'resource': spec['output'], 'cap': 0})
                    before = copy.deepcopy(self.town)
                    self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Production cap reached')
                    self.assertEqual(self.town, before)
                    logistics.configure_rules(self.town, {'resource': spec['output'], 'cap': None})
                    self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Working')
                    logistics.drop_off(self.town, building, self.worker)
                    storage = {'kind': 'storehouse', 'storage_enabled': {'food': True, spec['output']: True}}
                    self.assertGreater(economy.local_room(storage, spec['output']), 0)
                    self.assertTrue(economy.accepts_storage(building, spec['output']))
                    self.town['buildings'].append({'kind': 'market', 'x': 5, 'y': 6})
                    food, coins = self.town['food'], self.town['coins']
                    server.command(self.town, {'action': 'sell_goods', 'resource': spec['output'], 'amount': 1})
                    self.assertEqual(self.town['coins'], coins + economy.SALE_PRICES[spec['output']])
                    self.assertAlmostEqual(self.town['food'], food - economy.EDIBLE.get(spec['output'], 0))
                    self.assertEqual(economy.quantity(self.town, spec['output']), amount - 1)

    def test_new_recipe_missing_ingredients_and_full_buffer_preserve_inputs(self):
        index, building, job = self.workplace('butcher')
        self.worker[job] = 5
        self.select(index, 'meat_pies')
        building['input_buffer'] = {'meat': 2, 'flour': 2, 'wood': .5}
        before = copy.deepcopy(self.town)
        self.assertIn('eggs', logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)))
        self.assertEqual(self.town, before)
        building['input_buffer']['eggs'] = 1
        building['output_buffer'] = {'prepared_meat': 50, 'meat_pies': 20}
        self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Workplace buffer full')
        self.assertEqual(building['input_buffer']['eggs'], 1)
        self.assertNotIn('pending_output', self.worker)

    def test_recipe_survives_save_and_haulers_supply_and_collect_new_goods(self):
        index, building, job = self.workplace('workshop')
        self.worker[job] = 5
        self.select(index, 'carved_furniture')
        restored = json.loads(json.dumps(self.town))
        self.assertEqual(economy.production_recipe(restored['buildings'][index])[1], 'carved_furniture')
        economy.ensure_inventory(self.town)
        store = {'kind': 'storehouse', 'x': 5, 'y': 6,
                 'stored_goods': {'planks': 5, 'wool': 2},
                 'storage_enabled': {'planks': True, 'wool': True, 'carved_furniture': True}}
        self.town['buildings'].append(store)
        economy.add_stock(self.town, 'planks', 5)
        economy.add_stock(self.town, 'wool', 2)
        hauler = self.town['villagers'][1]
        hauler.update(job='hauling', talents={})
        self.assertEqual(logistics.transfer(self.town, building, store, 'supply', hauler), {'planks': 5, 'wool': 2})
        self.assertEqual(logistics.produce(self.town, building, self.worker, elapsed=economy.production_seconds(building)), 'Working')
        logistics.drop_off(self.town, building, self.worker)
        self.assertEqual(logistics.transfer(self.town, building, store, 'collect', hauler), {'carved_furniture': 1})
        self.assertEqual(store['stored_goods']['carved_furniture'], 1)
        self.assertEqual(economy.quantity(self.town, 'carved_furniture'), 1)
        self.assertAlmostEqual(hauler['hauling'], .16)
