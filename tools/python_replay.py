"""Disposable Python recording and protocol-v1 replay adapter (R03)."""
import argparse
from contextlib import ExitStack
import copy
import hashlib
import inspect
import json
import re
from pathlib import Path
import random
import secrets
import subprocess
import sys
import tempfile
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(ROOT / 'server'), str(ROOT)]
import server
import logistics
import navigation
import progression as economy
import shared_world
import work_cycles
from tools.replay_contract import (ContractError, canonical_json, compare_observation,
                                   require, validate_trace)

STATUS = ('Python synthetic replay only. Rust adapter and Rust-host benchmarks do not '
          'exist yet; tools/server_perf.py measures Python only. Backend-neutral '
          'execution/reporting is available in tools/replay.py.')
FIXTURES = ROOT / 'tests/replay/python'
RECIPES = ('relocation_v1', 'blocked_delivery_v1', 'workshop_v1', 'meals_v1', 'terrain_quarry_v1')


class Inputs:
    """Record/inject process-global gameplay entropy, checking exact draw signatures."""
    def __init__(self, seed):
        self.rng = random.Random(seed)
        self.seconds = 0
        self.draws = []
        self.expected = None

    def begin(self, expected=None):
        self.draws, self.expected = [], expected

    def finish(self):
        if self.expected is not None:
            require(len(self.draws) == len(self.expected), 'unused random draws')
        return copy.deepcopy(self.draws)

    def draw(self, method, *args):
        frame = inspect.currentframe().f_back.f_back
        site = f"{frame.f_globals['__name__']}.{frame.f_code.co_name}:{frame.f_lineno}"
        signature = {'site': site, 'method': method, 'args': json.loads(json.dumps(args))}
        if method == 'token_hex':
            require(site.startswith('server.command:'), 'ID entropy outside command fixture')
        if self.expected is not None:
            index = len(self.draws)
            require(index < len(self.expected), f'unexpected random draw at {site}: {method}')
            draw = self.expected[index]
            require({k: draw[k] for k in signature} == signature,
                    f'random draw {index} signature differs at {site}: {method}')
            value = copy.deepcopy(draw['value'])
        elif method == 'randbelow':
            value = self.rng.randrange(args[0])
        elif method == 'randbits':
            value = self.rng.getrandbits(args[0])
        elif method == 'token_hex':
            # Synthetic queue IDs only; never accounts, credentials or world registries.
            value = f'{self.rng.getrandbits(args[0] * 8):0{args[0] * 2}x}'
        elif method == 'randint':
            value = self.rng.randint(*args)
        elif method == 'choice':
            value = self.rng.choice(args[0])
        elif method == 'random':
            value = self.rng.random()
        else:
            raise ContractError(f'uncontrolled entropy method {method}')
        if method in ('randbelow', 'randbits', 'randint'):
            lower, upper = ((0, args[0] - 1) if method == 'randbelow' else
                            (0, 2**args[0] - 1) if method == 'randbits' else tuple(args))
            require(type(value) == int and lower <= value <= upper, f'invalid {method} draw at {site}')
        elif method == 'token_hex':
            require(isinstance(value, str) and re.fullmatch('[0-9a-f]{' + str(args[0] * 2) + '}', value),
                    f'invalid synthetic ID draw at {site}')
        elif method == 'choice':
            require(value in signature['args'][0], f'invalid choice draw at {site}')
        elif method == 'random':
            require(type(value) in (int, float) and 0 <= value < 1, f'invalid random draw at {site}')
        self.draws.append(dict(signature, value=value))
        return value

    def install(self, stack):
        for module, methods in ((secrets, ('randbits', 'randbelow', 'token_hex', 'choice')),
                                (random, ('randint', 'choice', 'random'))):
            for method in methods:
                stack.enter_context(patch.object(module, method,
                    lambda *args, method=method: self.draw(method, *args)))
        for method in ('randrange', 'getrandbits', 'uniform', 'shuffle', 'sample', 'choices'):
            stack.enter_context(patch.object(random, method, side_effect=ContractError('uncovered global random method')))
        for method in ('token_urlsafe', 'token_bytes'):
            stack.enter_context(patch.object(secrets, method, side_effect=ContractError('credential entropy forbidden')))
        for method in ('time', 'monotonic'):
            stack.enter_context(patch.object(time, method, lambda: self.seconds))
        stack.enter_context(patch.object(time, 'sleep', side_effect=ContractError('wall-clock sleep forbidden')))
        original_random = random.Random

        def terrain_rng(seed=None):
            module = inspect.currentframe().f_back.f_globals['__name__']
            require(seed is not None and module in {
                'terrain', 'shared_world', 'npcs', 'terrain_regions', 'chunk_world'},
                'only seeded terrain/NPC local RNGs are supported')
            return original_random(seed)
        stack.enter_context(patch.object(random, 'Random', terrain_rng))
        cache = navigation.RouteCache()
        for module in (navigation, server, logistics, work_cycles):
            stack.enter_context(patch.object(module, 'routes', cache))
        # Filesystem entry points with global/live roots are never part of fixtures.
        for name in ('load', 'save', 'activate'):
            stack.enter_context(patch.object(server, name, side_effect=ContractError('live save access forbidden')))


