import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import server
from chunk_world import CHUNK_SIZE, WORLD_SIZE, LEGACY_WORLD_SIZE, LEGACY_START_OFFSET, attach, chunk_key, generate_chunk
from shared_world import bind_map, export_world, new_world, walking_map
import test_http


class ExplorableMap(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.root = Path(self.directory.name)
        self.save_patch = patch.object(server, 'SAVE', self.root / 'save.json')
        self.save_patch.start()
        self.world = new_world(42, explorable=True, chunk_directory=self.root / 'save_chunks')
        self.world['towns']['own'] = server.new_town('Home', self.world)
        server.prepare_landscapes(self.world)
        self.manager = self.world['_chunks']
        self.town = self.world['towns']['own']

    def tearDown(self):
        self.save_patch.stop()
        self.directory.cleanup()

    def visit(self, x, y):
        self.town['player'].update(x=x, y=y)
        self.manager.activate()
        bind_map(self.world)

    def test_saved_chunk_trees_and_stumps_on_sand_are_removed_when_read(self):
        key = '0,0'
        record = generate_chunk(42, key)
        record['terrain'][0] = 'sg' + record['terrain'][0][2:]
        record['map_resources'] = [{'kind': 'tree', 'x': x, 'y': 0} for x in (0, 1)]
        record['tree_stumps'] = [{'kind': 'tree', 'x': 0, 'y': 0, 'remaining': 3}]
        self.manager.memory[key] = json.dumps(record)
        loaded = self.manager.read(key)
        self.assertEqual([(r['x'], r['y']) for r in loaded['map_resources'] if r['kind'] == 'tree'], [(1, 0)])
        self.assertEqual(loaded['tree_stumps'], [])

    def test_generated_chunks_only_have_trees_on_green_ground(self):
        for seed in range(5):
            for key in ('0,0', '3,4', '10,10'):
                record = generate_chunk(seed, key)
                for tree in record['map_resources']:
                    if tree['kind'] == 'tree':
                        self.assertIn(record['terrain'][tree['y'] % CHUNK_SIZE]
                                      [tree['x'] % CHUNK_SIZE], 'gfmb')

    def test_large_map_generates_only_a_bounded_working_set(self):
        self.assertEqual(self.town['map_size'], WORLD_SIZE)
        self.assertLessEqual(len(self.manager.records), 9)
        self.assertEqual(len(self.town['terrain']), WORLD_SIZE)
        self.assertEqual(self.town['terrain'][0][0], 'v')
        self.assertLess(len(json.dumps(export_world(self.world))), 8000)
        for x in (40, 120, 280, 360, 440):
            self.visit(x, 200)
            self.assertLessEqual(len(self.manager.records), 18)
        self.assertEqual(generate_chunk(42, '3,4'), generate_chunk(42, '3,4'))
        self.assertNotEqual(generate_chunk(42, '3,4'), generate_chunk(43, '3,4'))

    def test_camera_only_exploration_does_not_activate_or_repeatedly_generate(self):
        active = set(self.manager.records)
        with patch('chunk_world.generate_chunk', wraps=generate_chunk) as generate:
            first = self.manager.snapshot({'1,1'})
            second = self.manager.snapshot({'1,1'}, first['visible_chunks'])
            self.assertEqual(generate.call_count, 1)
        self.assertEqual(set(self.manager.records), active)

        self.assertEqual(second['terrain_chunks'], {})
        for x in range(10):
            self.manager.snapshot({f'{x},{y}' for y in range(10)})
        self.assertLessEqual(len(self.manager.views), 49)
        self.assertEqual(set(self.manager.records), active)

    def test_unchanged_snapshots_reuse_versions_and_detect_in_place_supply_changes(self):
        first = self.manager.snapshot()
        versions = {key: value['version'] for key, value in first['resource_chunks'].items()}
        resource = next(r for r in self.town['map_resources'] if r.get('supply', 0) > 1)
        key = chunk_key(resource['x'], resource['y'])
        original = json.loads(json.dumps(first))
        with patch('chunk_world.json.dumps', wraps=json.dumps) as encode:
            self.assertEqual(self.manager.snapshot(versions=versions)['resource_chunks'], {})
            encode.assert_not_called()
            resource['supply'] -= 1
            changed = self.manager.snapshot(versions=versions)['resource_chunks']
            self.assertEqual(set(changed), {key})
            self.assertNotEqual(changed[key]['version'], versions[key])
            self.assertEqual(encode.call_count, 1)
        self.assertEqual(first, original)

    def test_sync_detects_replacements_and_moves_without_a_length_change(self):
        self.manager.snapshot()
        resources = self.town['map_resources']
        original = resources[0]
        key = chunk_key(original['x'], original['y'])
        replacement = dict(original, supply=0)
        resources[0] = replacement
        self.manager.sync()
        self.assertTrue(any(r is replacement for r in self.manager.records[key]['map_resources']))
        other = next(k for k in self.manager.records if k != key)
        cx, cy = map(int, other.split(','))
        replacement.update(x=cx * CHUNK_SIZE, y=cy * CHUNK_SIZE)
        self.manager.sync()
        self.assertFalse(any(r is replacement for r in self.manager.records[key]['map_resources']))
        self.assertTrue(any(r is replacement for r in self.manager.records[other]['map_resources']))

    def test_camera_cache_is_bounded_and_catches_up_only_when_time_advances(self):
        self.manager.snapshot({'1,1'})
        with patch.object(self.manager, 'catch_up', wraps=self.manager.catch_up) as catch_up:
            self.manager.snapshot({'1,1'})
            catch_up.assert_not_called()
            self.world['shared_map']['chunk_clock'] += 3
            self.manager.snapshot({'1,1'})
            self.assertEqual(catch_up.call_count, 1)
        for x in range(5):
            self.manager.snapshot({f'{x},0'})
        self.assertLessEqual(len(self.manager.snapshots), len(self.manager.records) + 1)

    def test_removals_and_regrowth_survive_eviction_and_reload(self):
        self.visit(80, 80)
        landscape = self.world['shared_map']
        tree = next(r for r in landscape['map_resources'] if r['kind'] == 'tree' and r['x'] < 120
                    and landscape['terrain'][r['y']][r['x']] == 'f')
        position = tree['x'], tree['y']
        server.economy.take_supply(landscape, tree, 100)
        self.assertTrue(any((r['x'], r['y']) == position for r in landscape['tree_stumps']))
        self.visit(440, 440)
        self.world['shared_map']['chunk_clock'] += 600
        self.visit(80, 80)
        server.prepare_landscapes(self.world, regenerate=True)
        self.assertTrue(any((r['x'], r['y']) == position for r in landscape['map_resources']))
        # A chest removal must remain permanent even outside the working set.
        chest = {'kind': 'chest', 'x': 80, 'y': 80}
        landscape['map_resources'].append(chest)
        self.manager.flush()
        landscape['map_resources'].remove(chest)
        self.visit(440, 440)
        stored = json.loads(json.dumps(export_world(self.world)))
        restored = attach(stored, self.root / 'save_chunks')
        stored['towns']['own']['player'].update(x=80, y=80)
        restored.activate()
        self.assertFalse(any(r['kind'] == 'chest' and r['x'] == 80 and r['y'] == 80
                             for r in stored['shared_map']['map_resources']))

    def test_sleeping_supplies_catch_up_and_unchanged_chunks_are_not_rewritten(self):
        self.visit(80, 80)
        server.prepare_landscapes(self.world)
        resource = next(r for r in self.town['map_resources'] if r.get('regeneration', 0) and r['x'] < 120)
        position = resource['x'], resource['y']
        resource['supply'] = 0
        self.manager.flush()
        self.visit(440, 440)
        self.world['shared_map']['chunk_clock'] += 600
        self.visit(80, 80)
        resource = next(r for r in self.town['map_resources'] if (r['x'], r['y']) == position)
        self.assertEqual(resource['supply'], resource['supply_max'])
        self.manager.flush()
        paths = list(self.manager.directory.glob('*.json'))
        before = {p: p.stat().st_mtime_ns for p in paths}
        self.manager.flush()
        self.assertEqual(before, {p: p.stat().st_mtime_ns for p in paths})

    def test_movement_crosses_chunk_boundaries_and_checks_local_obstacles(self):
        self.visit(399.4, 400)
        landscape = self.world['shared_map']
        for key, record in self.manager.records.items():
            record['terrain'] = ['g' * CHUNK_SIZE] * CHUNK_SIZE
        landscape['map_resources'][:] = []
        for _ in range(8):
            server.command(self.town, {'action': 'walk', 'dx': 1, 'dy': 0}, self.world)
        self.assertGreater(self.town['player']['x'], 400)
        self.assertIn(chunk_key(self.town['player']['x'], 400), self.manager.records)
        self.town['player'].update(x=400., y=400.)
        landscape['map_resources'].append({'kind': 'rock', 'x': 401, 'y': 400})
        for _ in range(8):
            server.command(self.town, {'action': 'walk', 'dx': 1, 'dy': 0}, self.world)
        self.assertLessEqual(self.town['player']['x'], 400.32)
        self.assertLess(len(walking_map(self.world, self.town, local=True)['map_resources']), 26)

    def test_small_world_starters_and_boundaries_survive_reload(self):
        self.assertEqual(WORLD_SIZE, 480)
        for site in self.world['town_sites']:
            self.assertTrue(0 <= site['x'] < WORLD_SIZE and 0 <= site['y'] < WORLD_SIZE)
        stored = json.loads(json.dumps(export_world(self.world)))
        restored = attach(stored, self.root / 'save_chunks')
        self.assertEqual(restored.start_offset, self.manager.start_offset)
        self.assertEqual(restored._read('4,4'), self.manager._read('4,4'))
        self.visit(WORLD_SIZE - 1, WORLD_SIZE - 1)
        self.assertIn('11,11', self.manager.records)
        self.assertTrue(all(int(part) < 12 for key in self.manager.records for part in key.split(',')))
        with self.assertRaisesRegex(ValueError, 'outside'):
            self.manager.snapshot({'12,0'})

    def test_legacy_chunked_maps_keep_dimensions_and_starter_layout(self):
        # Recreate the persisted metadata used before the smaller default.
        with patch('chunk_world.WORLD_SIZE', LEGACY_WORLD_SIZE), patch('chunk_world.START_OFFSET', LEGACY_START_OFFSET):
            old = new_world(42, explorable=True)
        stored = json.loads(json.dumps(export_world(old)))
        del stored['shared_map']['starter_offset']
        original_sites = json.loads(json.dumps(stored['town_sites']))
        manager = attach(stored)
        self.assertEqual(len(manager.terrain), LEGACY_WORLD_SIZE)
        self.assertEqual(len(manager.terrain[1919]), LEGACY_WORLD_SIZE)
        self.assertEqual(manager._read('22,22'), old['_chunks']._read('22,22'))
        self.assertEqual(stored['town_sites'], original_sites)
        self.assertIn('47,47', manager.snapshot({'47,47'})['terrain_chunks'])
        stored['towns']['own'] = server.new_town('Home', stored)
        stored['towns']['own']['player'].update(x=1919, y=1919)
        manager.activate()
        self.assertIn('47,47', manager.records)

    def test_legacy_shared_maps_keep_their_dimensions(self):
        old = new_world(42)
        before = json.dumps(old)
        self.assertIsNone(attach(old, self.root / 'save_chunks'))
        self.assertEqual(json.dumps(old), before)
        self.assertEqual(old['shared_map']['map_size'], 120)

    def test_construction_and_relocation_work_at_new_town_coordinates(self):
        origin = self.town['town_origin']
        server.command(self.town, {'action': 'build', 'kind': 'home',
                                   'x': origin['x'] + 2, 'y': origin['y']}, self.world)
        self.assertEqual(len(self.town['buildings']), 2)
        server.command(self.town, {'action': 'move', 'index': 1,
                                   'x': origin['x'] + 3, 'y': origin['y']}, self.world)
        self.assertEqual(self.town['buildings'][1]['x'], origin['x'] + 3)
        for _ in range(12):
            server.tick(self.world)
        self.assertNotIn('construction', self.town['buildings'][1])
        with self.assertRaisesRegex(ValueError, 'territory'):
            server.command(self.town, {'action': 'build', 'kind': 'home',
                                       'x': 100, 'y': 100}, self.world)


class ExplorableMapHTTP(unittest.TestCase):
    setUp = test_http.LanHost.setUp
    tearDown = test_http.LanHost.tearDown
    call = test_http.LanHost.call

    def test_switching_and_deleting_worlds_releases_chunks(self):
        self.call('/api/worlds/create', {'name': 'First'})
        self.call('/api/join', {'name': 'Home'})
        first_id, first_world = server.ACTIVE_WORLD, server.WORLD
        directory = first_world['_chunks'].directory
        self.assertTrue(directory.exists())
        self.call('/api/worlds/create', {'name': 'Second'})
        self.assertNotIn('_chunks', first_world)
        self.assertNotIn('terrain', first_world['shared_map'])
        self.call('/api/worlds/open', {'world_id': first_id})
        self.assertIn('_chunks', server.WORLD)
        self.assertEqual(self.call('/api/worlds/delete', {'world_id': first_id, 'confirmation': 'First'})[0], 200)
        self.assertFalse(directory.exists())

    def test_new_world_streams_chunks_caches_terrain_and_saves_movement_on_tick(self):
        self.assertEqual(self.call('/api/worlds/create', {'name': 'Wilderness'})[0], 200)
        owner = json.loads(self.call('/api/join', {'name': 'Home'})[1])
        snapshot = json.loads(self.call('/api/world?chunks=1,1')[1])
        landscape = snapshot['shared_map']
        self.assertEqual(landscape['map_size'], WORLD_SIZE)
        self.assertNotIn('terrain', landscape)
        known = ';'.join(landscape['visible_chunks'])
        versions = ';'.join(f"{key}:{value['version']}" for key, value in landscape['resource_chunks'].items())
        again = json.loads(self.call('/api/world?chunks=1,1&known=' + known + '&resources=' + versions)[1])
        self.assertEqual(again['shared_map']['terrain_chunks'], {})
        self.assertEqual(again['shared_map']['resource_chunks'], {})
        # The profession catalog grows independently of streamed terrain.
        self.assertLess(len(json.dumps(again)) - len(json.dumps(again['progression'])), 20000)
        changed_world = json.loads(self.call('/api/world?known=' + known + '&world_id=old-world')[1])
        self.assertTrue(changed_world['shared_map']['terrain_chunks'])
        self.assertNotIn(owner['token'], json.dumps(snapshot))
        self.assertEqual(self.call(f'/api/world?chunks={WORLD_SIZE // CHUNK_SIZE},0')[0], 400)
        self.assertEqual(self.call('/api/world?chunks=' + ';'.join(['0,0'] * 50))[0], 400)
        self.assertEqual(self.call('/api/world?known=' + ';'.join(['0,0'] * 129))[0], 400)
        self.assertEqual(self.call('/api/world?resources=:0123456789abcdef')[0], 400)
        self.assertEqual(self.call('/api/world?resources=0,0:bad')[0], 400)
        with patch.object(server, 'save', wraps=server.save) as save:
            self.assertEqual(self.call('/api/command', {'action': 'walk', 'dx': 1, 'dy': 0}, owner['token'])[0], 200)
            save.assert_not_called()
            with server.LOCK:
                server.tick(server.WORLD)
                server.save()
            self.assertEqual(save.call_count, 1)
        player = dict(server.WORLD['towns'][owner['town_id']]['player'])
        server.load()
        self.assertEqual(server.WORLD['towns'][owner['town_id']]['player'], player)
        self.assertIs(server.WORLD['towns'][owner['town_id']]['terrain'], server.WORLD['shared_map']['terrain'])
