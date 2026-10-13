import copy
import json
import unittest

import progression as economy
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
        self.assertEqual(acre, {'kind': 'acre', 'farm_index': 1, 'x': 6, 'y': 3,
                                'field': {'stage': 'unplowed', 'elapsed': 0, 'progress': 0, 'cycle': 0}})
        self.town = json.loads(json.dumps(self.town))
        command(self.town, {'action': 'job', 'index': 0, 'job': 'farming'})
        food = self.town['food']
        tick({'towns': {'a': self.town}})
        self.assertAlmostEqual(self.town['food'], food - .78)
        self.assertFalse(self.town['buildings'][1].get('pending_output'))
        self.assertEqual(self.town['villagers'][0]['work_target'], acre)
        self.assertNotIn('vegetables', self.town.get('produced', {}))

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

    def test_moving_farm_preserves_acres_and_allows_overlapping_old_tiles(self):
        self.build()
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[],
                         town_origin={'x': 5, 'y': 5})
        acre = self.town['buildings'][2]
        acre.update(crop='wheat', level=2, stored_goods={'wheat': 4})
        self.town['buildings'].append({'kind': 'acre', 'x': 7, 'y': 2, 'farm_index': 1,
                                      'construction': {'progress': 0, 'total': 30}})
        self.town['villagers'][0].update(job='farming', building_index=2)
        coins = self.town['coins']
        command(self.town, {'action': 'move', 'index': 1, 'x': 6, 'y': 3})
        self.assertEqual([(b['x'], b['y']) for b in self.town['buildings'][1:]],
                         [(6, 3), (6, 4), (7, 3)])
        self.assertEqual((acre['crop'], acre['level'], acre['stored_goods']),
                         ('wheat', 2, {'wheat': 4}))
        self.assertIn('construction', self.town['buildings'][3])
        self.assertEqual(self.town['villagers'][0]['building_index'], 2)
        self.assertIs(work_source(self.town, 'farming', 0), acre)
        self.assertEqual(self.town['coins'], coins - 1)

    def test_blocked_acre_destination_rejects_whole_farm_move(self):
        self.build()
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[],
                         town_origin={'x': 5, 'y': 5})
        original = copy.deepcopy(self.town)
        for obstacle in ('resource', 'building', 'npc', 'boundary', 'player', 'range',
                         'map_edge', 'other_building', 'other_player'):
            with self.subTest(obstacle=obstacle):
                self.town = copy.deepcopy(original)
                destination = {'x': 7, 'y': 3}
                world = None
                if obstacle == 'resource':
                    self.town['map_resources'].append({'kind': 'water', 'x': 7, 'y': 4})
                elif obstacle == 'building':
                    self.town['buildings'].append({'kind': 'home', 'x': 7, 'y': 4})
                elif obstacle == 'npc':
                    self.town['npcs'].append({'x': 7, 'y': 4})
                elif obstacle == 'boundary':
                    row = self.town['terrain'][4]
                    self.town['terrain'][4] = row[:7] + 'v' + row[8:]
                elif obstacle == 'player':
                    self.town['player'].update(x=7., y=4.)
                elif obstacle == 'range':
                    destination = {'x': 35, 'y': 5}
                elif obstacle == 'map_edge':
                    self.town['buildings'][2]['y'] = 1
                    destination = {'x': 6, 'y': 0}
                else:
                    friend = {'buildings': [], 'player': {'x': 100., 'y': 100.}}
                    if obstacle == 'other_building':
                        friend['buildings'].append({'kind': 'home', 'x': 7, 'y': 4})
                    else:
                        friend['player'].update(x=7., y=4.)
                    world = {'shared_map': {}, 'towns': {'own': self.town, 'friend': friend}}
                before = copy.deepcopy(self.town)
                with self.assertRaises(ValueError):
                    command(self.town, {'action': 'move', 'index': 1, **destination}, world)
                self.assertEqual(self.town, before)

    def test_destroy_farm_removes_only_linked_acres_and_preserves_other_links(self):
        self.build()
        economy.ensure_inventory(self.town)
        farm, acre = self.town['buildings'][1:]
        farm['stored_goods'] = {'wheat': 4}
        acre['stored_goods'] = {'wheat': 2}
        economy.add_stock(self.town, 'wheat', 6)
        farm['pending_output'] = {'wheat': 1}
        self.town['buildings'].extend([
            {'kind': 'farm', 'x': 7, 'y': 3},
            {'kind': 'acre', 'x': 7, 'y': 4, 'farm_index': 3},
            {'kind': 'acre', 'x': 7, 'y': 2, 'farm_index': 1,
             'construction': {'progress': 0, 'total': 30}},
        ])
        self.town['villagers'][0].update(job='farming', building_index=2,
                                       pending_workplace={'kind': 'farm', 'x': 6, 'y': 2},
                                       pending_output={'wheat': 1}, work_animation={})
        self.town['villagers'][1].update(job='farming', building_index=4)
        wheat = economy.quantity(self.town, 'wheat')
        command(self.town, {'action': 'destroy', 'index': 1})
        self.assertEqual([b['kind'] for b in self.town['buildings']], ['hall', 'farm', 'acre'])
        self.assertEqual(self.town['buildings'][2]['farm_index'], 1)
        self.assertEqual(self.town['villagers'][1]['building_index'], 2)
        self.assertEqual(self.town['villagers'][0]['job'], 'idle')
        self.assertNotIn('building_index', self.town['villagers'][0])
        self.assertNotIn('pending_output', self.town['villagers'][0])
        self.assertIsNone(self.town['villagers'][0]['work_target'])
        self.assertEqual(self.town['buildings'][0]['stored_goods']['wheat'], 7)
        self.assertEqual(economy.quantity(self.town, 'wheat'), wheat + 1)


if __name__ == '__main__':
    unittest.main()
