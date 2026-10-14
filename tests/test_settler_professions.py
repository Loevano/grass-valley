import copy
import json
import unittest
from unittest.mock import patch

import progression as economy
import server


class SettlerProfessions(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Talents')
        self.town.update(terrain=['g' * 120] * 120, npcs=[], food=40, level=5)
        self.worker = self.town['villagers'][0]
        self.worker['talents'] = {}

    def workplace(self, kind):
        self.town['buildings'].append({'kind': kind, 'x': 4, 'y': 6})
        return len(self.town['buildings']) - 1

    def assign(self, index):
        server.command(self.town, {'action': 'assign_settler', 'index': 0,
                                  'building_index': index})

    def test_all_advanced_roles_require_individual_practice_through_both_controls(self):
        for job, requirements in economy.JOB_REQUIREMENTS.items():
            with self.subTest(job=job):
                self.setUp()
                index = self.workplace(economy.JOB_BUILDINGS[job])
                self.town['player'].update(requirements)
                self.town['villagers'][1].update(requirements)
                for action in ({'action': 'job', 'index': 0, 'job': job},
                               {'action': 'assign_settler', 'index': 0, 'building_index': index}):
                    before = copy.deepcopy(self.town)
                    with self.assertRaisesRegex(ValueError, 'Requires'):
                        server.command(self.town, action)
                    self.assertEqual(self.town, before)
                self.worker.update(requirements)
                self.assign(index)
                self.assertEqual(self.worker['job'], job)

    @patch('server.secrets.randbelow', return_value=1)
    def test_fishing_talent_affects_catch_and_training_even_when_unlucky(self, roll):
        self.town['map_resources'] = [{'kind': 'water', 'x': 4, 'y': 7}]
        index = self.workplace('fishing_hut')
        self.assign(index)
        for talent, output, practice in [(1, 1.25, .05), (-1, .75, .03)]:
            self.worker.update(fishing=0, talents={'fishing': talent})
            self.town.update(food=10, goods={})
            server.tick({'towns': {'a': self.town}})
            self.assertAlmostEqual(self.town['produced']['fish'], output)
            self.assertEqual(self.worker['fishing'], practice)
            self.town['produced'] = {}
        roll.return_value = 0
        before = self.worker['fishing']
        server.tick({'towns': {'a': self.town}})
        self.assertEqual(self.worker['fishing'], before + .03)
        self.assertEqual(self.town['produced'].get('fish', 0), 0)

    def test_bakery_and_butcher_batches_obey_inputs_storage_and_upkeep(self):
        for kind, job, prerequisite, inputs, output in [
            ('bakery', 'baking', 'milling', {'flour': 10}, 'bread'),
            ('butcher', 'butchery', 'husbandry', {'meat': 10}, 'prepared_meat'),
        ]:
            with self.subTest(kind=kind):
                self.setUp()
                self.worker.update({prerequisite: 10, job: 20})
                index = self.workplace(kind)
                self.assign(index)
                self.town.update(goods=dict(inputs), wood=10, food=40)
                building = self.town['buildings'][index]
                self.assertEqual(economy.extra_worker(self.town, self.worker, building, 100), 'Working')
                self.assertEqual(self.town['goods'][output], 4)
                input_key = next(iter(inputs))
                self.assertEqual(self.town['goods'][input_key], 6)
                self.assertEqual(self.worker[job], 20.1 if job == 'butchery' else 20.08)
                self.town['goods'][output] = 100
                before = copy.deepcopy(self.town)
                self.assertEqual(economy.extra_worker(self.town, self.worker, building, 100), 'Storage full')
                self.assertEqual(self.town, before)
                self.town['goods'][output] = 0
                self.town['goods'][input_key] = 0
                before = copy.deepcopy(self.town)
                self.assertTrue(economy.extra_worker(self.town, self.worker, building, 100).startswith('Waiting'))
                self.assertEqual(self.town, before)

    def test_legacy_unqualified_worker_pauses_without_consuming_materials(self):
        index = self.workplace('fish_pond')
        self.worker.update(job='breeding', building_index=index)
        self.town.update(goods={'fodder': 10})
        server.tick({'towns': {'a': self.town}})
        self.assertIn('Requires 20 fishing', self.worker['production_status'])
        self.assertEqual(self.town['goods']['fodder'], 10)
        self.assertNotIn('breeding', self.worker)
        self.worker['fishing'] = 20
        self.town['buildings'][index]['input_buffer'] = {'fodder': 10}
        server.tick({'towns': {'a': self.town}})
        self.assertEqual(self.worker['breeding'], .04)

    def test_talents_are_distinct_and_survive_roundtrip_without_reassignment(self):
        profiles = [v['talents'] for v in self.town['villagers'][1:]]
        self.assertEqual(len({json.dumps(p, sort_keys=True) for p in profiles}), 4)
        restored = json.loads(json.dumps(self.town))
        for index, villager in enumerate(restored['villagers']):
            before = copy.deepcopy(villager)
            economy.ensure_talents(villager, index + 1)
            self.assertEqual(villager, before)
        old = {'name': 'Old settler', 'fishing': 35}
        economy.ensure_talents(old, 2)
        self.assertEqual(old['fishing'], 35)
        self.assertEqual(old['talents']['fishing'], 1)

    def test_farming_expertise_improves_a_field_without_duplicate_output(self):
        index = self.workplace('farm')
        self.town['buildings'][index]['settler_level'] = 2
        self.town['map_resources'] = [{'kind': 'water', 'x': 4, 'y': 7}]
        self.worker.update(farming=20, talents={'farming': 1})
        self.assign(index)
        self.town['villagers'][1].update(farming=0, talents={})
        server.command(self.town, {'action': 'assign_settler', 'index': 1,
                                  'building_index': index})
        farm = self.town['buildings'][index]
        farm['field'] = {'stage': 'ripe', 'crop': 'vegetables', 'elapsed': 0, 'progress': 0, 'cycle': 0}
        self.worker['construction_position'] = {'x': 3, 'y': 6}
        server.tick({'towns': {'a': self.town}})
        self.assertEqual(self.town['produced']['vegetables'] + farm['field'].get('yield_remaining', 0), 15)
        # The larger farm buffer lets this worker use the remaining time to plow.
        self.assertEqual(self.worker['farming'], 20.039)
        # A second tender can start the next cycle in the remaining time, but
        # cannot repeat the completed harvest action or increase its yield.
        samples = self.town['villagers'][1]['work_animation']['playback']
        self.assertEqual(sum(b['time'] - a['time'] for a, b in zip(samples, samples[1:])
                             if a['phase'] == 'working' and a.get('farmAction') == 'harvesting'), 0)
        self.assertEqual(farm['field']['cycle'], 1)

    def test_new_foods_can_be_sold_without_double_counting_the_pantry(self):
        self.workplace('market')
        for output in ('bread', 'prepared_meat'):
            self.town.update(goods={output: 2}, food=2 * economy.EDIBLE[output], coins=0)
            server.command(self.town, {'action': 'sell_goods', 'resource': output, 'amount': 1})
            self.assertEqual(self.town['goods'][output], 1)
            self.assertEqual(self.town['food'], economy.EDIBLE[output])
            self.assertEqual(self.town['coins'], economy.sale_price(output))

    def test_new_workplaces_charge_materials_and_start_construction(self):
        for kind, wood in [('bakery', 30), ('butcher', 25)]:
            self.setUp()
            self.town.update(wood=100, goods={'planks': 10})
            server.command(self.town, {'action': 'build', 'kind': kind, 'x': 4, 'y': 6})
            self.assertEqual(self.town['wood'], 100 - wood)
            self.assertEqual(self.town['goods']['planks'], 5)
            self.assertIn('construction', self.town['buildings'][-1])


if __name__ == '__main__':
    unittest.main()
