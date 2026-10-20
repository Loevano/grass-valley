"""Read-only native town presentation and ordinary-map protocol parity.

All roots, tokens and names are disposable. Native state and presentation are
constructed separately so mismatches must fail admission rather than disappear.
"""
import copy
import http.client
import select
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import urlencode

import server
import test_http_snapshots as terrain
from test_http_snapshots import TerrainProtocol, fixture, reference, write_chunks

RUST_SOURCES = ('rust/crates/lantern-host/src/legacy/http_snapshot/metadata.rs',
                'rust/crates/lantern-host/src/http/read.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def populated_fixture(chunked=True, elapsed=2.1, completed=False, inside=True):
    config = fixture()
    entry = config['worlds'][0]
    if chunked:
        world = entry['world']
        world['sandbox'] = False
        world['town_sites'] = [{'x': 5, 'y': 5}, {'x': 85, 'y': 5}]
        # Explicit starter records keep founder geometry stable; camera chunks
        # continue using real terrain generation through each host's owner.
        record = {'terrain': ['g' * 40] * 40, 'map_resources': [], 'tree_stumps': [],
                  'clock': 17.125, 'terrain_version': 7, 'industry_version': 2,
                  'mineral_version': 1, 'village_version': 3, 'swamp_version': 2}
        entry['records'] = {f'{x},{y}': copy.deepcopy(record) for x in range(4) for y in range(2)}
        world['shared_map']['npcs'] = [{'name': 'Tobin', 'role': 'Valley traveler',
            'x': 18, 'y': 18, 'dialogue': 'Synthetic public story.', 'biome': 'g'}]
    else:
        world = server.new_world(42)
        entry['world'] = world
        entry['records'] = {}
    for key, name in [('first', 'Café 🌿'), ('second', 'Another town')]:
        town = server.new_town(name, world)
        world['towns'][key] = town
    town = world['towns']['first']
    x, y = town['town_origin']['x'], town['town_origin']['y']
    for kind, dx, dy in [('home', 5, 0), ('farm', 5, 5)] + ([] if chunked else [('well', 7, 7)]):
        town['buildings'].append({'kind': kind, 'x': x + dx, 'y': y + dy,
                                  'footprint': list(server.BUILDING_FOOTPRINTS.get(kind, (1, 1)))})
    water = {'kind': 'water', 'x': x + 6, 'y': y + 8}
    if chunked:
        record = entry['records'][f'{water["x"] // 40},{water["y"] // 40}']
        # V7 prunes isolated water; use a real connected three-tile pond.
        for dx in range(3):
            tile = dict(water, x=water['x'] + dx)
            record['map_resources'].append(tile)
            row = tile['y'] % 40
            column = tile['x'] % 40
            record['terrain'][row] = record['terrain'][row][:column] + 's' + record['terrain'][row][column + 1:]
    else:
        landscape = world['shared_map']
        landscape['map_resources'] = [r for r in landscape['map_resources']
            if (r['x'], r['y']) != (water['x'], water['y'])] + [water]
        landscape['terrain'][water['y']] = (landscape['terrain'][water['y']][:water['x']] + 's'
                                           + landscape['terrain'][water['y']][water['x'] + 1:])
    world['lanterns'] = {'members': {key: {'x': 1 + i, 'y': 2, 'inside': inside,
                            'checkpoint': 1, 'hits': i, 'last_step': .12}
                            for i, key in enumerate(world['towns'])},
                         'elapsed': elapsed, 'completed': completed,
                         'message': 'Synthetic public courier course.'}
    world['quarry'] = {'members': {key: {'x': 1, 'y': 3, 'inside': False} for key in world['towns']},
                      'started': True, 'completed': False, 'gate_latched': True,
                      'sequence': ['sun'], 'runes_solved': False, 'message': 'Synthetic public quarry.'}
    with tempfile.TemporaryDirectory() as directory:
        save = write_chunks(Path(directory), config)
        with patch.object(server, 'SAVE', save):
            server.prepare_landscapes(world)
            for town in world['towns'].values():
                server.ensure_resources(town)
                server.ensure_player(server.walking_map(world, town))
            # The native archive contains persistence facts, never bound terrain
            # mirrors or Python's private runtime ChunkMap object.
            entry['world'] = json.loads(json.dumps(server.export_world(world)))
            if chunked:
                for field in ('terrain', 'map_resources', 'tree_stumps'):
                    entry['world']['shared_map'].pop(field, None)
    entry['native_towns'] = True
    return config


class PopulatedProtocol:
    call = TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return populated_fixture(cls.chunked)

    def test_public_towns_founders_residents_adventures_and_catalogs(self):
        status, value = self.call('/api/world')
        self.assertEqual(status, 200, value)
        sampled_at = value.pop('players_sampled_at')
        self.assertEqual(value, reference(self.config, {}))
        self.assertEqual(set(value['towns']), {'first', 'second'})
        self.assertEqual(value['towns']['first']['name'], 'Café 🌿')
        self.assertEqual(len(value['towns']['first']['villagers']), 5)
        self.assertEqual(value['towns']['first']['wood'], 65)
        self.assertEqual(value['hall_ready'], {'first': True, 'second': False})
        for town in value['towns'].values():
            self.assertNotIn('terrain', town)
            self.assertNotIn('npcs', town)
        status, players = self.call('/api/players')
        self.assertEqual(status, 200)
        self.assertGreaterEqual(players['sampled_at'], sampled_at)
        self.assertEqual(players['players'], {key: town['player'] for key, town in value['towns'].items()})
        self.assertEqual(players['world_id'], 'current')
        worlds = self.call('/api/worlds')[1]
        self.assertEqual(worlds['worlds'][0]['town_count'], 2)
        self.assertEqual(worlds['worlds'][1]['town_count'], 0)
        self.assertEqual(self.call('/api/lanterns?ignored=yes')[1],
                         {'world_id': 'current', 'quest': server.lanterns.snapshot(value)})
        encoded = json.dumps(value)
        for private in ('SYNTHETIC_SECRET_TOKEN', 'pin_hash', 'credentials', 'tokens'):
            self.assertNotIn(private, encoded)
        self.assertTrue(value['shared_map']['npcs'])
        self.assertIn('quarry', value)

    def test_valid_chunk_queries_preserve_full_nonchunk_snapshot_or_reuse_chunk_caches(self):
        query = {'chunks': '3,3', 'known': '47,47', 'world_id': 'stale',
                 'resources': '47,47:0123456789abcdef'}
        status, value = self.call('/api/world?' + urlencode(query))
        self.assertEqual(status, 200, value)
        value.pop('players_sampled_at')
        self.assertEqual(value, reference(self.config, query))
        if self.chunked:
            self.assertIn('3,3', value['shared_map']['terrain_chunks'])
        else:
            self.assertEqual(len(value['shared_map']['terrain']), 120)
            self.assertNotIn('resource_chunks', value['shared_map'])
        self.assertEqual(self.call('/api/world?chunks=48,0')[0], 400)


class PythonPopulated(PopulatedProtocol, unittest.TestCase):
    test_slow_populated_world_transfer = TerrainProtocol.test_slow_world_socket_transfer_does_not_block_small_snapshot_endpoints
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class RustReadHarness(PopulatedProtocol):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)

    def read_copy(self, config, path):
        with tempfile.TemporaryDirectory(dir=self.directory.name) as directory:
            native = Path(directory) / 'native'
            self.make_native(native, config)
            before = {p.relative_to(native): p.read_bytes() for p in native.rglob('*') if p.is_file()}
            process = subprocess.Popen([str(self.binary), 'http-read-copy', '--experimental',
                '--source', str(native), '--bind', '127.0.0.1:0'],
                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                self.assertTrue(select.select([process.stdout], [], [], 10)[0])
                line = process.stdout.readline()
                if 'listening on ' not in line:
                    self.fail('Native read admission failed: ' + process.stderr.read())
                port = int(line.rsplit(':', 1)[1])
                connection = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
                try:
                    connection.request('GET', path)
                    response = connection.getresponse()
                    self.assertEqual(response.status, 200)
                    return json.loads(response.read())
                finally:
                    connection.close()
            finally:
                process.terminate()
                process.communicate(timeout=5)
                self.assertEqual(before, {p.relative_to(native): p.read_bytes() for p in native.rglob('*') if p.is_file()})

    def test_unknown_nested_fields_and_native_owner_mismatches_refuse_all_endpoints(self):
        for name in ('town', 'founder', 'resident', 'building', 'npc', 'routine', 'village', 'quest', 'member',
                     'native-player', 'native-coins', 'native-resident', 'native-building', 'native-work', 'native-owner', 'invalid-founder', 'native-actor-position', 'native-store'):
            with self.subTest(name=name):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                source = entry['world']
                entry['native_source'] = copy.deepcopy(source)
                town = source['towns']['first']
                targets = {'town': town, 'founder': town['player'], 'resident': town['villagers'][0],
                           'building': town['buildings'][0], 'npc': source['shared_map']['npcs'][0],
                           'quest': source['lanterns'], 'member': source['lanterns']['members']['first']}
                if name in targets:
                    targets[name]['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif name == 'routine':
                    source['shared_map']['npcs'][0]['routine'] = {'credentials': 'SYNTHETIC_PRIVATE_CANARY'}
                elif name == 'village':
                    source['shared_map']['villages'] = [{'name': 'Read only', 'x': 100, 'y': 100,
                        'biome': 'b', 'credentials': 'SYNTHETIC_PRIVATE_CANARY'}]
                elif name == 'native-player':
                    entry['native_source']['towns']['first']['player']['x'] += 1
                elif name == 'native-actor-position':
                    entry['native_source']['towns']['first']['villagers'][0]['x'] = 1.
                elif name == 'native-store':
                    native_town = entry['native_source']['towns']['first']
                    native_town['wood'] -= 1
                    native_town['buildings'][2]['stored_goods'] = {'wood': 1}
                elif name == 'native-coins':
                    entry['native_source']['towns']['first']['coins'] += 1
                elif name == 'native-resident':
                    entry['native_source']['towns']['first']['villagers'][0]['name'] = 'Different native name'
                elif name == 'native-building':
                    entry['native_source']['towns']['first']['buildings'][0]['x'] += 1
                elif name == 'native-work':
                    entry['native_work'] = True
                elif name == 'invalid-founder':
                    town['player']['x'] = -100.
                    entry['native_source']['towns']['first']['player']['x'] = -100.
                elif name == 'native-owner':
                    towns = entry['native_source']['towns']
                    towns['other-owner'] = towns.pop('first')
                path = Path(self.directory.name) / name
                self.make_native(path, config)
                result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
                    '--source', str(path), '--bind', '127.0.0.1:0'], capture_output=True, timeout=10)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn(b'listening', result.stdout)
                self.assertIn(b'coverage', result.stderr)
                self.assertNotIn(b'SYNTHETIC_PRIVATE_CANARY', result.stderr + result.stdout)


