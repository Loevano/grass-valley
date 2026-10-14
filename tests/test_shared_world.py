import copy
import json
from collections import deque
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server
from shared_world import bind_map, export_world, new_world
import test_http


class SharedMap(unittest.TestCase):
    def test_existing_world_removes_sandy_trees_before_regrowing_stumps(self):
        world = self.make_world(count=1)
        landscape = world['shared_map']
        x, y = next((x, y) for y, row in enumerate(landscape['terrain'])
                    for x, biome in enumerate(row) if biome == 's')
        landscape['map_resources'].append({'kind': 'tree', 'x': x, 'y': y})
        landscape['tree_stumps'] = [{'kind': 'tree', 'x': x, 'y': y, 'remaining': 3}]
        server.prepare_landscapes(world, regenerate=True)
        self.assertFalse(any(r['kind'] == 'tree' and (r['x'], r['y']) == (x, y)
                             for r in landscape['map_resources']))
        self.assertEqual(landscape['tree_stumps'], [])

    def make_world(self, count=4, seed=42):
        world = new_world(seed)
        for i in range(count):
            world['towns'][str(i)] = server.new_town(str(i), world)
        return world

    def test_sites_are_randomized_separated_and_reproducible(self):
        self.assertEqual(new_world(42), new_world(42))
        self.assertNotEqual(new_world(42)['town_sites'], new_world(43)['town_sites'])
        for seed in range(12):
            sites = new_world(seed)['town_sites']
            for i, a in enumerate(sites):
                for b in sites[i + 1:]:
                    self.assertGreaterEqual((a['x'] - b['x']) ** 2 + (a['y'] - b['y']) ** 2, 40 ** 2)

    def test_every_town_has_the_same_landscape_and_reachable_spawn(self):
        for seed in (0, 42, 99):
            world = self.make_world(seed=seed)
            landscape = world['shared_map']
            blocked = {(r['x'], r['y']) for r in landscape['map_resources'] if r['kind'] != 'tree'}
            blocked.update((b['x'], b['y']) for town in world['towns'].values() for b in town['buildings'])
            start = world['towns']['0']['player']
            queue = deque([(int(start['x']), int(start['y']))])
            seen = set(queue)
            while queue:
                x, y = queue.popleft()
                for nx, ny in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if (0 <= nx < 120 and 0 <= ny < 120 and (nx, ny) not in seen
                            and (nx, ny) not in blocked and landscape['terrain'][ny][nx] != 'v'):
                        seen.add((nx, ny))
                        queue.append((nx, ny))
            for town in world['towns'].values():
                self.assertIs(town['terrain'], landscape['terrain'])
                self.assertIs(town['map_resources'], landscape['map_resources'])
                self.assertIn((town['player']['x'], town['player']['y']), seen)
                self.assertTrue(any(r['kind'] == 'water' and abs(r['x'] - town['town_origin']['x'])
                                    + abs(r['y'] - town['town_origin']['y']) <= 10
                                    for r in town['map_resources']))

    def test_territory_and_visiting_player_block_construction_without_spending(self):
        world = self.make_world(2)
        town, friend = world['towns'].values()
        x, y = town['town_origin']['x'] + 2, town['town_origin']['y']
        friend['player'].update(x=x, y=y)
        before = copy.deepcopy(world)
        with self.assertRaisesRegex(ValueError, 'player'):
            server.command(town, {'action': 'build', 'kind': 'home', 'x': x, 'y': y}, world)
        self.assertEqual(world, before)
        friend['player'].update(x=x + 20, y=y)
        server.command(town, {'action': 'build', 'kind': 'home', 'x': x, 'y': y}, world)
        before = copy.deepcopy(world)
        with self.assertRaisesRegex(ValueError, 'territory'):
            server.command(town, {'action': 'move', 'index': 1, 'x': x + 30, 'y': y}, world)
        self.assertEqual(world, before)

    def test_walking_visits_other_town_and_collides_with_its_buildings(self):
        world = self.make_world(2)
        town, friend = world['towns'].values()
        hall = friend['buildings'][0]
        town['player'].update(x=float(hall['x']), y=float(hall['y'] + 2))
        friend_before = copy.deepcopy(friend['player'])
        for _ in range(8):
            server.command(town, {'action': 'walk', 'dx': 0, 'dy': -1}, world)
        self.assertGreaterEqual(town['player']['y'], hall['y'] + 1.68)
        self.assertEqual(friend['player'], friend_before)
        x = town['player']['x']
        server.command(town, {'action': 'walk', 'dx': 1, 'dy': 0}, world)
        self.assertGreater(town['player']['x'], x)

    def test_hall_moves_preserve_spacing_and_cannot_claim_more_land(self):
        world = self.make_world(2)
        world['shared_map'].update(terrain=['g' * 120 for _ in range(120)],
                                   map_resources=[], npcs=[])
        bind_map(world)
        town, friend = world['towns'].values()
        for t, x in ((town, 30), (friend, 74)):
            t['town_origin'] = {'x': x, 'y': 30}
            t['buildings'] = [{'kind': 'hall', 'x': x, 'y': 30}]
            t['player'].update(x=float(x), y=32.)
        before = copy.deepcopy(world)
        with self.assertRaisesRegex(ValueError, '40 tiles'):
            server.command(town, {'action': 'move', 'index': 0, 'x': 35, 'y': 30}, world)
        self.assertEqual(world, before)
        server.command(town, {'action': 'move', 'index': 0, 'x': 34, 'y': 30}, world)
        town['level'] = 2
        before = copy.deepcopy(world)
        with self.assertRaisesRegex(ValueError, 'territory'):
            server.command(town, {'action': 'build', 'kind': 'home', 'x': 66, 'y': 30}, world)
        self.assertEqual(world, before)

    def test_save_reload_stores_map_once_and_restores_shared_references(self):
        world = self.make_world(2)
        entries = {'new': {'name': 'New', 'world': world, 'tokens': {'private': '0'}}}
        with tempfile.TemporaryDirectory() as directory, patch.multiple(
                server, SAVE=Path(directory) / 'save.json', WORLDS=entries,
                ACTIVE_WORLD='new', WORLD=world, TOKENS=entries['new']['tokens']):
            server.save()
            saved = json.loads(server.SAVE.read_text())['worlds']['new']['world']
            self.assertNotIn('terrain', saved['towns']['0'])
            self.assertEqual(saved, export_world(world))
            server.load()
            self.assertEqual(server.TOKENS, {'private': '0'})
            self.assertEqual(server.WORLD['towns']['0']['buildings'], world['towns']['0']['buildings'])
            self.assertIs(server.WORLD['towns']['0']['terrain'], server.WORLD['towns']['1']['terrain'])

    def test_upgrades_expand_shared_territory_without_hall_moves_extending_it(self):
        world = self.make_world(1)
        world['shared_map'].update(terrain=['g' * 120 for _ in range(120)],
                                   map_resources=[], npcs=[])
        bind_map(world)
        town = world['towns']['0']
        town['town_origin'] = {'x': 30, 'y': 30}
        town['buildings'] = [{'kind': 'hall', 'x': 30, 'y': 30}]
        town['player'].update(x=30., y=32.)
        for level, reach in ((1, 30), (2, 35), (3, 40), (4, 45), (5, 50)):
            with self.subTest(level=level):
                town.update(level=level, wood=100)
                town['buildings'][0]['x'] = 30
                server.command(town, {'action': 'build', 'kind': 'home',
                                      'x': 30 + reach, 'y': 30}, world)
                server.command(town, {'action': 'move', 'index': 0, 'x': 31, 'y': 30}, world)
                before = copy.deepcopy(world)
                with self.assertRaisesRegex(ValueError, 'territory'):
                    server.command(town, {'action': 'build', 'kind': 'home',
                                          'x': 31 + reach, 'y': 30}, world)
                self.assertEqual(world, before)

    def test_old_world_is_not_converted(self):
        town = server.new_town('Legacy')
        world = {'towns': {'old': town}}
        before = copy.deepcopy(world)
        bind_map(world)
        self.assertEqual(world, before)
        self.assertNotIn('town_origin', town)
        self.assertIs(export_world(world), world)