def flat_town(seed):
    landscape = {'map_size': 16, 'terrain_seed': seed, 'terrain': ['g' * 16] * 16,
                 'map_resources': [], 'npcs': [], 'tree_stumps': [],
                 'forage_version': 1, 'forage_halls': [[5, 5]]}
    town = server.new_town('Replay a', {'shared_map': landscape, 'towns': {},
                                      'town_sites': [{'x': 5, 'y': 5}]})
    for worker in town['villagers']:
        worker['talents'] = {}
    economy.ensure_inventory(town)
    return town


def validate_initial(initial):
    recipe, parameters = initial['recipe'], initial['parameters']
    require(recipe in RECIPES, 'unknown synthetic recipe')
    require(set(parameters) == {'seed', 'setup_draws', 'initial_state'} and type(parameters['seed']) == int
            and 0 <= parameters['seed'] < 2**64 and isinstance(parameters['setup_draws'], list)
            and isinstance(parameters['initial_state'], dict),
            'recipe parameters require uint64 seed, setup_draws and initial_state only')
    real = recipe == 'terrain_quarry_v1'
    require(initial['terrain'] == ('real_generator' if real else 'synthetic_flat'),
            'recipe terrain mode differs')
    return recipe, parameters, real


def setup(initial):
    recipe, parameters, real = validate_initial(initial)
    if real:
        world = shared_world.new_world(seed=parameters['seed'], terrain_version=1)
        for key in ('a', 'b', 'spectator'):
            world['towns'][key] = server.new_town(f'Replay {key}', world)
        return world
    town = flat_town(parameters['seed'])
    world = {'towns': {'a': town}}
    worker = town['villagers'][0]
    if recipe == 'relocation_v1':
        camp = {'kind': 'woodcutter', 'x': 9, 'y': 3}
        tree = {'kind': 'tree', 'x': 12, 'y': 3}
        town['buildings'].append(camp)
        town['map_resources'] = [tree]
        worker.update(job='woodcutting', building_index=1, construction_position={'x': 9.25, 'y': 1})
        work_cycles.advance(town, town, worker, camp, tree, 0, 0)
        logistics.record_output(town, camp, 'wood', 2, worker)
        worker['work_animation'].update(phase='returning', x=11.25, y=2,
                                        path=[{'x': 10, 'y': 3}])
    elif recipe == 'blocked_delivery_v1':
        source = {'kind': 'woodcutter', 'x': 10, 'y': 3, 'output_buffer': {'wood': 3}}
        store = {'kind': 'storehouse', 'x': 2, 'y': 2, 'storage_allocations': {'wood': 100}}
        town['buildings'].extend([source, store])
        economy.add_stock(town, 'wood', 3)
        worker.update(job='hauling', building_index=2, logistics_position={'x': 10, 'y': 2})
        require(logistics.start_trip(town, town, worker, source, store, 'collect',
                                     server.logistics_route, hauler=True), 'initial trip unreachable')
        # A saved trip planned before a gate closed. Its next route now hits a wall.
        town['map_resources'] = [{'kind': 'rock', 'x': 7, 'y': y} for y in range(16) if y != 10]
        town['buildings'].append({'kind': 'woodcutter', 'x': 7, 'y': 10})
    elif recipe == 'workshop_v1':
        town['level'] = 4
        town['buildings'].append({'kind': 'workshop', 'x': 9, 'y': 9,
                                  'footprint': [2, 2], 'input_buffer': {'planks': 4, 'wood': 2}})
        worker.update(job='crafting', building_index=1, crafting=0, woodworking=2,
                      construction_position={'x': 9, 'y': 8})
    return world


