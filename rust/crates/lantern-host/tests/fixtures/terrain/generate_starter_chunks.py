#!/usr/bin/env python3
"""Freeze actual ChunkMap._read reconstruction; --check never refreshes."""
import argparse
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import ChunkMap


def build():
    cases = []
    for index, seed in enumerate((42, -(2**130 + 17), '42', '00042', 'É🌲', 0)):
        for version in range(1, 8):
            for size, offset in ((480, 160), (1920, None)):
                landscape = dict(chunked=1, chunk_size=40, terrain_seed=seed,
                                 terrain_version=version, map_size=size, chunk_clock=99999)
                if offset is not None:
                    landscape['starter_offset'] = offset
                manager = ChunkMap(dict(shared_map=landscape, towns={}))
                # Rotate through all nine starter chunks, including each edge.
                local = (index * 7 + version - 1) % 9
                start = (880 if offset is None else offset) // 40
                key = f'{start + local % 3},{start + local // 3}'
                cases.append(dict(landscape=landscape, key=key, record=manager._read(key)))
    # Absent offset remains 880 even for a current-size world; it must not
    # accidentally reconstruct the modern 160 starter. Missing version is v1.
    for version in (None, 7):
        landscape = dict(chunked=1, chunk_size=40, terrain_seed=42, map_size=480)
        if version is not None:
            landscape['terrain_version'] = version
        manager = ChunkMap(dict(shared_map=landscape, towns={}))
        cases.append(dict(landscape=landscape, key='4,4', record=manager._read('4,4')))
    return cases


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    target = Path(__file__).with_name('starter_chunks.json')
    body = json.dumps(build(), ensure_ascii=False, separators=(',', ':')) + '\n'
    if args.check:
        if target.read_text() != body:
            raise SystemExit('Python starter chunk fixtures differ; review compatibility before refreshing')
        print('86 real Python raw starter-selection chunks match')
    else:
        target.write_text(body)


if __name__ == '__main__':
    main()
