"""Benchmark any source revision with the same harness and disposable saves."""
import argparse
from concurrent.futures import ThreadPoolExecutor
from contextlib import nullcontext
import copy
import http.client
from http.server import ThreadingHTTPServer
import importlib.util
import json
import math
from pathlib import Path
import random
import sys
import tempfile
import threading
import time
from unittest.mock import patch


def percentile(values, fraction):
    values = sorted(values)
    return values[min(len(values) - 1, int(len(values) * fraction))]


def routing_scenarios(server, metrics, checks):
    """Same mass-replan workload against synchronous and queued revisions.

    Measure update responsiveness separately from total time to finish all
    requests. Workers from four towns share the host allowance; completed paths
    are retained, just as gameplay retains routes between task changes.
    """
    from navigation import routes
    diagnostics = []
    for count in (16, 64, 256):
        workers = [{'job': 'building'} for _ in range(count)]
        towns = []
        for town in range(4):
            towns.append({'map_size': 64, 'terrain': ['g' * 64] * 64,
                          'buildings': [], 'npcs': [],
                          'map_resources': [{'kind': 'rock', 'x': 30 + town, 'y': y}
                                            for y in range(55)], 'roads': []})
        world = {'towns': {str(i): town for i, town in enumerate(towns)}}
        for state in ('cold', 'warm', 'obstacle'):
            if state == 'cold':
                routes.graphs.clear()
            if state == 'obstacle':
                for town in towns:
                    town['map_resources'].append({'kind': 'rock', 'x': 29, 'y': 55})
            results, timings, work = {}, [], []
            started = time.perf_counter()
            for update in range(4000):
                before = time.perf_counter()
                with routes.update(world) if hasattr(routes, 'update') else nullcontext():
                    for index, worker in enumerate(workers):
                        if index in results:
                            continue
                        town = towns[index % 4]
                        start = (2 + index // 64, 2 + index % 16)
                        target = {'x': 50 + index % 8, 'y': 5 + index // 16}
                        with routes.worker(worker) if hasattr(routes, 'worker') else nullcontext():
                            path = server.construction_route(town, start, target)
                            results[index] = path
                timings.append((time.perf_counter() - before) * 1000)
                work.append(getattr(routes, 'work', 0))
                if len(results) == count:
                    break
            duration = (time.perf_counter() - started) * 1000
            valid = len(results) == count and all(path is not None for path in results.values())
            for index, path in results.items():
                if path is None:
                    continue
                blocked = {(r['x'], r['y']) for r in towns[index % 4]['map_resources']}
                valid = valid and all(tuple(p) not in blocked for p in path)
            bounded = not hasattr(routes, 'budget') or max(work) <= routes.budget
            name = f'server.routing.{count}.{state}'
            metrics[name + '.p95_ms'] = percentile(timings, .95)
            metrics[name + '.max_ms'] = max(timings)
            checks.append({'scenario': name, 'ok': valid and bounded})
            diagnostics.append({'scenario': name, 'updates': len(timings),
                                'all_routes_ms': duration, 'max_search_work': max(work),
                                'completed': len(results)})
    return diagnostics


def command_scenarios(server, cycles, metrics, checks):
    """Commands must acknowledge orders without finishing unrelated long routes."""
    from navigation import routes
    template = server.new_town('Command fixture')
    template.update(map_size=40, terrain=['g' * 40] * 40, level=5, wood=1000,
                    town_origin={'x': 5, 'y': 5}, npcs=[], roads=[], tree_stumps=[],
                    forage_version=1, forage_halls=[[5, 5]])
    template['buildings'] = [{'kind': 'hall', 'x': 5, 'y': 5},
        {'kind': 'barn', 'x': 22, 'y': 22, 'animals': {'pig': 2},
         'animal_growth': {'pig': [240, 240]}},
        {'kind': 'butcher', 'x': 30, 'y': 30},
        {'kind': 'home', 'x': 35, 'y': 2,
         'construction': {'progress': 0, 'total': 30}}]
    template['map_resources'] = [{'kind': 'rock', 'x': x, 'y': y}
                                for x, y in ((34, 2), (36, 2), (35, 1), (35, 3))]
    template['villagers'] = [{'name': f'Builder {i}', 'job': 'building',
        'construction_position': {'x': 2 + i % 4, 'y': 2 + i // 4}}
        for i in range(16)]
    template['villagers'][0].update(job='butchery', building_index=2, husbandry=2, talents={})
    for action in ('upgrade_capacity', 'butcher_animal'):
        timings = []
        valid = True
        for _ in range(cycles):
            town = copy.deepcopy(template)
            world = {'towns': {'owner': town}}
            routes.graphs.clear()
            data = {'action': action, 'index': 2}
            if action == 'butcher_animal':
                data.update(barn_index=1, animal='pig')
            started = time.perf_counter()
            server.command(town, data, world)
            timings.append((time.perf_counter() - started) * 1000)
            shop = town['buildings'][2]
            valid = valid and (shop.get('construction', {}).get('upgrade') == {'settler_level': 2}
                if action == 'upgrade_capacity' else shop.get('butchering', {}).get('stage') == 'fetching'
                and town['buildings'][1]['animals']['pig'] == 1)
            if hasattr(server, 'COMMAND_ROUTE_BUDGET'):
                valid = valid and routes.work <= server.COMMAND_ROUTE_BUDGET
        name = f'server.command.{action}'
        metrics[name + '.p95_ms'] = percentile(timings, .95)
        metrics[name + '.max_ms'] = max(timings)
        checks.append({'scenario': name, 'ok': valid})


def run(source, cycles):
    sys.path.insert(0, str(source))
    # Support both the current layout and revisions before the server move.
    if (source / 'server' / 'server.py').exists():
        sys.path.insert(0, str(source / 'server'))
    import server
    import work_cycles
    from shared_world import new_world
    # The current harness benchmarks baseline application code too.
    spec = importlib.util.spec_from_file_location('benchmark_harness', Path(__file__).parents[1] / 'server' / 'profile_server.py')
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    metrics, checks = {}, []
    routing = routing_scenarios(server, metrics, checks)
    command_scenarios(server, cycles, metrics, checks)
    # Exercise an active worker too: idle-town timings cannot detect discarded
    # simulation time at a delivery handoff. Use the same fixture for both refs.
    movement_samples, unused_tick = [], []
    for _ in range(max(30, cycles)):
        building = {'kind': 'woodcutter', 'x': 3, 'y': 3}
        source_tile = {'kind': 'tree', 'x': 12, 'y': 1}
        worker = {'job': 'woodcutting', 'work_animation': {
            'task': ['woodcutting', 'woodcutter', 3, 3, 'tree', 12, 1],
            'x': 3, 'y': 2, 'phase': 'depositing', 'elapsed': .6,
            'path': [], 'version': 1, 'facing': 1, 'waterReady': False}}
        town = {'map_size': 16, 'terrain': ['g' * 16] * 16,
                'buildings': [{'kind': 'hall', 'x': 5, 'y': 5}, building],
                'map_resources': [source_tile], 'villagers': [worker], 'npcs': []}
        started = time.perf_counter()
        delivered = work_cycles.advance(town, town, worker, building, source_tile, 0, 3)
        movement_samples.append((time.perf_counter() - started) * 1000)
        samples = worker['work_animation']['playback']
        travelled = sum(math.hypot(b['x'] - a['x'], b['y'] - a['y'])
                        for a, b in zip(samples, samples[1:]))
        unused_tick.append(max(0, (2.95 - travelled / 1.8) * 1000))
        checks.append({'scenario': 'worker_unloading', 'ok': delivered
                       and samples[-1]['time'] == 3 and travelled <= 2.95 * 1.8 + 1e-9})
    metrics['server.worker_unloading.p95_ms'] = percentile(movement_samples, .95)
    metrics['server.worker_unloading.unused_tick_ms'] = percentile(unused_tick, .95)
    for developed in (False, True):
        random.seed(42)
        label = 'developed' if developed else 'starting'
        result = harness.benchmark(cycles, developed, quiet=True)
        for operation, timings in result['timings'].items():
            for statistic in ('median_ms', 'p95_ms', 'max_ms'):
                metrics[f'server.{label}.{operation.replace(" ", "_")}.{statistic}'] = timings[statistic]

    random.seed(42)
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        world = new_world(42, explorable=True, chunk_directory=root / 'chunks')
        world['towns'] = {str(i): server.new_town(str(i), world) for i in range(4)}
        tokens = {f'fixture-{i}': str(i) for i in range(4)}
        entries = {'benchmark': {'name': 'Benchmark', 'world': world, 'tokens': tokens}}
        with patch.multiple(server, WORLD=world, TOKENS=tokens, SAVE=root / 'save.json', WORLDS=entries, ACTIVE_WORLD='benchmark'):
            server.prepare_landscapes(world)
            life = getattr(server, 'village_life', None)
            if life:
                for _ in range(200):
                    life.advance(world, .05)
            life_samples = []
            for _ in range(200):
                started = time.perf_counter()
                if life:
                    life.advance(world, .05)
                life_samples.append((time.perf_counter()-started)*1000)
            metrics['server.village_life.p95_ms'] = percentile(life_samples, .95)
            metrics['server.village_life.max_ms'] = max(life_samples)
            checks.append({'scenario': 'village_life', 'ok': not life or all(
                'routine' in npc for npc in world['shared_map']['npcs'] if npc.get('village'))})
            host = ThreadingHTTPServer(('127.0.0.1', 0), server.Handler)
            thread = threading.Thread(target=host.serve_forever, daemon=True)
            thread.start()
            samples = {'world': [], 'players': [], 'walk': [], 'tick_save': [], 'lock_wait': []}
            sample_lock = threading.Lock()

            def request(operation, client, cycle):
                connection = http.client.HTTPConnection('127.0.0.1', host.server_port, timeout=10)
                endpoint = {'world': '/api/world', 'players': '/api/players', 'walk': '/api/command'}[operation]
                body = json.dumps({'world_id': 'benchmark', 'action': 'walk', 'dx': 1 if cycle % 2 else -1, 'dy': 0}) if operation == 'walk' else None
                started = time.perf_counter()
                try:
                    connection.request('POST' if body else 'GET', endpoint, body,
                                       {'Content-Type': 'application/json', 'Authorization': f'fixture-{client}'})
                    response = connection.getresponse()
                    payload = response.read()
                    if response.status != 200:
                        raise RuntimeError(f'{operation}: HTTP {response.status}')
                    json.loads(payload)
                    with sample_lock:
                        samples[operation].append((time.perf_counter() - started) * 1000)
                finally:
                    connection.close()

            def tick_save():
                started = time.perf_counter()
                with server.LOCK:
                    waited = (time.perf_counter() - started) * 1000
                    server.tick(world)
                    server.save()
                with sample_lock:
                    samples['lock_wait'].append(waited)
                    samples['tick_save'].append((time.perf_counter() - started) * 1000)

            try:
                # Four independent owners contend with snapshots and saves.
                with ThreadPoolExecutor(max_workers=9) as clients:
                    for cycle in range(max(10, cycles)):
                        jobs = [clients.submit(request, op, i, cycle)
                                for i in range(4) for op in ('players', 'walk')]
                        jobs.append(clients.submit(request, 'world', 0, cycle))
                        if cycle % 3 == 0:
                            jobs.append(clients.submit(tick_save))
                        for job in jobs:
                            job.result(timeout=15)
                checks.append({'scenario': 'concurrent_http', 'ok': True})
            finally:
                host.shutdown()
                host.server_close()
                thread.join()
            for operation, values in samples.items():
                metrics[f'server.concurrent.{operation}.p95_ms'] = percentile(values, .95)
                metrics[f'server.concurrent.{operation}.max_ms'] = max(values)
    return {'schema': 1, 'metrics': metrics, 'checks': checks, 'errors': [], 'routing': routing,
            'environment': {'python': sys.version.split()[0], 'platform': sys.platform}}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--cycles', type=int, default=10)
    args = parser.parse_args()
    if args.cycles < 1:
        parser.error('--cycles must be positive')
    args.output.write_text(json.dumps(run(args.source.resolve(), args.cycles), indent=2))