def project(world, result, initial=False):
    """Explicit, stable selected state; derived route caches never enter observations."""
    towns = {}
    for owner, town in world['towns'].items():
        buildings = [{k: copy.deepcopy(b[k]) for k in (
            'kind', 'x', 'y', 'footprint', 'stored_goods', 'input_buffer', 'output_buffer',
            'pending_output', 'craft_queue', 'recipe', 'construction') if k in b}
            for b in town['buildings']]
        workers = []
        for worker in town['villagers']:
            selected = {k: copy.deepcopy(worker[k]) for k in (
                'name', 'job', 'building_index', 'pending_output', 'pending_workplace',
                'logistics_position', 'logistics_trip', 'production_work', 'production_status',
                'woodcutting', 'crafting', 'hauling') if k in worker}
            if 'work_animation' in worker:
                selected['work_animation'] = {k: copy.deepcopy(worker['work_animation'][k]) for k in (
                    'x', 'y', 'phase', 'task', 'elapsed', 'path', 'resume_phase')
                    if k in worker['work_animation']}
            workers.append(selected)
        towns[owner] = {'owner': owner, 'wood': town['wood'], 'food': town['food'],
                        'coins': town['coins'], 'goods': copy.deepcopy(town.get('goods', {})),
                        'produced': copy.deepcopy(town.get('produced', {})),
                        'player': copy.deepcopy(town['player']), 'buildings': buildings,
                        'workers': workers}
        # Physical ledger: building pending_output mirrors worker cargo; do not add it.
        towns[owner]['wood_ledger'] = {
            'stored': sum(b.get(field, {}).get('wood', 0) for b in town['buildings']
                          for field in ('stored_goods', 'output_buffer', 'input_buffer')),
            'carried': sum(w.get('pending_output', {}).get('wood', 0) +
                (w.get('logistics_trip', {}).get('cargo', {}).get('wood', 0)
                 if w.get('logistics_trip', {}).get('picked_up') and
                    not w.get('logistics_trip', {}).get('delivered') else 0)
                for w in town['villagers'])}
    state = {'towns': towns}
    if 'quarry' in world:
        state['quarry'] = copy.deepcopy(world['quarry'])
    if 'shared_map' in world:
        landscape = world['shared_map']
        state['terrain'] = {'seed': landscape['terrain_seed'], 'version': landscape.get('terrain_version', 1),
                            'rows_sha256': hashlib.sha256(canonical_json(landscape['terrain']).encode()).hexdigest(),
                            'resource_count': len(landscape['map_resources']),
                            'resources_sha256': hashlib.sha256(canonical_json(landscape['map_resources']).encode()).hexdigest(),
                            'npc_count': len(landscape['npcs']), 'town_sites': world['town_sites']}
        if initial:
            state['terrain']['rows'] = copy.deepcopy(landscape['terrain'])
    return {'public': copy.deepcopy(result), 'state': state}


def execute(world, recipe, operation, inputs, directory):
    kind = operation['kind']
    if kind == 'command':
        require(operation['town'] in world['towns'], 'unknown command town')
        try:
            value = server.command(world['towns'][operation['town']], operation['data'], world)
            result = {'result': value}
        except ContractError:
            raise
        except ValueError as error:
            result = {'error': {'type': 'ValueError', 'message': str(error)}}
        return world, result
    if kind == 'reload':
        target = Path(directory) / 'synthetic-world.json'
        target.write_text(json.dumps(shared_world.export_world(world)))
        world = json.loads(target.read_text())
        shared_world.bind_map(world)
        # Reset the isolated shared cache as a restarted host would.
        server.routes.__init__()
        server.migrate_footprints(world)
        for town in world['towns'].values():
            server.ensure_resources(town)
            server.ensure_villager_names(town)
            server.ensure_animal_names(town)
        return world, {'result': None}
    seconds, clock = operation['seconds'], operation['clock']
    inputs.seconds += seconds
    town = world['towns']['a']
    worker = town['villagers'][0]
    if clock == 'economy':
        require(seconds == server.TICK_SECONDS, 'economy advances require exactly three seconds')
        server.tick(world)
    elif clock == 'movement':
        require(recipe in ('relocation_v1', 'blocked_delivery_v1'), 'movement unsupported for recipe')
        if recipe == 'relocation_v1':
            camp, tree = town['buildings'][1], town['map_resources'][0]
            if work_cycles.advance(town, town, worker, camp, tree, 0, seconds):
                logistics.drop_off(town, camp, worker)
        else:
            logistics.advance_trip(town, town, worker, seconds)
    else:
        raise ContractError('quarry is command-driven; no adventure time service in these recipes')
    return world, {'result': None}


