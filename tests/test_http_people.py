"""Paired frozen personal/population projections over disposable native roots."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_http_populated as populated
import test_http_snapshots as terrain

RUST_SOURCES = ('rust/crates/lantern-host/src/legacy/http_snapshot/metadata.rs',
                'rust/crates/lantern-host/src/legacy/http_snapshot/metadata/people.rs',
                'rust/crates/lantern-host/examples/http_fixture.rs')


def personal_fixture(chunked=True):
    config = populated.progressed_fixture(chunked)
    entry = config['worlds'][0]
    entry['native_people'] = {'social': True, 'boat': True, 'progression': True,
                              'last_batch': 18446744073709551615}
    world = entry['world']
    town = world['towns']['first']
    town.update(food_shortage_seconds=3.125, housing_shortage_seconds=6.25,
                arrival_delay_seconds=12.5, departure_count=4,
                last_departure={'name': 'Departed café resident 🌿', 'job': 'woodcutting',
                                'reason': 'food and housing', 'practice': {'woodcutting': 2.5}})
    town['villagers'][0]['worker_tool'] = 'fine_axe'
    town['player'].update(facing=-1, items={'swamp_boots': 2, 'desert_clothing': 1},
        worker_tools={'fine_axe': 1, 'steel_sickle': 0},
        equipment={'feet': 'swamp_boots', 'body': 'desert_clothing'},
        keepsakes=['Synthetic survey keepsake', 'Lantern café 🌿'],
        fishing=61.5, gathering=20.25, woodcutting=2,
        activity={'job': 'gathering', 'kind': 'herb_patch', 'x': 10, 'y': 12},
        activity_status='Waiting for supply to regenerate', boat=True, boat_level=2)
    npc = world['shared_map']['npcs'][0]
    key = f'{npc["name"]}:{npc["x"]}:{npc["y"]}'
    town['local_visits'] = {key: {'met': True, 'gift_claimed': True, 'story_stage': 2,
                                'story_rocks': ['8:9', '12:14'], 'trade_remaining': 1.125},
                            'Historical visitor:19:19': {}}
    return config


class PersonalProtocol:
    call = terrain.TerrainProtocol.call
    chunked = True

    @classmethod
    def configuration(cls):
        return personal_fixture(cls.chunked)

    def test_personal_population_and_progression_snapshots_match_python(self):
        status, value = self.call('/api/world')
        self.assertEqual(status, 200)
        value.pop('players_sampled_at')
        self.assertEqual(value, terrain.reference(self.config, {}))
        town = value['towns']['first']
        self.assertEqual(town['departure_count'], 4)
        self.assertEqual(town['last_departure']['practice']['woodcutting'], 2.5)
        self.assertEqual(town['villagers'][0]['worker_tool'], 'fine_axe')
        status, players = self.call('/api/players')
        self.assertEqual(status, 200)
        self.assertEqual(players['players'], {k: t['player'] for k, t in value['towns'].items()})
        self.assertEqual(players['players']['first']['boat_level'], 2)
        for private in ('last_batch', 'returned_tools', 'SYNTHETIC_SECRET_TOKEN'):
            self.assertNotIn(private, json.dumps(value))
            self.assertNotIn(private, json.dumps(players))
        # Public reads do not consume a batch, trade timer, shortage clock or receipt.
        later = self.call('/api/world')[1]
        later.pop('players_sampled_at')
        self.assertEqual(later, value)


class PythonPeople(PersonalProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.PythonTerrain.setUpClass.__func__)
    stop = classmethod(terrain.PythonTerrain.stop.__func__)


class PythonPeopleOrdinary(PythonPeople):
    chunked = False


class RustPeople(PersonalProtocol, unittest.TestCase):
    setUpClass = classmethod(terrain.RustTerrain.setUpClass.__func__)
    stop = classmethod(terrain.RustTerrain.stop.__func__)
    make_native = classmethod(terrain.RustTerrain.make_native.__func__)
    read_copy = populated.RustReadHarness.read_copy

    def test_absence_defaults_completed_activities_boats_and_large_counts(self):
        for case in ('defaults', 'status-only', 'fishing', 'woodcutting', 'berries',
                     'boat-one', 'boat-three', 'on-water', 'no-boat', 'null-activity', 'large-counts'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                town = config['worlds'][0]['world']['towns']['first']
                player = town['player']
                if case == 'defaults':
                    town.pop('local_visits')
                    town.pop('last_departure')
                    for field in ('food_shortage_seconds', 'housing_shortage_seconds',
                                  'arrival_delay_seconds', 'departure_count'):
                        town.pop(field)
                    for field in ('items', 'worker_tools', 'equipment', 'keepsakes', 'activity',
                                  'activity_status', 'woodcutting', 'fishing', 'gathering'):
                        player.pop(field)
                    town['villagers'][0].pop('worker_tool')
                elif case == 'status-only':
                    player.pop('activity')
                    player['activity_status'] = 'Found 25 coins and swamp boots'
                elif case in ('fishing', 'woodcutting', 'berries'):
                    job, kind = {'fishing': ('fishing', 'water'),
                                 'woodcutting': ('woodcutting', 'tree'),
                                 'berries': ('gathering', 'berry_bush')}[case]
                    player['activity'].update(job=job, kind=kind)
                elif case in ('boat-one', 'boat-three'):
                    if case == 'boat-one':
                        player.pop('boat_level')
                    else:
                        player['boat_level'] = 3
                elif case == 'on-water':
                    player.update(x=town['town_origin']['x'] + 6.,
                                  y=town['town_origin']['y'] + 8.)
                elif case == 'null-activity':
                    player['activity'] = None
                    player['activity_status'] = None
                elif case == 'no-boat':
                    player.pop('boat')
                    player.pop('boat_level')
                elif case == 'large-counts':
                    town['departure_count'] = 18446744073709551615
                    player['items']['swamp_boots'] = 18446744073709551615
                value = self.read_copy(config, '/api/world')
                value.pop('players_sampled_at')
                self.assertEqual(value, terrain.reference(config, {}))

    def test_stale_missing_private_and_unowned_personal_state_refuses_startup(self):
        for case in ('shortage', 'delay', 'count', 'departure', 'departure-missing',
                     'departure-private', 'departure-practice', 'worker-tool', 'item',
                     'item-omitted', 'equipment', 'keepsakes', 'practice', 'facing',
                     'activity', 'activity-missing', 'activity-private', 'status', 'boat',
                     'boat-level', 'visit', 'visit-missing', 'visit-key', 'visit-private',
                     'native-visit-private', 'missing-social', 'missing-boat',
                     'stale-progression', 'pending-receipt', 'batch-private'):
            with self.subTest(case=case):
                config = copy.deepcopy(self.config)
                entry = config['worlds'][0]
                entry['native_source'] = copy.deepcopy(entry['world'])
                town = entry['world']['towns']['first']
                player = town['player']
                visit = next(iter(town['local_visits'].values()))
                if case == 'shortage':
                    town['food_shortage_seconds'] += .5
                elif case == 'delay':
                    town['arrival_delay_seconds'] += .5
                elif case == 'count':
                    town['departure_count'] += 1
                elif case == 'departure':
                    town['last_departure']['name'] = 'Different resident'
                elif case == 'departure-missing':
                    town.pop('last_departure')
                elif case == 'departure-practice':
                    town['last_departure']['practice']['woodcutting'] += 1
                elif case == 'worker-tool':
                    town['villagers'][0]['worker_tool'] = 'steel_axe'
                elif case == 'item':
                    player['items']['swamp_boots'] += 1
                elif case == 'item-omitted':
                    player.pop('worker_tools')
                elif case == 'equipment':
                    player['equipment'].pop('feet')
                elif case == 'keepsakes':
                    player['keepsakes'].reverse()
                elif case == 'practice':
                    player['fishing'] += 1
                elif case == 'facing':
                    player['facing'] = 1
                elif case == 'activity':
                    player['activity']['x'] += 1
                elif case == 'activity-missing':
                    player.pop('activity')
                elif case == 'status':
                    player['activity_status'] = 'Working'
                elif case == 'boat':
                    player['boat'] = False
                elif case == 'boat-level':
                    player['boat_level'] = 3
                elif case == 'visit':
                    visit['trade_remaining'] += .25
                elif case == 'visit-missing':
                    town.pop('local_visits')
                elif case == 'visit-key':
                    town['local_visits']['Different:0:0'] = town['local_visits'].pop(next(iter(town['local_visits'])))
                elif case == 'missing-social':
                    entry['native_people']['social'] = False
                    entry['native_people']['progression'] = False
                elif case == 'missing-boat':
                    entry['native_people']['boat'] = False
                elif case == 'stale-progression':
                    entry['native_people']['stale_progression'] = True
                elif case == 'pending-receipt':
                    entry['native_people']['returned_tool'] = True
                elif case == 'native-visit-private':
                    target = next(iter(entry['native_source']['towns']['first']['local_visits'].values()))
                    target['credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                else:
                    target = {'departure-private': town['last_departure'],
                              'activity-private': player['activity'],
                              'visit-private': visit, 'batch-private': player}[case]
                    target['last_batch' if case == 'batch-private' else 'credentials'] = 'SYNTHETIC_PRIVATE_CANARY'
                with tempfile.TemporaryDirectory(dir=self.directory.name) as directory:
                    native = Path(directory) / 'native'
                    self.make_native(native, config)
                    result = subprocess.run([str(self.binary), 'http-read-copy', '--experimental',
                        '--source', str(native), '--bind', '127.0.0.1:0'], capture_output=True, timeout=10)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertNotIn(b'listening', result.stdout)
                    self.assertIn(b'coverage', result.stderr)
                    self.assertNotIn(b'SYNTHETIC_PRIVATE_CANARY', result.stdout + result.stderr)


class RustPeopleOrdinary(RustPeople):
    chunked = False


if __name__ == '__main__':
    unittest.main()
