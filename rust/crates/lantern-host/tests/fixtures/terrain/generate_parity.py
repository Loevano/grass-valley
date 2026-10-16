"""Independent raw generator and CPython RNG vectors. No saves are read.

Use --check to verify frozen vectors. Explicit refresh needs compatibility review.
"""
import hashlib
import json
from pathlib import Path
import random
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import generate_chunk


def fixtures():
    # Ice/pools, swamp, farmland, rivers/seams, lake/islands, desert/oases,
    # world corners. v6/v7 raw generation intentionally remains identical.
    keys = ['0,0', '2,0', '3,1', '1,5', '2,5', '5,5', '6,6',
            '6,9', '7,9', '9,4', '10,4', '11,11']
    cases = [(42, version, key, 480) for version in range(1, 8) for key in keys]
    for seed in ['世界:é', '0007', -123456789012345678901234567890, 18446744073709551615]:
        cases += [(seed, version, key, 1920) for version in range(1, 8)
                  for key in ['0,0', '47,47', '40,17']]
    chunks = []
    for seed, version, key, size in cases:
        record = generate_chunk(seed, key, version=version, size=size)
        chunks.append(dict(seed=seed, namespace=str(seed), version=version, key=key, size=size,
                           sha256=hashlib.sha256(json.dumps(record, sort_keys=True).encode()).hexdigest(),
                           record=record))
    rngs = []
    for seed in [0, 42, -42, 2**160+123, '0007', '世界:é']:
        rng = random.Random(seed)
        draws = [rng.random() for _ in range(630)]  # exercises a complete twist
        bits = [rng.getrandbits(n) for n in [0, 1, 5, 32, 33, 64]*4]
        below = [rng.randrange(n) for n in [1, 2, 3, 17, 2**40+1]*8]
        shuffled = list(range(20))
        rng.shuffle(shuffled)
        choices = [rng.choice(['a', 'b', 'c']) for _ in range(20)]
        rngs.append(dict(seed=seed, integer=isinstance(seed, int), namespace=str(seed), draws=draws, bits=bits, below=below,
                         shuffled=shuffled, choices=choices))
    return (json.dumps(dict(chunks=chunks, rngs=rngs), separators=(',', ':'), ensure_ascii=False)+'\n').encode()


if __name__ == '__main__':
    target = Path(__file__).with_name('parity.json')
    expected = fixtures()
    if sys.argv[1:] == ['--check']:
        if target.read_bytes() != expected:
            raise SystemExit('Frozen generator/RNG vectors differ from real Python')
        print('168 raw chunks and 6 CPython RNG vectors match real Python')
    elif not sys.argv[1:]:
        target.write_bytes(expected)
    else:
        raise SystemExit('Usage: generate_parity.py [--check]')
