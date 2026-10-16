#!/usr/bin/env python3
"""Freeze real Python swamp apply v0–2; --check independently verifies, never refreshes."""
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
    village = dict(name='Reedhaven', biome='b', x=80, y=80,
                   layout_version=2, life_version=1, swamp_version=2)

    def add(villages, key, previous=None, size=480, version=7, seed=42):
        record = generate_chunk(seed, key, version=version, size=size)
        _apply_villages(record, key, villages, 40)
        if previous in (1, 2):
            # Genuine old/new saved geometry, rather than a fabricated marker.
            old = [dict(v, swamp_version=previous) for v in villages]
            swamp_village.apply(record, key, old, 40)
        if previous is not None:
            record['swamp_version'] = previous
        cx, cy = map(int, key.split(','))
        for dx, dy, owner in ((0, 0, None), (1, 1, 'Reedhaven'),
                               (13, 13, 'Reedhaven'), (24, 24, 'foreign'),
                               (39, 39, None)):
            item = dict(kind='berry_bush', x=cx*40+dx, y=cy*40+dy,
                        supply=.125, supply_max=12.5, regeneration=.375,
                        extension={'keep': ['private', 17]})
            if owner:
                item['village'] = owner
            record['map_resources'].append(item)
            record['tree_stumps'].append(dict(item, remaining=9.125))
        # Mark saved terrain edits too, so touched/core/no-touch behavior is visible.
        row = list(record['terrain'][0])
        row[0] = 'i'
        record['terrain'][0] = ''.join(row)
        record.update(extension={'keep': True}, clock=12.125)
        before = deepcopy(record)
        swamp_village.apply(record, key, villages, 40)
        again = deepcopy(record)
        swamp_village.apply(again, key, villages, 40)
        cases.append(dict(villages=villages, key=key, size=size, seed=seed,
                          generator=version, before=before, after=record, again=again if again != record else None))

    for previous in (None, 0, 1, 2):
        for cy in range(1, 4):
            for cx in range(1, 4):
                add([village], f'{cx},{cy}', previous)
    for version in range(1, 8):
        add([dict(village, x=96, y=96)], '2,2', previous=1,
            version=version, seed='00042' if version % 2 else 42)
    for size, x, y, key in ((480, 0, 0, '0,0'), (480, 479, 479, '11,11'),
                             (1920, 880, 920, '22,23'), (1920, 1919, 1919, '47,47')):
        for previous in (None, 1):
            add([dict(village, x=x, y=y)], key, previous, size=size)
    for layout in (None, 1, 2):
        for swamp in (None, 0, 1, 2):
            meta = deepcopy(village)
            meta.pop('layout_version')
            meta.pop('swamp_version')
            if layout is not None:
                meta['layout_version'] = layout
            if swamp is not None:
                meta['swamp_version'] = swamp
            add([meta], '2,2')
    # First eligible legacy metadata returns even outside the chunk; original
    # record version remains captured across organic edits and overlapping towns.
    near = dict(village, x=108, y=104)
    far = dict(village, x=300, y=300)
    for first in (village, near, far):
        for second in (near, far):
            for versions in ((1, 2), (2, 1), (2, 2)):
                for previous in (0, 1):
                    add([dict(first, swamp_version=versions[0]),
                         dict(second, swamp_version=versions[1])], '2,2', previous)
    add([], '2,2')
    return cases


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    data = swamp_village.layout()
    geometry = dict(roads=[[list(p), code] for p, code in data['roads'].items()],
                    decor=data['decor'], footprint=sorted(data['footprint']),
                    water=sorted(data['water']))
    cases = build()
    for filename, value in (('organic_swamp.json', cases),
                            ('organic_swamp_layout.json', geometry)):
        target = Path(__file__).with_name(filename)
        body = json.dumps(value, ensure_ascii=False, separators=(',', ':')) + '\n'
        if args.check:
            if target.read_text() != body:
                raise SystemExit(f'Python {filename} differs; review before refreshing')
        else:
            target.write_text(body)
    print(f'{len(cases)} real Python versioned swamp cases and exact layout match')


if __name__ == '__main__':
    main()
