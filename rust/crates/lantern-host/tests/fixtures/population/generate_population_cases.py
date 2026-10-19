"""Freeze Python post-production population rules without terrain/work fixtures."""
import argparse
import copy
import json
from pathlib import Path
import sys
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import server as s
import progression as p


def cases():
    result = []
    for name, count, food, homes, history, sandbox, roll in [
        ('no_room', 5, 100, [], {}, False, 9999),
        ('meal_before_arrival', 5, 20, [dict(kind='home')], {}, False, 9999),
        ('arrival', 5, 21, [dict(kind='home')], {}, False, 9999),
        ('grace', 5, 0, [], dict(food_shortage_seconds=54), False, 0),
        ('departure', 5, 0, [], dict(food_shortage_seconds=57), False, 0),
        ('loyalty', 5, 0, [dict(kind='home', level=3)], dict(food_shortage_seconds=57), False, 400),
        ('unprotected', 5, 0, [dict(kind='home')], dict(food_shortage_seconds=57), False, 400),
        ('housing', 6, 100, [], dict(housing_shortage_seconds=57), False, 0),
        ('both', 6, 0, [], dict(food_shortage_seconds=57, housing_shortage_seconds=57), False, 0),
        ('last', 1, 0, [], dict(food_shortage_seconds=60), False, 0),
        ('replacement_wait', 4, 100, [], dict(arrival_delay_seconds=6), False, 0),
        ('replacement_due', 4, 100, [], dict(arrival_delay_seconds=3), False, 0),
        ('recovery', 5, .15, [], dict(food_shortage_seconds=57), False, 0),
        ('sandbox', 5, 0, [dict(kind='home', settler_level=3)], dict(food_shortage_seconds=57), True, 0),
        ('names_beyond_pool', 33, 100, [dict(kind='home', level=3) for _ in range(6)], {}, False, 0),
        ('autofill', 5, 0, [dict(kind='home'), dict(kind='home', settler_level=2)], {}, False, 9999),
        ('new_construction', 5, 100, [dict(kind='home', construction=dict(progress=0, total=30))], {}, False, 9999),
        ('upgrade_preserves', 9, 100, [dict(kind='home',level=2,construction=dict(progress=0,total=60,upgrade=dict(level=3)))],dict(housing_shortage_seconds=57),False,0),
    ]:
        t = dict(villagers=[dict(name=s.VILLAGER_NAMES[i % 32] + (f' {i // 32 + 1}' if i >= 32 else ''), job='idle') for i in range(count)],
                 buildings=[dict(kind='hall',x=1,y=1)] + homes, food=food, sandbox=sandbox, player={}, **history)
        for i,v in enumerate(t['villagers']): p.ensure_talents(v,i)
        before = copy.deepcopy(t)
        with patch.object(s.secrets,'randbelow',side_effect=lambda n: roll if n == 10000 else 0):
            s.feed_settlers(t)
            if not sandbox and len(t['villagers']) < s.housing_capacity(t) and t['food'] >= 20 and not t.get('arrival_delay_seconds'):
                p.eat(t,10)
                t['villagers'].append(dict(name=s.next_villager_name(t),job='idle',woodcutting=0,farming=0,fishing=0))
                p.ensure_talents(t['villagers'][-1],len(t['villagers'])-1)
            s.fill_cottages(t)
        result.append(dict(name=name,before=before,roll=roll,after={k: value for k,value in t.items() if k not in ('buildings','player','resource_activity')},housing=s.housing_capacity(t),loyalty=s.housing_loyalty(t)))
    return result

if __name__ == '__main__':
    parser=argparse.ArgumentParser()
    parser.add_argument('--check',action='store_true')
    args=parser.parse_args()
    ranges=[]
    for n in range(0x110000):
        if chr(n).isdigit():
            if ranges and n==ranges[-1][1]+1: ranges[-1][1]=n
            else: ranges.append([n,n])
    digits=ROOT / 'rust/crates/lantern-host/src/population/name_digits.json'
    expected=json.dumps(ranges,separators=(',',':'))+'\n'
    if digits.read_text()!=expected: raise SystemExit('Python name digit ranges drift')
    path=Path(__file__).with_name('python.json')
    value=json.dumps(cases(),indent=2,sort_keys=True)+'\n'
    if args.check:
        if path.read_text()!=value: raise SystemExit('population fixture drift')
        print('population fixture matches Python')
    else: path.write_text(value)
