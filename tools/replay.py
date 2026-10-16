"""Backend-neutral offline replay interface and comparison reports (protocol v1)."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import sys
from typing import Protocol

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
from tools.replay_contract import (ContractError, compare_observation, json_value,
                                   require, validate_draws, validate_operation, validate_trace)

STATUS = ('Rust replay supports transport smoke only; gameplay and Rust-host benchmarks remain unavailable. '
          'tools/server_perf.py measures Python only. No performance measurements are made here.')


class BackendUnavailable(Exception):
    """An adapter is absent; this is never a parity pass."""


class ReplayBackend(Protocol):
    name: str

    def replay(self, request: dict) -> dict:
        """Run a fresh disposable world; return protocol v1 observations, never expected data."""
        ...


class RustBackend:
    name = 'rust'

    def replay(self, request):
        validate_request(request)
        try:
            result = subprocess.run(
                ['cargo', 'run', '--quiet', '-p', 'lantern-host', '--locked', '--offline', '--', 'replay'],
                cwd=ROOT / 'rust', input=json.dumps(request, allow_nan=False),
                text=True, capture_output=True, timeout=300)
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ContractError('Rust replay command unavailable or timed out.') from error
        if result.returncode == 2:
            raise BackendUnavailable('Rust replay workload unsupported; only transport_smoke_v1 is implemented. '
                                     'Gameplay is not ported.')
        require(result.returncode == 0, f'Rust replay command failed (exit {result.returncode}).')
        try:
            response = json.loads(result.stdout)
        except ValueError as error:
            raise ContractError('Rust replay returned invalid JSON.') from error
        return validate_response(response, request)


def validate_request(request):
    json_value(request)
    require(isinstance(request, dict) and set(request) == {
        'protocol_version', 'trace_id', 'initial', 'steps'}, 'invalid replay request envelope')
    require(type(request['protocol_version']) == int and request['protocol_version'] == 1,
            'unsupported replay protocol')
    require(isinstance(request['trace_id'], str) and request['trace_id'], 'trace_id required')
    initial = request['initial']
    require(isinstance(initial, dict) and set(initial) == {'recipe', 'terrain', 'parameters'},
            'invalid replay setup')
    require(isinstance(initial['recipe'], str) and initial['recipe'] and
            initial['terrain'] in ('synthetic_flat', 'real_generator'), 'invalid recipe/terrain')
    require(isinstance(initial['parameters'], dict) and
            set(initial['parameters']) == {'seed', 'setup_draws'} and
            type(initial['parameters']['seed']) == int and
            0 <= initial['parameters']['seed'] < 2**64, 'setup requires uint64 seed and setup_draws only')
    validate_draws(initial['parameters']['setup_draws'], 'setup')
    require(isinstance(request['steps'], list) and request['steps'], 'replay steps required')
    for index, step in enumerate(request['steps']):
        require(isinstance(step, dict) and set(step) == {'operation', 'random_draws'},
                f'step {index}: invalid request step')
        validate_operation(step['operation'], f'step {index}')
        validate_draws(step['random_draws'], f'step {index}')
    return request


def request_for(trace):
    validate_trace(trace)
    require(trace['provenance']['kind'] == 'recorded_python', 'not a recorded Python baseline')
    initial = copy.deepcopy(trace['initial'])
    require(isinstance(initial['parameters'].get('initial_state'), dict),
            'recorded initial state object required')
    initial['parameters'].pop('initial_state')
    return validate_request({'protocol_version': 1, 'trace_id': trace['id'], 'initial': initial,
        'steps': [{key: copy.deepcopy(step[key]) for key in ('operation', 'random_draws')}
                  for step in trace['steps']]})


def validate_response(response, request):
    json_value(response)
    require(isinstance(response, dict) and set(response) == {
        'protocol_version', 'trace_id', 'initial_state', 'observations'}, 'invalid replay response envelope')
    require(type(response['protocol_version']) == int and response['protocol_version'] == 1,
            'unsupported response protocol')
    require(response['trace_id'] == request['trace_id'], 'response trace_id differs')
    require(isinstance(response['initial_state'], dict), 'initial_state must be an object')
    require(isinstance(response['observations'], list) and
            len(response['observations']) == len(request['steps']), 'response observation count differs')
    for index, observation in enumerate(response['observations']):
        require(isinstance(observation, dict) and set(observation) == {'public', 'state'} and
                isinstance(observation['state'], dict), f'step {index}: invalid observation')
    return response


def replay_trace(backend: ReplayBackend, trace):
    """Collect actuals without comparing them; never send the oracle to the backend."""
    request = request_for(trace)
    # Keep validation's copy intact even if an adapter mutates its own input.
    return validate_response(backend.replay(copy.deepcopy(request)), request)


def differences(trace, expected_initial, expected_steps, actual):
    failures = []
    initial = compare_observation(expected_initial, actual['initial_state'])
    if initial:
        failures.append({'stage': 'initial', 'differences': initial})
    for index, (expected, observation) in enumerate(zip(expected_steps, actual['observations'])):
        step = trace['steps'][index]
        found = compare_observation(expected, observation, step['numeric_fields'], step['invariants'])
        if found:
            failures.append({'stage': 'step', 'step': index, 'operation': step['operation'],
                             'differences': found})
    return failures


def baseline_differences(trace, response):
    return differences(trace, trace['initial']['parameters']['initial_state'],
                       [step['expected'] for step in trace['steps']], response)


def failure_message(trace, failures):
    messages = []
    for failure in failures:
        stage = ('initial state' if failure['stage'] == 'initial' else
                 f"step {failure['step']} {failure['operation']}")
        messages.append(f"{trace['id']} {stage}: " + '\n'.join(failure['differences']))
    return '\n'.join(messages)


def compare_backends(trace, reference: ReplayBackend, candidate: ReplayBackend):
    result = {'trace_id': '<invalid fixture>'}
    try:
        request_for(trace)
    except (ContractError, TypeError, KeyError) as error:
        result.update(status='failed', fixture_error=str(error), comparison={'status': 'not_run'})
        return result
    result.update(trace_id=trace['id'], baseline=copy.deepcopy(trace['provenance']))
    actuals = {}
    for role, backend in (('reference', reference), ('candidate', candidate)):
        name = getattr(backend, 'name', None)
        summary = {'backend': name if isinstance(name, str) else '<invalid backend>'}
        try:
            require(isinstance(name, str) and name, 'backend name required')
            response = replay_trace(backend, trace)
            found = baseline_differences(trace, response)
            summary.update(status='failed' if found else 'pass', completed_steps=len(response['observations']),
                           failures=found)
            actuals[role] = response
        except BackendUnavailable as error:
            summary.update(status='unavailable', reason=str(error))
        except ContractError as error:
            summary.update(status='failed', reason=str(error))
        except Exception as error:
            # Unexpected adapter exceptions are failures, not game command errors.
            summary.update(status='failed', reason=f'adapter raised {type(error).__name__}')
        result[role] = summary
    if len(actuals) == 2:
        try:
            found = differences(trace, actuals['reference']['initial_state'],
                                actuals['reference']['observations'], actuals['candidate'])
            result['comparison'] = {'status': 'failed' if found else 'pass', 'failures': found}
        except ContractError as error:
            result['comparison'] = {'status': 'failed', 'reason': str(error)}
    else:
        result['comparison'] = {'status': 'inconclusive', 'reason': 'Two complete backend responses required.'}
    statuses = [result[role]['status'] for role in ('reference', 'candidate', 'comparison')]
    result['status'] = ('failed' if 'failed' in statuses else
                        'inconclusive' if any(s != 'pass' for s in statuses) else 'pass')
    return result


def markdown_report(report):
    lines = [f"Replay status: {report['status']}", '', STATUS, '',
             f"Mode: {report['mode']}. Python/Python is a repeatability check, not Rust parity.", '',
             f"Runner revision: {report['runner_revision']}; working tree dirty: {report['runner_dirty']}.", '',
             '| Trace | Reference vs baseline | Candidate vs baseline | Paired comparison | Result |',
             '| --- | --- | --- | --- | --- |']
    for trace in report['traces']:
        lines.append('| ' + ' | '.join(str(v).replace('|', '\\|').replace('\n', ' ') for v in (
            trace['trace_id'], trace.get('reference', {}).get('status', 'not_run'),
            trace.get('candidate', {}).get('status', 'not_run'), trace['comparison']['status'], trace['status'])) + ' |')
    for trace in report['traces']:
        lines.extend(['', f"Trace: {trace['trace_id']}; recorded Python revision: {trace.get('baseline', {}).get('python_revision')}", ''])
        if trace.get('fixture_error'):
            lines.append(f"- Fixture: {trace['fixture_error']}")
        for role in ('reference', 'candidate', 'comparison'):
            summary = trace.get(role, {})
            if summary.get('reason'):
                lines.append(f"- {role}: {summary['reason']}")
            for failure in summary.get('failures', []):
                lines.append(f"- {role}: {failure_message({'id': trace['trace_id']}, [failure])}")
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--candidate', choices=('python', 'rust'), default='rust')
    parser.add_argument('--output', type=Path, required=True, help='directory for report.json and report.md')
    parser.add_argument('traces', nargs='*', type=Path)
    args = parser.parse_args()
    # Lazy import: the contract/comparator can be used without the Python game.
    from tools.python_replay import PythonBackend, FIXTURES
    paths = args.traces or sorted(FIXTURES.glob('*.json'))
    if not paths:
        parser.error('no traces found')
    candidate = PythonBackend() if args.candidate == 'python' else RustBackend()
    results = []
    for path in paths:
        try:
            trace = json.loads(path.read_text())
            require(isinstance(trace, dict), 'trace must be an object')
            results.append(compare_backends(trace, PythonBackend(), candidate))
        except (OSError, ValueError) as error:
            results.append({'trace_id': path.name, 'status': 'failed', 'fixture_error': str(error),
                            'comparison': {'status': 'not_run'}})
    status = ('failed' if any(r['status'] == 'failed' for r in results) else
              'inconclusive' if any(r['status'] != 'pass' for r in results) else 'pass')
    report = {'schema_version': 1, 'status': status,
              'mode': 'python_repeatability' if args.candidate == 'python' else 'python_vs_rust',
              'runner_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
              'runner_dirty': bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
              'rust_adapter': 'transport_smoke_only', 'rust_host_benchmarks': 'unavailable',
              'performance': 'not_run', 'traces': results}
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    (args.output / 'report.md').write_text(markdown_report(report))
    print(STATUS)
    print(f"Replay status: {status}; {len(results)} traces. Reports: {args.output}")
    return {'pass': 0, 'failed': 1, 'inconclusive': 2}[status]


if __name__ == '__main__':
    sys.exit(main())
