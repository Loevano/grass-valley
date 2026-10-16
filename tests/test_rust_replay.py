"""Rust stdio transport acceptance; fixtures here establish no gameplay parity."""
import copy
import json
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch

from tools.replay import (BackendUnavailable, RustBackend, request_for,
                          validate_request, validate_response)
from tools.replay_contract import ContractError

FIXTURE = Path('tests/replay/rust/transport_smoke_v1.json')


class RustReplayTransport(unittest.TestCase):
    def setUp(self):
        self.request = json.loads(FIXTURE.read_text())

    def test_real_rust_cli_runs_independent_smoke_requests_without_mutating_input(self):
        original = copy.deepcopy(self.request)
        validate_request(self.request)
        first = RustBackend().replay(self.request)
        second = RustBackend().replay(self.request)
        self.assertEqual(first, second)
        self.assertEqual(self.request, original)
        validate_response(first, self.request)
        self.assertEqual(first['initial_state'], {'recipe': 'transport_smoke_v1', 'gameplay_ported': False})
        self.assertEqual(first['observations'], [
            {'public': {'result': None}, 'state': first['initial_state']}
            for _ in self.request['steps']])

    def test_real_gameplay_recipe_is_explicitly_unsupported(self):
        trace = json.loads(Path('tests/replay/python/meals_v1.json').read_text())
        with self.assertRaisesRegex(BackendUnavailable, 'Gameplay is not ported'):
            RustBackend().replay(request_for(trace))

    def test_oracle_and_private_inputs_are_rejected_before_process_launch(self):
        for field in ('expected', 'pin_hash'):
            request = copy.deepcopy(self.request)
            request[field] = 'never-send-me'
            with patch('tools.replay.subprocess.run') as run, self.assertRaises(ContractError):
                RustBackend().replay(request)
            run.assert_not_called()

    def test_process_uses_only_fixed_offline_cargo_command_and_request_envelope(self):
        response = RustBackend().replay(self.request)
        completed = subprocess.CompletedProcess([], 0, json.dumps(response), '')
        with patch('tools.replay.subprocess.run', return_value=completed) as run:
            RustBackend().replay(self.request)
        self.assertEqual(run.call_args.args[0], [
            'cargo', 'run', '--quiet', '-p', 'lantern-host', '--locked', '--offline', '--', 'replay'])
        self.assertEqual(json.loads(run.call_args.kwargs['input']), self.request)
        self.assertEqual(run.call_args.kwargs['cwd'].name, 'rust')

    def test_unavailable_timed_out_failed_or_invalid_responses_cannot_pass_or_leak_output(self):
        for error in (FileNotFoundError('private-path'), subprocess.TimeoutExpired('cargo', 300)):
            with self.subTest(error=error), patch('tools.replay.subprocess.run', side_effect=error):
                with self.assertRaises(ContractError):
                    RustBackend().replay(self.request)
        for code, stdout in ((101, 'private-error'), (1, 'private-error'), (0, 'private-error'),
                             (0, '{}'), (0, '{"pin_hash":"private-error"}')):
            completed = subprocess.CompletedProcess([], code, stdout, 'private-error')
            with self.subTest(code=code, stdout=stdout), \
                    patch('tools.replay.subprocess.run', return_value=completed):
                with self.assertRaises(ContractError) as error:
                    RustBackend().replay(self.request)
                self.assertNotIn('private-error', str(error.exception))
        with patch('tools.replay.subprocess.run', return_value=subprocess.CompletedProcess([], 2, '', '')):
            with self.assertRaises(BackendUnavailable):
                RustBackend().replay(self.request)
