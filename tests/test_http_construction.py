"""R20-1h.a: frozen native construction controllers and saved walk parity."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_http_populated as populated
import test_http_snapshots as terrain

RUST_SOURCES = ('rust/crates/lantern-host/src/legacy/http_snapshot/metadata.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/construction.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def construction_fixture(chunked=True):
    config = populated.populated_fixture(chunked)
    town = config['worlds'][0]['world']['towns']['first']
    x, y = town['town_origin']['x'], town['town_origin']['y']
    site = {'kind': 'home', 'x': x + 12, 'y': y + 8, 'footprint': [1, 1],
            'construction': {'progress': 2.125, 'total': 30}}
    waiting = dict(site, x=x + 15)
    town['buildings'].extend([site, waiting])
    road = {'x': x + 10, 'y': y + 4, 'construction': {'progress': .125, 'total': 4}}
    town['roads'] = [road]
    for worker in town['villagers']:
        worker.pop('x', None)
        worker.pop('y', None)
    first, helper, pending, available, idle = town['villagers']
    target = {k: site[k] for k in ('kind', 'x', 'y', 'footprint')}
    first.update(job='building', facing=-1,
        construction_position={'x': site['x'] - 1.875, 'y': site['y']},
        construction_target=target,
        construction_walk={'target': copy.deepcopy(target), 'job': 'building',
            'path': [[site['x'] - 1.875, site['y']], [site['x'] - 2, site['y']],
                     [site['x'] - 1, site['y']]], 'step': 0})
    road_target = {'kind': 'road', 'x': road['x'], 'y': road['y']}
    helper.update(construction_position={'x': road['x'] - 1, 'y': road['y']},
        construction_target=road_target, construction_arrived=True,
        construction_walk={'target': copy.deepcopy(road_target), 'job': 'idle',
            'path': [[road['x'] - 2, road['y']], [road['x'] - 1, road['y']]], 'step': 1})
    pending.update(job='building', construction_position={'x': x + 1.125, 'y': y + 3},
        construction_target={k: waiting[k] for k in ('kind', 'x', 'y', 'footprint')})
    # An unclaimed dedicated builder retains its native registration.
    available.update(job='building', construction_position={'x': 0, 'y': 0})
    idle.update(construction_position={'x': x + 3.125, 'y': y + 2.25})
    return config


class ConstructionProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return construction_fixture(cls.chunked)

    def test_complete_frozen_snapshots_and_repeated_reads_match_python(self):
        status, value = self.call('/api/world')
        self.assertEqual(status, 200)
        value.pop('players_sampled_at')
        self.assertEqual(value, terrain.reference(self.config, {}))
        workers = value['towns']['first']['villagers']
        self.assertEqual(workers[0]['construction_walk']['step'], 0)
        self.assertTrue(workers[1]['construction_arrived'])
        self.assertNotIn('construction_walk', workers[2])
        for _ in range(2):
            later = self.call('/api/world')[1]
            later.pop('players_sampled_at')
            self.assertEqual(later, value)
        players = self.call('/api/players')[1]
        self.assertEqual(players['players'], {k: t['player'] for k, t in value['towns'].items()})
        self.assertNotIn('SYNTHETIC_SECRET_TOKEN', json.dumps(value))


class PythonConstruction(ConstructionProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonConstructionOrdinary(PythonConstruction):
    chunked = False


class RustConstruction(ConstructionProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_waiting_arrival_interruption_defaults_and_durable_reload(self):
        for case in ('last-waypoint', 'working', 'interrupted', 'absent-footprint', 'false-arrived', 'top-pose'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                worker = config['worlds'][0]['world']['towns']['first']['villagers'][0]
                if case in ('last-waypoint', 'working'):
                    walk = worker['construction_walk']
                    walk['step'] = len(walk['path']) - 1
                    worker['construction_position'] = dict(zip(('x', 'y'), walk['path'][-1]))
                    if case == 'working': worker['construction_arrived'] = True
                elif case == 'interrupted':
                    worker['job'] = 'idle'
                    worker.pop('construction_target')
                    worker.pop('construction_walk')
                elif case == 'absent-footprint':
                    worker['construction_target'].pop('footprint')
                elif case == 'false-arrived': worker['construction_arrived'] = False
                elif case == 'top-pose': worker.update(worker['construction_position'])
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_missing_private_unknown_and_unsupported_state_refuses(self):
        cases = ('pose', 'facing', 'job', 'target', 'target-kind', 'target-dimensions',
                 'walk-target', 'path', 'step', 'walk-job', 'arrived', 'missing-target',
                 'missing-walk', 'missing-native-controller', 'private-pose', 'private-target',
                 'private-walk', 'private-path', 'unknown-worker', 'workplace-playback')
        for case in cases:
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                entry['native_source'] = copy.deepcopy(entry['world'])
                worker = entry['world']['towns']['first']['villagers'][0]
                walk = worker['construction_walk']
                if case == 'pose': worker['construction_position']['x'] += .125
                elif case == 'facing': worker['facing'] = 1
                elif case == 'job': worker['job'] = 'idle'
                elif case == 'target': worker['construction_target']['x'] += 1
                elif case == 'target-kind': worker['construction_target']['kind'] = 'farm'
                elif case == 'target-dimensions': worker['construction_target']['footprint'] = [2, 1]
                elif case == 'walk-target': walk['target']['y'] += 1
                elif case == 'path': walk['path'][0][0] += .125
                elif case == 'step': walk['step'] += 1
                elif case == 'walk-job': walk['job'] = 'idle'
                elif case == 'arrived': worker['construction_arrived'] = True
                elif case == 'missing-target': worker.pop('construction_target')
                elif case == 'missing-walk': worker.pop('construction_walk')
                elif case == 'missing-native-controller':
                    native = entry['native_source']['towns']['first']['villagers'][0]
                    native['job'] = 'idle'
                    native.pop('construction_target')
                    native.pop('construction_walk')
                elif case.startswith('private-'):
                    target = {'private-pose': worker['construction_position'],
                              'private-target': worker['construction_target'], 'private-walk': walk,
                              'private-path': None}[case]
                    if target is None: walk['path'][0] = {'credentials': 'SYNTHETIC_PRIVATE_CANARY'}
                    else: target['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'unknown-worker': worker['construction_secret'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'workplace-playback': worker['work_animation'] = {'phase': 'waiting', 'path': []}
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


class RustConstructionOrdinary(RustConstruction):
    chunked = False


if __name__ == '__main__':
    unittest.main()