class SharedMapHTTP(unittest.TestCase):
    setUp = test_http.LanHost.setUp
    tearDown = test_http.LanHost.tearDown
    call = test_http.LanHost.call

    def test_two_clients_get_one_map_and_cannot_edit_each_others_towns(self):
        self.assertEqual(self.call('/api/worlds/create', {'name': 'Shared'})[0], 200)
        owner = json.loads(self.call('/api/join', {'name': 'Home'})[1])
        friend = json.loads(self.call('/api/join', {'name': 'Friend'})[1])
        first = json.loads(self.call('/api/world')[1])
        second = json.loads(self.call('/api/world', token=friend['token'])[1])
        self.assertGreaterEqual(second.pop('players_sampled_at'), first.pop('players_sampled_at'))
        self.assertEqual(first, second)
        self.assertNotIn('terrain', first['towns'][owner['town_id']])
        friend_hall = first['towns'][friend['town_id']]['buildings'][0]
        status, _ = self.call('/api/command', {'action': 'build', 'kind': 'home',
                                             'x': friend_hall['x'] + 1, 'y': friend_hall['y'],
                                             'town_id': friend['town_id']}, owner['token'])
        self.assertEqual(status, 400)
        self.assertEqual(len(server.WORLD['towns'][friend['town_id']]['buildings']), 1)
        self.assertNotIn(owner['token'], json.dumps(first))
