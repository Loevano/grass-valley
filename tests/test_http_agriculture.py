"""R20-1g frozen linked agriculture; synthetic owners, never live hydration."""
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
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/agriculture.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/buildings.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def agriculture_fixture(chunked=True):
    config = populated.populated_fixture(chunked)
    town = config['worlds'][0]['world']['towns']['first']
    farm = town['buildings'][2]
    farm.update(crop='wheat', field={'stage': 'plowed', 'elapsed': 0, 'progress': 1.125, 'cycle': 2})
    for i, stage in enumerate(('unplowed', 'plowed', 'sprouting', 'thirsty', 'growing', 'ripe')):
        state = {'stage': stage, 'elapsed': 1.125 if stage in ('sprouting', 'growing') else 0,
                 'progress': 0 if stage in ('sprouting', 'growing') else 1.25, 'cycle': i + 3}
        if stage not in ('unplowed', 'plowed'):
            state['crop'] = 'wheat'
        if stage == 'ripe':
            state.update(progress=2, yield_remaining=3.125)
        town['buildings'].append({'kind': 'acre', 'x': farm['x'] + i % 3 - 1,
            'y': farm['y'] + i // 3 + 1, 'footprint': [1, 1], 'farm_index': 2,
            'crop': 'vegetables', 'field': state})
    index = len(town['buildings'])
    orchard = {'kind': 'orchard', 'x': farm['x'] + 9, 'y': farm['y'], 'footprint': [1, 1]}
    town['buildings'].append(orchard)
    for i, stage in enumerate(('sapling', 'blossom', 'fruit', 'ripe', 'harvested')):
        town['buildings'].append({'kind': 'orchard_tree', 'x': orchard['x'] + i % 3 - 1,
            'y': orchard['y'] + i // 3 + 1, 'footprint': [1, 1], 'orchard_index': index,
            'tree': {'stage': stage, 'elapsed': 0 if stage == 'ripe' else 2.125,
                     'fruit': 0 if stage == 'harvested' else 3.125}})
    return config


class AgricultureProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return agriculture_fixture(cls.chunked)

    def test_full_agricultural_snapshots_are_frozen_and_match_python(self):
        status, value = self.call('/api/world')
        self.assertEqual(status, 200)
        value.pop('players_sampled_at')
        self.assertEqual(value, terrain.reference(self.config, {}))
        buildings = value['towns']['first']['buildings']
        self.assertEqual(sum(b['kind'] == 'acre' for b in buildings), 6)
        self.assertEqual(sum(b['kind'] == 'orchard_tree' for b in buildings), 5)
        for _ in range(2):
            later = self.call('/api/world')[1]
            later.pop('players_sampled_at')
            self.assertEqual(later, value)
        self.assertNotIn('SYNTHETIC_SECRET_TOKEN', json.dumps(value))


class PythonAgriculture(AgricultureProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonAgricultureOrdinary(PythonAgriculture):
    chunked = False


class RustAgriculture(AgricultureProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_defaults_optional_fields_relocated_groups_and_reload(self):
        for case in ('defaults', 'optional-null', 'implicit-crop', 'relocated', 'reordered'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                town = config['worlds'][0]['world']['towns']['first']
                buildings = town['buildings']
                if case == 'defaults':
                    for b in buildings:
                        for key in ('crop', 'field', 'tree'):
                            b.pop(key, None)
                elif case == 'optional-null':
                    next(b for b in buildings if b['kind'] == 'acre')['field'].update(crop=None, yield_remaining=None)
                elif case == 'implicit-crop':
                    next(b for b in buildings if b.get('field', {}).get('stage') == 'sprouting')['field'].pop('crop')
                elif case == 'relocated':
                    # Persisted group relocation: both owners reload translated
                    # geometry with the same identities, parents and crop clocks.
                    for b in buildings:
                        if b['kind'] in ('farm', 'acre', 'orchard', 'orchard_tree'):
                            b['x'] += 2
                            b['y'] += 3
                elif case == 'reordered':
                    order = list(reversed(range(len(buildings))))
                    town['buildings'] = [buildings[i] for i in order]
                    for b in town['buildings']:
                        for key in ('farm_index', 'orchard_index'):
                            if key in b:
                                b[key] = order.index(b[key])
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_missing_private_and_unknown_agriculture_refuses(self):
        cases = ('parent', 'wrong-parent-kind', 'negative-parent', 'missing-parent', 'null-parent',
                 'geometry', 'footprint', 'crop', 'stage', 'elapsed', 'progress', 'cycle',
                 'field-crop', 'yield', 'tree-stage', 'tree-clock', 'fruit', 'missing-field',
                 'missing-tree', 'missing-fields-owner', 'missing-trees-owner', 'stale-child',
                 'assigned', 'private-field', 'private-tree', 'unknown-building', 'wrong-kind-field',
                 'unknown-crop', 'null-field', 'null-tree')
        for case in cases:
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                entry['native_source'] = copy.deepcopy(entry['world'])
                buildings = entry['world']['towns']['first']['buildings']
                acre = next(b for b in buildings if b['kind'] == 'acre')
                ripe = next(b for b in buildings if b.get('field', {}).get('stage') == 'ripe')
                tree = buildings[-1]
                if case == 'parent': acre['farm_index'] = len(buildings)
                elif case == 'wrong-parent-kind': acre['farm_index'] = 1
                elif case == 'negative-parent': acre['farm_index'] = -1
                elif case == 'missing-parent': acre.pop('farm_index')
                elif case == 'null-parent': acre['farm_index'] = None
                elif case == 'geometry': acre['x'] += 1
                elif case == 'footprint': acre['footprint'] = [2, 1]
                elif case == 'crop': acre['crop'] = 'wheat'
                elif case in ('stage', 'elapsed', 'progress', 'cycle'):
                    acre['field'][case] = 'plowed' if case == 'stage' else 1.5
                elif case == 'field-crop': ripe['field']['crop'] = 'vegetables'
                elif case == 'yield': ripe['field']['yield_remaining'] += .25
                elif case == 'tree-stage': tree['tree']['stage'] = 'sapling'
                elif case == 'tree-clock': tree['tree']['elapsed'] += .25
                elif case == 'fruit': tree['tree']['fruit'] = 1
                elif case == 'missing-field': acre.pop('field')
                elif case == 'missing-tree': tree.pop('tree')
                elif case in ('missing-fields-owner', 'missing-trees-owner', 'stale-child', 'assigned'):
                    key = {'missing-fields-owner': 'missing_fields', 'missing-trees-owner': 'missing_trees',
                           'stale-child': 'stale_child', 'assigned': 'assigned'}[case]
                    entry['native_agriculture'] = {key: True}
                elif case == 'private-field': acre['field']['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'private-tree': tree['tree']['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'unknown-building': acre['unknown_agriculture'] = True
                elif case == 'wrong-kind-field': buildings[0]['field'] = acre['field']
                elif case == 'unknown-crop': acre['crop'] = 'credentials'
                elif case == 'null-field': acre['field'] = None
                elif case == 'null-tree': tree['tree'] = None
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


class RustAgricultureOrdinary(RustAgriculture):
    chunked = False


if __name__ == '__main__':
    unittest.main()
