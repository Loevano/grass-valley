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

    def test_young_animals_do_not_produce_and_cannot_breed(self):
        self.buy('sheep')
        self.buy('sheep')
        self.tick()
        self.assertNotIn('wool', self.barn.get('pending_output', {}))
        with self.assertRaisesRegex(ValueError, 'fully grown'):
            server.command(self.town, {'action': 'breed_animal', 'index': 1, 'animal': 'sheep'})
        self.barn['animal_growth']['sheep'] = [300, 300]
        self.tick()
        self.assertEqual(self.barn['pending_output']['wool'], .5)
        self.town.setdefault('goods', {}).update(fodder=3, herbs=1)
        server.command(self.town, {'action': 'breed_animal', 'index': 1, 'animal': 'sheep'})
        self.assertEqual(self.barn['animal_growth']['sheep'], [300, 300, 0])

    def test_selection_only_consumes_one_adult_and_rejects_invalid_requests(self):
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
        self.assertEqual(self.barn['animals']['pig'], 1)
        self.assertEqual(self.barn['animal_growth']['pig'], [10])
        self.assertEqual(self.shop['butchering']['remaining'], 60)
        before = copy.deepcopy(self.town)
        with self.assertRaisesRegex(ValueError, 'current animal'):
            self.use()
        self.assertEqual(self.town, before)

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
        self.assertEqual(self.barn['animals']['pig'], 0)
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
        self.assertEqual(self.barn['animals']['pig'], 0)


if __name__ == '__main__':
    unittest.main()