class PythonOrdinary(PopulatedProtocol, unittest.TestCase):
    chunked = False
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class RustPopulated(RustReadHarness, unittest.TestCase):
    test_slow_populated_world_transfer = TerrainProtocol.test_slow_world_socket_transfer_does_not_block_small_snapshot_endpoints

    def test_courier_phase_boundaries_completion_and_membership_match_python(self):
        for elapsed, completed, inside in [(0, False, True), (1.4, False, True),
                (2., False, True), (2.6, False, True), (3.4, False, True),
                (100.125, False, True), (2.1, True, True), (2.1, False, False)]:
            with self.subTest(elapsed=elapsed, completed=completed, inside=inside):
                config = copy.deepcopy(self.config)
                world = config['worlds'][0]['world']
                world['lanterns'].update(elapsed=elapsed, completed=completed)
                for member in world['lanterns']['members'].values():
                    member['inside'] = inside
                value = self.read_copy(config, '/api/lanterns')
                self.assertEqual(value, {'world_id': 'current', 'quest': server.lanterns.snapshot(world)})



class RustOrdinary(RustReadHarness, unittest.TestCase):
    chunked = False

    def test_native_village_owner_supplies_routines_without_exposing_private_extensions(self):
        config = copy.deepcopy(self.config)
        entry = config['worlds'][0]
        landscape = entry['world']['shared_map']
        landscape['villages'] = [{'name': 'Readhaven', 'biome': 'b', 'x': 80, 'y': 80,
                                 'layout_version': 1, 'life_version': 0, 'swamp_version': 0}]
        landscape['npcs'].append({'name': 'Synthetic local', 'role': 'village trader',
            'x': 79, 'y': 81, 'biome': 'b', 'village': 'Readhaven',
            'routine': {'x': 79., 'y': 81., 'path': [], 'stop': -1, 'wait': 1.,
                        'elapsed': 0., 'phase': 'idle', 'job': 'idle', 'facing': 1,
                        'origin': [79, 81], 'goal': None, 'paused': False, 'source': None}})
        entry['native_village'] = copy.deepcopy(entry['world'])
        live = entry['native_village']['shared_map']['npcs'][-1]['routine']
        live.update(x=79.25, y=81.5, elapsed=.75)
        expected = copy.deepcopy(config)
        expected['worlds'][0]['world']['shared_map']['npcs'][-1]['routine'].update(live)
        value = self.read_copy(config, '/api/world')
        value.pop('players_sampled_at')
        self.assertEqual(value, reference(expected, {}))
        # Private fields inside a selected owner are never silently exported.
        entry['native_village']['shared_map']['npcs'][-1]['routine']['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
        with tempfile.TemporaryDirectory(dir=self.directory.name) as directory:
            native = Path(directory) / 'native'
            self.make_native(native, config)
            result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
                '--source', str(native), '--bind', '127.0.0.1:0'], capture_output=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'coverage', result.stderr)
            self.assertNotIn(b'SYNTHETIC_PRIVATE_CANARY', result.stdout + result.stderr)




if __name__ == '__main__':
    unittest.main()
