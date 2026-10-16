"""R03 v1 trace format and independent comparison primitives (no game imports)."""
import argparse
import json
import math
import re
from pathlib import Path


TOLERANCES = {'position': 1e-6, 'fraction': 1e-9}
STATUS = ('Format validation only. Python recording/replay is in tools/python_replay.py; paired reports are in tools/replay.py. '
          'Rust replay supports transport smoke only; gameplay and Rust-host benchmarks remain unavailable; tools/server_perf.py '
          'measures Python only.')
PRIVATE_KEYS = {'token', 'tokens', 'pin', 'pin_hash', 'password', 'credentials',
                'player_accounts', 'accounts', 'authorization'}


class ContractError(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise ContractError(message)


def json_value(value, path='$'):
    """Reject non-JSON/nonfinite data and known private fields, without printing values."""
    if isinstance(value, dict):
        for key, child in value.items():
            require(isinstance(key, str), f'{path}: object key must be a string')
            require(key.lower() not in PRIVATE_KEYS, f'{path}: private field forbidden')
            json_value(child, f'{path}.{key}')
    elif isinstance(value, list):
        for index, child in enumerate(value):
            json_value(child, f'{path}[{index}]')
    else:
        require(value is None or type(value) in (str, bool, int, float), f'{path}: not JSON')
        if type(value) == float:
            require(math.isfinite(value), f'{path}: nonfinite number')


def canonical_json(value):
    """Only object-key order/JSON whitespace normalize; arrays and values stay intact."""
    json_value(value)
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False,
                      allow_nan=False)


def at(value, path):
    require(isinstance(path, list) and all(type(p) in (str, int) for p in path),
            'path must be an array of string keys/integer indexes')
    for part in path:
        if isinstance(value, dict) and type(part) == str and part in value:
            value = value[part]
        elif isinstance(value, list) and type(part) == int and 0 <= part < len(value):
            value = value[part]
        else:
            raise ContractError(f'missing path {path!r}')
    return value


def number(value):
    return type(value) in (int, float) and (type(value) == int or math.isfinite(value))


def compare_observation(expected, actual, numeric_fields=(), invariants=()):
    """Return path-specific differences; independent invariants also check the reference."""
    json_value(expected)
    json_value(actual)
    tolerances = {}
    for field in numeric_fields:
        require(set(field) == {'path', 'kind'} and field['kind'] in TOLERANCES,
                'numeric field requires path and position/fraction kind')
        value = at(expected, field['path'])
        require(number(value), f"nonnumeric tolerance path {field['path']!r}")
        key = tuple(field['path'])
        require(key not in tolerances, f'duplicate tolerance path {key!r}')
        tolerances[key] = TOLERANCES[field['kind']]
    failures = []

    def walk(left, right, path):
        label = repr(list(path))
        if number(left) and number(right):
            # Integer reference quantities/IDs remain exact even when misannotated.
            tolerance = tolerances.get(path, 0) if type(left) == float else 0
            if (left != right if tolerance == 0 else
                    not left - tolerance <= right <= left + tolerance):
                failures.append(f'{label}: expected {left!r}, got {right!r} (absolute tolerance {tolerance})')
        elif type(left) != type(right):
            failures.append(f'{label}: expected type {type(left).__name__}, got {type(right).__name__}')
        elif isinstance(left, dict):
            for key in sorted(left.keys() | right.keys()):
                if key not in left or key not in right:
                    failures.append(f'{list(path + (key,))!r}: missing or unexpected field')
                else:
                    walk(left[key], right[key], path + (key,))
        elif isinstance(left, list):
            if len(left) != len(right):
                failures.append(f'{label}: expected length {len(left)}, got {len(right)}')
            for index, (a, b) in enumerate(zip(left, right)):
                walk(a, b, path + (index,))
        elif left != right:
            failures.append(f'{label}: expected {left!r}, got {right!r}')

    walk(expected, actual, ())
    for rule in invariants:
        require(isinstance(rule, dict) and rule.get('kind') in ('equals', 'sum', 'nonnegative'),
                'unknown invariant kind')
        kind = rule['kind']
        keys = {'name', 'kind', 'paths'} | ({'value'} if kind != 'nonnegative' else set())
        require(set(rule) == keys and isinstance(rule['name'], str) and rule['name'],
                'invariant requires name, kind, paths and (except nonnegative) value')
        require(isinstance(rule['paths'], list) and rule['paths'], 'invariant paths required')
        if kind == 'sum':
            require(number(rule['value']), 'sum invariant value must be numeric')
        for label, observation in (('reference', expected), ('candidate', actual)):
            try:
                values = [at(observation, path) for path in rule['paths']]
                if kind == 'equals':
                    ok = all(canonical_json(v) == canonical_json(rule['value']) for v in values)
                else:
                    require(all(number(v) for v in values), 'invariant requires numeric values')
                    total = (sum(values) if all(type(v) == int for v in values)
                             else math.fsum(values)) if kind == 'sum' else None
                    ok = (total == rule['value'] if kind == 'sum'
                          else all(v >= 0 for v in values))
                if not ok:
                    failures.append(f"{label} invariant {rule['name']!r} ({kind}) failed at {rule['paths']!r}")
            except ContractError as error:
                failures.append(f"{label} invariant {rule['name']!r}: {error}")
    return failures


