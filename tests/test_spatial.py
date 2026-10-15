import json
import unittest
from unittest.mock import patch

import logistics
import server
import work_cycles
from spatial import SpatialIndex


class WorkerSpatialLookups(unittest.TestCase):
    def setUp(self):
        self.town = server.new_town('Spatial')
        self.town.update(terrain=['g' * 120] * 120, map_resources=[], npcs=[])

    def test_nearby_preserves_order_and_manhattan_range_across_cells(self):
        resources = [
            {'kind': 'tree', 'x': 8, 'y': 7},
            {'kind': 'water', 'x': 7, 'y': 8},
            {'kind': 'tree', 'x': 6, 'y': 7},
            {'kind': 'tree', 'x': 8, 'y': 8},
            {'kind': 'tree', 'x': 119, 'y': 119},
        ]
        index = SpatialIndex(resources)
        self.assertEqual(index.nearby('tree', 7, 7, 1), [resources[0], resources[2]])
        self.assertEqual(index.nearby('tree', 119, 119, 0), [resources[-1]])
        self.assertEqual(index.nearby('tree', 0, 0, 3), [])
        self.assertIs(index.nearby('tree', 7, 7, 1)[0], resources[0])

    def test_assignment_keeps_source_ties_and_observes_live_supply(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 7, 'y': 7})
        first = {'kind': 'tree', 'x': 8, 'y': 7, 'supply': 8}
        second = {'kind': 'tree', 'x': 6, 'y': 7, 'supply': 8}
        self.town['map_resources'] = [first, second]
        self.town['villagers'][0].update(job='woodcutting', building_index=1)
        index = SpatialIndex(self.town['map_resources'])
        self.assertIs(server.work_assignment(self.town, 'woodcutting', 0, index)[1], first)
        first['supply'] = 0
        self.assertIs(server.work_assignment(self.town, 'woodcutting', 0, index)[1], second)
        self.assertEqual(server.work_assignment(self.town, 'woodcutting', 0, index),
                         server.work_assignment(self.town, 'woodcutting', 0))

    def test_felled_tree_releases_route_and_is_not_selected_again(self):
        tree = {'kind': 'tree', 'x': 7, 'y': 7, 'supply': .5}
        self.town['map_resources'] = [tree]
        index = SpatialIndex(self.town['map_resources'])
        walking = dict(self.town, _spatial_index=index)
        self.assertIn((7, 7), index.blocked(walking))
        server.economy.take_supply(self.town, tree, .5)
        self.assertEqual(index.nearby('tree', 7, 7, 10), [])
        self.assertNotIn((7, 7), index.blocked(walking))
        self.assertEqual(server.construction_route(walking, (6, 7), {'x': 8, 'y': 7}),
                         [(6, 7), (7, 7)])

    def test_routes_share_obstacles_and_preserve_npc_and_building_collisions(self):
        self.town['npcs'] = [{'x': 6, 'y': 7}]
        self.town['map_resources'] = [{'kind': 'rock', 'x': 7, 'y': 7}]
        index = SpatialIndex(self.town['map_resources'])
        walking = dict(self.town, _spatial_index=index)
        self.assertIs(index.blocked(walking), index.blocked(dict(walking)))
        target = {'x': 8, 'y': 7}
        self.assertEqual(server.construction_route(walking, (5, 7), target),
                         server.construction_route(self.town, (5, 7), target))
        self.town['buildings'].append({'kind': 'home', 'x': 8, 'y': 6})
        self.assertIn((8, 6), index.blocked(walking))

    def test_hauling_cache_allows_completed_acres_without_opening_other_obstacles(self):
        self.town['terrain'] = ['v' * 120] * 7 + ['v' * 5 + 'ggggg' + 'v' * 110] + ['v' * 120] * 112
        workplace = {'kind': 'woodcutter', 'x': 5, 'y': 7}
        acre = {'kind': 'acre', 'x': 7, 'y': 7, 'construction': {'progress': 0, 'total': 30}}
        storage = {'kind': 'storehouse', 'x': 9, 'y': 7}
        self.town['buildings'] = [workplace, acre, storage]
        walking = dict(self.town, _spatial_index=SpatialIndex(self.town['map_resources']))
        worker = {'job': 'woodcutting', 'logistics_position': {'x': 6, 'y': 7}}
        self.assertFalse(logistics.start_trip(self.town, walking, worker, workplace, storage,
                                             'collect', server.construction_route))
        del acre['construction']
        self.assertTrue(logistics.start_trip(self.town, walking, worker, workplace, storage,
                                            'collect', server.construction_route))
        self.assertIn([7, 7], worker['logistics_trip']['path'])
        worker['logistics_trip']['delivered'] = True
        logistics.advance_trip(self.town, walking, worker, 10)
        self.assertNotIn('logistics_trip', worker)
        self.assertEqual(worker['logistics_position'], {'x': 6, 'y': 7})
        self.assertIn((7, 7), walking['_spatial_index'].blocked(walking))

    def test_tick_cache_is_not_saved_and_refreshes_after_resource_replacement(self):
        self.town['buildings'].append({'kind': 'woodcutter', 'x': 7, 'y': 7})
        worker = self.town['villagers'][0]
        # Replace the source while still approaching, before there is cargo
        # that must finish delivery to the original workplace.
        worker.update(job='woodcutting', building_index=1, construction_position={'x': 1, 'y': 1})
        self.town['map_resources'] = [{'kind': 'tree', 'x': 8, 'y': 7}]
        world = {'towns': {'own': self.town}}
        server.tick(world)
        self.assertEqual(worker['work_target']['x'], 8)
        self.town['map_resources'][0] = {'kind': 'tree', 'x': 6, 'y': 7}
        server.tick(world)
        self.assertEqual(worker['work_target']['x'], 6)
        self.assertNotIn('_spatial_index', json.dumps(world))

    def test_work_cycles_reuse_resource_scan_and_match_uncached_playback(self):
        import copy

        building = {'kind': 'woodcutter', 'x': 7, 'y': 7}
        source = {'kind': 'tree', 'x': 9, 'y': 7, 'supply': 8}
        self.town['buildings'].append(building)
        self.town['map_resources'] = [source]
        worker = self.town['villagers'][0]
        worker.update(job='woodcutting', building_index=1,
                      construction_position={'x': 8, 'y': 7})
        uncached = copy.deepcopy(self.town)
        index = SpatialIndex(self.town['map_resources'])
        walking = dict(self.town, _spatial_index=index)
        before = set(index.blocked(walking))
        with patch('work_cycles.occupied_tiles', side_effect=AssertionError('Repeated obstacle scan')):
            for _ in range(4):
                work_cycles.advance(self.town, walking, worker, building, source, 0, .1)
        for _ in range(4):
            work_cycles.advance(uncached, uncached, uncached['villagers'][0],
                                uncached['buildings'][1], uncached['map_resources'][0], 0, .1)
        self.assertEqual(worker, uncached['villagers'][0])
        self.assertEqual(index.blocked(walking), before)

    def test_farmer_access_does_not_open_cached_fields_for_other_workers(self):
        farm = {'kind': 'farm', 'x': 7, 'y': 7}
        acre = {'kind': 'acre', 'x': 8, 'y': 7}
        self.town['buildings'].extend([farm, acre])
        worker = self.town['villagers'][0]
        worker.update(job='farming', building_index=1,
                      construction_position={'x': 8, 'y': 7})
        index = SpatialIndex(self.town['map_resources'])
        walking = dict(self.town, _spatial_index=index)
        before = set(index.blocked(walking))
        work_cycles.advance(self.town, walking, worker, farm, acre, 0, .1)
        self.assertIn((8, 7), index.blocked(walking))
        self.assertEqual(index.blocked(walking), before)

    def test_cached_obstacles_preserve_resource_and_npc_footprints(self):
        from spatial import occupied_tiles

        self.town['map_resources'] = [{'kind': 'rock', 'x': 7, 'y': 7, 'footprint': [2, 2]}]
        self.town['npcs'] = [{'x': 9, 'y': 9, 'footprint': [2, 1]}]
        index = SpatialIndex(self.town['map_resources'])
        self.assertEqual(index.blocked(self.town), occupied_tiles(
            self.town['buildings'] + self.town['map_resources'] + self.town['npcs']))


if __name__ == '__main__':
    unittest.main()
