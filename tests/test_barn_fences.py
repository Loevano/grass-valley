"""Solid pen boundaries, automatic human access and saved entrance orientation."""
import copy
import json
import unittest

import barns
import server
import work_cycles
from fixtures import flat_town
from shared_world import walking_map


class BarnFences(unittest.TestCase):
    def setUp(self):
        self.town = flat_town(size=28)
        self.barn = {'kind': 'barn', 'x': 12, 'y': 12, 'animals': {'chicken': 1}}
        self.town['buildings'].append(self.barn)

    def rotate(self, orientation):
        return server.command(self.town, {'action': 'set_fence_orientation', 'index': 1,
                                          'orientation': orientation})

    def test_people_cannot_walk_through_rails_but_can_cross_the_gate(self):
        self.town['player'].update(x=8.1, y=12.)
        for _ in range(8):
            server.move_player(self.town, {'dx': 1, 'dy': 0})
        self.assertLessEqual(self.town['player']['x'], 8.32)
        self.town['player'].update(x=16.2, y=13.5)
        for _ in range(8):
            server.move_player(self.town, {'dx': -1, 'dy': 0})
        self.assertLess(self.town['player']['x'], 15.)

    def test_all_four_orientations_have_one_clear_two_tile_entrance(self):
        for orientation in range(4):
            self.rotate(orientation)
            actual, a, b, segments = barns.pen(self.barn, 28)
            nx, ny = barns.NORMALS[orientation]
            midpoint = (a[0] + b[0]) / 2, (a[1] + b[1]) / 2
            outside = midpoint[0] + nx, midpoint[1] + ny
            inside = midpoint[0] - nx, midpoint[1] - ny
            self.assertEqual(actual, orientation)
            self.assertFalse(barns.crosses(outside, inside, segments))
            self.assertFalse(barns.blocked(*midpoint, segments))
            self.assertTrue(barns.crosses((8, 8), (9, 9), segments), 'No diagonal corner shortcuts')

    def test_routes_use_the_gate_and_orientation_invalidates_cached_routes(self):
        start, target = (7, 12), {'x': 11, 'y': 12}
        first = server.construction_route(self.town, start, target)
        self.assertTrue(any(x >= 16 for x, y in first), 'Approach the opposite-side entrance')
        for a, b in zip(first, first[1:]):
            self.assertFalse(barns.crosses(a, b, barns.fences(self.town['buildings'], 28)))
        self.rotate(2)
        second = server.construction_route(self.town, start, target)
        self.assertLess(len(second), len(first))
        self.assertTrue(all(x < 12 for x, y in second))

    def test_orientation_is_free_persists_and_moves_with_the_barn(self):
        coins = self.town['coins']
        self.rotate(3)
        self.assertEqual(self.town['coins'], coins)
        restored = json.loads(json.dumps(self.town))
        restored['sandbox'] = True
        server.command(restored, {'action': 'move', 'index': 1, 'x': 20, 'y': 20})
        moved = restored['buildings'][1]
        self.assertEqual(moved['fence_orientation'], 3)
        self.assertEqual(barns.pen(moved, 28)[1][1], 16.5)

    def test_large_workplace_cannot_be_served_through_solid_rails(self):
        target = {'kind': 'mill', 'x': 16, 'y': 10, 'footprint': [2, 2]}
        self.town['buildings'].append(target)
        path = server.construction_route(self.town, (15, 10), target)
        self.assertIsNotNone(path)
        self.assertGreaterEqual(path[-1][0], 16, 'Reach an outside working position via the gate')
        for a, b in zip(path, path[1:]):
            self.assertFalse(barns.crosses(a, b, barns.fences(self.town['buildings'], 28)))

    def test_fishing_piers_cannot_bridge_solid_rails(self):
        hut = {'kind': 'fishing_hut', 'x': 16, 'y': 10}
        self.town['buildings'].append(hut)
        self.town['map_resources'] = [{'kind': 'water', 'x': 15, 'y': 10}]
        from spatial import occupied_tiles
        blocked = occupied_tiles(self.town['buildings'] + self.town['map_resources'])
        self.assertEqual(work_cycles.fishing_pier(hut, self.town, blocked, 28,
                                                 self.town['terrain'], None), [])
        old = {'task': ['fishing', 'fishing_hut', 16, 10],
               'pier': [hut, {'x': 15, 'y': 10}]}
        self.assertIsNone(work_cycles.retained_pier(old, hut, self.town, blocked,
                                                   self.town['terrain']))

    def test_invalid_controls_and_rotating_onto_people_are_rejected(self):
        for value in [-1, 4, True, '1', None]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                self.rotate(value)
        with self.assertRaises(ValueError):
            server.command(self.town, {'action': 'set_fence_orientation', 'index': 0, 'orientation': 0})
        self.town['player'].update(x=15.5, y=13.5)
        with self.assertRaisesRegex(ValueError, 'cleared'):
            self.rotate(2)
        self.assertNotIn('fence_orientation', self.barn)

    def test_map_edge_legacy_barns_fall_back_to_an_accessible_side(self):
        self.barn.update(x=27, y=0)
        orientation, a, b, segments = barns.pen(self.barn, 28)
        self.assertEqual(orientation, 1)
        self.assertTrue(all(-.5 <= n <= 27.5 for segment in segments for n in segment))
        with self.assertRaisesRegex(ValueError, 'face into the map'):
            self.rotate(0)
        self.barn['construction'] = {'progress': 0, 'total': 30}
        self.assertFalse(barns.fences([self.barn], 28))
        self.barn['construction']['upgrade'] = {'level': 2}
        self.assertTrue(barns.fences([self.barn], 28))

    def test_keeper_enters_through_gate_and_old_routes_replan_after_rotation(self):
        worker = {'job': 'husbandry', 'building_index': 1,
                  'logistics_position': {'x': 7, 'y': 12}}
        stock = copy.deepcopy(self.barn)
        work_cycles.advance(self.town, self.town, worker, self.barn, self.barn, 0, 0)
        self.assertTrue(any(p['x'] >= 16 for p in worker['work_animation']['path']))
        self.rotate(2)
        fences = barns.fences(self.town['buildings'], 28)
        for _ in range(200):
            before = worker['work_animation']['x'], worker['work_animation']['y']
            work_cycles.advance(self.town, self.town, worker, self.barn, self.barn, 0, .1)
            state = worker['work_animation']
            self.assertFalse(barns.crosses(before, (state['x'], state['y']), fences))
        self.assertLessEqual(max(abs(state['x'] - 12), abs(state['y'] - 12)), 3)
        self.barn.pop('fence_orientation')
        self.assertEqual(self.barn, stock)

    def test_shared_map_collision_includes_other_towns_pens(self):
        own = flat_town(size=28)
        own['player'].update(x=8.1, y=12.)
        walking = walking_map({'towns': {'own': own, 'friend': self.town}}, own)
        for _ in range(8):
            server.move_player(walking, {'dx': 1, 'dy': 0})
        self.assertLessEqual(own['player']['x'], 8.32)


if __name__ == '__main__':
    unittest.main()
