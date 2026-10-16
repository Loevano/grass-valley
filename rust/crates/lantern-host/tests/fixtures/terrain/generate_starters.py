#!/usr/bin/env python3
"""Freeze actual shared_world.new_world starter geometry; --check never refreshes."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from shared_world import new_world


def build():
    cases = []
    for seed in (0, 42, -(2**130 + 17), '42', '00042', '', 'É🌲/seed:'):
        for version in range(1, 8):
            world = new_world(seed, terrain_version=version)
            landscape = world['shared_map']
            cases.append(dict(seed=str(seed), seed_kind='integer' if isinstance(seed, int) else 'text',
                              version=version, sites=world['town_sites'],
                              terrain=landscape['terrain'], map_resources=landscape['map_resources']))
    return cases


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = Path(__file__).with_name('starters.json')
    body = json.dumps(build(), ensure_ascii=False, separators=(',', ':')) + '\n'
    if args.check:
        if target.read_text() != body:
            raise SystemExit('Python starter fixtures differ; review compatibility before refreshing')
        print('49 real Python authored starters match')
    else:
        target.write_text(body)


if __name__ == '__main__':
    main()
