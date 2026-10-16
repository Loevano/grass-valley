#!/usr/bin/env python3
"""Independent actual Python ChunkMap.catch_up oracle; --check never refreshes."""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
from chunk_world import ChunkMap, generate_chunk
from shared_world import new_world
from spatial import occupied_tiles


def build():
    cases = []
    def add(label, record, version, clock=0, key='0,0', size=480, obstacles=(), previous=True):
        original = deepcopy(record)
        if not previous: original.pop('clock', None)
        world = dict(towns={'synthetic': dict(buildings=list(obstacles), villagers=[],
                                             wood=.125, cargo={'iron': .0625})},
                     shared_map=dict(map_size=size, terrain_version=version, chunk_clock=clock))
        before = deepcopy(world)
        manager = ChunkMap(world)
        expected = deepcopy(original)
        manager.catch_up(key, expected)
        assert world == before, 'catch_up must not mutate production/world'
        again = deepcopy(expected)
        manager.catch_up(key, again)
        cases.append(dict(label=label, key=key, size=size, version=version, clock=clock,
                          occupied=[list(p) for p in sorted(occupied_tiles(obstacles))], source=original,
                          expected=expected, repeat=again))

    for version in range(1, 8):
        for size, seed, key in [(480, '00042', '2,5'), (1920, -(2**130+17), '22,22')]:
            record = generate_chunk(seed, key, version=version, size=size)
            record['private_extension'] = {'quantity': 123456789012345678901234567890, 'secret': 'SYNTHETIC_SECRET'}
            # Force depleted resources in actual generated worlds.
            for r in record['map_resources'][::3]: r['supply'] = .125
            add(f'generated-{version}-{size}', record, version, 12345.125, key, size)
        # Real authored starter, including chest resources and saved markers.
        base = new_world(42, terrain_version=version)['shared_map']
        record = ChunkMap(dict(towns={}, shared_map=dict(map_size=480, terrain_seed=42,
                            terrain_version=version, starter_offset=160)), base=base)._read('5,5')
        add(f'starter-{version}', record, version, 900, '5,5')

    # Isolated rule fixtures deliberately use small deterministic records.
    def fixture():
        return dict(terrain=['gfmbsrdivpwc'+'g'*28]+['g'*40]*39,
                    map_resources=[], tree_stumps=[], clock=3,
                    extension={'precise': 123456789012345678901234567890})
    for version in range(1,8):
        record=fixture()
        for x in range(12):
            record['map_resources'].append(dict(kind='tree',x=x,y=0,supply=80,species='ancient' if x==1 else 'oak'))
            record['tree_stumps'].append(dict(kind='tree',x=x,y=0,remaining=10.1234567895,extension='.125'))
        for x, kind in enumerate(['water','berry_bush','herb_patch','rock','chest','village_decor']):
            record['map_resources'].append(dict(kind=kind,x=x,y=1,supply=.125,extension={'retained':True}))
        add(f'rules-{version}',record,version,100)
        add(f'future-clock-{version}',record,version,.125)
    for industry in [None,0,1,2]:
        for mineral in [None,0,1]:
            record=fixture()
            if industry is not None: record['industry_version']=industry
            if mineral is not None: record['mineral_version']=mineral
            record['map_resources']=[dict(kind='wild_horse',x=x,y=2) for x in range(10)]
            record['map_resources'] += [dict(kind='rock',x=x,y=3,ore='saved') for x in range(10)]
            add(f'industry-{industry}-{mineral}',record,7,0,obstacles=[dict(x=0,y=0,footprint=[3,4])],previous=False)
    # Ensure obstacle expansion blocks actual newly selected horses, not just
    # unrelated points; add a stump on another spawn point before tree filtering.
    record=dict(terrain=['g'*40]*40,map_resources=[],tree_stumps=[])
    points=[(x,y) for y in range(200,240) for x in range(80,120)
            if (x*73+y*137+x*y*11)%499==71 and (x*31+y*53)%10==0]
    assert len(points)>=2
    x,y=points[1]
    record['tree_stumps']=[dict(kind='tree',x=x,y=y,remaining=4)]
    add('occupied-horse-spawns',record,7,3,key='2,5',obstacles=[dict(x=points[0][0],y=points[0][1],footprint=[2,2])])
    for remaining in [2.7,3.0000000005,3.0000000015,10.1234567895,1000000.0000000005,
                      3.141592653589793, 9007199254740991, .0000000005]:
        record=fixture();record['terrain']=['f'*40]*40
        record['tree_stumps']=[dict(kind='tree',x=0,y=0,remaining=remaining)]
        add(f'round9-{remaining}',record,7,3.125)
        add(f'round9-zero-elapsed-{remaining}',record,7,3)
    add('largest-exact-integer-clock',fixture(),7,2**53)
    # Exact clocks retain Python's JSON int/float distinction. Close absolute
    # integers may exceed binary64's range without making elapsed time overflow.
    pairs=[(2**53+1,2**53),(2**53,2**53+1),(2**64+1,2**64),
           (10**400+7,10**400),(10**400,10**400),(10**400,10**400+1),
           (2**53+1,float(2**53)),(float(2**53),2**53+1),
           (float(2**53+1),2**53),(125.0,125),(0.0,0),
           (2**53+1,0),(10**308*3,0)]
    for i,(clock,previous) in enumerate(pairs):
        record=fixture();record['clock']=previous
        record['terrain']=['f'*40]*40
        record['map_resources']=[dict(kind='rock',x=0,y=0,supply=.125)]
        # A large int/3 can succeed even when int*float overflows: keep this
        # supplies-only case distinct from the failing stump conversion below.
        if i!=12:record['tree_stumps']=[dict(kind='tree',x=1,y=0,remaining=12.125)]
        add(f'exact-clock-{i}',record,7,clock)
    for label,clock,previous,stumps,resources in [
        ('mixed-huge-now',10**400,0.0,False,False),
        ('mixed-huge-previous',0.0,10**400,False,False),
        ('integer-division-overflow',10**309,0,False,True),
        ('integer-stump-conversion-overflow',10**308*3,0,True,False),
    ]:
        record=fixture();record['clock']=previous
        if resources:record['map_resources']=[dict(kind='rock',x=0,y=0,supply=.125)]
        if stumps:
            record['terrain']=['f'*40]*40
            record['tree_stumps']=[dict(kind='tree',x=1,y=0,remaining=12.125)]
        manager=ChunkMap(dict(towns={},shared_map=dict(map_size=480,terrain_version=7,chunk_clock=clock)))
        try:manager.catch_up('0,0',deepcopy(record))
        except OverflowError:pass
        else:raise AssertionError('Expected Python OverflowError')
        cases.append(dict(label=label,key='0,0',size=480,version=7,clock=clock,
                          occupied=[],source=record,expected_error=True))
    record=fixture();record['clock']=0
    add('enormous-unused-elapsed',record,7,10**400)
    return cases


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    target=Path(__file__).with_name('catch_up.json')
    result=build()
    if args.check:
        assert json.loads(target.read_text())==result, 'Python oracle differs from frozen catch-up fixtures'
        print(f'Checked {len(result)} actual Python catch-up cases.')
    else:
        target.write_text(json.dumps(result,separators=(',',':'))+'\n')
        print(f'Wrote {len(result)} actual Python catch-up cases.')
if __name__=='__main__':main()
