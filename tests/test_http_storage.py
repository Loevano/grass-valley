"""R20-1f frozen storage parity; synthetic owners, never playable hydration."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import server
import test_http_populated as populated
import test_http_snapshots as terrain

RUST_SOURCES = ('rust/crates/lantern-host/src/legacy/http_snapshot/metadata.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/ledger.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/buildings.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def storage_fixture(chunked=True):
    config = populated.populated_fixture(chunked)
    entry = config['worlds'][0]
    entry['native_storage'] = {'mirrors': True}
    town = entry['world']['towns']['first']
    town.update(inventory_version=1, food_balance_version=2, market_storage_version=1)
    town['buildings'][0]['stored_goods'] = {'wood': 65, 'food': 40}
    x, y = town['town_origin']['x'], town['town_origin']['y']
    for kind, dx, dy in [('storehouse', 9, 0), ('sawpit', 9, 5)]:
        town['buildings'].append({'kind': kind, 'x': x + dx, 'y': y + dy,
            'footprint': list(server.BUILDING_FOOTPRINTS.get(kind, (1, 1)))})
    store, sawpit = town['buildings'][-2:]
    store.update(level=2, storage_enabled={'wood': False, 'food': True, 'bread': False},
                 stored_goods={'wood': 7.25, 'bread': 3.5, 'cakes': 1.25})
    sawpit.update(stored_goods={'planks': 1.5}, output_buffer={'planks': 2.25},
                  input_buffer={'wood': 4.125}, pending_output={'planks': 2.875},
                  orphaned_output={'planks': .5})
    # Real existing residents retain ledger output ownership independently of a
    # playback adapter. Their saved idle actor state is checked, never fabricated
    # to stand in for a producing/moving actor (still refused below).
    for worker, amount in zip(town['villagers'], (1.125, 1.25)):
        worker.update(pending_output={'planks': amount},
                      pending_workplace={k: sawpit[k] for k in ('kind', 'x', 'y')})
    town['wood'] += 7.25
    town['goods'] = {'bread': 3.5, 'cakes': 1.25, 'planks': 3.75}
    town['food'] += sum(n * server.economy.EDIBLE.get(k, 0) for k, n in town['goods'].items())
    return config


class StorageProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return storage_fixture(cls.chunked)

    def test_full_storage_snapshots_are_frozen_and_match_python(self):
        status, value = self.call('/api/world')
        self.assertEqual(status, 200)
        value.pop('players_sampled_at')
        self.assertEqual(value, terrain.reference(self.config, {}))
        town = value['towns']['first']
        self.assertEqual(town['goods']['planks'], 3.75)
        self.assertEqual(town['buildings'][-1]['input_buffer'], {'wood': 4.125})
        for _ in range(2):
            later = self.call('/api/world')[1]
            later.pop('players_sampled_at')
            self.assertEqual(later, value)
        players = self.call('/api/players')[1]
        self.assertEqual(players['players'], {k: t['player'] for k, t in value['towns'].items()})
        for private in ('SYNTHETIC_SECRET_TOKEN', 'credentials', 'legacy_allocations'):
            self.assertNotIn(private, json.dumps(value))


class PythonStorage(StorageProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonStorageOrdinary(PythonStorage):
    chunked = False


class RustStorage(StorageProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_legacy_filters_defaults_empty_buffers_and_reload(self):
        for case in ('legacy', 'null-filter', 'empty-filter', 'no-mirrors', 'empty-buffers',
                     'unfinished', 'equipment', 'zero-keys'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                town = entry['world']['towns']['first']
                store, sawpit = town['buildings'][-2:]
                if case in ('legacy', 'null-filter'):
                    store.pop('storage_enabled')
                    store['storage_allocations'] = {'food': 10, 'wood': 0, 'bread': 0}
                    if case == 'null-filter':
                        store['storage_enabled'] = None
                elif case == 'empty-filter':
                    store['storage_enabled'] = {}
                elif case == 'no-mirrors':
                    entry['native_storage']['mirrors'] = False
                elif case == 'empty-buffers':
                    for field in ('pending_output', 'orphaned_output', 'input_buffer'):
                        sawpit[field] = {}
                    for worker in town['villagers']:
                        worker.pop('pending_output', None)
                        worker.pop('pending_workplace', None)
                elif case == 'unfinished':
                    store['construction'] = {'progress': 1.125, 'total': 30, 'upgrade': {'level': 3}}
                elif case == 'equipment':
                    town['buildings'].append({'kind': 'mill', 'x': store['x'] + 3, 'y': store['y'] + 3,
                        'footprint': list(server.BUILDING_FOOTPRINTS.get('mill', (1, 1))),
                        'profession_upgrades': ['improved_millstones'],
                        'input_buffer': {'wheat': 3.125}, 'output_buffer': {'flour': 2.5}})
                    town['goods']['flour'] = 2.5
                elif case == 'zero-keys':
                    sawpit['input_buffer']['planks'] = 0
                    town['goods']['stone'] = 0
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_missing_unknown_and_private_storage_refuse_before_listening(self):
        cases = ('stored', 'output', 'input', 'pending', 'orphan', 'goods', 'food', 'filter',
                 'allocations', 'missing-store', 'missing-town-owner', 'missing-building-owner',
                 'missing-mirror', 'stale-stored', 'stale-pending', 'worker-output',
                 'worker-place', 'worker-kind', 'worker-owner', 'worker-missing',
                 'unowned-workplace', 'native-work', 'private-buffer', 'private-filter',
                 'private-allocations', 'private-goods', 'private-worker', 'unknown-building',
                 'migration', 'unknown-town')
        for case in cases:
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                entry['native_source'] = copy.deepcopy(entry['world'])
                town = entry['world']['towns']['first']
                store, sawpit = town['buildings'][-2:]
                worker = town['villagers'][0]
                if case in ('stored', 'output', 'input', 'pending', 'orphan'):
                    field = {'stored': 'stored_goods', 'output': 'output_buffer', 'input': 'input_buffer',
                             'pending': 'pending_output', 'orphan': 'orphaned_output'}[case]
                    sawpit[field][next(iter(sawpit[field]))] += .25
                elif case in ('goods', 'food'):
                    if case == 'goods': town['goods']['planks'] += 1
                    else: town['food'] += 1
                elif case == 'filter': store['storage_enabled']['wood'] = True
                elif case == 'allocations': store['storage_allocations'] = {'wood': 20}
                elif case == 'missing-store': entry['native_storage']['missing_store'] = True
                elif case == 'missing-town-owner': entry['native_towns'] = False
                elif case == 'missing-building-owner': entry['native_source']['towns']['first']['buildings'].pop(1)
                elif case == 'missing-mirror': sawpit.pop('input_buffer')
                elif case in ('stale-stored', 'stale-pending'):
                    entry['native_storage'][case.replace('-', '_')] = True
                elif case == 'worker-output': worker['pending_output']['planks'] += .25
                elif case == 'worker-place': worker['pending_workplace']['x'] += 1
                elif case == 'worker-kind': worker['pending_workplace']['kind'] = 'mill'
                elif case == 'worker-owner':
                    other = town['villagers'][2]
                    other.update(pending_output=worker.pop('pending_output'),
                                 pending_workplace=worker.pop('pending_workplace'))
                elif case == 'worker-missing': worker.pop('pending_workplace')
                elif case == 'unowned-workplace': town['villagers'][2]['pending_workplace'] = worker['pending_workplace']
                elif case == 'native-work': entry['native_work'] = True
                elif case.startswith('private-'):
                    target = {'private-buffer': sawpit['input_buffer'], 'private-filter': store['storage_enabled'],
                              'private-allocations': store.setdefault('storage_allocations', {}),
                              'private-goods': town['goods'], 'private-worker': worker['pending_workplace']}[case]
                    target['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'unknown-building': sawpit['unknown_storage'] = {}
                elif case == 'unknown-town': town['unknown_inventory'] = {}
                elif case == 'migration': town['inventory_version'] = 0
                with tempfile.TemporaryDirectory(dir=self.directory.name) as directory:
                    native = Path(directory) / 'native'
                    self.make_native(native, config)
                    before = {p.relative_to(native): p.read_bytes() for p in native.rglob('*') if p.is_file()}
                    result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
                        '--source', str(native), '--bind', '127.0.0.1:0'], capture_output=True, timeout=10)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn(b'listening', result.stdout)
                    self.assertIn(b'coverage', result.stderr)
                    self.assertNotIn(b'SYNTHETIC_PRIVATE_CANARY', result.stdout + result.stderr)
                    self.assertEqual(before, {p.relative_to(native): p.read_bytes() for p in native.rglob('*') if p.is_file()})


class RustStorageOrdinary(RustStorage):
    chunked = False


if __name__ == '__main__':
    unittest.main()