class PythonBackend:
    name = 'python'

    def replay(self, request):
        from tools.replay import validate_request
        validate_request(request)
        initial = copy.deepcopy(request['initial'])
        # Expected initial state never crosses the backend boundary.
        initial['parameters']['initial_state'] = {}
        validate_initial(initial)
        inputs = Inputs(initial['parameters']['seed'])
        observations = []
        with ExitStack() as stack, tempfile.TemporaryDirectory(prefix='lantern-replay-') as directory:
            inputs.install(stack)
            inputs.begin(initial['parameters']['setup_draws'])
            world = setup(initial)
            inputs.finish()
            initial_state = project(world, {'result': None}, initial=True)['state']
            for index, step in enumerate(request['steps']):
                inputs.begin(step['random_draws'])
                try:
                    world, result = execute(world, initial['recipe'], step['operation'], inputs, directory)
                    inputs.finish()
                    observations.append(project(world, result))
                except ContractError as error:
                    raise ContractError(f"{request['trace_id']} step {index} {step['operation']}: {error}") from error
        return {'protocol_version': 1, 'trace_id': request['trace_id'],
                'initial_state': initial_state, 'observations': observations}


def run_trace(trace, recording=False):
    if not recording:
        from tools.replay import replay_trace, baseline_differences, failure_message
        response = replay_trace(PythonBackend(), trace)
        failures = baseline_differences(trace, response)
        require(not failures, failure_message(trace, failures))
        return response['observations']
    initial = trace['initial']
    validate_initial(initial)
    inputs = Inputs(initial['parameters']['seed'])
    observations = []
    with ExitStack() as stack, tempfile.TemporaryDirectory(prefix='lantern-replay-') as directory:
        inputs.install(stack)
        inputs.begin()
        world = setup(initial)
        draws = inputs.finish()
        initial_state = project(world, {'result': None}, initial=True)['state']
        initial['parameters']['setup_draws'] = draws
        initial['parameters']['initial_state'] = initial_state
        for index, step in enumerate(trace['steps']):
            inputs.begin()
            try:
                world, result = execute(world, initial['recipe'], step['operation'], inputs, directory)
                draws = inputs.finish()
                actual = project(world, result)
                step.update(expected=actual, random_draws=draws)
                failures = compare_observation(step['expected'], actual,
                                               step['numeric_fields'], step['invariants'])
                require(not failures, '\n'.join(failures))
            except ContractError as error:
                raise ContractError(f"{trace['id']} step {index} {step['operation']}: {error}") from error
            observations.append(actual)
    validate_trace(trace)
    return observations


def source_revision():
    dirty = subprocess.check_output(['git', 'status', '--porcelain', '--', 'server'], cwd=ROOT, text=True)
    require(not dirty, 'recording requires clean Python game sources under server/')
    return subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--record', type=Path, help='write baselines into a new/empty directory only')
    parser.add_argument('traces', nargs='*', type=Path)
    args = parser.parse_args()
    print(STATUS)
    try:
        if args.record:
            require(not args.traces, '--record does not accept input traces')
            revision = source_revision()
            require(not args.record.exists() or not any(args.record.iterdir()), 'record destination must be empty')
            traces = plans(revision)
            for trace in traces:
                run_trace(trace, recording=True)
                run_trace(trace)
            args.record.mkdir(parents=True, exist_ok=True)
            for trace in traces:
                (args.record / (trace['id'] + '.json')).write_text(json.dumps(trace, indent=2) + '\n')
            print(f'Recorded and replayed {len(traces)} traces; Python source revision {revision}.')
        else:
            paths = args.traces or sorted(FIXTURES.glob('*.json'))
            require(paths, 'no Python traces found')
            for path in paths:
                trace = json.loads(path.read_text())
                first = run_trace(trace)
                require(first == run_trace(trace), f'{trace["id"]}: repeated replay differs')
                print(f"PASS {trace['id']}: {len(first)} steps, replayed twice; Python baseline {trace['provenance']['python_revision']}")
    except (ContractError, OSError, ValueError) as error:
        parser.exit(1, f'{error}\n')


# Versioned scenario scripts and independent assertions are intentionally reviewable
# here, separate from observation recording. No baseline is refreshed by replay.
def command(town='a', **data):
    return {'kind': 'command', 'town': town, 'data': data}


def advance(clock='economy', seconds=3):
    return {'kind': 'advance', 'clock': clock, 'seconds': seconds}


def equals(name, path, value):
    return {'name': name, 'kind': 'equals', 'paths': [path], 'value': value}


