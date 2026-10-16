#!/usr/bin/env python3
"""Actual Python ordinary ensure phase; stop explicitly at swamp.ensure boundary.

No terrain sampling/RNG occurs in this phase. Geometry rule cases deliberately
contain only metadata. One real generated record checks lossless opaque terrain.
--check independently recomputes both catalog and oracle; never refreshes either.
"""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[6]
sys.path.insert(0, str(ROOT / 'server'))
import villages
from chunk_world import generate_chunk


def catalog():
    return [dict(name=n, biome=b, dx=dx, dy=dy, names=names, dialogue=d,
                 residents=villages.RESIDENTS[n]) for n,b,dx,dy,names,d in villages.THEMES]


def build():
    cases = []
    def add(label, world):
        before = deepcopy(world)
        with patch.object(villages.swamp_village, 'ensure') as swamp:
            villages.ensure_villages(world)
            assert swamp.call_count == 1
        after = deepcopy(world)
        with patch.object(villages.swamp_village, 'ensure'):
            villages.ensure_villages(world)
        cases.append(dict(label=label, before=before, after=after, repeated=world))

    def world(size=480, offset=160):
        result = dict(towns={}, shared_map=dict(map_size=size), private=dict(pin_hash='synthetic-only', quantity=.125))
        if offset is not None:
            result['shared_map']['starter_offset'] = offset
        return result

    for size, offset in ((120,None),(480,160),(480,None),(1920,None),(1920,880),(480,0),(480,479)):
        for seed in (42,'00042'):
            w = world(size,offset)
            w['shared_map']['terrain_seed']=seed
            add(f'placement-{size}-{offset}-{seed!r}',w)
    for version in range(1,8):
        w=world(); w['shared_map']['terrain_version']=version
        add(f'generator-marker-{version}',w)
    # Preferred site 80,200; exact inclusive obstacle boundary and full footprint.
    for dx,dy in ((-26,0),(26,26),(27,0),(-28,0)):
        for field in ('buildings','roads','npcs'):
            w=world(); obstacle=dict(x=80+dx,y=200+dy,footprint=[3,2],extension=.25)
            if field=='npcs':
                obstacle['name']='Unrelated'; w['shared_map']['npcs']=[obstacle]
            else: w['towns']['private-town-key']={field:[obstacle]}
            add(f'placement-block-{field}-{dx}-{dy}',w)
    w=world(120,None); w['towns']={'a':dict(buildings=[dict(x=0,y=0,footprint=[120,120])])}
    add('no-room-no-npcs-created',w)
    for ti,(name,biome,*_) in enumerate(villages.THEMES):
        for layout in (None,0,1,2):
            for life in (None,0,1):
                w=world(); v=dict(name=name,biome=biome,x=80,y=200,extension={'saved':.125})
                if layout is not None: v['layout_version']=layout
                if life is not None: v['life_version']=life
                v['swamp_version']=ti%3
                w['shared_map']['villages']=[v]
                w['shared_map']['npcs']=[dict(name=_[2][0],village=name,x=79,y=201,dialogue='old',routine={'path':[[80,201]],'fraction':.125},extension='keep')]
                add(f'markers-{name}-{layout}-{life}',w)
    for field in ('buildings','roads','npcs'):
        for dx in (10,11,-12):
            w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=80,y=200)]
            item=dict(x=80+dx,y=200,footprint=[3,2],name='Other')
            if field=='npcs': w['shared_map']['npcs']=[item]
            else: w['towns']['a']={field:[item]}
            add(f'expand-obstacle-{field}-{dx}',w)
    for dx,dy in [(x,y) for _,x,y in villages.PLANTINGS]+[(x,y) for _,x,y,_,_ in villages.LIFE_BUILDINGS]:
        w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=80,y=200,layout_version=2)]
        w['towns']['a']=dict(buildings=[dict(x=79+dx,y=199+dy,footprint=[2,2])])
        add(f'life-footprint-{dx}-{dy}',w)
    for footprint in ([0,2],[-1,2],[2,0],[2,-1],[1,1]):
        w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=80,y=200)]
        w['towns']['a']=dict(roads=[dict(x=80,y=200,footprint=footprint)])
        add(f'empty-or-unit-footprint-{footprint}',w)
    for owner in (None,'Sunwell','Reedhaven'):
        w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=80,y=200,layout_version=2)]
        item=dict(name='Saved',x=73,y=193)
        if owner is not None: item['village']=owner
        w['shared_map']['npcs']=[item]
        add(f'life-ignores-npc-{owner}',w)
    w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=80,y=200)]
    w['shared_map']['npcs']=[dict(name='Safiya',role='saved',x=74,y=199,village='Sunwell',extension=17)]
    add('existing-own-resident-is-not-deduplicated',w)
    for x,y in ((0,0),(479,479)):
        w=world(); w['shared_map']['villages']=[dict(name='Sunwell',biome='d',x=x,y=y)]
        add(f'saved-edge-village-{x}-{y}',w)
    # Own residents don't block, other villages do; sequential additions matter.
    for reverse in (False,True):
        w=world(); vs=[dict(name='Sunwell',biome='d',x=80,y=200),dict(name='Frostbrook',biome='i',x=91,y=200)]
        if reverse: vs.reverse()
        w['shared_map']['villages']=vs
        w['shared_map']['npcs']=[dict(name='Owned',village=vs[0]['name'],x=80,y=200)]
        add(f'expansion-order-{reverse}',w)
    w=world(); w['shared_map']['villages']=[]
    add('existing-empty-prevents-placement',w)
    w=world(); w['shared_map'].update(generate_chunk('00042','2,2',version=7,size=480));
    add('real-terrain-record-opaque-preservation',w)
    return cases


def main():
    parser=argparse.ArgumentParser(); parser.add_argument('--check',action='store_true'); args=parser.parse_args()
    targets=[(Path(__file__).with_name('village_metadata.json'),build()),
             (ROOT/'rust/crates/lantern-host/src/legacy/village_overlay/metadata_catalog.json',catalog())]
    for path,data in targets:
        text=json.dumps(data,ensure_ascii=False,separators=(',',':'))+'\n'
        if args.check:
            if path.read_text()!=text: raise SystemExit('Python ordinary metadata oracle/catalog differs; review before refreshing')
        else: path.write_text(text)
    print(f'{len(targets[0][1])} actual Python ordinary metadata cases and catalog match')

if __name__=='__main__': main()
