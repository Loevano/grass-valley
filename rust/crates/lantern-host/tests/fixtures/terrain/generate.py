"""Freeze real Python terrain for R07 read/loading tests (no live save access).

Run `python3 rust/crates/lantern-host/tests/fixtures/terrain/generate.py --check`
from the repository root. Without --check, explicitly refresh this fixture.
Generator changes require review; never refresh merely to make a test pass.
"""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import generate_chunk


def fixtures():
    cases = [(42, version, '3,4', 480) for version in range(1, 8)]
    # Real adjacent v7 records also fill and overflow the 49-entry decoded cache.
    cases += [(42, 7, f'{x},{y}', 480) for x in range(10) for y in range(5)
              if (x, y) != (3, 4)]
    cases += [('text-seed', 1, '47,47', 1920),
              (-123456789012345678901234567890, 3, '0,0', 1920)]
    result = []
    for seed, version, key, size in cases:
        record = generate_chunk(seed, key, version=version, size=size)
        digest = hashlib.sha256(json.dumps(record, sort_keys=True).encode()).hexdigest()
        result.append(dict(seed=seed, version=version, key=key, size=size,
                           sha256=digest, record=record))
    return (json.dumps(result, separators=(',', ':'), ensure_ascii=False) + '\n').encode()


if __name__ == '__main__':
    target = Path(__file__).with_name('chunks.json')
    expected = fixtures()
    if sys.argv[1:] == ['--check']:
        if target.read_bytes() != expected:
            raise SystemExit('Frozen Python terrain differs from real generation')
        print('58 real Python chunk golden records match, versions 1–7 and legacy seeds/dimensions')
    elif not sys.argv[1:]:
        target.write_bytes(expected)
    else:
        raise SystemExit('Usage: generate.py [--check]')
