import copy
import json
import unittest
from unittest.mock import patch

import logistics
import progression as economy
import server


class AnimalGrowth(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Animals')
        self.town.update(level=5, coins=100, terrain=['g' * 120] * 120,
                         npcs=[], map_resources=[], forage_version=1, forage_halls=[[5, 5]])
        self.barn = {'kind': 'barn', 'x': 3, 'y': 6, 'input_buffer': {'fodder': 40}}
        self.shop = {'kind': 'butcher', 'x': 8, 'y': 6}
        self.town['buildings'].extend([self.barn, self.shop])
        for v in self.town['villagers']:
            v['talents'] = {}
        self.worker = self.town['villagers'][0]
        self.worker.update(job='husbandry', building_index=1)
        self.butcher = self.town['villagers'][1]
        self.butcher.update(job='butchery', building_index=2, husbandry=2,
                            work_animation={'phase': 'working'})

    def buy(self, animal='pig'):
        server.command(self.town, {'action': 'buy_animal', 'index': 1, 'animal': animal})

    def use(self, **changes):
        server.command(self.town, {'action': 'butcher_animal', 'index': 2,
                                  'barn_index': 1, 'animal': 'pig', **changes})

    def tick(self):
        with patch('work_cycles.advance', return_value=False), patch('logistics.worker', return_value=False):
            server.tick({'towns': {'a': self.town}})

    def test_full_rations_grow_once_per_tick_and_short_rations_pause(self):
        self.buy()
        second = self.town['villagers'][2]
        second.update(job='husbandry', building_index=1)
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [3])
        self.assertAlmostEqual(self.barn['input_buffer']['fodder'], 39.6)
        self.barn['input_buffer'] = {'fodder': .39}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [3])
        self.assertAlmostEqual(self.barn['input_buffer']['fodder'], .39)
        self.barn['input_buffer'] = {'vegetables': .4}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [6])
        self.assertEqual(self.barn['input_buffer']['vegetables'], 0)

    def test_maturity_requires_five_minutes_and_pigs_never_produce_recurring_meat(self):
        self.buy()
        self.barn['animal_growth']['pig'] = [294]
        self.tick()
        self.assertEqual(economy.mature_animals(self.barn, 'pig'), 0)
        self.tick()
        self.assertEqual(economy.mature_animals(self.barn, 'pig'), 1)
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [300])
        self.assertEqual(economy.quantity(self.town, 'meat'), 0)
        self.assertNotIn('meat', self.barn.get('pending_output', {}))

    def test_farm_animals_are_free_and_cannot_breed(self):
        self.town['coins'] = 0
        for animal in economy.ANIMAL_MEAT:
            self.buy(animal)
        self.assertEqual(self.town['coins'], 0)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'free and replaced automatically'):
            server.command(self.town, {'action': 'breed_animal', 'index': 1, 'animal': 'sheep'})
        self.assertEqual(self.town, before)

    def test_products_are_fixed_per_growth_cycle_regardless_of_skill_and_talent(self):
        for animal, amount in economy.ANIMAL_PRODUCTS.items():
            for skill, talents in ((0, {}), (100, {'husbandry': 1})):
                with self.subTest(animal=animal, skill=skill, talents=talents):
                    self.setUp()
                    self.buy(animal)
                    self.worker.update(husbandry=skill, talents=talents)
                    self.barn['input_buffer'] = {'fodder': 100}
                    output = economy.ANIMALS[animal]['output']
                    for _ in range(100):
                        self.assertEqual(logistics.produce(self.town, self.barn, self.worker), 'Working')
                        economy.grow_animals(self.barn, 3)
                        # A hauler clears the output between cycles.
                        self.barn['pending_output'] = {}
                        self.worker.pop('pending_output', None)
                    self.assertAlmostEqual(self.town['produced'][output], amount)
                    self.assertAlmostEqual(self.barn['input_buffer']['fodder'],
                                           100 - 100 * economy.ANIMALS[animal]['feed'])
                    self.assertEqual(self.barn['animal_growth'][animal], [300])
                    before = copy.deepcopy(self.barn)
                    practice = self.worker['husbandry']
                    self.assertIn('ready for the butcher', logistics.produce(self.town, self.barn, self.worker))
                    self.assertEqual(self.barn, before)
                    self.assertEqual(self.worker['husbandry'], practice)
                    self.assertEqual(logistics.requirements(self.barn, self.worker), {})

    def test_young_animals_produce_but_adults_do_not_use_feed(self):
        self.buy('sheep')
        self.buy('sheep')
        self.barn['animal_growth']['sheep'] = [0, 300]
        self.tick()
        self.assertEqual(self.barn['pending_output']['wool'], .25)
        self.assertAlmostEqual(self.barn['input_buffer']['fodder'], 40 - economy.ANIMALS['sheep']['feed'])
        self.assertEqual(self.barn['animal_growth']['sheep'], [3, 300])
        self.barn['input_buffer'] = {}
        self.tick()
        self.assertEqual(self.barn['pending_output']['wool'], .25)
        self.assertEqual(self.barn['animal_growth']['sheep'], [3, 300])

    def test_blocked_outputs_pause_feed_and_growth_and_final_partial_cycle_is_bounded(self):
        self.buy('sheep')
        self.barn['animal_growth']['sheep'] = [299]
        self.town['rules'] = {'production_caps': {'wool': 0}}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [299])
        self.assertEqual(self.barn['input_buffer']['fodder'], 40)
        self.town['rules']['production_caps'].clear()
        self.barn['output_buffer'] = {'wool': 50}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [299])
        self.assertEqual(self.barn['input_buffer']['fodder'], 40)
        self.barn['output_buffer'].clear()
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [300])
        self.assertAlmostEqual(self.barn['pending_output']['wool'], 25 / 300)

    def test_selection_replaces_only_one_adult_and_rejects_invalid_requests(self):
        self.buy()
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'fully grown'):
            self.use()
        self.assertEqual(self.town, before)
        self.buy()
        self.barn['animal_growth']['pig'] = [10, 300]
        for changes in ({'barn_index': True}, {'barn_index': -1}, {'barn_index': 2},
                        {'animal': 'unknown'}, {'index': 1}):
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.use(**changes)
            self.assertEqual(self.town, before)
        self.use()
        self.assertEqual(self.barn['animals']['pig'], 2)
        self.assertEqual(self.barn['animal_growth']['pig'], [10, 0])
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'current animal'):
            self.use()
        self.assertEqual(self.town, before)

    def test_full_barn_gets_one_young_replacement_of_the_same_species_and_resumes_growth(self):
        for animal in economy.ANIMAL_MEAT:
            with self.subTest(animal=animal):
                self.setUp()
                self.barn['animals'] = {animal: 4}
                self.use(animal=animal)
                self.assertEqual(self.barn['animals'], {animal: 4})
                self.assertEqual(self.barn['animal_growth'][animal], [0, 300, 300, 300])
                saved = json.loads(json.dumps(self.town))
                self.assertEqual(saved['buildings'][1]['animal_growth'][animal], [0, 300, 300, 300])
                self.assertEqual(saved['buildings'][2]['butchering']['animal'], animal)
                self.tick()
                self.assertEqual(self.barn['animal_growth'][animal], [3, 300, 300, 300])
                if animal in economy.ANIMAL_PRODUCTS:
                    output = economy.ANIMALS[animal]['output']
                    self.assertEqual(self.barn['pending_output'][output], .25)
                else:
                    self.assertNotIn('meat', self.barn.get('pending_output', {}))

    def test_processing_takes_one_minute_shared_by_workers_and_survives_save(self):
        self.barn['animals'] = {'pig': 1}  # Older saves retain adult animals.
        self.shop['recipe'] = 'meat_pies'  # Raw butchery needs only base qualification.
        self.use()
        self.worker.update(job='butchery', building_index=2, husbandry=2,
                           work_animation={'phase': 'working'})
        for _ in range(19):
            self.tick()
        self.assertEqual(self.shop['butchering']['remaining'], 3)
        self.assertEqual(economy.quantity(self.town, 'meat'), 0)
        self.assertEqual(json.loads(json.dumps(self.town))['buildings'][2]['butchering']['remaining'], 3)
        self.tick()
        self.assertNotIn('butchering', self.shop)
        self.assertEqual(self.shop['output_buffer']['meat'], 8)
        self.assertEqual(economy.quantity(self.town, 'meat'), 8)
        self.assertEqual(self.barn['animals']['pig'], 1)
        self.assertEqual(self.barn['animal_growth']['pig'], [0])
        self.tick()
        self.assertEqual(economy.quantity(self.town, 'meat'), 8)

    def test_processing_pauses_without_worker_and_preserves_meat_when_capped_or_full(self):
        self.barn['animals'] = {'cow': 1}
        self.use(animal='cow')
        self.butcher['job'] = 'idle'
        self.tick()
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        self.butcher['job'] = 'butchery'
        self.butcher['work_animation'] = {'phase': 'working'}
        self.town['rules'] = {'production_caps': {'meat': 0}}
        for _ in range(20):
            self.tick()
        self.assertEqual(self.shop['butchering']['remaining'], 0)
        self.assertEqual(economy.quantity(self.town, 'meat'), 0)
        self.town['rules']['production_caps'].clear()
        self.shop['output_buffer'] = {'meat': 45}
        self.tick()
        self.assertIn('butchering', self.shop)
        self.shop['output_buffer'].clear()
        self.tick()
        self.assertEqual(self.shop['output_buffer']['meat'], 10)

    def test_unqualified_butcher_cannot_consume_animal(self):
        self.barn['animals'] = {'pig': 1}
        self.butcher['husbandry'] = 0
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'qualified butcher'):
            self.use()
        self.assertEqual(self.town, before)

    def test_real_worker_arrives_and_finishes_processing_at_shop(self):
        self.barn['animals'] = {'pig': 1}
        self.worker['job'] = 'idle'
        self.butcher.pop('work_animation', None)
        self.use()
        world = {'towns': {'a': self.town}}
        for _ in range(40):
            server.tick(world)
            if 'butchering' not in self.shop:
                break
        self.assertNotIn('butchering', self.shop)
        self.assertEqual(economy.quantity(self.town, 'meat'), 8)
        self.assertEqual(self.barn['animals']['pig'], 1)
        self.assertEqual(self.barn['animal_growth']['pig'], [0])


if __name__ == '__main__':
    unittest.main()
