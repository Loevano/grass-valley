"""R12 slice 2 vectors from real Python construction, isolated disposable towns."""
import copy
import json
from pathlib import Path
from generate import base, b
from catalog import ROOT
import sys
sys.path.insert(0, str(ROOT / 'tests'))
from fixtures import flat_town
import server


def case(name, roles, steps, roads=(), unreachable=False, event=None, sandbox=False):
    source=base();source['sandbox']=sandbox
    source['resources']=[]
    source['buildings'] += [b('home',40,30,footprint=[1,1],construction=dict(progress=0,total=30)),
                            b('storehouse',40,35,footprint=[1,1],construction=dict(progress=0,total=30))]
    source['roads']=list(roads)
    if unreachable:
        source['resources']=[b('rock',x,y) for x,y in ((39,30),(41,30),(40,29),(40,31))]
    workers=[dict(job=role,construction_position=dict(x=35,y=30)) for role in roles]
    town=flat_town(size=120)
    town.update(level=10,sandbox=sandbox,buildings=copy.deepcopy(source['buildings']),
                roads=copy.deepcopy(source['roads']),map_resources=copy.deepcopy(source['resources']),npcs=[],villagers=workers)
    outcomes=[];events=[]
    for tick,elapsed in enumerate(steps):
        change=None
        if tick==1 and event=='block':
            walk=workers[0]['construction_walk'];x,y=walk['path'][walk['step']+1]
            town['map_resources'].append(b('rock',x,y));change=dict(kind='block',x=x,y=y)
        if tick==2 and event=='move':
            town['buildings'][1]['x']=44;change=dict(kind='move',x=44,y=30)
        if tick==2 and event=='reassign':
            workers[0]['job']='farming';change=dict(kind='busy')
        if tick==3 and event=='reassign':
            workers[0]['job']='building';change=dict(kind='dedicated')
        if tick==1 and event=='open':
            town['map_resources']=[];change=dict(kind='open')
        events.append(change)
        server.construct(town,town,elapsed=elapsed)
        def worker(w):
            target=w.get('construction_target')
            walk=w.get('construction_walk')
            return dict(position=list(w['construction_position'].values()),
                        claim=([target['kind'],target['x'],target['y']] if target else None),
                        path=(walk['path'] if walk else None),step=(walk['step'] if walk else None),
                        working=bool(w.get('construction_arrived')))
        outcomes.append(copy.deepcopy(dict(
            remaining=[(s['construction']['total']-s['construction']['progress'] if 'construction' in s else None) for s in town['buildings']],
            roads=[(s['construction']['total']-s['construction']['progress'] if 'construction' in s else None) for s in town['roads']],
            workers=[worker(w) for w in workers])))
        # Python JSON reload is part of every vector, including saved walks.
        town=json.loads(json.dumps(town));workers=town['villagers']
    return dict(name=name,source=source,roles=roles,steps=steps,events=events,outcomes=outcomes)


def main():
    vectors=[case('dedicated priority and one worker per site',['idle','building','idle'],[1]*6+[3]*12),
             case('blocked next edge replans without lost work',['building'],[1]*6+[3]*12,event='block'),
             case('site moves while walking',['building'],[1]*10+[3]*12,event='move'),
             case('busy worker releases claim and resumes',['building'],[1]*8+[3]*12,event='reassign'),
             case('unreachable first site and retained second claim',['building'],[1]*6+[3]*12,unreachable=True,event='open'),
             case('completed roads speed departure steps',['building'],[1]*6+[3]*12,roads=[dict(x=x,y=30) for x in range(35,39)]),
             case('road construction after building sites',['building','building','idle'],[1]*6+[3]*14,roads=[dict(x=36,y=30,construction=dict(progress=0,total=10))]),
             case('sandbox completes without workers',[],[0],sandbox=True),
             case('no workers pauses progress',[],[3,3]),
             case('small updates retain waypoint timing',['building'],[.125]*10+[3]*14)]
    Path(__file__).with_name('builder_ticks.json').write_text(json.dumps(vectors,separators=(',',':'))+'\n')


if __name__=='__main__':main()
