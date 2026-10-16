#!/usr/bin/env python3
"""Freeze real Python map/ensure_terrain vectors; --check never refreshes goldens."""
import argparse
import copy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from terrain import ensure_terrain, generate_map


def build():
    maps = []
    for seed in (0, 42, -(2**130 + 17), '42', '', '00042', 'É🌲/seed:'):
        for size in (1, 12, 13, 40, 120):
            terrain, resources = generate_map(size, seed)
            maps.append(dict(seed=str(seed), seed_kind='integer' if isinstance(seed, int) else 'text',
                             size=size, terrain=terrain, map_resources=resources))
    hydration = []
    for size in (12, 120):
        rows, resources = generate_map(size, 42)
        for mode in ('saved', 'absent', 'wrong_height'):
            source = dict(map_size=size, terrain_seed=42, terrain_version=7,
                          map_resources=[dict(resources[0], supply=0.125, supply_max=3.5,
                                              regeneration=0.0125, extension={'kept': 'private'})],
                          tree_stumps=[dict(kind='tree', x=2, y=2, remaining=0.0625,
                                            species='ancient', extension='kept')])
            if mode != 'absent':
                source['terrain'] = rows if mode == 'saved' else rows[:1]
            expected = copy.deepcopy(source)
            ensure_terrain(expected)
            hydration.append(dict(mode=mode, source=source, expected=expected))
    return dict(maps=maps, hydration=hydration)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = Path(__file__).with_name('maps.json')
    body = json.dumps(build(), ensure_ascii=False, separators=(',', ':')) + '\n'
    if args.check:
        if target.read_text() != body:
            raise SystemExit('Python map fixtures differ; review generator compatibility before refreshing')
        print('35 real Python maps and 6 terrain hydration vectors match')
    else:
        target.write_text(body)


if __name__ == '__main__':
    main()
