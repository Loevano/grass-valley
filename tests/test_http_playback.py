"""R20-1h.b.2: independent native observation checkpoints, never route caches."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_http_populated as populated
import test_http_snapshots as terrain
import test_http_workplace as workplace

RUST_SOURCES = workplace.RUST_SOURCES + (
    'rust/crates/lantern-core/src/model.rs',
    'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/work.rs',
    'rust/crates/lantern-host/src/work_observation.rs',
    'rust/crates/lantern-host/src/persistence/snapshot.rs',
    'rust/crates/lantern-host/src/legacy/http_snapshot.rs',)


def sample(x, y, phase, time=0, elapsed=.125, facing=1):
    return dict(x=x, y=y, phase=phase, time=time, elapsed=elapsed, facing=facing,
                waterReady=False, waterSource=None, farmAction=None)


def playback_fixture(chunked=True):
    config = workplace.workplace_fixture(chunked)
    entry = config['worlds'][0]
    entry['native_people'] = {'observations': True}
    town = entry['world']['towns']['first']
    farm = town['buildings'][2]
    # Generic producer movement is isolated from the R16 agricultural activity
    # adapter. A sawyer's source is its own completed workplace.
    index = len(town['buildings'])
    shop = dict(kind='sawpit', x=farm['x'] + 6, y=farm['y'] + 3, footprint=[1, 1])
    town['buildings'].append(shop)
    worker = town['villagers'][2]
    worker.update(job='woodworking', building_index=index)
    worker.pop('construction_position')
    x, y = shop['x'] - 3, shop['y']
    animation = dict(task=['woodworking', 'sawpit', shop['x'], shop['y'],
                           'sawpit', shop['x'], shop['y']],
        x=x + .45, y=y, facing=1, phase='walking', elapsed=.125,
        path=[{'x': x + 1, 'y': y}, {'x': x + 2, 'y': y}], version=7, relocation=2,
        waterReady=False, waterSource=None, farmAction=None,
        playback=[sample(x, y, 'walking'), sample(x + .45, y, 'walking', .25)])
    # Reserve a tiny synthetic walking lane in the generated landscape. Ordinary
    # seed 42 has a tree on the first waypoint; geometry checks must reject that
    # real obstacle unless the fixture explicitly clears it.
    lane = {(cx, cy) for cx in range(x, x + 4) for cy in range(y, y + 3)}
    if chunked:
        for record in entry['records'].values():
            record['map_resources'] = [r for r in record['map_resources'] if (r['x'], r['y']) not in lane]
    else:
        landscape = entry['world']['shared_map']
        landscape['map_resources'] = [r for r in landscape['map_resources'] if (r['x'], r['y']) not in lane]
    worker['work_animation'] = animation
    worker.pop('facing', None)
    entry['native_source'] = copy.deepcopy(entry['world'])
    entry['native_source']['towns']['first']['villagers'][2]['_native_trace'] = dict(
        origin=[x, y], facing=1, mode='ready', path=[[x + 1, y], [x + 2, y]], seconds=.25)
    return config


def static_case(config, phase='working', mode=None):
    entry = config['worlds'][0]
    for key in ('world', 'native_source'):
        worker = entry[key]['towns']['first']['villagers'][2]
        a = worker['work_animation']
        a.update(phase=phase, path=[], playback=[sample(a['x'], a['y'], phase)])
        worker.pop('_native_trace', None)
        if mode:
            worker['_native_trace'] = dict(origin=[a['x'], a['y']], facing=1, mode=mode)
    # Trace inputs are native-only, never public archive extensions.
    entry['world']['towns']['first']['villagers'][2].pop('_native_trace', None)


class PlaybackProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return playback_fixture(cls.chunked)

    def test_complete_frozen_snapshot_and_repeated_reads(self):
        for _ in range(3):
            status, value = self.call('/api/world')
            self.assertEqual(status, 200)
            value.pop('players_sampled_at')
            self.assertEqual(value, terrain.reference(self.config, {}))
            self.assertNotIn('SYNTHETIC_SECRET', json.dumps(value))
            self.assertNotIn('_native_trace', json.dumps(value))


class PythonPlayback(PlaybackProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonPlaybackOrdinary(PythonPlayback):
    chunked = False


class RustPlayback(PlaybackProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_stationary_waiting_arrived_suspension_interruption_and_reload(self):
        for case in ('working', 'depositing', 'returning', 'arrived', 'pending', 'unreachable',
                     'blocked', 'suspended', 'interrupted', 'turning', 'implicit-farm', 'implicit-none', 'optional-defaults'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                if case in ('working', 'depositing', 'arrived'):
                    static_case(config, 'depositing' if case == 'depositing' else 'working',
                                'arrived' if case == 'arrived' else None)
                elif case == 'returning':
                    for key in ('world', 'native_source'):
                        a = entry[key]['towns']['first']['villagers'][2]['work_animation']
                        a['phase'] = 'returning'
                        for s in a['playback']: s['phase'] = 'returning'
                elif case in ('pending', 'unreachable', 'blocked'):
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][2]
                        a = w['work_animation']
                        x, y = a['playback'][0]['x'], a['y']
                        a.update(x=x, phase='waiting', resume_phase='walking', path=[])
                        if case == 'pending': a['route_pending'] = True
                        a['playback'] = [sample(x, y, 'waiting')]
                        if case == 'blocked':
                            a['playback'] = [sample(x, y, 'waiting'), sample(x, y, 'waiting', .25)]
                        if key == 'native_source':
                            w['_native_trace']['mode'] = case
                            if case != 'blocked': w['_native_trace'].pop('seconds')
                elif case == 'interrupted':
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][2]
                        a = w.pop('work_animation')
                        w.update(job='idle', building_index=None, x=a['x'], y=a['y'], facing=a['facing'])
                        w.pop('_native_trace', None)
                elif case == 'turning':
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][2]
                        a = w['work_animation']
                        x, y = a['playback'][0]['x'], a['playback'][0]['y']
                        duration, step = 1.25, 1 / 1.8
                        first_left = duration - step
                        second_left = first_left - step
                        end_y = y + 1 + second_left / step
                        a.update(x=x + 1, y=end_y, facing=-1, path=[{'x': x + 1, 'y': y + 2}],
                            playback=[sample(x, y, 'walking'),
                                sample(x + 1, y, 'walking', duration - first_left),
                                sample(x + 1, y + 1, 'walking', duration - second_left, facing=-1),
                                sample(x + 1, end_y, 'walking', duration, facing=-1)])
                        if key == 'native_source':
                            w['_native_trace'].update(path=[[x + 1, y], [x + 1, y + 1], [x + 1, y + 2]], seconds=duration)
                elif case == 'suspended':
                    # The same R11 observation belongs to the retained intent;
                    # the helper's current pose is independently checked by R12.
                    for key in ('world', 'native_source'):
                        town = entry[key]['towns']['first']
                        w = town['villagers'][2]
                        shop = town['buildings'][w['building_index']]
                        shop['construction'] = {'progress': 1.125, 'total': 30, 'upgrade': {'level': 2}}
                        target = {k: shop[k] for k in ('kind', 'x', 'y', 'footprint')}
                        w.update(construction_target=target,
                            construction_position={'x': shop['x'] - 1, 'y': shop['y']},
                            construction_arrived=True,
                            construction_walk=dict(target=copy.deepcopy(target), job='woodworking',
                                path=[[shop['x'] - 1, shop['y']]], step=0))
                elif case.startswith('implicit-'):
                    for key in ('world', 'native_source'):
                        w = entry[key]['towns']['first']['villagers'][3]
                        w.pop('building_index', None)
                        if case == 'implicit-none': w['job'] = 'mining'
                    if case == 'implicit-farm':
                        entry['native_source']['towns']['first']['villagers'][3]['building_index'] = 2
                elif case == 'optional-defaults':
                    static_case(config)
                    for key in ('world', 'native_source'):
                        a = entry[key]['towns']['first']['villagers'][2]['work_animation']
                        for field in ('waterReady', 'waterSource', 'farmAction'): a.pop(field)
                        a['route_pending'] = False
                        for s in a['playback']: s['waterReady'] = None
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_missing_private_history_and_owner_facts_refuse(self):
        for case in ('task', 'phase', 'elapsed', 'pose', 'facing', 'path', 'history-middle',
                     'history-time', 'version', 'relocation', 'missing-frame', 'missing-owner',
                     'private-frame', 'private-sample', 'private-path', 'field-action', 'cargo-owner',
                     'native-elapsed', 'native-assignment', 'new-obstacle', 'void-terrain'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                w = entry['world']['towns']['first']['villagers'][2]
                a = w['work_animation']
                if case == 'task': a['task'][3] += 1
                elif case == 'phase': a['phase'] = 'returning'
                elif case == 'elapsed': a['elapsed'] += .25
                elif case == 'pose': a['x'] += .125
                elif case == 'facing': a['facing'] = -1
                elif case == 'path': a['path'][0]['x'] += 1
                elif case == 'history-middle': a['playback'][0]['x'] += .125
                elif case == 'history-time': a['playback'][1]['time'] += .125
                elif case == 'version': a['version'] += 1
                elif case == 'relocation': a['relocation'] += 1
                elif case == 'missing-frame': w.pop('work_animation')
                elif case == 'missing-owner': entry['native_people']['missing_observation'] = True
                elif case == 'private-frame': a['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'private-sample': a['playback'][0]['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'private-path': a['path'][0]['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                elif case == 'field-action': a['farmAction'] = 'harvesting'
                elif case == 'native-elapsed':
                    entry['native_source']['towns']['first']['villagers'][2]['work_animation']['elapsed'] += .25
                elif case == 'native-assignment':
                    entry['native_source']['towns']['first']['villagers'][2]['building_index'] = 2
                elif case == 'new-obstacle':
                    pos = a['path'][0]
                    for key in ('world', 'native_source'):
                        entry[key]['towns']['first']['buildings'].append(dict(kind='home', footprint=[1, 1], **pos))
                elif case == 'void-terrain':
                    pos = a['path'][0]
                    x, y = pos['x'], pos['y']
                    if self.chunked:
                        record = entry['records'][f'{x // 40},{y // 40}']
                        x, y = x % 40, y % 40
                        row = record['terrain'][y]
                        record['terrain'][y] = row[:x] + 'v' + row[x + 1:]
                    else:
                        for key in ('world', 'native_source'):
                            rows = entry[key]['shared_map']['terrain']
                            rows[y] = rows[y][:x] + 'v' + rows[y][x + 1:]
                elif case == 'cargo-owner': w['logistics_trip'] = {'operation': 'collect'}
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


class RustPlaybackOrdinary(RustPlayback):
    chunked = False


if __name__ == '__main__':
    unittest.main()
