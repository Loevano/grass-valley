"""Benchmark any source revision with the same harness and disposable saves."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import http.client
from http.server import ThreadingHTTPServer
import importlib.util
import json
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


def run(source, cycles):
    sys.path.insert(0, str(source))
    # Support both the current layout and revisions before the server move.
    if (source / 'server' / 'server.py').exists():
        sys.path.insert(0, str(source / 'server'))
    import server
    from shared_world import new_world
    # The current harness benchmarks baseline application code too.
    spec = importlib.util.spec_from_file_location('benchmark_harness', Path(__file__).parents[1] / 'server' / 'profile_server.py')
    harness = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(harness)
    metrics, checks = {}, []
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
    return {'schema': 1, 'metrics': metrics, 'checks': checks, 'errors': [],
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
