#!/usr/bin/env python3
"""Frozen actual Python swamp ensure worlds/order/cache plans; --check never writes.
Geometry-only metadata rules do not sample terrain/RNG. Retained opaque terrain
comes from real generation, not a terrain stub. Full-chain cases invoke villages.
"""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys
from types import SimpleNamespace
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[6]
sys.path.insert(0,str(ROOT/'server'))
import swamp_village as swamp
import villages
from chunk_world import generate_chunk
from npcs import local_key

def build():
    cases=[]
    def world(x=80,y=200,swamp_version=None,layout=2,life=1,size=480):
        v=dict(name='Reedhaven',biome='b',x=x,y=y,layout_version=layout,life_version=life,saved_extension=.125)
        if swamp_version is not None:v['swamp_version']=swamp_version
        return dict(towns={'private-town':dict(local_visits={'unmoved':{'story':3,'quantity':.125}})},shared_map=dict(map_size=size,starter_offset=160,villages=[v],npcs=[]),private=dict(credential='synthetic-only',quantity=123456789012345678901234567890))
    def add(label,w,full=False):
        if full:
            # Fixture starts precisely at the phase boundary used by Rust.
            with patch.object(swamp,'ensure'):
                villages.ensure_villages(w)
        before=deepcopy(w)
        # Trace the actual runtime cache call sequence while keeping the oracle
        # output a manager-free private world; no pretend terrain is sampled.
        traced=deepcopy(w); calls=[]
        traced['_chunks']=SimpleNamespace(records={'0,0':{'map_resources':[],'tree_stumps':[]}},views={'1,0':{}},landscape=traced['shared_map'],membership=True,resource_index=True)
        def capture(record,key,vs,size):
            ordinal=next(i for i,v in enumerate(traced['shared_map']['villages']) if v is vs[0])
            calls.append((ordinal,key,size))
        with patch.object(swamp,'apply',side_effect=capture):swamp.ensure(traced)
        plan=[i for i,key,size in calls if key=='0,0']
        assert calls==[(i,key,40) for i in plan for key in ('0,0','1,0')]
        if plan:
            assert traced['_chunks'].membership is None and traced['_chunks'].resource_index is None
            assert traced['shared_map']['map_resources']==[] and traced['shared_map']['tree_stumps']==[]
        swamp.ensure(w); after=deepcopy(w); swamp.ensure(w)
        cases.append(dict(label=label,before=before,after=after,repeated=w,plan=plan))
    for sv in (None,0,1,2):
        for layout in (0,1,2):
            for life in (0,1):add(f'markers-{sv}-{layout}-{life}',world(swamp_version=sv,layout=layout,life=life))
    for seed in (42,'00042',-7):
        for size,offset in ((120,0),(480,160),(1920,None)):
            w=dict(towns={},shared_map=dict(map_size=size,terrain_seed=seed))
            if offset is not None:w['shared_map']['starter_offset']=offset
            add(f'ordinary-chain-{size}-{seed!r}',w,full=True)
    for version in range(1,8):
        for seed in (42,'00042'):
            w=world(swamp_version=1);w['shared_map'].update(terrain_version=version,terrain_seed=seed)
            add(f'version-and-typed-seed-{version}-{seed!r}',w)
    w=world();w['shared_map']['villages']=[];w['shared_map'].pop('npcs');add('empty-villages-no-npcs',w)
    touched=swamp.layout()['footprint']|swamp.layout()['water']
    # Exact perimeter of organic shape plus old-square-only holes and outside.
    points=set([(min(x for x,y in touched),0),(max(x for x,y in touched),10),(0,min(y for x,y in touched)),(10,max(y for x,y in touched)),(-31,0),(31,0)])
    points.update(list(sorted(touched))[::97]);points.update([(-19,-19),(18,18),(-18,17),(16,-8),(17,14),(0,0),(30,10)])
    for sv in (None,1):
        for field in ('buildings','roads','npcs'):
            for x,y in sorted(points):
                w=world(swamp_version=sv); item=dict(x=80+x,y=200+y,footprint=[1,1],name='Other',extension='saved')
                if field=='npcs':w['shared_map']['npcs']=[item]
                else:w['towns']['private-town'][field]=[item]
                add(f'obstacle-{sv}-{field}-{x}-{y}',w)
    for fp in ([0,3],[-1,3],[3,0],[3,-1],[3,2],[1000,1000]):
        w=world(swamp_version=1);w['towns']['private-town']['buildings']=[dict(x=60,y=180,footprint=fp)]
        add(f'footprint-{fp}',w)
    for x,y in ((0,0),(24,200),(25,200),(26,200),(449,200),(450,200),(80,22),(80,23),(80,454),(80,455),(479,479)):
        add(f'bounds-{x}-{y}',world(x,y,swamp_version=1))
    def resident(name,x=70,y=190,**extra):return dict(name=name,x=x,y=y,village='Reedhaven',role='saved-role',dialogue='saved-dialogue',extension={'keep':.125},**extra)
    for owner in ('Reedhaven','Sunwell',None):
        w=world();n=resident('Custom',80,200)
        if owner is None:n.pop('village')
        else:n['village']=owner
        w['shared_map']['npcs']=[n];add(f'npc-owner-{owner}',w)
    for name in swamp.NAMES+tuple(n for n,x,y in swamp.SLEEPERS):
        w=world(swamp_version=1);n=resident(name,routine=dict(x=70.5,y=190.5,path=[[70,190]],phase='walking',wait=.25,origin=[70,190],saved_progress=.125))
        w['shared_map']['npcs']=[n];w['towns']['private-town']['local_visits'][local_key(n)]={'story':7,'gift':True,'trade_remaining':1.25,'extension':{'saved':3}}
        add(f'existing-{name}',w)
    # Saved traders/custom names, fractional ties, on-road lexemes and road-order ties.
    for x,y in ((80,200),(80.5,200.5),(79.5,199.5),(70.5,190.5),(110,230),(-3.5,-2.5),(2147483000,2147483000)):
        w=world(swamp_version=1);w['shared_map']['npcs']=[resident('Fen',routine=dict(x=x,y=y,path=[[80,200]],phase='walking',wait=3,origin=[80,200],extension='keep'))]
        add(f'routine-round-nearest-{x}-{y}',w)
    for state in (None,{},dict(x=80,y=200)):
        w=world();w['shared_map']['npcs']=[resident('Saved',routine=state)];add(f'falsey-or-minimal-routine-{state}',w)
    w=world(); homes=swamp.layout()['homes'];n=resident('Aster');w['shared_map']['npcs']=[n]
    old=local_key(n);new=f'Aster:{80+homes[0][0]}:{200+homes[0][1]}'
    w['towns']['private-town']['local_visits']={old:{'story':9},'middle':{'keep':3},new:{'story':1}}
    w['towns']['second']={'local_visits':{old:{'gift':7},'after':{'keep':5}}};add('visit-destination-collision',w)
    w=world();n=resident('Aster',80+homes[0][0],200+homes[0][1]);w['shared_map']['npcs']=[n]
    w['towns']['private-town']['local_visits']={local_key(n):{'story':2},'tail':{'keep':1}};add('equal-visit-key-pop-reinserts',w)
    w=world();w['shared_map']['npcs']=[resident('Aster',70,190),resident('Fen'),resident('Aster',71,191),resident('Fen',72,192,routine=dict(x=70,y=190))];add('duplicate-resident-last-value-first-order',w)
    for reverse in (False,True):
        w=world(swamp_version=1);w['shared_map']['villages'].append(dict(name='Reedhaven',biome='b',x=81,y=201,layout_version=2,life_version=1,swamp_version=0))
        if reverse:w['shared_map']['villages'].reverse()
        w['shared_map']['npcs']=[resident('Aster')];w['towns']['private-town']['local_visits']['Aster:70:190']={'story':11,'extension':.125};add(f'repeated-named-village-order-{reverse}',w)
    w=world();w['shared_map'].update(generate_chunk('00042','2,2',version=7,size=480));add('opaque-real-v7-terrain-preserved',w)
    return cases

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    cases=build();text=json.dumps(cases,ensure_ascii=False,separators=(',',':'))+'\n';path=Path(__file__).with_name('swamp_metadata.json')
    if args.check:
        if path.read_text()!=text:raise SystemExit('Actual Python swamp metadata oracle differs; review before refreshing')
    else:path.write_text(text)
    print(f'{len(cases)} actual Python swamp metadata worlds and cache plans match')
if __name__=='__main__':main()
