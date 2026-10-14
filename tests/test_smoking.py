import copy
import json
import unittest

from fixtures import flat_town
import logistics
import progression as economy
import server


class Smoking(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Smoking', size=24)
        self.town.update(level=5, goods={}, food=0, wood=0)
        self.smoke = {'kind': 'smokehouse', 'x': 3, 'y': 2}
        self.town['buildings'].extend([self.smoke, {'kind': 'market', 'x': 4, 'y': 2}])
        self.worker = self.town['villagers'][0]
        self.worker.update(job='preserving', fishing=10, preserving=0, talents={}, building_index=1)
        economy.ensure_inventory(self.town)

    def test_one_fish_retains_species_and_gains_food_and_sale_value(self):
        for raw, spec in economy.FISH.items():
            with self.subTest(fish=raw):
                self.setUp()
                output = f'smoked_{raw}'
                self.smoke['input_buffer'] = {raw: 1, 'wood': .25}
                self.assertEqual(logistics.produce(self.town, self.smoke, self.worker), 'Working')
                self.assertEqual(self.smoke['input_buffer'], {raw: 0, 'wood': 0})
                self.assertEqual(self.worker['pending_output'], {output: 1})
                self.assertEqual(self.worker['preserving'], .04)
                self.assertEqual(self.town['food'], 0)
                logistics.drop_off(self.town, self.smoke, self.worker)
                self.assertEqual(self.town['food'], spec['food'] + .5)
                upgrade = next(u for u in economy.HALL_UPGRADES if u['level'] == 5)
                self.town['produced'].update(upgrade.get('production', {}))
                self.town['produced'][output] = upgrade.get('processed_food', 1)
                self.assertTrue(economy.milestone(self.town, 5))
                self.assertEqual(economy.sale_price(output), spec['coins'] + 1)
                self.assertGreater(economy.sale_price(output), spec['coins'] + .25 * economy.sale_price('wood'))
                saved = json.loads(json.dumps(self.town))
                economy.ensure_inventory(saved)
                self.assertEqual(saved['food'], spec['food'] + .5)
                logistics.transfer(saved, saved['buildings'][1], saved['buildings'][0], 'collect', saved['villagers'][0])
                self.assertEqual(saved['buildings'][0]['stored_goods'][output], 1)
                coins = saved['coins']
                server.command(saved, {'action': 'sell_goods', 'resource': output, 'amount': 1})
                self.assertEqual(saved['coins'] - coins, spec['coins'] + 1)
                self.assertEqual(saved['food'], 0)
                self.assertEqual(economy.quantity(saved, output), 0)

    def test_mixed_batch_pauses_atomically_for_each_species_cap_or_storage_limit(self):
        self.worker['talents'] = {'fishing': 1}
        self.smoke['input_buffer'] = {'fish': .5, 'sturgeon': .75, 'wood': .3125}
        for output in ('smoked_fish', 'smoked_sturgeon'):
            for blockage in ('cap', 'storage'):
                with self.subTest(output=output, blockage=blockage):
                    self.town['rules'] = {'production_caps': {output: 0}} if blockage == 'cap' else {}
                    self.smoke['output_buffer'] = {output: 45} if blockage == 'storage' else {}
                    before = copy.deepcopy((self.smoke, self.worker))
                    self.assertEqual(logistics.produce(self.town, self.smoke, self.worker),
                                     'Production cap reached' if blockage == 'cap' else 'Workplace buffer full')
                    self.assertEqual(self.smoke['input_buffer'], before[0]['input_buffer'])
                    self.assertEqual(self.worker, before[1])
        self.town['rules'] = {}
        self.smoke['output_buffer'] = {}
        self.assertEqual(logistics.produce(self.town, self.smoke, self.worker), 'Working')
        self.assertEqual(self.worker['pending_output'], {'smoked_fish': .5, 'smoked_sturgeon': .75})

    def test_existing_smoked_fish_keeps_its_food_value(self):
        self.smoke['output_buffer'] = {'smoked_fish': 2}
        economy.add_stock(self.town, 'smoked_fish', 2)
        saved = json.loads(json.dumps(self.town))
        economy.ensure_inventory(saved)
        economy.eat(saved, .75)
        self.assertEqual(saved['food'], 2.25)
        self.assertEqual(economy.quantity(saved, 'smoked_fish'), 1.5)

    def test_full_smoked_species_waits_for_hauler_without_consuming_inputs(self):
        self.smoke.update(output_buffer={'smoked_sturgeon': 45},
                          input_buffer={'sturgeon': 1, 'wood': .25})
        economy.add_stock(self.town, 'smoked_sturgeon', 45)
        self.assertTrue(logistics.worker(self.town, self.town, self.worker, self.smoke,
                                         server.construction_route, 0))
        self.assertNotIn('logistics_trip', self.worker)
        self.assertEqual(logistics.produce(self.town, self.smoke, self.worker), 'Workplace buffer full')
        self.assertEqual(self.smoke['input_buffer'], {'sturgeon': 1, 'wood': .25})
        self.assertEqual(self.smoke['output_buffer'], {'smoked_sturgeon': 45})


if __name__ == '__main__':
    unittest.main()
