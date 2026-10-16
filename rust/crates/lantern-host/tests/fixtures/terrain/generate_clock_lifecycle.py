#!/usr/bin/env python3
"""Actual Python exact-clock read/flush traces; JSON strings keep huge integers raw."""
import argparse
from collections import OrderedDict
from copy import deepcopy
import json
from pathlib import Path
import sys
ROOT=Path(__file__).resolve().parents[6]
sys.path.insert(0,str(ROOT/'server'))
from chunk_world import ChunkMap, generate_chunk


def build():
    cases=[]
    for i,(now,previous,next_clock) in enumerate([
        (2**53,2**53,2**53+1),
        (2**53+1,2**53,2**53+2),
        (2**64,2**64,2**64+1),
        (10**400,10**400,10**400+7),
        (10**400,10**400+1,10**400+2),
        (float(2**53),2**53+1,2**53+1),
        (2**53,float(2**53),2**53+1),
        (125.0,125,126),
        (0.0,-0.0,0),
    ]):
        w=dict(towns={},town_sites=[dict(x=0,y=0)],shared_map=dict(chunked=1,chunk_size=40,
            terrain_seed='00042',terrain_version=7,map_size=480,starter_offset=160,
            chunk_clock=now,villages=[],npcs=[]))
        active={};camera={}
        for key,target in [('0,0',active),('2,5',camera)]:
            r=generate_chunk('00042',key,7)
            # Keep actual generated geometry and resource order, deplete stock.
            for resource in r['map_resources']:resource['supply']=.125
            r['clock']=previous
            target[key]=r
        runtime=deepcopy(w);m=ChunkMap(runtime)
        m.records=deepcopy(active);m.views=OrderedDict(deepcopy(camera))
        runtime['shared_map'].update(map_resources=m.records['0,0']['map_resources'],tree_stumps=[])
        def encoded(v):return json.dumps(v,separators=(',',':'))
        actions=[dict(op='read',key='0,0'),dict(op='read',key='2,5'),dict(op='read',key='2,5'),
                 dict(op='clock',clock=encoded(next_clock)),dict(op='read',key='0,0'),
                 dict(op='read',key='2,5'),dict(op='flush')]
        outputs=[]
        for action in actions:
            record=None
            if action['op']=='clock':runtime['shared_map']['chunk_clock']=json.loads(action['clock'])
            elif action['op']=='read':record=deepcopy(m.read(action['key']))
            elif action['op']=='flush':m.flush()
            outputs.append(dict(read=encoded(record) if record is not None else None,
                active={k:encoded(v) for k,v in m.records.items()},
                memory=dict(m.memory),camera=list(m.views),clock=encoded(runtime['shared_map']['chunk_clock'])))
        cases.append(dict(label=f'exact-clock-lifecycle-{i}',world=encoded(w),
            active={k:encoded(v) for k,v in active.items()},camera={k:encoded(v) for k,v in camera.items()},
            actions=actions,outputs=outputs))
    return cases


def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    target=Path(__file__).with_name('clock_lifecycle.json');result=build()
    if args.check:
        assert json.loads(target.read_text())==result,'Python clock lifecycle differs from frozen oracle'
        print(f'Checked {len(result)} actual Python clock lifecycle traces.')
    else:
        target.write_text(json.dumps(result,separators=(',',':'))+'\n')
        print(f'Wrote {len(result)} actual Python clock lifecycle traces.')
if __name__=='__main__':main()