def validate_operation(operation, prefix="operation"):
    require(isinstance(operation, dict), f'{prefix}: operation required')
    op = operation.get('kind')
    keys = {'advance': {'kind', 'clock', 'seconds'},
            'command': {'kind', 'town', 'data'}, 'reload': {'kind'}}
    require(isinstance(op, str) and op in keys and set(operation) == keys[op], f'{prefix}: invalid operation')
    if op == 'advance':
        require(operation['clock'] in ('movement', 'economy', 'adventure') and
                number(operation['seconds']) and operation['seconds'] >= 0,
                f'{prefix}: invalid explicit time advance')
    if op == 'command':
        require(isinstance(operation['town'], str) and operation['town'] and
                isinstance(operation['data'], dict) and
                isinstance(operation['data'].get('action'), str), f'{prefix}: invalid command')


def validate_draws(draws, prefix="random draws"):
    require(isinstance(draws, list), f'{prefix}: draws must be ordered')
    for draw in draws:
        require(isinstance(draw, dict) and set(draw) == {'site', 'method', 'args', 'value'} and
                isinstance(draw['site'], str) and draw['site'] and
                isinstance(draw['method'], str) and draw['method'] and
                isinstance(draw['args'], list), f'{prefix}: invalid random draw')


def validate_trace(trace):
    json_value(trace)
    require(isinstance(trace, dict) and set(trace) == {
        'format_version', 'id', 'provenance', 'initial', 'steps'}, 'invalid trace envelope')
    require(type(trace['format_version']) == int and trace['format_version'] == 1,
            'unsupported trace format')
    require(isinstance(trace['id'], str) and trace['id'], 'trace id required')
    provenance = trace['provenance']
    require(isinstance(provenance, dict) and set(provenance) == {'kind', 'python_revision'},
            'invalid provenance')
    require(provenance['kind'] in ('contract_example', 'recorded_python'), 'invalid baseline kind')
    require(isinstance(provenance['python_revision'], str) and
            re.fullmatch('[0-9a-f]{40}', provenance['python_revision']), 'full Python revision required')
    initial = trace['initial']
    require(isinstance(initial, dict) and set(initial) == {'recipe', 'parameters', 'terrain'},
            'initial state requires recipe, parameters and terrain')
    require(isinstance(initial['recipe'], str) and initial['recipe'] and
            isinstance(initial['parameters'], dict), 'invalid synthetic recipe')
    require(initial['terrain'] in ('synthetic_flat', 'real_generator'), 'invalid terrain mode')
    require(isinstance(trace['steps'], list) and trace['steps'], 'steps required')
    for index, step in enumerate(trace['steps']):
        prefix = f'step {index}'
        require(isinstance(step, dict) and set(step) == {
            'operation', 'random_draws', 'expected', 'numeric_fields', 'invariants'},
            f'{prefix}: invalid step envelope')
        validate_operation(step['operation'], prefix)
        validate_draws(step['random_draws'], prefix)
        require(isinstance(step['expected'], dict) and set(step['expected']) == {'public', 'state'},
                f'{prefix}: public and authoritative state projections required')
        require(isinstance(step['numeric_fields'], list) and isinstance(step['invariants'], list),
                f'{prefix}: numeric_fields and invariants must be arrays')
        failures = compare_observation(step['expected'], step['expected'],
                                       step['numeric_fields'], step['invariants'])
        require(not failures, f'{prefix}: ' + '; '.join(failures))
    return trace


def main():
    parser = argparse.ArgumentParser(description=STATUS)
    parser.add_argument('trace', type=Path, help='synthetic trace JSON only; never a save file')
    args = parser.parse_args()
    print(STATUS)
    try:
        trace = validate_trace(json.loads(args.trace.read_text()))
    except (OSError, ValueError, TypeError, KeyError) as error:
        parser.exit(1, f'Invalid trace: {error}\n')
    print(f"Valid v1 format: {trace['id']} ({trace['provenance']['kind']}); no replay performed.")


if __name__ == '__main__':
    main()
