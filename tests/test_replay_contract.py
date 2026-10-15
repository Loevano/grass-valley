"""Contract tests use fabricated observations, not recorded Python gameplay."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import unittest

from tools.replay_contract import (ContractError, STATUS, canonical_json,
                                   compare_observation, validate_trace)


EXAMPLE = Path('tests/replay/contract_v1.json')


class ReplayContract(unittest.TestCase):
    def setUp(self):
        self.trace = json.loads(EXAMPLE.read_text())
        self.step = self.trace['steps'][0]

    def compare(self, actual, expected=None):
        return compare_observation(expected if expected is not None else self.step['expected'],
                                   actual, self.step['numeric_fields'], self.step['invariants'])

    def test_example_is_valid_and_validation_does_not_mutate_it(self):
        before = copy.deepcopy(self.trace)
        validate_trace(self.trace)
        self.assertEqual(before, self.trace)
        self.assertEqual(self.compare(copy.deepcopy(self.step['expected'])), [])

    def test_corrupt_cargo_owner_position_and_rewards_have_useful_paths(self):
        for field, value in [('cargo', 1), ('owner', 'town-b'), ('position', {'x': 5, 'y': 5}),
                             ('reward', 100)]:
            with self.subTest(field=field):
                actual = copy.deepcopy(self.step['expected'])
                actual['state'][field] = value
                failures = self.compare(actual)
                self.assertTrue(any(field in failure for failure in failures), failures)
                if field != 'position':
                    self.assertTrue(any('candidate invariant' in failure for failure in failures))

    def test_independent_oracle_rejects_same_bug_in_reference_and_candidate(self):
        wrong = copy.deepcopy(self.step['expected'])
        wrong['state']['cargo'] = 1
        failures = self.compare(wrong, wrong)
        self.assertTrue(any("reference invariant 'wood conserved'" in f for f in failures))
        self.assertTrue(any("candidate invariant 'wood conserved'" in f for f in failures))
        self.step['expected'] = wrong
        with self.assertRaisesRegex(ContractError, 'step 0.*wood conserved'):
            validate_trace(self.trace)

    def test_narrow_numeric_tolerances_and_exact_integer_counts(self):
        actual = copy.deepcopy(self.step['expected'])
        actual['state']['position']['x'] += 0.5e-6
        actual['state']['food'] += 0.5e-9
        self.assertEqual(self.compare(actual), [])
        actual['state']['food'] += 2e-9
        self.assertTrue(any('food' in f for f in self.compare(actual)))
        actual['state']['position']['x'] += 2e-6
        self.assertTrue(any('position' in f for f in self.compare(actual)))
        self.step['numeric_fields'].append({'path': ['state', 'cargo'], 'kind': 'fraction'})
        actual = copy.deepcopy(self.step['expected'])
        actual['state']['cargo'] += 0.5e-9
        self.assertTrue(any('cargo' in f and 'tolerance 0)' in f for f in self.compare(actual)))

    def test_missing_null_extra_fields_and_array_order_are_distinct(self):
        expected = {'id': 2**60 + 1, 'phase': 'walking', 'timer': 0.1, 'rows': [1, 2], 'null': None}
        for actual in ({**expected, 'id': 2**60}, {**expected, 'rows': [2, 1]},
                       {**expected, 'timer': 0.10000000001}, {**expected, 'phase': 'waiting'},
                       {**expected, 'extra': 1}, {k: v for k, v in expected.items() if k != 'null'},
                       {**expected, 'id': True}, {**expected, 'id': float(2**60)}):
            self.assertTrue(compare_observation(expected, actual))

    def test_integer_conservation_does_not_round_through_binary64(self):
        expected = {'cargo': 2**60, 'stored': 1}
        rules = [{'name': 'large ledger', 'kind': 'sum',
                  'paths': [['cargo'], ['stored']], 'value': 2**60 + 1}]
        self.assertEqual(compare_observation(expected, expected, invariants=rules), [])
        broken = {'cargo': 2**60, 'stored': 0}
        self.assertTrue(compare_observation(broken, broken, invariants=rules))

    def test_normalization_only_changes_key_order_and_whitespace(self):
        self.assertEqual(canonical_json({'b': 2, 'a': 1}), canonical_json({'a': 1, 'b': 2}))
        self.assertNotEqual(canonical_json([2, 1]), canonical_json([1, 2]))
        self.assertNotEqual(canonical_json({'x': None}), canonical_json({}))
        self.assertNotEqual(canonical_json({'x': 1.0}), canonical_json({'x': 1}))

    def test_invalid_schema_time_randomness_and_provenance_are_rejected(self):
        cases = [lambda t: t.update(format_version=2),
                 lambda t: t['provenance'].update(python_revision='HEAD'),
                 lambda t: t['initial'].update(save_path='savegame.json'),
                 lambda t: t['steps'][0]['operation'].update(seconds=-1),
                 lambda t: t['steps'][0]['operation'].update(seconds=True),
                 lambda t: t['steps'][0]['operation'].update(clock='wall'),
                 lambda t: t['steps'][0].update(random_draws=[{'value': 0.2}]),
                 lambda t: t['steps'][0]['numeric_fields'].append({'path': ['missing'], 'kind': 'position'}),
                 lambda t: t['steps'][0]['numeric_fields'][0].update(kind='anything'),
                 lambda t: t['steps'][0]['invariants'][0].update(kind='ignore')]
        for mutate in cases:
            trace = copy.deepcopy(self.trace)
            mutate(trace)
            with self.subTest(trace=trace), self.assertRaises(ContractError):
                validate_trace(trace)

    def test_command_reload_and_ordered_random_draw_format(self):
        self.step['operation'] = {'kind': 'command', 'town': 'town-a', 'data': {'action': 'move', 'index': 1, 'x': 8, 'y': 4}}
        self.step['random_draws'] = [{'site': 'example.choice', 'method': 'choice', 'args': [['a', 'b']], 'value': 'b'}]
        validate_trace(self.trace)
        self.step['operation'] = {'kind': 'reload'}
        self.step['random_draws'] = []
        validate_trace(self.trace)

    def test_private_fields_and_nonfinite_values_rejected_without_leaking_values(self):
        for key, value in [('pin_hash', 'do-not-log-this'), ('Authorization', 'do-not-log-this'),
                           ('number', float('nan')), ('number', float('inf'))]:
            self.trace['initial']['parameters'] = {key: value}
            with self.assertRaises(ContractError) as error:
                validate_trace(self.trace)
            self.assertNotIn('do-not-log-this', str(error.exception))

    def test_invariants_report_missing_and_negative_values(self):
        actual = copy.deepcopy(self.step['expected'])
        actual['state']['food'] = -1
        del actual['state']['owner']
        failures = self.compare(actual)
        self.assertTrue(any('nonnegative stock' in f for f in failures))
        self.assertTrue(any('owner preserved' in f and 'missing path' in f for f in failures))

    def test_cli_clearly_reports_unavailable_backends_and_benchmarks(self):
        result = subprocess.run([sys.executable, 'tools/replay_contract.py', str(EXAMPLE)],
                                text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(STATUS, result.stdout)
        self.assertIn('contract_example', result.stdout)
        self.assertIn('no replay performed', result.stdout)
