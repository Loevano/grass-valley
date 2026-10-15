import copy
import json
import unittest
from unittest.mock import patch

from fixtures import flat_town
import logistics
import progression as economy
import server


class AnimalGrowth(unittest.TestCase):
    def setUp(self):
        self.town = flat_town('Animals')
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

    def test_growth_without_feed_workers_or_animation(self):
        self.buy()
        for worker in self.town['villagers']:
            worker['job'] = 'idle'
        self.barn['input_buffer'] = {}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [3])
        self.assertEqual(logistics.requirements(self.barn, self.worker), {})

    def test_species_timers_restart_automatically_and_yield_meat(self):
        for animal, duration in [('chicken', 60), ('sheep', 120), ('pig', 240), ('cow', 360)]:
            with self.subTest(animal=animal):
                self.setUp()
                self.buy(animal)
                self.assertEqual(economy.animal_duration(animal), duration)
                self.barn['animal_growth'][animal] = [duration - 3]
                self.worker['job'] = 'idle'
                self.tick()
                self.assertEqual(self.barn['animal_growth'][animal], [0])
                self.assertEqual(self.barn['animals'], {animal: 1})
                self.assertEqual(self.barn['output_buffer']['meat'], economy.ANIMAL_MEAT[animal])
                self.tick()
                self.assertEqual(self.barn['animal_growth'][animal], [3])

    def test_each_assigned_keeper_adds_base_speed_without_duplicate_output(self):
        self.buy('sheep')
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [6])
        self.assertEqual(self.barn['output_buffer']['wool'], 1.25)
        self.town['villagers'][2].update(job='husbandry', building_index=1)
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [15])
        self.assertEqual(self.barn['output_buffer']['wool'], 3.125)
        self.assertEqual(self.barn['input_buffer']['fodder'], 40)
        self.town['villagers'][2]['construction_target'] = {'kind': 'road', 'x': 10, 'y': 10}
        with patch('server.construct'):
            self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [21])

    def test_counts_are_free_preserve_growth_and_respect_capacity_and_unlocks(self):
        def set_count(animal, count, **changes):
            server.command(self.town, dict(action='set_animal_count', index=1,
                                            animal=animal, count=count, **changes))
        self.town['coins'] = 0
        set_count('chicken', 2)
        self.barn['animal_growth']['chicken'] = [12, 15]
        set_count('chicken', 3)
        self.assertEqual(self.barn['animal_growth']['chicken'], [12, 15, 0])
        set_count('pig', 1)
        for animal, count in [('cow', 1), ('pig', True), ('pig', -1), ('pig', 1.5),
                              ('horse', 0), ('unknown', 0), ('pig', '1')]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                set_count(animal, count)
            self.assertEqual(self.town, before)
        set_count('chicken', 1)
        self.assertEqual(self.barn['animal_growth']['chicken'], [12])
        set_count('pig', 0)
        self.assertEqual(self.barn['animal_growth']['pig'], [])
        self.assertEqual(self.town['coins'], 0)
        self.town['level'] = 2
        with self.assertRaisesRegex(ValueError, 'unlocked'):
            set_count('sheep', 1)
        with self.assertRaises(ValueError):
            server.command(
                self.town, dict(action='set_animal_count', index=2, animal='chicken', count=1))

    def test_fixed_products_and_overflow_across_multiple_cycles_survive_save(self):
        for animal, amount in economy.ANIMAL_PRODUCTS.items():
            with self.subTest(animal=animal):
                self.setUp()
                self.buy(animal)
                duration = economy.animal_duration(animal)
                for _ in range(duration // 3):
                    logistics.advance_barn(self.town, self.barn, 3)
                    self.barn['output_buffer'] = {}  # Hauling leaves room for the next tick.
                self.assertAlmostEqual(self.town['produced'][economy.ANIMALS[animal]['output']], amount)
                self.assertEqual(self.barn['animal_growth'][animal], [0])
                saved = json.loads(json.dumps(self.town))
                restored = saved['buildings'][1]
                self.barn['output_buffer'] = {}
                restored['output_buffer'] = {}
                logistics.advance_barn(self.town, self.barn, duration * 2 + 5)
                logistics.advance_barn(saved, restored, duration * 2 + 5)
                self.assertEqual(self.barn, restored)
                self.assertEqual(self.barn['animal_growth'][animal], [5])
                self.assertEqual(self.barn['output_buffer']['meat'], 2 * economy.ANIMAL_MEAT[animal])

    def test_storage_and_caps_limit_goods_but_never_growth(self):
        self.buy('sheep')
        self.worker['job'] = 'idle'
        self.town['rules'] = {'production_caps': {'wool': 0, 'meat': 0, 'hides': 0, 'manure': 0}}
        self.barn['animal_growth']['sheep'] = [119]
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [2])
        self.assertFalse(self.barn.get('output_buffer'))
        self.town['rules']['production_caps'].clear()
        self.barn['output_buffer'] = {'wool': 10}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['sheep'], [5])
        self.assertEqual(self.barn['output_buffer']['wool'], 10)
        self.barn['output_buffer']['wool'] = 9.75
        self.tick()
        self.assertEqual(self.barn['output_buffer']['wool'], 10)
        self.assertEqual(self.barn['animal_growth']['sheep'], [8])

    def test_initial_construction_stops_growth_but_upgrading_keeps_base_growth(self):
        self.buy('pig')
        self.barn['construction'] = {'total': 100, 'progress': 0}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [0])
        self.barn['construction']['upgrade'] = {'level': 2}
        self.tick()
        self.assertEqual(self.barn['animal_growth']['pig'], [3])

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
                self.assertEqual(self.barn['animal_growth'][animal], [0] + [economy.animal_duration(animal)] * 3)
                saved = json.loads(json.dumps(self.town))
                self.assertEqual(saved['buildings'][1]['animal_growth'][animal], [0] + [economy.animal_duration(animal)] * 3)
                self.assertEqual(saved['buildings'][2]['butchering']['animal'], animal)
                self.tick()
                self.assertEqual(self.barn['animal_growth'][animal], [6] * 4)
                if animal in economy.ANIMAL_PRODUCTS:
                    output = economy.ANIMALS[animal]['output']
                    self.assertGreater(self.barn['output_buffer'][output], 0)
                else:
                    self.assertGreater(self.barn['output_buffer']['meat'], 0)

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
        self.assertEqual(self.barn['animal_growth']['pig'], [60])
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
        self.assertGreater(self.barn['animal_growth']['pig'][0], 0)


if __name__ == '__main__':
    unittest.main()
