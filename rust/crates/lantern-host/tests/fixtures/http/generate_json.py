"""Independent Python json.dumps / BLAKE2b-64 wire-version oracle.

Synthetic values only. Run with --check to detect drift without rewriting data.
"""
import hashlib
import json
import math
from pathlib import Path
import random
import struct
import sys


def generate():
    values = ['-0', '0', '-0.0', '0.0', '1.0', '1e16', '1e-7', '1E+020',
              str(10**250), str(-(10**250))]
    values += [repr(v) for v in [float.fromhex('0x0.0000000000001p-1022'),
        1e-4, 1e-5, 1e15, 1e16, 1e23, 1.2345678901234567,
        2.2250738585072014e-308, 1.7976931348623157e308]]
    rng = random.Random(20)
    for _ in range(300):
        value = struct.unpack('!d', rng.randbytes(8))[0]
        if math.isfinite(value):
            values.append(repr(value))
    inputs = ['{"z":' + value + ',"a":"Café 🌿\\t\\u0001/\\u007f"}' for value in values]
    inputs += ['{"map_resources":[{"label":"Reedworker’s shed","x":1,"y":2,"kind":"village_decor"}],"tree_stumps":[]}',
               '{"z":{"🐦":true,"leaf":null},"a":[false,"\\n",1.00]}']
    cases = []
    for text in inputs:
        canonical = json.dumps(json.loads(text), separators=(',', ':'))
        cases.append({'input': text, 'canonical': canonical,
                      'version': hashlib.blake2b(canonical.encode(), digest_size=8).hexdigest()})
    return json.dumps(cases, ensure_ascii=True, indent=2) + '\n'


if __name__ == '__main__':
    path = Path(__file__).with_name('python_json.json')
    text = generate()
    if '--check' in sys.argv:
        if path.read_text() != text:
            raise SystemExit('HTTP JSON oracle drift; review before regenerating.')
    else:
        path.write_text(text)