def plans(revision):
    def trace(recipe, operations):
        return {'format_version': 1, 'id': recipe, 'provenance': {
            'kind': 'recorded_python', 'python_revision': revision},
            'initial': {'recipe': recipe, 'parameters': {'seed': 42, 'setup_draws': [], 'initial_state': {}},
                        'terrain': 'real_generator' if recipe == 'terrain_quarry_v1' else 'synthetic_flat'},
            'steps': [{'operation': op, 'random_draws': [], 'expected': {},
                       'numeric_fields': [], 'invariants': []} for op in operations]}
    relocation = trace('relocation_v1', [advance('movement', 0),
        command(action='move', index=1, x=8, y=4), {'kind': 'reload'},
        *[advance('movement', .5) for _ in range(10)]])
    for step in relocation['steps']:
        step['invariants'] = [equals('worker assignment retained', ['state', 'towns', 'a', 'workers', 0, 'building_index'], 1)]
    for step in relocation['steps'][:3]:
        step['invariants'].extend([equals('cargo retained', ['state', 'towns', 'a', 'workers', 0, 'pending_output', 'wood'], 2),
            equals('position retained', ['state', 'towns', 'a', 'workers', 0, 'work_animation', 'x'], 11.25)])
    relocation['steps'][-1]['invariants'].append(equals('output credited once', ['state', 'towns', 'a', 'wood'], 67))
    blocked = trace('blocked_delivery_v1', [advance('movement', 3), advance('movement', 3),
        {'kind': 'reload'}, command(action='destroy', index=3),
        advance('movement', 0), advance('movement', 10), advance('movement', 10)])
    for step in blocked['steps'][:3]:
        step['invariants'] = [equals('carried wood survives unreachable route', ['state', 'towns', 'a', 'workers', 0, 'logistics_trip', 'cargo', 'wood'], 3),
            equals('not delivered while blocked', ['state', 'towns', 'a', 'workers', 0, 'logistics_trip', 'delivered'], False)]
    blocked['steps'][-1]['invariants'] = [equals('delivery credited once', ['state', 'towns', 'a', 'buildings', 2, 'stored_goods', 'wood'], 3)]
    workshop = trace('workshop_v1', [command(action='queue_workshop', index=1, recipe='tools', amount=2),
        command(action='queue_workshop', index=1, recipe='tools', amount=1),
        command(action='queue_workshop', index=1, recipe='tools', amount=0),
        advance(), {'kind': 'reload'}, *[advance() for _ in range(12)]])
    workshop['steps'][-1]['invariants'] = [equals('only funded order produced', ['state', 'towns', 'a', 'produced', 'tools'], 2.0),
        equals('waiting order remains', ['state', 'towns', 'a', 'buildings', 1, 'craft_queue', 0, 'remaining'], 1)]
    meals = trace('meals_v1', [advance(), advance(), {'kind': 'reload'}, advance()])
    for index in range(4):
        meals['steps'][index]['numeric_fields'] = [{'path': ['state', 'towns', 'a', 'food'], 'kind': 'fraction'}]
        # Conservation against independently calculated 5 idle residents * .6/min.
        meals['steps'][index]['invariants'] = [equals('wood untouched by meals', ['state', 'towns', 'a', 'wood'], 65),
            {'name': 'food nonnegative', 'kind': 'nonnegative', 'paths': [['state', 'towns', 'a', 'food']]}]
    ops = [command(action='join_quarry'), command('b', action='join_quarry'),
           command(action='quarry_walk', dx=1, dy=0), command(action='quarry_walk', dx=0, dy=-1)]
    ops.extend(command('b', action='quarry_walk', dx=1, dy=0) for _ in range(5))
    ops.extend([{'kind': 'reload'}, command('b', action='quarry_walk', dx=1, dy=0),
                command('b', action='quarry_walk', dx=0, dy=-1)])
    ops.extend(command('b', action='quarry_interact', target=target) for target in ('sun', 'leaf'))
    ops.extend([command('b', action='quarry_walk', dx=1, dy=0), command('b', action='quarry_interact', target='moon'),
                command('b', action='quarry_walk', dx=0, dy=1), command('b', action='quarry_walk', dx=0, dy=1),
                command('b', action='quarry_interact', target='treasure'),
                command('b', action='quarry_interact', target='treasure')])
    quarry = trace('terrain_quarry_v1', ops)
    for index, step in enumerate(quarry['steps']):
        step['invariants'] = [equals('spectator receives no reward', ['state', 'towns', 'spectator', 'coins'], 20)]
        for owner in ('a', 'b'):
            step['invariants'].append(equals(f'{owner} exact-once reward', ['state', 'towns', owner, 'coins'], 40 if index >= 18 else 20))
    for fixture, total in ((relocation, 67), (blocked, 68)):
        for step in fixture['steps']:
            step['invariants'].append({'name': 'physical wood conserved', 'kind': 'sum',
                'paths': [['state', 'towns', 'a', 'wood_ledger', key] for key in ('stored', 'carried')],
                'value': total})
    return [relocation, blocked, workshop, meals, quarry]


if __name__ == '__main__':
    main()
