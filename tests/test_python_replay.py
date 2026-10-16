"""Repeatable Python reference traces with independent accounting/continuity checks."""
import copy
import json
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from tools import python_replay as replay
from tools.replay_contract import ContractError, validate_trace

# Explicit paths keep the affected-check dependency graph reviewable.
TRACE_PATHS = [
    'tests/replay/python/relocation_v1.json',
    'tests/replay/python/blocked_delivery_v1.json',
    'tests/replay/python/workshop_v1.json',
    'tests/replay/python/meals_v1.json',
    'tests/replay/python/terrain_quarry_v1.json',
]


def baseline(recipe):
    return json.loads(next(Path(path).read_text() for path in TRACE_PATHS
                           if Path(path).stem == recipe))


class PythonReplay(unittest.TestCase):
    def test_every_recorded_trace_replays_twice_in_fresh_worlds(self):
        for path in TRACE_PATHS:
            with self.subTest(path=path):
                trace = json.loads(Path(path).read_text())
                before = copy.deepcopy(trace)
                first = replay.run_trace(trace)
                self.assertEqual(first, replay.run_trace(trace))
                self.assertEqual(trace, before)
                self.assertEqual(trace['provenance']['kind'], 'recorded_python')
                self.assertEqual(trace['provenance']['python_revision'],
                                 '9637eb5bf982fa2342556feaa4059fcb3f0d4b17')

    def test_relocation_reload_keeps_pose_cargo_and_credits_once(self):
        observations = replay.run_trace(baseline('relocation_v1'))
        for observation in observations[:3]:
            town = observation['state']['towns']['a']
            worker = town['workers'][0]
            self.assertEqual(worker['pending_output'], {'wood': 2})
            self.assertEqual((worker['work_animation']['x'], worker['work_animation']['y']), (11.25, 2))
            self.assertEqual(sum(town['wood_ledger'].values()), 67)
        final = observations[-1]['state']['towns']['a']
        self.assertNotIn('pending_output', final['workers'][0])
        self.assertEqual(final['buildings'][1]['output_buffer']['wood'], 2)
        self.assertEqual(final['wood'], 67)
        self.assertEqual(final['wood_ledger'], {'stored': 67, 'carried': 0})

    def test_blocked_delivery_waits_reloads_recovers_and_never_duplicates(self):
        observations = replay.run_trace(baseline('blocked_delivery_v1'))
        for observation in observations[:5]:
            town = observation['state']['towns']['a']
            worker = town['workers'][0]
            self.assertEqual(worker['logistics_position'], {'x': 8, 'y': 2})
            self.assertTrue(worker['logistics_trip']['picked_up'])
            self.assertFalse(worker['logistics_trip']['delivered'])
            self.assertEqual(worker['logistics_trip']['cargo'], {'wood': 3})
            self.assertEqual(town['wood_ledger'], {'stored': 65, 'carried': 3})
        final = observations[-1]['state']['towns']['a']
        self.assertNotIn('logistics_trip', final['workers'][0])
        self.assertEqual(final['buildings'][2]['stored_goods']['wood'], 3)
        self.assertEqual(final['wood_ledger'], {'stored': 68, 'carried': 0})

    def test_workshop_fifo_partial_reload_invalid_order_and_funded_output(self):
        observations = replay.run_trace(baseline('workshop_v1'))
        before = observations[1]['state']['towns']['a']
        invalid = observations[2]
        self.assertEqual(invalid['public']['error'], {'type': 'ValueError',
            'message': 'Request between 1 and 999 items.'})
        self.assertEqual(invalid['state']['towns']['a'], before)
        partial = observations[3]['state']['towns']['a']
        reloaded = observations[4]['state']['towns']['a']
        self.assertEqual(partial['workers'][0]['production_work']['elapsed'], 3)
        self.assertEqual(partial['workers'][0]['production_work'], reloaded['workers'][0]['production_work'])
        first, second = before['buildings'][1]['craft_queue']
        self.assertNotEqual(first['id'], second['id'])
        final = observations[-1]['state']['towns']['a']
        self.assertEqual(final['produced']['tools'], 2)
        self.assertEqual(final['buildings'][1]['output_buffer']['tools'], 2)
        self.assertEqual(final['buildings'][1]['craft_queue'], [second])
        self.assertNotIn('pending_output', final['workers'][0])

    def test_meals_use_independent_rate_and_match_physical_stock(self):
        observations = replay.run_trace(baseline('meals_v1'))
        for observation, ticks in zip(observations, (1, 2, 2, 3)):
            town = observation['state']['towns']['a']
            # 5 idle residents * .6 food/min * 3 seconds per economic tick.
            self.assertAlmostEqual(town['food'], 40 - ticks * 5 * .6 * 3 / 60, places=9)
            self.assertEqual(town['goods'], {})
            self.assertAlmostEqual(town['food'], town['buildings'][0]['stored_goods']['food'], places=9)
            self.assertEqual(town['wood'], 65)
        self.assertEqual(observations[1]['state'], observations[2]['state'])

    def test_real_terrain_multitown_adventure_rewards_and_reload(self):
        trace = baseline('terrain_quarry_v1')
        self.assertEqual(trace['initial']['terrain'], 'real_generator')
        initial = trace['initial']['parameters']['initial_state']
        terrain = initial['terrain']
        self.assertEqual(len(terrain['rows']), 120)
        self.assertTrue(all(len(row) == 120 for row in terrain['rows']))
        self.assertGreater(len(set(''.join(terrain['rows']))), 1)
        self.assertGreater(terrain['resource_count'], 100)
        observations = replay.run_trace(trace)
        self.assertEqual(observations[8]['state'], observations[9]['state'])
        self.assertEqual(observations[-2]['state'], observations[-1]['state'])
        self.assertIn('error', observations[-1]['public'])
        final = observations[-1]['state']['towns']
        for owner in ('a', 'b'):
            self.assertEqual(final[owner]['coins'], 20 + 20)
            self.assertEqual(final[owner]['player']['keepsakes'], ['Quarry lantern stone'])
        self.assertEqual(final['spectator']['coins'], 20)
        self.assertNotIn('keepsakes', final['spectator']['player'])
        for owner in final:
            self.assertEqual(final[owner]['player']['x'], initial['towns'][owner]['player']['x'])
            self.assertEqual(final[owner]['player']['y'], initial['towns'][owner]['player']['y'])

    def test_corrupt_runtime_observations_fail_with_trace_step_and_field(self):
        cases = [('blocked_delivery_v1', ['state', 'towns', 'a', 'workers', 0, 'logistics_trip', 'cargo', 'wood'], 1),
                 ('relocation_v1', ['state', 'towns', 'a', 'owner'], 'b'),
                 ('relocation_v1', ['state', 'towns', 'a', 'workers', 0, 'work_animation', 'x'], 0),
                 ('terrain_quarry_v1', ['state', 'towns', 'spectator', 'coins'], 40)]
        original = replay.project
        for recipe, path, value in cases:
            corrupted = []

            def corrupt(*args, **kwargs):
                observation = original(*args, **kwargs)
                # Batch adapters collect the whole trace before comparison. Inject
                # only step 0, while its cargo/trip still exists.
                if not kwargs.get('initial') and not corrupted:
                    corrupted.append(True)
                    target = observation
                    for key in path[:-1]:
                        target = target[key]
                    target[path[-1]] = value
                return observation
            with self.subTest(recipe=recipe, path=path), patch.object(replay, 'project', corrupt):
                with self.assertRaisesRegex(ContractError, recipe + ' step 0') as error:
                    replay.run_trace(baseline(recipe))
                self.assertIn(str(path[-1]), str(error.exception))

    def test_random_tape_rejects_missing_extra_reordered_site_method_args_and_value(self):
        original = baseline('workshop_v1')
        self.assertEqual(len(original['steps'][0]['random_draws']), 1)
        for field, value in [('site', 'wrong.site'), ('method', 'randbelow'), ('args', [999]), ('value', 'bad-id')]:
            trace = copy.deepcopy(original)
            trace['steps'][0]['random_draws'][0][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ContractError, 'draw.*(signature|synthetic ID)|synthetic ID draw'):
                replay.run_trace(trace)
        trace = copy.deepcopy(original)
        trace['steps'][0]['random_draws'] = []
        with self.assertRaisesRegex(ContractError, 'unexpected random draw'):
            replay.run_trace(trace)
        trace = copy.deepcopy(original)
        trace['steps'][0]['random_draws'] *= 2
        with self.assertRaisesRegex(ContractError, 'unused random draws'):
            replay.run_trace(trace)
        # Swapped queue ID results are valid entropy but still fail snapshot parity.
        trace = copy.deepcopy(original)
        a, b = trace['steps'][0]['random_draws'], trace['steps'][1]['random_draws']
        a[0]['value'], b[0]['value'] = b[0]['value'], a[0]['value']
        with self.assertRaisesRegex(ContractError, 'craft_queue'):
            replay.run_trace(trace)

    def test_recipe_initial_state_clock_and_terrain_are_validated(self):
        mutations = [lambda t: t['initial'].update(recipe='savegame.json'),
                     lambda t: t['initial'].update(terrain='real_generator'),
                     lambda t: t['initial']['parameters'].update(seed=True),
                     lambda t: t['initial']['parameters'].update(path='savegame.json'),
                     lambda t: t['initial']['parameters']['initial_state']['towns']['a'].update(wood=999),
                     lambda t: t['steps'][0]['operation'].update(seconds=1),
                     lambda t: t['steps'][0]['operation'].update(clock='adventure')]
        for mutate in mutations:
            trace = baseline('meals_v1')
            mutate(trace)
            with self.subTest(trace=trace['id']), self.assertRaises(ContractError):
                replay.run_trace(trace)

    def test_recording_reproduces_committed_results(self):
        for trace in replay.plans('9637eb5bf982fa2342556feaa4059fcb3f0d4b17'):
            with self.subTest(recipe=trace['id']):
                replay.run_trace(trace, recording=True)
                self.assertEqual(trace, baseline(trace['id']))
                validate_trace(trace)

    def test_private_saves_and_globals_untouched_and_guards_restore_after_failure(self):
        before_world = copy.deepcopy(replay.server.WORLD)
        before_worlds = copy.deepcopy(replay.server.WORLDS)
        original_routes = replay.server.routes
        with tempfile.TemporaryDirectory() as directory:
            private = Path(directory) / 'private-save.json'
            private.write_text('private sentinel: never a replay input')
            original_randbits = secrets.randbits
            with patch.object(replay.server, 'SAVE', private):
                replay.run_trace(baseline('meals_v1'))
                trace = baseline('workshop_v1')
                trace['steps'][0]['random_draws'] = []
                with self.assertRaises(ContractError):
                    replay.run_trace(trace)
            self.assertIs(secrets.randbits, original_randbits)
            self.assertEqual(private.read_text(), 'private sentinel: never a replay input')
            self.assertEqual(list(Path(directory).iterdir()), [private])
        self.assertEqual(replay.server.WORLD, before_world)
        self.assertEqual(replay.server.WORLDS, before_worlds)
        self.assertIs(replay.server.routes, original_routes)

    def test_live_save_sleep_and_unseeded_rng_guards_and_clock_origin(self):
        from contextlib import ExitStack
        import random
        import time
        inputs = replay.Inputs(42)
        with ExitStack() as stack:
            inputs.install(stack)
            self.assertEqual(time.monotonic(), 0)
            inputs.seconds = 3
            self.assertEqual(time.time(), 3)
            for action in (replay.server.load, replay.server.save, lambda: replay.server.activate(None),
                           lambda: time.sleep(.1), lambda: random.Random(),
                           lambda: secrets.token_urlsafe(24), lambda: random.uniform(0, 1)):
                with self.assertRaises(ContractError):
                    action()

    def test_recording_refuses_dirty_game_sources_and_existing_destination(self):
        with patch.object(replay.subprocess, 'check_output', return_value=' M server/server.py\n'):
            with self.assertRaisesRegex(ContractError, 'clean Python game sources'):
                replay.source_revision()
        result = subprocess.run([sys.executable, 'tools/python_replay.py', '--record', str(replay.FIXTURES)],
                                text=True, capture_output=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('destination must be empty', result.stderr)
        self.assertIn('Rust gameplay and Rust-host benchmarks do not exist', result.stdout)
