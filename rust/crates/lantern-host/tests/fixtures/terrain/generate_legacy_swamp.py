#!/usr/bin/env python3
"""Freeze actual swamp-v1 materialization on real terrain; --check never refreshes."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import generate_chunk
from villages import _apply_villages
import swamp_village


def build():
    cases = []

    def add(villages, key, previous=0, size=480, version=7, seed=42):
        record = generate_chunk(seed, key, version=version, size=size)
        _apply_villages(record, key, villages, 40)
        if previous:
            swamp_village.apply(record, key, villages, 40)
            record['swamp_version'] = previous
        cx, cy = map(int, key.split(','))
        # Saved player edits include the retained center, cleared outskirts,
        # named village items and foreign items; both resources and stumps.
        for dx, dy, owner in ((0, 0, None), (13, 13, 'Reedhaven'),
                               (24, 24, 'foreign'), (39, 39, None)):
            item = dict(kind='berry_bush', x=cx*40+dx, y=cy*40+dy,
                        supply=.125, supply_max=12.5, regeneration=.375,
                        extension={'keep': ['private', 17]})
            if owner:
                item['village'] = owner
            record['map_resources'].append(item)
            record['tree_stumps'].append(dict(item, remaining=9.125))
        record['extension'] = {'keep': True}
        record['clock'] = 12.125
        before = deepcopy(record)
        swamp_village.apply(record, key, villages, 40)
        cases.append(dict(villages=villages, key=key, size=size, seed=seed,
                          generator=version, before=before, after=record))

    village = dict(name='Reedhaven', biome='b', x=80, y=80,
                   layout_version=2, life_version=1, swamp_version=1)
    for previous in (0, 1):
        for cy in range(1, 4):
            for cx in range(1, 4):
                add([village], f'{cx},{cy}', previous)
    for version in range(1, 8):
        add([dict(village, x=96, y=96)], '2,2', version=version,
            seed='00042' if version % 2 else 42)
    for size, x, y, key in ((480, 0, 0, '0,0'), (480, 479, 479, '11,11'),
                             (1920, 880, 920, '22,23'), (1920, 1919, 1919, '47,47')):
        add([dict(village, x=x, y=y)], key, size=size)
    for layout in (None, 1, 2):
        for swamp in (None, 0, 1):
            meta = deepcopy(village)
            meta.pop('layout_version')
            meta.pop('swamp_version')
            if layout is not None:
                meta['layout_version'] = layout
            if swamp is not None:
                meta['swamp_version'] = swamp
            add([meta], '2,2')
    for reverse in (False, True):
        villages = [dict(village, x=90, y=90), dict(village, x=108, y=104)]
        if reverse:
            villages.reverse()
        add(villages, '2,2')
    add([dict(village, x=300, y=300), village], '2,2')
    add([village, dict(village, x=300, y=300)], '2,2')
    add([], '2,2')
    return cases


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = Path(__file__).with_name('legacy_swamp.json')
    cases = build()
    body = json.dumps(cases, ensure_ascii=False, separators=(',', ':')) + '\n'
    if args.check:
        if target.read_text() != body:
            raise SystemExit('Python legacy swamp fixtures differ; review before refreshing')
        print(f'{len(cases)} real Python legacy swamp cases match')
    else:
        target.write_text(body)


if __name__ == '__main__':
    main()
