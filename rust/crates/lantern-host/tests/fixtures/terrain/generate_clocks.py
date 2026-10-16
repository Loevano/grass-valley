#!/usr/bin/env python3
"""Independent Python clock arithmetic oracle; --check never refreshes goldens."""
import argparse
import json
from pathlib import Path
import struct


def build():
    pairs = [(str(a), str(b)) for a, b in [
        (2**53-1, 2**53-2), (2**53+1, 2**53), (2**53, 2**53+1),
        (2**64+1, 2**64), (10**400+7, 10**400), (10**400, 10**400),
        (10**400, 10**400+1), (2**53+1, 0), (2**54-1, 0),
        (10**308*3, 0), (10**309, 0),
    ]]
    pairs += [('9007199254740993','9007199254740992.0'),
              ('9007199254740992.0','9007199254740993'),
              ('9007199254740993.0','9007199254740992'),
              ('1.2500e2','125'), ('-0.0','-0'), ('-1e-999','0'),
              ('0.0000000000000000001','0'), ('1.7976931348623157e308','0'),
              (str(10**400),'0.0'), ('0.0',str(10**400)),
              (str(10**400),str(10**400)+'.0')]
    # Exercise every binary64 exponent and exact quotient tie neighbors. Python's
    # true division is the oracle; no Rust implementation logic is reused here.
    for exponent in range(1, 1026):
        for offset in (-1, 0, 1):
            pairs.append((str(2**exponent+offset), '0'))
    cases=[]
    def result(operation):
        try:
            value=operation()
            if isinstance(value,float):
                return dict(bits=struct.unpack('>Q',struct.pack('>d',value))[0])
            return dict(integer=str(value))
        except OverflowError:
            return dict(error='overflow')
    for now,previous in pairs:
        # Nonfinite supplied float scalars are rejected by the import boundary;
        # they are intentionally absent from the supported arithmetic vectors.
        a,b=json.loads(now),json.loads(previous)
        if isinstance(a,float) and not __import__('math').isfinite(a):continue
        if isinstance(b,float) and not __import__('math').isfinite(b):continue
        def elapsed():return max(0,a-b)
        cases.append(dict(now=now,previous=previous,equal=a==b,
                          elapsed=result(elapsed),thirds=result(lambda:elapsed()/3),
                          binary64=result(lambda:float(elapsed()))))
    return cases


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    target=Path(__file__).with_name('clocks.json');result=build()
    if args.check:
        assert json.loads(target.read_text())==result,'Python arithmetic differs from frozen clocks'
        print(f'Checked {len(result)} actual Python clock arithmetic cases.')
    else:
        target.write_text(json.dumps(result,separators=(',',':'))+'\n')
        print(f'Wrote {len(result)} actual Python clock arithmetic cases.')
if __name__=='__main__':main()
