import json
import unittest
from unittest.mock import patch

from chunk_world import ChunkTerrain
from navigation import RouteCache
import server
import work_cycles


class ShortestRouteCache(unittest.TestCase):
    def setUp(self):
        self.cache = RouteCache()
        self.terrain = ['g' * 12] * 12
        self.blocked = {(2, y) for y in range(3)}
        self.target = {'x': 4, 'y': 0}

    def route(self, start=(0, 0), target=None, adjacent=False, bounds=None):
        return self.cache.route(start, target or self.target, self.blocked, 12,
                                self.terrain, adjacent, bounds)

    def test_shortest_detour_is_reused_without_sharing_mutable_paths(self):
        with patch.object(self.cache, 'search', wraps=self.cache.search) as search:
            path = self.route()
            self.assertEqual(len(path), 10)
            self.assertEqual(path[-1], self.target)
            self.assertTrue(all((p['x'], p['y']) not in self.blocked for p in path))
            original = [dict(p) for p in path]
            path[0]['x'] = 99
            path.pop()
            self.assertEqual(self.route(), original)
            self.assertEqual(search.call_count, 1)

    def test_failed_search_is_cached_until_an_obstacle_changes(self):
        self.blocked = {(1, 0), (0, 1)}
        with patch.object(self.cache, 'search', wraps=self.cache.search) as search:
            self.assertIsNone(self.route())
            self.assertIsNone(self.route())
            self.assertEqual(search.call_count, 1)
            self.blocked.remove((1, 0))
            self.assertIsNotNone(self.route())
            self.assertEqual(search.call_count, 2)

    def test_terrain_changes_and_bounds_do_not_reuse_previous_routes(self):
        self.blocked = set()
        self.assertEqual(len(self.route()), 4)
        self.terrain[0] = 'gv' + 'g' * 10
        self.assertEqual(len(self.route()), 6)
        self.assertIsNone(self.route(bounds=(0, 11, 0, 0)))
        self.terrain[0] = 'g' * 12
        self.assertEqual(len(self.route(bounds=(0, 11, 0, 0))), 4)

    def test_exact_adjacent_and_nearest_bank_queries_are_separate(self):
        self.blocked = set()
        self.assertEqual(len(self.route(adjacent=True)), 3)
        self.assertEqual(len(self.route()), 4)
        self.assertEqual(self.route(start=(4, 0)), [])
        banks = {(6, 0), (0, 2)}
        self.assertEqual(self.route(target=banks)[-1], {'x': 0, 'y': 2})
        banks.remove((0, 2))
        self.assertEqual(len(self.route(target=banks)), 6)

    def test_chunk_loading_eviction_and_terrain_edits_invalidate_routes(self):
        records = {'0,0': {'terrain': ['g' * 40] * 40}}
        terrain = ChunkTerrain(records, 80)
        args = ((38, 1), {'x': 41, 'y': 1}, set(), 80, terrain, False, (38, 42, 1, 1))
        self.assertIsNone(self.cache.route(*args))
        records['1,0'] = {'terrain': ['g' * 40] * 40}
        self.assertEqual(len(self.cache.route(*args)), 3)
        records['1,0']['terrain'][1] = 'v' + 'g' * 39
        self.assertIsNone(self.cache.route(*args))
        records['1,0']['terrain'][1] = 'g' * 40
        self.assertIsNotNone(self.cache.route(*args))
        del records['1,0']
        self.assertIsNone(self.cache.route(*args))

    def test_cache_bounds_graphs_routes_and_waypoints(self):
        self.cache = RouteCache(graphs=2, routes=2, steps=5)
        self.blocked = set()
        self.route(target={'x': 2, 'y': 0})
        self.route(target={'x': 3, 'y': 0})
        self.route(target={'x': 4, 'y': 0})
        self.assertTrue(all(len(paths) <= 2 and steps <= 5 for paths, steps in self.cache.graphs.values()))
        for tile in ((1, 1), (1, 2), (1, 3)):
            self.blocked.add(tile)
            self.route()
        self.assertEqual(len(self.cache.graphs), 2)
        # Paths exceeding the waypoint budget are returned but not retained.
        self.blocked = set()
        self.route(target={'x': 11, 'y': 0})
        self.assertEqual(next(reversed(self.cache.graphs.values()))[0], {})

    def test_host_building_and_work_routes_share_cache_and_keep_path_contracts(self):
        town = {'map_size': 12, 'buildings': [dict(kind='storehouse', **self.target)],
                'map_resources': [], 'npcs': [], 'terrain': self.terrain}
        before = json.dumps(town)
        with patch.object(server, 'routes', self.cache), patch.object(work_cycles, 'routes', self.cache), \
                patch.object(self.cache, 'search', wraps=self.cache.search) as search:
            construction = server.construction_route(town, (0, 0), self.target)
            work = work_cycles.route({'x': 0, 'y': 0}, self.target, {(4, 0)}, 12, self.terrain)
            self.assertEqual(construction, [(0, 0)] + [(p['x'], p['y']) for p in work])
            self.assertEqual(search.call_count, 1)
        self.assertEqual(json.dumps(town), before)


if __name__ == '__main__':
    unittest.main()
