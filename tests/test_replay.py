"""Backend-neutral protocol/reporting tests, plus real Python adapter acceptance."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from tools.replay import (BackendUnavailable, RustUnavailable,
                          compare_backends, markdown_report, replay_trace, request_for,
                          validate_request)
from tools.replay_contract import ContractError
from tools.python_replay import PythonBackend


MEALS = Path('tests/replay/python/meals_v1.json')
BLOCKED = Path('tests/replay/python/blocked_delivery_v1.json')
RELOCATION = Path('tests/replay/python/relocation_v1.json')
QUARRY = Path('tests/replay/python/terrain_quarry_v1.json')
WORKSHOP = Path('tests/replay/python/workshop_v1.json')


class StubBackend:
    """Fabricated observations test the harness only, never simulate a real backend."""
    name = 'test-double'

    def __init__(self, trace, mutate=None):
        self.response = {'protocol_version': 1, 'trace_id': trace['id'],
                         'initial_state': copy.deepcopy(trace['initial']['parameters']['initial_state']),
                         'observations': [copy.deepcopy(step['expected']) for step in trace['steps']]}
        if mutate:
            mutate(self.response)
        self.requests = []

    def replay(self, request):
        self.requests.append(copy.deepcopy(request))
        return copy.deepcopy(self.response)


class ReplayInterface(unittest.TestCase):
    def setUp(self):
        self.trace = json.loads(MEALS.read_text())

    def test_oracle_never_reaches_backend_and_inputs_are_independent(self):
        before = copy.deepcopy(self.trace)
        backend = StubBackend(self.trace)
        result = compare_backends(self.trace, backend, backend)
        self.assertEqual(result['status'], 'pass')
        self.assertEqual(len(backend.requests), 2)
        request = backend.requests[0]
        self.assertEqual(set(request), {'protocol_version', 'trace_id', 'initial', 'steps'})
        self.assertEqual(set(request['initial']['parameters']), {'seed', 'setup_draws'})
        self.assertEqual(set(request['steps'][0]), {'operation', 'random_draws'})
        self.assertNotIn('provenance', request)
        request['steps'][0]['operation']['seconds'] = 999
        self.assertEqual(backend.requests[1]['steps'][0]['operation']['seconds'], 3)
        self.assertEqual(self.trace, before)

    def test_actual_python_adapter_pairs_every_fixture_and_preserves_baselines(self):
        for path in (MEALS, BLOCKED, RELOCATION, QUARRY, WORKSHOP):
            with self.subTest(path=path):
                trace = json.loads(path.read_text())
                result = compare_backends(trace, PythonBackend(), PythonBackend())
                self.assertEqual(result['status'], 'pass', result)
                self.assertEqual(result['comparison']['failures'], [])
                self.assertEqual(result['baseline'], trace['provenance'])
                self.assertEqual(result['reference']['completed_steps'], len(trace['steps']))
                self.assertEqual(result['candidate']['completed_steps'], len(trace['steps']))

    def test_candidate_corruptions_report_cargo_owner_position_rewards_and_step(self):
        cases = [(BLOCKED, ['towns', 'a', 'workers', 0, 'logistics_trip', 'cargo', 'wood'], 9),
                 (RELOCATION, ['towns', 'a', 'owner'], 'b'),
                 (RELOCATION, ['towns', 'a', 'workers', 0, 'work_animation', 'x'], 0),
                 (QUARRY, ['towns', 'spectator', 'coins'], 999)]
        for path, field, value in cases:
            trace = json.loads(path.read_text())
            def mutate(response):
                target = response['observations'][0]['state']
                for key in field[:-1]:
                    target = target[key]
                target[field[-1]] = value
            with self.subTest(path=path, field=field):
                result = compare_backends(trace, StubBackend(trace), StubBackend(trace, mutate))
                self.assertEqual(result['status'], 'failed')
                self.assertEqual(result['reference']['status'], 'pass')
                self.assertEqual(result['candidate']['status'], 'failed')
                failure = result['comparison']['failures'][0]
                self.assertEqual(failure['step'], 0)
                self.assertEqual(failure['operation'], trace['steps'][0]['operation'])
                self.assertIn(str(field[-1]), str(failure['differences']))

    def test_matching_backend_bugs_do_not_pass_baselines_or_invariants(self):
        trace = json.loads(BLOCKED.read_text())
        def bug(response):
            for observation in response['observations']:
                observation['state']['towns']['a']['wood_ledger']['carried'] += 1
        result = compare_backends(trace, StubBackend(trace, bug), StubBackend(trace, bug))
        self.assertEqual(result['status'], 'failed')
        for role in ('reference', 'candidate', 'comparison'):
            self.assertEqual(result[role]['status'], 'failed')
            self.assertIn('physical wood conserved', str(result[role]['failures']))

    def test_missing_rust_is_inconclusive_and_cannot_be_a_pass(self):
        result = compare_backends(self.trace, StubBackend(self.trace), RustUnavailable())
        self.assertEqual(result['reference']['status'], 'pass')
        self.assertEqual(result['candidate']['status'], 'unavailable')
        self.assertEqual(result['comparison']['status'], 'inconclusive')
        self.assertEqual(result['status'], 'inconclusive')
        with self.assertRaises(BackendUnavailable):
            replay_trace(RustUnavailable(), self.trace)

    def test_bad_reference_still_checks_candidate_and_failure_beats_unavailable(self):
        def bug(response):
            response['initial_state']['towns']['a']['coins'] = -1
        reference = StubBackend(self.trace, bug)
        candidate = StubBackend(self.trace)
        result = compare_backends(self.trace, reference, candidate)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['candidate']['status'], 'pass')
        self.assertEqual(len(candidate.requests), 1)
        result = compare_backends(self.trace, reference, RustUnavailable())
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['comparison']['status'], 'inconclusive')

    def test_response_protocol_rejects_wrong_id_count_shape_nonfinite_and_private_data(self):
        mutations = [lambda r: r.update(protocol_version=True), lambda r: r.update(protocol_version=2),
                     lambda r: r.update(trace_id='other-trace'), lambda r: r.update(initial_state=[]),
                     lambda r: r['observations'].pop(), lambda r: r['observations'].append(r['observations'][0]),
                     lambda r: r['observations'][0].update(unexpected=0),
                     lambda r: r['observations'][0]['state'].update(bad=float('nan')),
                     lambda r: r['observations'][0]['state'].update(pin_hash='never-log-me')]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                result = compare_backends(self.trace, StubBackend(self.trace), StubBackend(self.trace, mutate))
                self.assertEqual(result['status'], 'failed')
                self.assertEqual(result['candidate']['status'], 'failed')
                self.assertEqual(result['comparison']['status'], 'inconclusive')
                self.assertNotIn('never-log-me', json.dumps(result))

    def test_request_protocol_refuses_oracles_paths_bad_time_draws_and_examples(self):
        mutations = [lambda r: r.update(expected={}), lambda r: r.update(protocol_version=True),
                     lambda r: r['initial']['parameters'].update(initial_state={}),
                     lambda r: r['initial']['parameters'].update(save_path='savegame.json'),
                     lambda r: r['steps'][0]['operation'].update(kind={}),
                     lambda r: r['steps'][0]['operation'].update(seconds=-1),
                     lambda r: r['steps'][0].update(random_draws=[{'value': 2}])]
        for mutate in mutations:
            request = request_for(self.trace)
            mutate(request)
            with self.subTest(request=request), self.assertRaises(ContractError):
                validate_request(request)
        example = json.loads(Path('tests/replay/contract_v1.json').read_text())
        with self.assertRaisesRegex(ContractError, 'not a recorded Python baseline'):
            request_for(example)

    def test_adapter_exception_and_mutated_request_are_not_passes(self):
        class Broken:
            name = 'broken'
            def replay(self, request):
                raise RuntimeError('not-a-command-error')
        result = compare_backends(self.trace, StubBackend(self.trace), Broken())
        self.assertEqual(result['status'], 'failed')
        self.assertIn('RuntimeError', result['candidate']['reason'])
        stub = StubBackend(self.trace)
        class Mutating:
            name = 'mutating'
            def replay(self, request):
                request['steps'].pop()
                return stub.replay(request)
        response = replay_trace(Mutating(), self.trace)
        self.assertEqual(len(response['observations']), 4)
        self.assertEqual(len(self.trace['steps']), 4)

    def test_reference_missing_tolerance_field_becomes_reported_failure(self):
        def missing(response):
            del response['observations'][0]['state']['towns']['a']['food']
        result = compare_backends(self.trace, StubBackend(self.trace, missing), StubBackend(self.trace))
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(result['reference']['status'], 'failed')
        self.assertEqual(result['comparison']['status'], 'failed')
        self.assertIn('missing path', result['comparison']['reason'])

    def test_invalid_private_fixture_does_not_execute_or_leak_provenance(self):
        self.trace['provenance']['pin_hash'] = 'never-log-me'
        backend = StubBackend(self.trace)
        result = compare_backends(self.trace, backend, backend)
        self.assertEqual(result['status'], 'failed')
        self.assertEqual(backend.requests, [])
        self.assertNotIn('never-log-me', json.dumps(result))
        self.assertNotIn('baseline', result)

    def test_reports_and_exit_codes_for_python_selfcheck_and_missing_rust(self):
        with tempfile.TemporaryDirectory() as directory:
            for backend, status, code in (('python', 'pass', 0), ('rust', 'inconclusive', 2)):
                output = Path(directory) / backend
                result = subprocess.run([sys.executable, 'tools/replay.py', '--candidate', backend,
                    '--output', str(output), str(MEALS)], text=True, capture_output=True)
                self.assertEqual(result.returncode, code, result.stderr)
                report = json.loads((output / 'report.json').read_text())
                markdown = (output / 'report.md').read_text()
                self.assertEqual(report['status'], status)
                self.assertEqual(report['performance'], 'not_run')
                self.assertEqual(report['rust_adapter'], 'unavailable')
                self.assertEqual(report['rust_host_benchmarks'], 'unavailable')
                self.assertIn('Rust adapter and Rust-host benchmarks do not exist', result.stdout)
                self.assertIn('not Rust parity', markdown)
                self.assertIn(self.trace['provenance']['python_revision'], markdown)
                self.assertEqual(markdown, markdown_report(report))
            bad = json.loads(MEALS.read_text())
            bad['steps'][0]['expected']['state']['towns']['a']['owner'] = 'wrong-owner'
            path = Path(directory) / 'bad-trace.json'
            path.write_text(json.dumps(bad))
            output = Path(directory) / 'failed'
            result = subprocess.run([sys.executable, 'tools/replay.py', '--candidate', 'python',
                '--output', str(output), str(path)], text=True, capture_output=True)
            self.assertEqual(result.returncode, 1, result.stderr)
            report = json.loads((output / 'report.json').read_text())
            self.assertEqual(report['status'], 'failed')
            self.assertIn('owner', (output / 'report.md').read_text())

    def test_neutral_module_imports_no_game_code(self):
        result = subprocess.run([sys.executable, '-c',
            'import sys; import tools.replay; assert "server" not in sys.modules; '
            'assert "tools.python_replay" not in sys.modules'], text=True, capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr)
