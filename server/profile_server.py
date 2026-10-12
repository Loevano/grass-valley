"""Profile a synthetic four-player workload without touching the host's save."""
import argparse
import cProfile
import copy
import json
from pathlib import Path
import pstats
import statistics
import tempfile
import time
from unittest.mock import patch

import server
from shared_world import export_world, new_world


def benchmark(ticks, developed=False, quiet=False):
    samples = {name: [] for name in ('tick', 'save', 'world poll', 'walk')}

    def measure(name, operation):
        started = time.perf_counter()
        result = operation()
        samples[name].append((time.perf_counter() - started) * 1000)
        return result

    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        world = new_world(42, explorable=True, chunk_directory=root / 'chunks')
        for index in range(4):
            world['towns'][str(index)] = server.new_town(str(index), world)
            if developed:
                town = world['towns'][str(index)]
                prototype = town['villagers'][0]
                town['villagers'] = [dict(copy.deepcopy(prototype), name=f'Worker {index}/{i}', job='idle')
                                     for i in range(50)]
        entries = {'benchmark': {'name': 'Benchmark', 'world': world, 'tokens': {}}}
        with patch.multiple(server, SAVE=root / 'save.json', WORLD=world,
                            WORLDS=entries, ACTIVE_WORLD='benchmark'):
            server.prepare_landscapes(world)
            server.save()
            known, versions = set(), {}

            def poll():
                nonlocal known, versions
                server.prepare_landscapes(world)
                for town in world['towns'].values():
                    server.ensure_player(server.walking_map(world, town))
                landscape = world['_chunks'].snapshot(known=known, versions=versions)
                known.update(landscape['visible_chunks'])
                versions.update({k: v['version'] for k, v in landscape['resource_chunks'].items()})
                return len(json.dumps(dict(export_world(world), shared_map=landscape),
                                      separators=(',', ':')).encode())

            poll()  # Warm the terrain and resource version caches.
            for cycle in range(ticks):
                measure('tick', lambda: server.tick(world))
                measure('save', server.save)
                for _ in range(4):
                    payload_bytes = measure('world poll', poll)
                for _ in range(20):
                    for town in world['towns'].values():
                        measure('walk', lambda: server.command(
                            town, {'action': 'walk', 'dx': 1 if cycle % 2 else -1, 'dy': 0}, world))

    result = {}
    for name, values in samples.items():
        ordered = sorted(values)
        result[name] = {'median_ms': statistics.median(values),
                        'p95_ms': ordered[min(len(ordered) - 1, int(len(ordered) * .95))],
                        'max_ms': ordered[-1], 'samples': len(values)}
        if not quiet:
            print(f'{name:12} median {result[name]["median_ms"]:8.3f} ms '
                  f'p95 {result[name]["p95_ms"]:8.3f} ms')
    if not quiet:
        print(f'Last cached world payload: {payload_bytes} bytes')
    return {'timings': result, 'payload_bytes': payload_bytes}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ticks', type=int, default=20)
    parser.add_argument('--profile', action='store_true', help='Print the 25 most expensive functions')
    parser.add_argument('--developed', action='store_true', help='Use four towns with 50 workers each')
    parser.add_argument('--json', action='store_true', help='Emit machine-readable benchmark results')
    args = parser.parse_args()
    if args.ticks < 1:
        parser.error('--ticks must be positive')
    if args.profile:
        profiler = cProfile.Profile()
        profiler.runcall(benchmark, args.ticks, args.developed)
        pstats.Stats(profiler).strip_dirs().sort_stats('cumulative').print_stats(25)
    else:
        result = benchmark(args.ticks, args.developed, args.json)
        if args.json:
            print(json.dumps(result))
