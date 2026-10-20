"""R20-1i.a frozen woodcutting claims; synthetic owners, never live hydration."""
import copy
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_http_playback as playback
import test_http_populated as populated
import test_http_snapshots as terrain

RUST_SOURCES = playback.RUST_SOURCES + (
    'rust/crates/lantern-host/src/gathering.rs',)


def gathering_fixture(chunked=True):
    config = playback.playback_fixture(chunked)
    entry = config['worlds'][0]
    entry['native_gathering'] = {'claims': True}
    for key in ('world', 'native_source'):
        town = entry[key]['towns']['first']
        worker = town['villagers'][2]
        camp = town['buildings'][worker['building_index']]
        camp['kind'] = 'woodcutter'
        worker['job'] = 'woodcutting'
        a = worker['work_animation']
        a['task'] = ['woodcutting', 'woodcutter', camp['x'], camp['y'],
                     'tree', camp['x'] - 2, camp['y'] + 1]
        a['elapsed'] = 0
        for s in a['playback']:
            s['elapsed'] = 0
    town = entry['world']['towns']['first']
    task = town['villagers'][2]['work_animation']['task']
    tree = dict(kind='tree', x=task[5], y=task[6], supply=5.125,
                supply_max=8, regeneration=0)
    landscape = (entry['records'][f'{tree["x"] // 40},{tree["y"] // 40}']
                 if chunked else entry['world']['shared_map'])
    landscape['map_resources'].append(tree)
    return config


def phase_case(config, phase):
    entry = config['worlds'][0]
    playback.static_case(config, phase)
    clock = .125 if phase in ('working', 'depositing') else 0
    for key in ('world', 'native_source'):
        a = entry[key]['towns']['first']['villagers'][2]['work_animation']
        a['elapsed'] = clock
        for s in a['playback']:
            s['elapsed'] = clock


class GatheringProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return gathering_fixture(cls.chunked)

    def test_complete_snapshot_and_repeated_reads(self):
        for _ in range(3):
            status, value = self.call('/api/world')
            self.assertEqual(status, 200)
            value.pop('players_sampled_at')
            self.assertEqual(value, terrain.reference(self.config, {}))


class PythonGathering(GatheringProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonGatheringOrdinary(PythonGathering):
    chunked = False


class RustGathering(GatheringProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_work_return_depleted_deposit_wait_and_interruption_reload(self):
        for case in ('working', 'returning', 'depleted', 'depositing', 'pending',
                     'unreachable', 'blocked', 'interrupted'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                if case in ('pending', 'unreachable', 'blocked'):
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][2]
                        a = w['work_animation']
                        x, y = a['playback'][0]['x'], a['y']
                        a.update(x=x, phase='waiting', resume_phase='walking', path=[],
                                 playback=[playback.sample(x, y, 'waiting', elapsed=0)])
                        if case == 'pending': a['route_pending'] = True
                        if case == 'blocked':
                            a['playback'].append(playback.sample(x, y, 'waiting', .25, elapsed=0))
                        if key == 'native_source':
                            w['_native_trace']['mode'] = case
                            if case != 'blocked': w['_native_trace'].pop('seconds')
                elif case == 'interrupted':
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][2]
                        a = w.pop('work_animation')
                        w.update(job='idle', building_index=None, x=a['x'], y=a['y'], facing=a['facing'])
                        w.pop('_native_trace', None)
                else:
                    phase_case(config, 'returning' if case == 'depleted' else case)
                    if case in ('returning', 'depleted', 'depositing'):
                        for key in ('world', 'native_source'):
                            town = entry[key]['towns']['first']
                            w = town['villagers'][2]
                            camp = town['buildings'][w['building_index']]
                            w['pending_output'] = {'wood': .375}
                            w['pending_workplace'] = {k: camp[k] for k in ('kind', 'x', 'y')}
                            camp['pending_output'] = {'wood': .375}
                    if case == 'depleted':
                        task = entry['world']['towns']['first']['villagers'][2]['work_animation']['task']
                        landscapes = (entry['records'].values() if self.chunked else
                                      [entry['world']['shared_map']])
                        for landscape in landscapes:
                            landscape['map_resources'] = [r for r in landscape['map_resources']
                                if (r['x'], r['y']) != tuple(task[5:7])]
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_missing_stale_wrong_claim_clock_source_and_private_refuse(self):
        for case in ('missing-claim', 'stale-claim', 'missing-source', 'wrong-source',
                     'outbound-clock', 'working-clock', 'deposit-clock', 'missing-frame',
                     'stale-frame', 'private-frame', 'private-sample', 'private-worker'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                w = entry['world']['towns']['first']['villagers'][2]
                a = w['work_animation']
                if case == 'missing-claim': entry['native_gathering']['missing'] = True
                elif case == 'stale-claim': entry['native_gathering']['stale'] = True
                elif case == 'missing-source':
                    landscapes = entry['records'].values() if self.chunked else [entry['world']['shared_map']]
                    for landscape in landscapes:
                        landscape['map_resources'] = [r for r in landscape['map_resources']
                            if (r['x'], r['y']) != tuple(a['task'][5:7])]
                elif case == 'wrong-source': a['task'][4] = 'bush'
                elif case.endswith('-clock'):
                    phase = {'working-clock': 'working', 'deposit-clock': 'depositing'}.get(case, 'walking')
                    phase_case(config, phase)
                    animation = entry['world']['towns']['first']['villagers'][2]['work_animation']
                    animation['elapsed'] = 4
                    for s in animation['playback']: s['elapsed'] = 4
                elif case == 'missing-frame': w.pop('work_animation')
                elif case == 'stale-frame': a['elapsed'] += .125
                elif case == 'private-frame': a['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'private-sample': a['playback'][0]['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'private-worker': w['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
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


class RustGatheringOrdinary(RustGathering):
    chunked = False


if __name__ == '__main__':
    unittest.main()
