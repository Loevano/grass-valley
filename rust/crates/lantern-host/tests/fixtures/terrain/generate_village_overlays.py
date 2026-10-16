#!/usr/bin/env python3
"""Freeze actual ordinary village phase on real terrain; --check never refreshes."""
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

    def add(villages, previous, key, generator=7, size=480, seed=42, saved_swamp=False):
        record = generate_chunk(seed, key, version=generator, size=size)
        if previous:
            # Genuine earlier overlays followed by player edits.
            old = deepcopy(villages)
            for village in old:
                if previous == 1:
                    village.pop('layout_version', None)
                if previous < 3:
                    village.pop('life_version', None)
            _apply_villages(record, key, old, 40)
            record['village_version'] = previous
        if saved_swamp:
            swamp_village.apply(record, key, villages, 40)
        cx, cy = map(int, key.split(','))
        for dx, dy in ((0, 0), (13, 13), (35, 36)):
            x, y = cx * 40 + dx, cy * 40 + dy
            record['map_resources'].append(dict(kind='berry_bush', x=x, y=y,
                supply=.125, supply_max=12.5, regeneration=.375, extension={'private': ['keep', 17]}))
            record['tree_stumps'].append(dict(x=x, y=y, remaining=9.125, extension='stump'))
        record['extension'] = {'ordered': ['keep', 17]}
        record['clock'] = 12.125
        before = deepcopy(record)
        _apply_villages(record, key, villages, 40)
        cases.append(dict(villages=villages, previous=previous, key=key,
                          size=size, generator=generator, seed=seed,
                          before=before, after=record))

    themes = (('Reedhaven', 'b'), ('Sunwell', 'd'), ('Frostbrook', 'i'))
    for ti, (name, biome) in enumerate(themes):
        for previous in range(4):
            for layout in (None, 1, 2):
                for life in (None, 0, 1):
                    x, y = ((96, 96), (80, 80), (119, 119))[(previous + (layout or 0) + (life or 0)) % 3]
                    village = dict(name=name, biome=biome, x=x, y=y)
                    if layout is not None:
                        village['layout_version'] = layout
                    if life is not None:
                        village['life_version'] = life
                    add([village], previous, '2,2', generator=1+(previous+ti)%7,
                        seed='00042' if life == 0 else 42)
    for name, biome in themes:
        village = dict(name=name, biome=biome, x=80, y=80, layout_version=2, life_version=1)
        for cy in range(1, 4):
            for cx in range(1, 4):
                add([village], 0, f'{cx},{cy}')
    villages = [dict(name=n, biome=b, x=90+i*5, y=92+i*3,
                     layout_version=2, life_version=1) for i, (n,b) in enumerate(themes)]
    for previous in range(4):
        add(villages, previous, '2,2')
    # Python keeps a pending terrain buffer during version-1 expansion; life
    # upgrades on an adjacent nonexpanded village can have different ordering.
    for reverse in (False, True):
        mixed = deepcopy(villages)
        mixed[1]['layout_version'] = 1
        if reverse:
            mixed.reverse()
        add(mixed, 1, '2,2')
    for size, x, y, key in ((480, 0, 0, '0,0'), (480, 479, 479, '11,11'),
                             (1920, 880, 920, '22,23'), (1920, 1919, 1919, '47,47')):
        add([dict(name='Sunwell', biome='d', x=x, y=y, layout_version=2,
                  life_version=1)], 0, key, size=size)
    # Already organic saved terrain contains p/w/c road/courtyard codes. The
    # ordinary phase must preserve it, even though it does not apply swamp edits.
    for key in ('1,1', '2,1', '1,2', '2,2'):
        add([dict(name='Reedhaven', biome='b', x=80, y=80, layout_version=2,
                  life_version=1, swamp_version=2)], 3, key, saved_swamp=True)
    add([], 0, '2,2')
    return cases


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = Path(__file__).with_name('village_overlays.json')
    cases = build()
    body = json.dumps(cases, ensure_ascii=False, separators=(',', ':')) + '\n'
    if args.check:
        if target.read_text() != body:
            raise SystemExit('Python ordinary village fixtures differ; review before refreshing')
        print(f'{len(cases)} real Python ordinary village overlay cases match')
    else:
        target.write_text(body)


if __name__ == '__main__':
    main()
