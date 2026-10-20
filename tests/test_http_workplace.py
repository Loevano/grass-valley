"""R20-1h.b: R18 assignments and frozen profession upgrade helpers."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_http_construction as construction
import test_http_populated as populated
import test_http_snapshots as terrain

RUST_SOURCES = construction.RUST_SOURCES + (
    'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/work.rs',)


def workplace_fixture(chunked=True):
    config = construction.construction_fixture(chunked)
    town = config['worlds'][0]['world']['towns']['first']
    farm = town['buildings'][2]
    farm['construction'] = {'progress': 3.125, 'total': 30, 'upgrade': {'level': 2}}
    worker, _, waiting, unassigned, _ = town['villagers']
    target = {k: farm[k] for k in ('kind', 'x', 'y', 'footprint')}
    worker.update(job='farming', building_index=2,
        construction_target=target,
        construction_position={'x': farm['x'] - 1, 'y': farm['y']},
        construction_walk={'target': copy.deepcopy(target), 'job': 'farming',
            'path': [[farm['x'] - 2, farm['y']], [farm['x'] - 1, farm['y']]], 'step': 1},
        construction_arrived=True)
    worker['facing'] = 1
    waiting.update(job='farming', building_index=2)
    waiting.pop('construction_target')
    unassigned.update(job='farming', building_index=None)
    return config


class WorkplaceProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return workplace_fixture(cls.chunked)

    def test_complete_snapshot_assignments_helpers_and_repeated_reads(self):
        for _ in range(3):
            status, value = self.call('/api/world')
            self.assertEqual(status, 200)
            value.pop('players_sampled_at')
            self.assertEqual(value, terrain.reference(self.config, {}))
            workers = value['towns']['first']['villagers']
            self.assertEqual(workers[0]['building_index'], 2)
            self.assertEqual(workers[2]['building_index'], 2)
            self.assertIsNone(workers[3]['building_index'])
            self.assertNotIn('SYNTHETIC_SECRET', json.dumps(value))


class PythonWorkplace(WorkplaceProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonWorkplaceOrdinary(PythonWorkplace):
    chunked = False


class RustWorkplace(WorkplaceProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_waiting_walking_interrupted_and_reload(self):
        for case in ('walking', 'waiting', 'interrupted', 'unassigned', 'linked-parent', 'omitted-assignment'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                worker = config['worlds'][0]['world']['towns']['first']['villagers'][0]
                worker.pop('construction_arrived')
                if case == 'linked-parent':
                    town = config['worlds'][0]['world']['towns']['first']
                    farm = town['buildings'][2]
                    worker['building_index'] = len(town['buildings'])
                    town['buildings'].append({'kind': 'acre', 'x': farm['x'] + 1,
                        'y': farm['y'] + 1, 'footprint': [1, 1], 'farm_index': 2})
                    worker['construction_arrived'] = True
                elif case == 'omitted-assignment':
                    entry = config['worlds'][0]
                    worker['construction_arrived'] = True
                    entry['native_source'] = copy.deepcopy(entry['world'])
                    worker.pop('building_index')
                elif case == 'walking':
                    worker['construction_walk']['step'] = 0
                    worker['construction_position']['x'] -= 1
                elif case == 'waiting':
                    worker.pop('construction_walk')
                else:
                    worker.pop('construction_walk')
                    worker.pop('construction_target')
                    if case == 'unassigned': worker['building_index'] = None
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_assignment_controller_private_and_uncovered_work_refuse(self):
        for case in ('different', 'null', 'negative', 'fractional', 'out-of-range',
                     'boolean', 'private-index', 'controller', 'work-animation', 'implicit', 'suspended-work'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                entry['native_source'] = copy.deepcopy(entry['world'])
                worker = entry['world']['towns']['first']['villagers'][0]
                if case == 'different': worker['building_index'] = 1
                elif case == 'null': worker['building_index'] = None
                elif case == 'negative': worker['building_index'] = -1
                elif case == 'fractional': worker['building_index'] = 2.5
                elif case == 'out-of-range': worker['building_index'] = 10**15
                elif case == 'boolean': worker['building_index'] = True
                elif case == 'private-index': worker['building_index'] = {'credentials': 'SYNTHETIC_PRIVATE_CANARY'}
                elif case == 'controller':
                    entry['native_source']['towns']['first']['villagers'][0]['job'] = 'building'
                elif case == 'suspended-work': entry['native_people'] = {'suspended_work': True}
                elif case == 'work-animation': worker['work_animation'] = {'phase': 'waiting', 'path': []}
                elif case == 'implicit':
                    for world in (entry['world'], entry['native_source']):
                        world['towns']['first']['villagers'][3].pop('building_index')
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


class RustWorkplaceOrdinary(RustWorkplace):
    chunked = False


if __name__ == '__main__':
    unittest.main()
