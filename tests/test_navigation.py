import json
import random
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

    def route(self, start=(0, 0), target=None, adjacent=False, bounds=None, roads=(), road_mode='smooth'):
        return self.cache.route(start, target or self.target, self.blocked, 12,
                                self.terrain, adjacent, bounds, roads, road_mode)

    def test_longer_road_route_wins_only_when_faster(self):
        self.blocked = set()
        target = {'x': 8, 'y': 0}
        roads = {(x, 1) for x in range(9)}
        expected = [{'x': x, 'y': 1} for x in range(9)] + [target]
        for mode in ('smooth', 'departure', 'arrival'):
            with self.subTest(mode=mode):
                self.assertEqual(self.route(target=target, roads=roads, road_mode=mode), expected)
        # A road two rows away adds too much travel to justify the detour.
        far_roads = {(x, 2) for x in range(9)}
        self.assertEqual(len(self.route(target=target, roads=far_roads)), 8)

    def test_road_edits_invalidate_cached_routes_and_preserve_other_workers(self):
        self.blocked = set()
        roads = set()
        with patch.object(self.cache, 'search', wraps=self.cache.search) as search:
            direct = self.route(target={'x': 8, 'y': 0}, roads=roads)
            roads.update((x, 1) for x in range(9))
            fast = self.route(target={'x': 8, 'y': 0}, roads=roads)
            self.assertEqual(len(fast), 10)
            fast[0]['x'] = 99
            self.assertEqual(self.route(target={'x': 8, 'y': 0}, roads=roads)[0], {'x': 0, 'y': 1})
            self.assertEqual(search.call_count, 2)
            roads.clear()
            self.assertEqual(self.route(target={'x': 8, 'y': 0}, roads=roads), direct)

    def test_adjacent_goals_and_water_banks_are_chosen_by_time(self):
        self.blocked = set()
        roads = {(x, 1) for x in range(9)}
        path = self.route(target={'x': 8, 'y': 0}, roads=roads, adjacent=True)
        self.assertEqual(path[-1], {'x': 8, 'y': 1})
        self.assertEqual(len(path), 9)
        path = self.route(target={(0, 4), (5, 0)}, roads={(x, 0) for x in range(6)})
        self.assertEqual(path[-1], {'x': 5, 'y': 0})

    def test_adjacent_arrival_matches_builder_hauler_and_smooth_timing(self):
        town = {'map_size': 12, 'buildings': [], 'map_resources': [], 'terrain': self.terrain,
                'roads': [{'x': 1, 'y': 1}]}
        target = {'x': 2, 'y': 1}
        # Builders do not gain speed from a road they stop on; haulers and
        # smooth walkers do. These queries must not share a cached result.
        self.assertEqual(server.construction_route(town, (0, 3), target)[-1], (2, 2))
        self.assertEqual(server.logistics_route(town, (0, 3), target)[-1], (1, 1))
        self.assertEqual(work_cycles.route({'x': 0, 'y': 3}, target, set(), 12,
                                           self.terrain, roads={(1, 1)})[-1], {'x': 1, 'y': 1})

    def test_weighted_search_matches_exhaustive_costs_on_small_maps(self):
        rng = random.Random(42)
        for mode in ('smooth', 'departure', 'arrival'):
            for _ in range(12):
                tiles = [(x, y) for x in range(5) for y in range(5)]
                blocked = {p for p in tiles if p not in ((0, 0), (4, 4)) and rng.random() < .2}
                roads = {p for p in tiles if p not in blocked and rng.random() < .5}
                def cost(a, b):
                    before, after = (2 if a in roads else 3), (2 if b in roads else 3)
                    return 2 * before if mode == 'departure' else 2 * after if mode == 'arrival' else before + after
                # Bellman-Ford reference: exhaustive relaxation, independent of
                # the production heap and heuristic, including disconnected maps.
                costs = {(0, 0): 0}
                for _ in tiles:
                    for a in tiles:
                        if a not in costs or a in blocked:
                            continue
                        for b in tiles:
                            if b not in blocked and abs(a[0] - b[0]) + abs(a[1] - b[1]) == 1:
                                costs[b] = min(costs.get(b, float('inf')), costs[a] + cost(a, b))
                path = self.cache.route((0, 0), {'x': 4, 'y': 4}, blocked, 5, [], False, roads=roads, road_mode=mode)
                if (4, 4) not in costs:
                    self.assertIsNone(path)
                else:
                    points = [(0, 0)] + [(p['x'], p['y']) for p in path]
                    self.assertEqual(sum(cost(a, b) for a, b in zip(points, points[1:])), costs[(4, 4)])

    def test_multiple_goals_match_exhaustive_travel_costs(self):
        rng = random.Random(51)
        for diagonal in (False, True):
            for mode in ('smooth', 'departure', 'arrival'):
                for _ in range(8):
                    tiles = [(x, y) for x in range(5) for y in range(5)]
                    goals = {(3, 4), (4, 3), (4, 4)}
                    blocked = {p for p in tiles if p != (0, 0) and rng.random() < .25}
                    roads = {p for p in tiles if p not in blocked and rng.random() < .5}
                    def cost(a, b):
                        before, after = (2 if a in roads else 3), (2 if b in roads else 3)
                        weight = 2 * before if mode == 'departure' else 2 * after if mode == 'arrival' else before + after
                        return weight * (2 ** .5 if a[0] != b[0] and a[1] != b[1] else 1)
                    costs = {(0, 0): 0}
                    for _ in tiles:
                        for a in tiles:
                            if a not in costs or a in blocked:
                                continue
                            for b in tiles:
                                dx, dy = abs(a[0] - b[0]), abs(a[1] - b[1])
                                if b in blocked or max(dx, dy) != 1 or (not diagonal and dx + dy != 1):
                                    continue
                                if dx and dy and ({(a[0], b[1]), (b[0], a[1])} & blocked):
                                    continue
                                costs[b] = min(costs.get(b, float('inf')), costs[a] + cost(a, b))
                    expected = min((costs.get(goal, float('inf')) for goal in goals))
                    path = self.cache.route((0, 0), goals, blocked, 5, [], False,
                                            roads=roads, road_mode=mode, diagonal=diagonal)
                    if expected == float('inf'):
                        self.assertIsNone(path)
                    else:
                        points = [(0, 0)] + [(p['x'], p['y']) for p in path]
                        self.assertIn(points[-1], goals)
                        self.assertAlmostEqual(sum(cost(a, b) for a, b in zip(points, points[1:])), expected)

    def test_host_routes_use_completed_shared_roads_and_ignore_construction(self):
        town = {'map_size': 12, 'buildings': [], 'map_resources': [], 'terrain': self.terrain,
                'roads': [{'x': x, 'y': 1, 'construction': {'progress': 9, 'total': 10}} for x in range(9)]}
        target = {'x': 8, 'y': 0}
        before = json.dumps(town)
        self.assertEqual(len(server.construction_route(town, (0, 0), target)), 8)
        self.assertEqual(json.dumps(town), before)
        for road in town['roads']:
            del road['construction']
        for routing in (server.construction_route, server.logistics_route):
            before = json.dumps(town)
            path = routing(town, (0, 0), target)
            self.assertTrue(any(y == 1 for x, y in path))
            self.assertTrue(any(a[0] != b[0] and a[1] != b[1] for a, b in zip(path, path[1:])))
            self.assertEqual(json.dumps(town), before)
        from shared_world import walking_map
        friend = dict(town)
        own = dict(town, roads=[])
        walking = walking_map({'towns': {'own': own, 'friend': friend}}, own)
        self.assertEqual(server.construction_route(walking, (0, 0), target),
                         server.construction_route(town, (0, 0), target))
        # A completed road can shorten the next work leg without changing
        # snapshots or the existing per-worker movement state.
        worker = {'job': 'woodcutting', 'construction_position': {'x': 0, 'y': 0}}
        hall = {'kind': 'hall', 'x': 10, 'y': 10}
        camp = {'kind': 'woodcutter', 'x': 10, 'y': 9}
        source = dict(kind='tree', **target)
        own.update(buildings=[hall, camp], roads=friend['roads'], map_resources=[source])
        before = json.dumps(own)
        work_cycles.advance(own, own, worker, camp, source, 0, 0)
        self.assertEqual(worker['work_animation']['path'][0], {'x': 1, 'y': 1})
        self.assertEqual(json.dumps(own), before)

    def test_diagonal_routes_are_separate_from_cardinal_cached_routes(self):
        target = {'x': 3, 'y': 3}
        cardinal = self.cache.route((0, 0), target, set(), 12, self.terrain, False)
        diagonal = self.cache.route((0, 0), target, set(), 12, self.terrain, False, diagonal=True)
        self.assertEqual(len(cardinal), 6)
        self.assertEqual(diagonal, [{'x': i, 'y': i} for i in range(1, 4)])
        diagonal[0]['x'] = 99
        self.assertEqual(self.cache.route((0, 0), target, set(), 12, self.terrain, False, diagonal=True)[0], {'x': 1, 'y': 1})

    def test_diagonal_routes_cannot_cross_blocked_or_void_corners(self):
        target = {'x': 3, 'y': 3}
        path = self.cache.route((0, 0), target, {(1, 0)}, 12, self.terrain, False, diagonal=True)
        self.assertEqual(path[0], {'x': 0, 'y': 1})
        self.assertIsNone(self.cache.route((0, 0), target, {(1, 0), (0, 1)}, 12, self.terrain, False, diagonal=True))
        terrain = ['gv'+'g'*10]+self.terrain[1:]
        self.assertEqual(self.cache.route((0, 0), target, set(), 12, terrain, False, diagonal=True)[0], {'x': 0, 'y': 1})

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
