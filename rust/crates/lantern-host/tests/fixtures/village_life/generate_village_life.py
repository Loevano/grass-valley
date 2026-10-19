"""Freeze actual Python ambient routing, paused visits and independent restocks.
Small geometry fixtures exercise ambient rules; they never replace terrain tests.
"""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys
from types import SimpleNamespace
ROOT=Path(__file__).resolve().parents[6]
sys.path.insert(0,str(ROOT/'server'))
import village_life as life
import villages
import npcs
import barns
import swamp_village as swamp


def world(layout=2,life_version=1,swamp=0):
    village=dict(name='Reedhaven',biome='b',x=80,y=80,layout_version=layout,life_version=life_version,swamp_version=swamp)
    roles=('winter woodworker','marsh fisher','remedy apprentice','trail warden','caravan innkeeper','cloth dyer','village trader')
    locals=[dict(name=f'Local {i}',role=role,x=79+i%2*2,y=81+i//2,village='Reedhaven',extension={'quantity':.125}) for i,role in enumerate(roles)]
    return dict(towns={'first':dict(buildings=[],player=dict(x=10.,y=10.),local_visits={'stable:1:1':dict(met=True,gift_claimed=True,story=3,trade_remaining=7.5)})},shared_map=dict(map_size=480,villages=[village],npcs=locals),private_quantity=123456789012345678901234567890)


def build():
    cases=[]
    def add(label,w,events,active=None):
        before=deepcopy(w);records=deepcopy(active);out=[]
        for event in events:
            op=event['op']
            if op=='advance':
                if records is not None:w['_chunks']=SimpleNamespace(records=deepcopy(records))
                life.advance(w,event['seconds']);w.pop('_chunks',None)
            elif op=='restock':
                for t in w['towns'].values():npcs.advance_trades(t,event['seconds'])
            elif op=='active':records=deepcopy(event['records'])
            elif op=='visitor':w['towns'][event['town']]['player']=deepcopy(event['position'])
            elif op=='building':w['towns'][event['town']]['buildings'].append(deepcopy(event['building']))
            elif op=='block_next':
                state=w['shared_map']['npcs'][event['npc']]['routine']
                target=state['path'][0]
                event={**event,'target':target}
                w['towns'][event['town']]['buildings'].append(dict(kind='home',x=target[0],y=target[1],footprint=[1,1]))
            out.append(dict(**event,after=deepcopy(w) if event.get('check',True) else None))
        routes=[]
        for i,v in enumerate(w['shared_map']['villages']):
            vx,vy=v['x'],v['y'];blocked=life.blocked_tiles(w,v)
            if records is not None:
                w['_chunks']=SimpleNamespace(records=deepcopy(records));blocked=life.blocked_tiles(w,v);w.pop('_chunks',None)
            fences=barns.fences([b for t in w['towns'].values() for b in t['buildings']],480)
            for start,goal in [((vx-1,vy+1),(vx+2,vy+3)),((vx+15,vy+7),(vx+20,vy+7)),((vx,vy),(vx,vy)),((vx,vy),(vx+40,vy+40))]:
                routes.append(dict(village=i,start=start,goal=goal,path=life.route(start,goal,blocked,v,fences)))
            if swamp.organic(v):
                data=swamp.layout()
                for x,y in sorted(data['footprint']-data['water']-set(data['roads'])):
                    start=(vx,vy);goal=(vx+x,vy+y)
                    path=life.route(start,goal,blocked,v,fences)
                    if path:
                        assert any((px-vx,py-vy) not in data['roads'] for px,py in path)
                        routes.append(dict(village=i,start=start,goal=goal,path=path,fallback=True))
                        break
                else: raise AssertionError('No organic off-lane fallback covered')
        cases.append(dict(label=label,before=before,active=active,events=out,routes=routes))
    def steps(count=80,seconds=.5):return [dict(op='advance',seconds=seconds,check=(i%20==19 or i==count-1)) for i in range(count)]
    for layout,version in [(1,0),(2,0),(2,1)]:add(f'ordinary-{layout}-{version}',world(layout,version),steps(240))
    add('old-swamp-v1',world(swamp=1),steps(120))
    w=dict(towns={'first':dict(buildings=[],roads=[],player=dict(x=10.,y=10.),local_visits={})},shared_map=dict(map_size=480,starter_offset=160))
    villages.ensure_villages(w)
    add('prepared-three-villages-organic-and-sleepers',w,steps(240))
    w=world();w['towns']['second']=dict(buildings=[],player=dict(x=80.,y=81.))
    add('second-founder-pauses-all-nearby',w,steps(4)+[dict(op='visitor',town='second',position=dict(x=10.,y=10.))]+steps(20))
    w=world();add('pause-at-distance-boundary',w,[dict(op='visitor',town='first',position=dict(x=80.8,y=81.)),dict(op='advance',seconds=.5),dict(op='visitor',town='first',position=dict(x=80.80001,y=81.)),dict(op='advance',seconds=.5)])
    w=world();add('new-building-replans-at-origin',w,steps(8)+[dict(op='block_next',town='first',npc=0)]+steps(80))
    w=world();w['towns']['first']['buildings']=[dict(kind='barn',x=82,y=79,fence_orientation=2,footprint=[1,1])]
    add('barn-fences-and-source-missing',w,steps(80))
    w=world();w['towns']['first']['buildings']=[dict(kind='barn',x=82,y=79,construction=dict(upgrade={}),fence_orientation=1)]
    add('upgrading-barn-still-fenced',w,steps(40))
    w=world();w['towns']['first']['buildings']=[dict(kind='barn',x=82,y=79,construction=dict(total=30),fence_orientation=1)]
    add('new-barn-unfenced',w,steps(40))
    w=world();records={'1,1':dict(map_resources=[])}
    add('active-removals-replace-authored-obstacles',w,steps(80)+[dict(op='active',records={'1,1':dict(map_resources=[dict(kind='water',x=79,y=79),dict(kind='tree',x=75,y=76)])})]+steps(40),records)
    w=world();w['shared_map']['npcs'].append(dict(name='Non-village',role='trader',x=80,y=81))
    add('standalone-local-obstacle-and-no-town-founder',w,steps(40))
    w=world();w['towns']={};add('absent-player-towns-still-ambient',w,steps(40))
    w=world();add('long-stall-transition-cap',w,[dict(op='advance',seconds=10000),dict(op='advance',seconds=.05)])
    w=world();add('zero-time-and-restock-clock',w,[dict(op='advance',seconds=0),dict(op='advance',seconds=30),dict(op='restock',seconds=3),dict(op='advance',seconds=.05),dict(op='restock',seconds=6)])
    w=world();w['shared_map']['villages']=[];add('absent-villages',w,steps(2)+[dict(op='restock',seconds=3)])
    w=world();w['shared_map']['npcs'][0]['routine']=dict(x=79.0000005,y=81.,path=[],stop=-1,wait=1.,elapsed=0.,phase='idle',job='idle',facing=1,origin=[79,81],saved_extension='keep')
    add('saved-fractional-rounding',w,[dict(op='advance',seconds=.1),dict(op='advance',seconds=.0000005)])
    w=world();w['shared_map']['npcs'][0]['routine']=dict(x=79.6,y=81.5,path=[[80,81]],stop=0,wait=8.,elapsed=0.,phase='walking',job='woodcutting',facing=1,origin=[79.5,81.5],goal=[82,83],source={'x':75,'y':76})
    add('saved-fractional-origin-retreat',w,[dict(op='block_next',town='first',npc=0)]+steps(30))
    w=world();w['shared_map']['npcs'][0].update(x=79.0,y=81.0,routine=dict(x=79.2,y=81.0,path=[[80.0,81.0]],stop=0,wait=8.,elapsed=0.,phase='walking',job='woodcutting',facing=1,origin=[79.0,81.0],goal=[82.0,83.0],source={'x':75.0,'y':76.0}))
    add('saved-integral-float-goal-and-source',w,steps(30))
    return cases

if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--check',action='store_true');args=parser.parse_args()
    result=build();text=json.dumps(result,ensure_ascii=False,separators=(',',':'))+'\n';path=Path(__file__).with_name('python.json')
    if args.check:
        if path.read_text()!=text:raise SystemExit('village life oracle drift')
    else:path.write_text(text)
    print(f'{len(result)} Python village life traces match')
