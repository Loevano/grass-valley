"""Paired active-terrain HTTP contracts; all data roots and names are synthetic.

Populated town/founder/adventure admission remains explicitly unsupported.
"""
import copy
import hashlib
import http.client
import json
from pathlib import Path
import select
import socket
import subprocess
import tempfile
import threading
import unittest
from http.server import ThreadingHTTPServer
from unittest.mock import patch
from urllib.parse import urlencode

import server
from test_http_bootstrap import assets

ROOT = Path(__file__).resolve().parents[1]
# Literal cross-language dependencies for the affected-check selector.
RUST_SOURCES = ('rust/crates/lantern-host/src/http.rs',
                'rust/crates/lantern-host/src/http/read.rs',
                'rust/crates/lantern-host/src/http_bootstrap.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def fixture():
    rows = ['g' * 40 for _ in range(40)]
    rows[1] = 'gf' + 'g' * 38
    rows[9] = 'g' * 8 + 'f' + 'g' * 31
    record = {'terrain': rows, 'map_resources': [
        {'kind': 'tree', 'x': 1, 'y': 1, 'species': 'oak',
         'supply': 1.2345678901234567, 'supply_max': 8, 'regeneration': 0},
        {'kind': 'village_decor', 'x': 5, 'y': 7, 'variant': 'barrels',
         'label': 'Café 🌿\t\x7f', 'biome': 'b', 'village': 'Reedhaven'},
    ], 'tree_stumps': [{'kind': 'tree', 'x': 8, 'y': 9, 'remaining': 123.456789,
                       'species': 'ancient'}],
        'clock': 17.125, 'terrain_version': 7, 'industry_version': 2,
        'mineral_version': 1, 'village_version': 3, 'swamp_version': 2,
        'SYNTHETIC_PRIVATE_ENVELOPE': {'tokens': 'SYNTHETIC_SECRET'}}
    def world(size, version, offset, seed, sandbox):
        return {'towns': {}, 'sandbox': sandbox, 'town_sites': [{'x': 5, 'y': 5}], 'shared_map': {
            'chunked': 1, 'chunk_size': 40, 'map_size': size, 'terrain_version': version,
            'terrain_seed': seed, 'starter_offset': offset, 'chunk_clock': 17.125,
            'npcs': [], 'npc_roster_version': 3, 'npc_placement_version': 1,
            'forage_version': 1, 'forage_halls': [], 'villages': []}}
    return {'format': 'lantern-http-synthetic-v1', 'active': 'current', 'worlds': [
        {'id': 'current', 'name': 'Synthetic map 🍂',
         'world': world(480, 7, 160, 42, True), 'records': {'0,0': record}},
        {'id': 'legacy', 'name': 'Synthetic legacy dimensions',
         'world': world(1920, 1, 880, 105, False), 'records': {}},
    ]}


def write_chunks(root, config):
    save = root / 'savegame.json'
    for entry in config['worlds']:
        seed = entry['world']['shared_map']['terrain_seed']
        directory = root / 'savegame_chunks' / str(seed)
        directory.mkdir(parents=True, exist_ok=True)
        for key, record in entry['records'].items():
            (directory / (key + '.json')).write_text(json.dumps(record))
    return save


def reference(config, query):
    """Independent Python owners over separate copies, using audited exports."""
    entry = next(e for e in config['worlds'] if e['id'] == config['active'])
    world = copy.deepcopy(entry['world'])
    with tempfile.TemporaryDirectory() as temp:
        save = write_chunks(Path(temp), config)
        with patch.object(server, 'SAVE', save):
            server.prepare_landscapes(world)
            keys = server.parse_keys(query.get('chunks', ''))
            known = server.parse_keys(query.get('known', ''))
            versions = server.parse_versions(query.get('resources', ''))
            if query.get('world_id', config['active']) != config['active']:
                known, versions = set(), {}
            snapshot = dict(server.export_world(world), shared_map=world['_chunks'].snapshot(keys, known, versions))
    return dict(snapshot, world_id=entry['id'], name=entry['name'], hall_ready={},
                simulation_clock={'tick_seconds': 3, 'next_tick_in': 3},
                hall_upgrades=json.loads(json.dumps(server.HALL_UPGRADES)),
                progression=json.loads(json.dumps(server.economy.public_catalog())),
                local_stories=json.loads(json.dumps(server.npcs.STORIES)),
                quarry_catalog=json.loads(json.dumps(server.quests.CATALOG)))


class TerrainProtocol:
    @classmethod
    def configuration(cls):
        config = fixture()
        config['active'] = getattr(cls, 'active_world', 'current')
        return config

    def call(self, path):
        connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=10)
        connection.request('GET', path)
        response = connection.getresponse()
        body = response.read()
        headers = {key.lower(): value for key, value in response.getheaders()}
        connection.close()
        self.assertEqual(headers.get('content-type'), 'application/json')
        self.assertEqual(headers.get('cache-control'), 'no-store')
        self.assertEqual(int(headers['content-length']), len(body))
        return response.status, json.loads(body)

    def world(self, **query):
        status, value = self.call('/api/world?' + urlencode(query))
        self.assertEqual(status, 200, value)
        self.assertIsInstance(value.pop('players_sampled_at'), (float, int))
        self.assertEqual(value, reference(self.config, query))
        for resource in value['shared_map']['resource_chunks'].values():
            payload = {k: resource[k] for k in ('map_resources', 'tree_stumps')}
            encoded = json.dumps(payload, separators=(',', ':')).encode()
            self.assertEqual(resource['version'], hashlib.blake2b(encoded, digest_size=8).hexdigest())
        self.assertNotIn('SYNTHETIC_SECRET', json.dumps(value))
        self.assertNotIn('SYNTHETIC_PRIVATE_ENVELOPE', json.dumps(value))
        return value

    def test_saved_generated_chunks_and_all_public_snapshot_fields(self):
        value = self.world(chunks='2,0;10,0;0,0')
        self.assertEqual(value['shared_map']['visible_chunks'], ['0,0', '0,1', '1,0', '1,1', '10,0', '2,0'])
        self.assertEqual(set(value['shared_map']['terrain_chunks']), {'0,0', '0,1', '1,0', '1,1', '10,0', '2,0'})
        self.assertEqual(set(value['shared_map']['resource_chunks']), {'0,0', '0,1', '1,0', '1,1', '10,0', '2,0'})
        self.assertEqual(value['towns'], {})
        self.assertEqual(self.call('/api/players')[1]['world_id'], self.config['active'])
        self.assertEqual(self.call('/api/players')[1]['players'], {})
        self.assertEqual(self.call('/api/lanterns')[1], {'world_id': self.config['active'], 'quest': None})
        worlds = self.call('/api/worlds')[1]
        self.assertEqual(worlds, {'active_world': self.config['active'], 'can_manage': True, 'worlds': [
            {'id': 'current', 'name': 'Synthetic map 🍂', 'sandbox': True, 'town_count': 0},
            {'id': 'legacy', 'name': 'Synthetic legacy dimensions', 'sandbox': False, 'town_count': 0}]})

    def test_cache_reuse_and_changed_world_id_invalidate_known_and_versions(self):
        first = self.world(chunks='0,0;2,0')['shared_map']
        known = ';'.join(first['visible_chunks'])
        versions = ';'.join(key + ':' + value['version'] for key, value in first['resource_chunks'].items())
        cached = self.world(chunks='0,0;2,0', known=known, resources=versions, world_id=self.config['active'])['shared_map']
        self.assertEqual(cached['terrain_chunks'], {})
        self.assertEqual(cached['resource_chunks'], {})
        changed = self.world(chunks='0,0;2,0', known=known, resources=versions, world_id='different-world')['shared_map']
        self.assertEqual(changed, first)
        partial = self.world(chunks='0,0;2,0', known=';'.join(k for k in first['visible_chunks'] if k != '2,0'), resources=';'.join(k + ':' + v['version'] for k, v in first['resource_chunks'].items() if k != '2,0'))['shared_map']
        self.assertEqual(set(partial['terrain_chunks']), {'2,0'})
        self.assertEqual(set(partial['resource_chunks']), {'2,0'})

    def test_active_bounds_and_known_cache_keys_follow_existing_contract(self):
        size = next(e for e in self.config['worlds'] if e['id'] == self.config['active'])['world']['shared_map']['map_size']
        status, value = self.call('/api/world?chunks=' + str(size // 40) + ',0&world_id=stale')
        self.assertEqual(status, 400)
        self.assertEqual(value, {'error': 'Map chunk outside the world.'})
        self.world(chunks='0,0', known='47,47', resources='47,47:0123456789abcdef')
        if size == 1920:
            edge = self.world(chunks='47,47')
            self.assertIn('47,47', edge['shared_map']['terrain_chunks'])
        empty = self.world()
        self.assertEqual(empty['shared_map']['visible_chunks'], ['0,0', '0,1', '1,0', '1,1'])

    def test_slow_world_socket_transfer_does_not_block_small_snapshot_endpoints(self):
        keys = ';'.join(f'{x},{y}' for x in range(7) for y in range(7))
        slow = socket.socket()
        self.addCleanup(slow.close)
        slow.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024)
        slow.settimeout(20)
        slow.connect(('127.0.0.1', self.port))
        slow.sendall(('GET /api/world?' + urlencode({'chunks': keys}) +
                      ' HTTP/1.1\r\nHost: localhost\r\n\r\n').encode())
        headers = bytearray()
        while not headers.endswith(b'\r\n\r\n'):
            byte = slow.recv(1)
            self.assertTrue(byte)
            headers.extend(byte)
        self.assertIn(b'200', headers.split(b'\r\n', 1)[0])
        length = next(int(line.split(b':', 1)[1]) for line in headers.lower().split(b'\r\n')
                      if line.startswith(b'content-length:'))
        self.assertGreater(length, 512 * 1024, 'Exercise a substantial pending world body.')
        for path in ('/api/players', '/api/worlds', '/api/lanterns'):
            connection = http.client.HTTPConnection('127.0.0.1', self.port, timeout=3)
            try:
                connection.request('GET', path)
                response = connection.getresponse()
                self.assertEqual(response.status, 200)
                response.read()
            finally:
                connection.close()



class PythonTerrain(TerrainProtocol, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.config = cls.configuration()
        root = Path(cls.directory.name)
        save = write_chunks(root, cls.config)
        entries = {e['id']: {'name': e['name'], 'world': copy.deepcopy(e['world']), 'tokens': {}}
                   for e in cls.config['worlds']}
        for name, value in {'SAVE': save, 'NEXT_TICK_AT': None, 'WORLDS': entries,
                            'WORLD': entries[cls.config['active']]['world'], 'ACTIVE_WORLD': cls.config['active'],
                            'TOKENS': {}}.items():
            patcher = patch.object(server, name, value)
            patcher.start()
            cls.addClassCleanup(patcher.stop)
        cls.host = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
        cls.addClassCleanup(cls.host.server_close)
        cls.port = cls.host.server_port
        cls.thread = threading.Thread(target=cls.host.serve_forever,
                                      kwargs={'poll_interval': .01}, daemon=True)
        cls.thread.start()
        cls.addClassCleanup(cls.stop)

    @classmethod
    def stop(cls):
        cls.host.shutdown()
        cls.thread.join()


class RustTerrain(TerrainProtocol, unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        subprocess.run(['cargo', 'build', '-p', 'lantern-host', '--bin', 'lantern-host',
                        '--example', 'http_fixture', '--locked', '--offline'],
                       cwd=ROOT / 'rust', check=True, capture_output=True, timeout=180)
        cls.directory = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.config = cls.configuration()
        cls.asset_root = assets(cls.directory.name)
        cls.native = Path(cls.directory.name) / 'native'
        cls.builder = ROOT / 'rust/target/debug/examples/http_fixture'
        cls.binary = ROOT / 'rust/target/debug/lantern-host'
        cls.make_native(cls.native, cls.config)
        cls.before = {p.relative_to(cls.native): p.read_bytes() for p in cls.native.rglob('*') if p.is_file()}
        cls.process = subprocess.Popen([str(cls.binary), 'http-read-copy', '--experimental',
            '--source', str(cls.native), '--bind', '127.0.0.1:0', '--asset-root', str(cls.asset_root)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        cls.addClassCleanup(cls.stop)
        if not select.select([cls.process.stdout], [], [], 10)[0]:
            raise RuntimeError('Rust read-only HTTP did not report its socket')
        line = cls.process.stdout.readline()
        if 'listening on ' not in line:
            raise RuntimeError('Rust read-only HTTP admission failed: ' + cls.process.stderr.read())
        cls.port = int(line.rsplit(':', 1)[1])

    @classmethod
    def make_native(cls, path, config):
        result = subprocess.run([str(cls.builder), str(path)], input=json.dumps(config),
                                capture_output=True, text=True, timeout=10)
        if result.returncode:
            raise RuntimeError('Synthetic native fixture construction failed: ' + result.stderr)

    @classmethod
    def stop(cls):
        cls.process.terminate()
        cls.process.communicate(timeout=5)
        after = {p.relative_to(cls.native): p.read_bytes() for p in cls.native.rglob('*') if p.is_file()}
        if after != cls.before:
            raise AssertionError('Read-only HTTP changed its native source')

    def test_native_source_lock_and_unsupported_coverage_are_explicit(self):
        locked = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
            '--source', str(self.native), '--bind', '127.0.0.1:0'], capture_output=True, timeout=5)
        self.assertNotEqual(locked.returncode, 0)
        self.assertIn(b'locked', locked.stderr)
        for name in ('private-resource', 'populated', 'inactive-populated', 'private-map', 'missing-markers', 'no-starter-sites', 'outside-starter-sites'):
            with self.subTest(name=name):
                config = fixture()
                active = config['worlds'][1 if name == 'inactive-populated' else 0]
                if name == 'private-resource':
                    active['records']['0,0']['map_resources'][0]['credentials'] = 'SYNTHETIC_SECRET'
                elif name.endswith('populated'):
                    active['world']['towns']['SYNTHETIC_SECRET'] = {'buildings': [], 'villagers': []}
                elif name == 'private-map':
                    active['world']['shared_map']['credentials'] = 'SYNTHETIC_SECRET'
                elif name == 'missing-markers':
                    del active['world']['shared_map']['npc_roster_version']
                elif name == 'no-starter-sites':
                    active['world']['town_sites'] = []
                else:
                    active['world']['town_sites'] = [{'x': 480, 'y': 5}]
                path = Path(self.directory.name) / name
                self.make_native(path, config)
                result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
                    '--source', str(path), '--bind', '127.0.0.1:0'], capture_output=True, timeout=5)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(b'coverage', result.stderr)
                self.assertNotIn(b'SYNTHETIC_SECRET', result.stderr + result.stdout)

    def test_damaged_latest_generation_is_not_silently_recovered_by_http(self):
        path = Path(self.directory.name) / 'damaged-latest'
        self.make_native(path, fixture())
        (path / f'generation-{2:020}').mkdir()
        before = {p.relative_to(path): p.read_bytes() for p in path.rglob('*') if p.is_file()}
        result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
            '--source', str(path), '--bind', '127.0.0.1:0'], capture_output=True, timeout=5)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn(b'will not silently fall back', result.stderr)
        self.assertNotIn(b'listening', result.stdout)
        self.assertEqual(before, {p.relative_to(path): p.read_bytes() for p in path.rglob('*') if p.is_file()})


class PythonLegacyTerrain(PythonTerrain):
    active_world = 'legacy'


class RustLegacyTerrain(RustTerrain):
    active_world = 'legacy'


if __name__ == '__main__':
    unittest.main()
