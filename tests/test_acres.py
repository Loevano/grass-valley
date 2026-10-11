import copy
import json
import unittest

from server import construct, command, new_town, tick, work_source


class FarmAcres(unittest.TestCase):
    def setUp(self):
        self.town = new_town('Fields')
        command(self.town, {'action': 'build', 'kind': 'farm', 'x': 6, 'y': 2})
        self.finish_construction()

    def finish_construction(self):
        for _ in range(100):
            if not any('construction' in b for b in self.town['buildings']):
                return
            construct(self.town, self.town)
        self.fail('Construction did not finish')

    def build(self, **changes):
        data = {'action': 'build', 'kind': 'acre', 'farm_index': 1, 'x': 6, 'y': 3}
        command(self.town, {**data, **changes})
        self.finish_construction()

    def test_build_save_and_produce_food_at_acre(self):
        wood = self.town['wood']
        self.build()
        self.assertEqual(self.town['wood'], wood - 5)
        acre = self.town['buildings'][-1]
        self.assertEqual(acre, {'kind': 'acre', 'farm_index': 1, 'x': 6, 'y': 3})
        self.town = json.loads(json.dumps(self.town))
        command(self.town, {'action': 'job', 'index': 0, 'job': 'farming'})
        food = self.town['food']
        tick({'towns': {'a': self.town}})
        self.assertAlmostEqual(self.town['food'], food + 1 - .78)
        self.assertEqual(self.town['buildings'][1]['output_buffer']['vegetables'], 1)
        self.assertEqual(self.town['villagers'][0]['work_target'], acre)
        self.assertEqual(self.town['villagers'][0]['farming'], 1.04)

    def test_invalid_acres_do_not_spend_wood(self):
        for change in [{'farm_index': None}, {'farm_index': True}, {'farm_index': 0},
                       {'farm_index': 99}, {'x': 2, 'y': 2}, {'x': 5, 'y': 5},
                       {'x': 9, 'y': 2}, {'x': -1}, {'x': 120}]:
            before = copy.deepcopy(self.town)
            with self.assertRaises(ValueError):
                self.build(**change)
            self.assertEqual(before, self.town)

    def test_acres_follow_their_own_farm_without_nearby_water(self):
        self.build()
        command(self.town, {'action': 'build', 'kind': 'farm', 'x': 7, 'y': 3})
        self.finish_construction()
        self.assertEqual(work_source(self.town, 'farming', 0)['kind'], 'acre')
        self.assertEqual(work_source(self.town, 'farming', 1)['kind'], 'farm')
        self.town['map_resources'] = [r for r in self.town['map_resources'] if r['kind'] != 'water']
        self.assertEqual(work_source(self.town, 'farming', 0)['kind'], 'acre')

    def test_relocation_keeps_association_and_enforces_reach(self):
        self.build()
        command(self.town, {'action': 'move', 'index': 2, 'x': 6, 'y': 4})
        self.assertEqual(self.town['buildings'][2]['farm_index'], 1)
        before = copy.deepcopy(self.town)
        with self.assertRaises(ValueError):
            command(self.town, {'action': 'move', 'index': 2, 'x': 2, 'y': 2})
        self.assertEqual(self.town, before)


if __name__ == '__main__':
    unittest.main()
